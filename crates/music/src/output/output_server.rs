//! The sink Sonora plays on, read from PipeWire or PulseAudio over the PulseAudio native
//! protocol, which `pipewire-pulse` speaks too. One socket carries the queries and the change
//! events.

use std::io::Cursor;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use pulseaudio::protocol::{
    self, AuthParams, AuthReply, ClientInfoList, Command, CommandReply, GetSinkInfo, Prop, Props,
    SampleFormat, ServerInfo, SetClientNameReply, SinkInfo, SinkInputInfoList, SubscriptionEvent,
    SubscriptionEventFacility, SubscriptionEventType, SubscriptionMask,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::UnixStream;
use tokio::sync::watch;

use super::{Format, Sample, Sink};

/// How long the events of one change are let pass before the server is asked again. A new
/// stream shows on the default sink for a few milliseconds, until the server links it
/// where it plays, and a stream Sonora opens takes a moment to show up at all.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(250);

/// How much room each read of the socket makes, enough for most messages at once.
const READ: usize = 4096;

/// How long the authentication cookie is. `pipewire-pulse` takes any cookie of this length
/// and checks nothing else, so zeros stand in when there is no cookie file.
const COOKIE: usize = 256;

/// One open connection, past the handshake.
struct Connection {
    socket: UnixStream,
    /// Bytes read that do not make a whole message yet, kept so a read cut short loses none.
    incoming: Vec<u8>,
    version: u16,
    seq: u32,
    /// Set when an event came in while waiting for a reply, so it is not lost.
    stirred: bool,
    /// The sink's index as last read, so its own changes can be told from others'. `None` while
    /// the server has no stream of Sonora's.
    sink: Option<u32>,
    /// Sonora's own stream on the server as last read, `None` while it has none.
    input: Option<u32>,
    /// Whether Sonora has had a stream on this connection. Until it has, the default sink
    /// stands in.
    played: bool,
}

/// Reports the sink Sonora's stream plays on, the server's default until it has one, and again
/// once anything that can move it has settled: the sink, its port or format, the server's
/// default, or Sonora's own stream. `None` means the server has no stream of Sonora's, because it
/// is open on a device the server does not see or, for a moment, not open at all. Returns once
/// the server cannot be reached or hangs up; nothing is retried. Run it on tokio.
pub async fn follow(mut report: impl FnMut(Option<Sink>) + Send) -> Result<()> {
    let mut connection = Connection::open().await?;
    let mut streams = super::watch_device_changes();
    loop {
        report(connection.played_sink().await?);
        connection.wait_for_change(&mut streams).await?;
        tokio::time::sleep(SETTLE).await;
    }
}

impl Connection {
    async fn open() -> Result<Self> {
        let socket = connect().await?;
        let mut connection = Self {
            socket,
            incoming: Vec::new(),
            version: protocol::MAX_VERSION,
            seq: 0,
            stirred: false,
            sink: None,
            input: None,
            played: false,
        };
        let auth = AuthParams {
            version: protocol::MAX_VERSION,
            supports_shm: false,
            supports_memfd: false,
            cookie: cookie(),
        };
        let reply: AuthReply = connection.request(&Command::Auth(auth)).await?;
        connection.version = protocol::MAX_VERSION.min(reply.version);
        let mut props = Props::new();
        props.set(Prop::ApplicationName, c"Sonora");
        let _: SetClientNameReply = connection.request(&Command::SetClientName(props)).await?;
        connection
            .acknowledge(&Command::Subscribe(
                SubscriptionMask::SINK | SubscriptionMask::SINK_INPUT | SubscriptionMask::SERVER,
            ))
            .await?;
        Ok(connection)
    }

    async fn played_sink(&mut self) -> Result<Option<Sink>> {
        // The process id is on the client that owns a stream, not on the stream. A sandbox
        // can give another app the same id, so the client's name has to be ours too.
        let clients: ClientInfoList = self.request(&Command::GetClientInfoList).await?;
        let pid = format!("{}\0", std::process::id());
        let names = own_names();
        let own: Vec<u32> = clients
            .iter()
            .filter(|client| {
                let named = |prop| {
                    let value = client.props.get(prop);
                    names.iter().any(|name| value == Some(name.as_bytes()))
                };
                client.props.get(Prop::ApplicationProcessId) == Some(pid.as_bytes())
                    && (named(Prop::ApplicationProcessBinary) || named(Prop::ApplicationName))
            })
            .map(|client| client.index)
            .collect();
        let inputs: SinkInputInfoList = self.request(&Command::GetSinkInputInfoList).await?;
        let mine = inputs.iter().find(|input| {
            input
                .client_index
                .is_some_and(|client| own.contains(&client))
        });
        self.input = mine.map(|input| input.index);
        let device = super::active_device_info().map(|device| device.id);
        self.played |= mine.is_some() || device.is_some();
        let wanted = match mine {
            Some(input) => GetSinkInfo {
                index: Some(input.sink_index),
                name: None,
            },
            None if !self.played => {
                let server: ServerInfo = self.request(&Command::GetServerInfo).await?;
                let name = server
                    .default_sink_name
                    .context("the sound server has no default sink")?;
                GetSinkInfo {
                    index: None,
                    name: Some(name),
                }
            }
            None => {
                self.sink = None;
                return Ok(None);
            }
        };
        let sink: SinkInfo = self.request(&Command::GetSinkInfo(wanted)).await?;
        self.sink = Some(sink.index);
        Ok(Some(of(&sink, device)))
    }

    /// Waits for an event that can change what `played_sink` reads: the server's default
    /// moving, a stream coming or Sonora's moving or going, the sink changing or going, or
    /// Sonora's own stream opening or closing, which `streams` hears.
    async fn wait_for_change(&mut self, streams: &mut watch::Receiver<()>) -> Result<()> {
        if std::mem::take(&mut self.stirred) {
            return Ok(());
        }
        loop {
            let frame = tokio::select! {
                frame = self.frame() => frame?,
                changed = streams.changed() => {
                    changed?;
                    return Ok(());
                }
            };
            if let (_, Command::SubscribeEvent(event)) =
                protocol::read_command_message(&mut Cursor::new(&frame), self.version)?
                && self.concerns(&event)
            {
                return Ok(());
            }
        }
    }

    fn concerns(&self, event: &SubscriptionEvent) -> bool {
        match event.event_facility {
            SubscriptionEventFacility::Server => true,
            SubscriptionEventFacility::Sink => {
                event.event_type != SubscriptionEventType::New && event.index == self.sink
            }
            SubscriptionEventFacility::SinkInput => {
                event.event_type == SubscriptionEventType::New || event.index == self.input
            }
            _ => false,
        }
    }

    /// Sends `command` and reads its reply, noting any event that arrives first.
    async fn request<T: CommandReply>(&mut self, command: &Command) -> Result<T> {
        let seq = self.send(command).await?;
        let frame = self.reply(seq).await?;
        let (_, reply) = protocol::read_reply_message::<T>(&mut Cursor::new(&frame), self.version)?;
        Ok(reply)
    }

    /// Sends a command whose reply carries nothing.
    async fn acknowledge(&mut self, command: &Command) -> Result<()> {
        let seq = self.send(command).await?;
        self.reply(seq).await.map(drop)
    }

    async fn send(&mut self, command: &Command) -> Result<u32> {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        let mut message = Vec::new();
        protocol::write_command_message(&mut message, seq, command, self.version)?;
        self.socket.write_all(&message).await?;
        Ok(seq)
    }

    /// The frame answering `seq`, an error reply turned into an error.
    async fn reply(&mut self, seq: u32) -> Result<Vec<u8>> {
        loop {
            let frame = self.frame().await?;
            match protocol::read_command_message(&mut Cursor::new(&frame), self.version)? {
                (_, Command::SubscribeEvent(event)) => {
                    self.stirred |= self.concerns(&event);
                }
                (answered, Command::Error(error)) if answered == seq => {
                    bail!("the sound server refused: {error:?}")
                }
                (answered, Command::Reply) if answered == seq => return Ok(frame),
                _ => {}
            }
        }
    }

    /// One whole message, its header included. A wait cut short keeps what it read for the
    /// next one, so this can race another future.
    async fn frame(&mut self) -> Result<Vec<u8>> {
        loop {
            if let Some(header) = self.incoming.first_chunk::<4>() {
                let length = u32::from_be_bytes(*header) as usize;
                if length > protocol::MAX_MEMBLOCKQ_LENGTH {
                    bail!("the sound server sent a {length} byte message");
                }
                let whole = protocol::DESCRIPTOR_SIZE + length;
                if self.incoming.len() >= whole {
                    let rest = self.incoming.split_off(whole);
                    return Ok(std::mem::replace(&mut self.incoming, rest));
                }
            }
            self.incoming.reserve(READ);
            if self.socket.read_buf(&mut self.incoming).await? == 0 {
                bail!("the sound server hung up");
            }
        }
    }
}

/// What the server says about a sink, keyed by its name and the port that plays, for Sonora's
/// stream on `device`.
fn of(sink: &SinkInfo, device: Option<String>) -> Sink {
    let sink_name = sink.name.to_string_lossy();
    let port = sink.ports.get(sink.active_port);
    let id = match port {
        Some(port) => format!("{sink_name}#{}", port.name.to_string_lossy()),
        None => sink_name.to_string(),
    };
    let name = sink
        .description
        .as_ref()
        .map_or(sink_name.to_string(), |name| {
            name.to_string_lossy().into_owned()
        });
    let port = port
        .filter(|_| sink.ports.len() > 1)
        .map(|port| port.description.as_ref().unwrap_or(&port.name))
        .map(|port| port.to_string_lossy().into_owned());
    let spec = sink.sample_spec;
    let sample = match spec.format {
        SampleFormat::U8 => Sample::Integer(8),
        SampleFormat::S16Le | SampleFormat::S16Be => Sample::Integer(16),
        SampleFormat::S24Le
        | SampleFormat::S24Be
        | SampleFormat::S24In32Le
        | SampleFormat::S24In32Be => Sample::Integer(24),
        SampleFormat::S32Le | SampleFormat::S32Be => Sample::Integer(32),
        SampleFormat::Float32Le | SampleFormat::Float32Be => Sample::Float(32),
        _ => Sample::Other,
    };
    let format = (spec.sample_rate > 0 && spec.channels > 0).then_some(Format {
        rate: spec.sample_rate,
        channels: u16::from(spec.channels),
        sample,
    });
    Sink {
        id,
        name,
        port,
        format,
        device,
    }
}

/// The names a client of this process goes by, as its binary or application name, nul
/// ended as the server sends them: the executable's file name and the name it was run as.
fn own_names() -> Vec<String> {
    let exe = std::env::current_exe().ok();
    let run = std::env::args_os().next().map(PathBuf::from);
    [exe, run]
        .into_iter()
        .flatten()
        .filter_map(|path| Some(format!("{}\0", path.file_name()?.to_str()?)))
        .collect()
}

/// The server's socket, the first of `pulse_sockets` that answers.
async fn connect() -> Result<UnixStream> {
    for path in pulse_sockets() {
        if let Ok(socket) = UnixStream::connect(&path).await {
            return Ok(socket);
        }
    }
    bail!("no sound server socket answers")
}

/// Where the PulseAudio socket may be, in the order libpulse looks: `PULSE_SERVER` when it
/// names a local socket, then the runtime directories. PipeWire answers there too.
pub(super) fn pulse_sockets() -> impl Iterator<Item = PathBuf> {
    let server = std::env::var("PULSE_SERVER").ok();
    let named = server
        .as_deref()
        .and_then(|server| server.strip_prefix("unix:"))
        .map(PathBuf::from);
    let runtime = [
        std::env::var_os("PULSE_RUNTIME_PATH").map(|path| PathBuf::from(path).join("native")),
        std::env::var_os("XDG_RUNTIME_DIR").map(|path| PathBuf::from(path).join("pulse/native")),
    ];
    named.into_iter().chain(runtime.into_iter().flatten())
}

/// The authentication cookie from where libpulse keeps it, or zeros.
fn cookie() -> Vec<u8> {
    let home = dirs::home_dir();
    let candidates = [
        std::env::var_os("PULSE_COOKIE").map(PathBuf::from),
        std::env::var_os("XDG_CONFIG_HOME")
            .map(|config| PathBuf::from(config).join("pulse/cookie")),
        home.as_ref().map(|home| home.join(".config/pulse/cookie")),
        home.map(|home| home.join(".pulse-cookie")),
    ];
    candidates
        .into_iter()
        .flatten()
        .filter_map(|path| std::fs::read(path).ok())
        .find(|cookie| cookie.len() == COOKIE)
        .unwrap_or_else(|| vec![0; COOKIE])
}
