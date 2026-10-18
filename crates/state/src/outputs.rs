use std::time::Duration;

use anyhow::Result;
use gpui::{App, Context, Entity, Task};
use music::output::{self, DeviceInfo, DeviceOption, Failure, Opened, OutputId, Sink};

use crate::{AppSettings, Io, Outcome, Playback, Toasts, join};

/// How long a newly picked device holds the picker before it is let go, whether or not it has
/// answered. CoreAudio takes about ten seconds to give up on a device that will not start.
const SWITCH_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a picked device's stream is kept open for the restarted engines to join it.
/// TODO: Investigate if we should use a signal instead of a duration.
/// Perhaps rearchitect SHARED
const HANDOVER: Duration = Duration::from_secs(2);

/// How long the first reconnect to the sound server waits, doubling up to `RETRY_MOST`. It goes
/// away on a restart and may not be up yet when Sonora starts with the session.
#[cfg(target_os = "linux")]
const RETRY_FIRST: Duration = Duration::from_secs(2);
#[cfg(target_os = "linux")]
const RETRY_MOST: Duration = Duration::from_secs(60);

/// The output devices the system offers, the one chosen, what the stream Sonora plays into
/// runs at, and on Linux the sink the sound server plays it on. The list is read off the main
/// thread, since that opens devices on some platforms, and both are read again whenever a stream
/// opens or closes. A device picked here is opened first and only moved onto, and saved, once it
/// has. One that will not open leaves everything as it was and says so in a toast. The output
/// the sound plays on settles in `settled_binding_key`, which the equalizer follows to switch to
/// the profile bound to it.
pub struct Outputs {
    settings: Entity<AppSettings>,
    playback: Entity<Playback>,
    devices: Vec<DeviceOption>,
    /// Whether `devices` has been read at least once, so an empty list means none.
    listed: bool,
    active: Option<DeviceInfo>,
    /// The device a stream was last open on, kept while the stream reopens so what it offers
    /// stays on screen.
    known: Option<DeviceInfo>,
    failure: Option<Failure>,
    /// Whether nothing is heard because no stream would open.
    failing: bool,
    /// The device chosen, by cpal id, or `None` to follow the system's default.
    selected: Option<String>,
    /// A device picked and being opened, shown as chosen until it answers.
    trying: Option<Option<String>>,
    /// Opens the picked device off the main thread. Dropping it drops the pick.
    opening: Option<Task<()>>,
    /// Runs out `SWITCH_TIMEOUT` after a pick. While it is set the picker is held.
    holding: Option<Task<()>>,
    /// The picked device's stream, kept open until the engines have joined it.
    handover: Option<(Opened, Task<()>)>,
    listing: Option<Task<()>>,
    /// The sink the sound server last played Sonora on. It names the output while Sonora plays on
    /// the device it was seen with, so the server going away and back is not a move.
    sink: Option<Sink>,
    /// The output the sound plays on once it settled, kept while none is known so the equalizer
    /// following it sees no move.
    settled: Option<OutputId>,
    /// Whether the sound server answers, so its reports, which wait for a new stream to settle,
    /// name the output rather than the device watch. Assumed until the server is found missing.
    #[cfg(target_os = "linux")]
    server: bool,
    _watch: Task<()>,
    #[cfg(target_os = "linux")]
    _follow: Task<()>,
}

impl Outputs {
    /// Hands the device, rate and buffer saved in settings to the engines, so call it before the
    /// first one starts.
    pub fn new(
        settings: Entity<AppSettings>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let selected = settings.read(cx).output_device().map(str::to_owned);
        output::choose_device(selected.clone());
        output::choose_rate(settings.read(cx).output_rate());
        output::choose_buffer(settings.read(cx).output_buffer());

        let mut changes = output::watch_device_changes();
        let watch = cx.spawn(async move |this, cx| {
            while changes.changed().await.is_ok() {
                let updated = this.update(cx, |this, cx| {
                    this.read_device();
                    this.failure = output::latest_device_failure();
                    this.failing = output::failing();
                    this.forget_broken(cx);
                    this.refresh(cx);
                    this.follow_device(cx);
                    cx.notify();
                });
                if updated.is_err() {
                    return;
                }
            }
        });

        let mut outputs = Self {
            settings,
            playback,
            devices: Vec::new(),
            listed: false,
            active: output::active_device_info(),
            known: output::active_device_info(),
            failure: output::latest_device_failure(),
            failing: output::failing(),
            selected,
            trying: None,
            opening: None,
            holding: None,
            handover: None,
            listing: None,
            sink: None,
            settled: None,
            #[cfg(target_os = "linux")]
            server: true,
            _watch: watch,
            #[cfg(target_os = "linux")]
            _follow: follow(cx),
        };
        outputs.refresh(cx);
        outputs
    }

    pub fn devices(&self) -> &[DeviceOption] {
        &self.devices
    }

    /// Whether the system's devices have been read yet.
    pub fn listed(&self) -> bool {
        self.listed
    }

    /// The device Sonora plays on, or `None` while no stream is open.
    pub fn active(&self) -> Option<&DeviceInfo> {
        self.active.as_ref()
    }

    /// The device playing, or while its stream reopens, the one that last played.
    pub fn current(&self) -> Option<&DeviceInfo> {
        self.active.as_ref().or(self.known.as_ref())
    }

    /// The key equalizer profiles are bound to for the output the sound plays on: the sound
    /// server's sink while it plays Sonora, otherwise the device Sonora's stream is open on, or
    /// last was.
    pub fn binding_key(&self) -> Option<OutputId> {
        let (id, name) = match (self.sink(), self.current()) {
            (Some(sink), _) => (&sink.id, &sink.name),
            (None, Some(device)) => (&device.id, &device.name),
            (None, None) => return None,
        };
        Some(OutputId {
            id: id.clone(),
            name: name.clone(),
        })
    }

    /// The binding key as last settled. Unlike `binding_key` it waits for the sound
    /// server's report after a stream opens, so the equalizer following it never switches
    /// profiles on a device that is only named for a moment.
    pub fn settled_binding_key(&self) -> Option<&OutputId> {
        self.settled.as_ref()
    }

    /// The sink the sound server plays Sonora on, while Sonora plays on the device it was seen
    /// with, or before Sonora has played anywhere, the server's default.
    pub fn sink(&self) -> Option<&Sink> {
        self.sink.as_ref().filter(|sink| self.plays(sink))
    }

    /// Whether `sink` was seen with the device Sonora plays on, or last did.
    fn plays(&self, sink: &Sink) -> bool {
        sink.device.as_deref() == self.current().map(|device| device.id.as_str())
    }

    /// Reads the device Sonora plays on again.
    fn read_device(&mut self) {
        self.active = output::active_device_info();
        if self.active.is_some() {
            self.known.clone_from(&self.active);
        }
    }

    /// Why nothing plays, when the device to play on would not open and nothing took its place.
    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref().filter(|_| self.active.is_none())
    }

    /// Whether nothing is heard because no stream would open: every device to try would not open,
    /// or there was none.
    pub fn failing(&self) -> bool {
        self.failing
    }

    /// The device playing or being tried, or `None` when Sonora follows the system's default.
    /// It may not be connected, in which case Sonora plays on the default until it is.
    pub fn chosen(&self) -> Option<&str> {
        match &self.trying {
            Some(trying) => trying.as_deref(),
            None => self.selected.as_deref(),
        }
    }

    /// The rate the device plays at, or is reopening at once a new one is chosen: the one saved in
    /// settings when the device offers it, otherwise the one it runs at.
    pub fn rate(&self, cx: &App) -> Option<u32> {
        let active = self.current()?;
        let wanted = self
            .settings
            .read(cx)
            .output_rate()
            .filter(|rate| active.rates.contains(rate));
        Some(wanted.unwrap_or(active.format.rate))
    }

    /// Opens the device at `rate` from now on, and saves it. The engines reopen at it, paused or
    /// not.
    pub fn set_rate(&mut self, rate: u32, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.set_output_rate(Some(rate), cx));
        output::choose_rate(Some(rate));
        cx.notify();
    }

    /// The buffer the device plays with, in frames, or is reopening with once a new one is
    /// chosen: the one saved in settings when the device allows it, otherwise the one it has.
    pub fn buffer(&self, cx: &App) -> Option<u32> {
        let active = self.current()?;
        let wanted = self
            .settings
            .read(cx)
            .output_buffer()
            .filter(|frames| active.buffers.contains(frames));
        Some(wanted.unwrap_or(active.buffer))
    }

    /// Asks the device for a buffer of `frames` from now on, and saves it. The engines reopen
    /// with it, paused or not.
    pub fn set_buffer(&mut self, frames: u32, cx: &mut Context<Self>) {
        self.settings.update(cx, |settings, cx| {
            settings.set_output_buffer(Some(frames), cx)
        });
        output::choose_buffer(Some(frames));
        cx.notify();
    }

    /// Whether a device was just picked and has not answered yet, so no other can be picked.
    pub fn switching(&self) -> bool {
        self.holding.is_some()
    }

    /// Opens `id`, or the system's default when `None`, and once it opens moves playback onto
    /// it, paused or not, and saves it. A device that will not open leaves playback and the saved
    /// device as they were, with a toast saying so.
    pub fn choose(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if self.switching() {
            return;
        }
        // the same device again, with nothing wrong, would only cut the sound for a moment
        if id == self.selected && self.failure.is_none() {
            return;
        }
        self.trying = Some(id.clone());
        let io = Io::global(cx);
        let wanted = id.clone();
        self.opening = Some(cx.spawn(async move |this, cx| {
            let opened = join(io.spawn_blocking(move || output::open(wanted.as_deref()))).await;
            this.update(cx, |this, cx| this.picked(id, opened, cx)).ok();
        }));
        self.holding = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SWITCH_TIMEOUT).await;
            this.update(cx, |this, cx| {
                this.holding = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Takes the answer to a pick: moves playback onto a device that opened and saves it, or
    /// toasts that it would not.
    fn picked(&mut self, id: Option<String>, opened: Result<Opened>, cx: &mut Context<Self>) {
        self.opening = None;
        self.holding = None;
        self.trying = None;
        let opened = match opened {
            Ok(opened) => opened,
            Err(error) => {
                log::warn!("playback: cannot select the output device: {error:#}");
                self.toast_rejected(id.as_deref(), &error, cx);
                cx.notify();
                return;
            }
        };
        self.selected = id.clone();
        output::choose_device(id.clone());
        self.save(id, cx);
        self.playback
            .update(cx, |playback, cx| playback.restart_outputs(cx));
        let release = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HANDOVER).await;
            this.update(cx, |this, _| this.handover = None).ok();
        });
        self.handover = Some((opened, release));
        cx.notify();
    }

    /// Forgets the saved device once it is there but would not open, so the next run does not
    /// wait on it again. The system's default already plays in its place. A device that is gone
    /// is never tried, so it never fails and stays saved.
    fn forget_broken(&mut self, cx: &mut Context<Self>) {
        let Some(failure) = self.failure.as_ref().filter(|failure| failure.saved) else {
            return;
        };
        if self.settings.read(cx).output_device() != Some(failure.id.as_str()) {
            return;
        }
        log::warn!(
            "playback: forgetting the saved output device, which would not open: {}",
            failure.error
        );
        self.selected = None;
        self.save(None, cx);
        output::choose_device(None);
    }

    /// Says in a toast that the pick `id`, or the system's default when `None`, would not open,
    /// with the error below.
    fn toast_rejected(&self, id: Option<&str>, error: &anyhow::Error, cx: &mut Context<Self>) {
        let error = format!("{error:#}");
        let Some(id) = id else {
            Toasts::with_detail(
                Outcome::Failed,
                "toast-output-default-failed",
                None,
                error,
                cx,
            );
            return;
        };
        let name = self
            .devices
            .iter()
            .find(|device| device.id == id)
            .map_or(id, |device| device.name.as_str())
            .to_owned();
        let name = Some(name.into());
        Toasts::with_detail(
            Outcome::Failed,
            "toast-output-select-failed",
            name,
            error,
            cx,
        );
    }

    fn save(&self, id: Option<String>, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.set_output_device(id, cx));
    }

    /// Settles on the output played on now, notifying when it changed. While nothing names an
    /// output the last one stays.
    fn follow_output(&mut self, cx: &mut Context<Self>) {
        let Some(output) = self.binding_key() else {
            return;
        };
        if self.settled.as_ref() != Some(&output) {
            self.settled = Some(output);
            cx.notify();
        }
    }

    /// Settles on the output once the stream has opened or closed. Where the sound server
    /// answers it waits for the server's report instead, since a new stream takes a moment to
    /// show up there and the device alone would name the output wrongly in between.
    fn follow_device(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "linux")]
        if self.server {
            return;
        }
        self.follow_output(cx);
    }

    /// Takes in the sink the sound server plays Sonora on, or `None` while it has no stream of
    /// Sonora's. Either is read against the device open now, which the device watch may not have
    /// caught up with, and a sink seen with another device than that is left out.
    #[cfg(target_os = "linux")]
    fn arrive(&mut self, sink: Option<Sink>, cx: &mut Context<Self>) {
        self.server = true;
        self.read_device();
        if let Some(sink) = sink.filter(|sink| self.plays(sink))
            && self.sink.as_ref() != Some(&sink)
        {
            log::info!("outputs: playing on {} ({})", sink.name, sink.id);
            self.sink = Some(sink);
        }
        cx.notify();
        self.follow_output(cx);
    }

    /// Lets the device watch name the output while the sound server does not answer.
    #[cfg(target_os = "linux")]
    fn lose(&mut self, cx: &mut Context<Self>) {
        self.server = false;
        self.follow_output(cx);
    }

    /// Reads the system's devices again, as when a settings page comes into view. A saved device
    /// that can never be found again, such as an ALSA device saved before a sound server played,
    /// is forgotten here, and the system's default plays in its place.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected.clone();
        let listed = cx.background_executor().spawn(async move {
            let retired = selected.filter(|id| output::retired(id));
            (output::devices(), retired)
        });
        self.listing = Some(cx.spawn(async move |this, cx| {
            let (devices, retired) = listed.await;
            this.update(cx, |this, cx| {
                this.listing = None;
                this.listed = true;
                if let Some(id) = retired.filter(|id| this.selected.as_ref() == Some(id)) {
                    log::info!(
                        "playback: the saved output device {id} is not offered here anymore, \
                         playing on the system default"
                    );
                    this.selected = None;
                    this.save(None, cx);
                    output::choose_device(None);
                    cx.notify();
                }
                if this.devices != devices {
                    this.devices = devices;
                    cx.notify();
                }
            })
            .ok();
        }));
    }
}

/// Follows the sink the sound server plays Sonora on, on tokio, reconnecting with a growing pause
/// whenever the server is not there or hangs up.
#[cfg(target_os = "linux")]
fn follow(cx: &mut Context<Outputs>) -> Task<()> {
    let io = Io::global(cx);
    cx.spawn(async move |this, cx| {
        let mut pause = RETRY_FIRST;
        loop {
            let (sender, mut sinks) = tokio::sync::mpsc::unbounded_channel();
            let server = io.spawn(output::follow(move |sink| {
                sender.send(sink).ok();
            }));
            while let Some(sink) = sinks.recv().await {
                pause = RETRY_FIRST;
                if this.update(cx, |this, cx| this.arrive(sink, cx)).is_err() {
                    return;
                }
            }
            if let Ok(Err(error)) = server.await {
                log::debug!("outputs: the sound server is not answering: {error:#}");
            }
            if this.update(cx, |this, cx| this.lose(cx)).is_err() {
                return;
            }
            cx.background_executor().timer(pause).await;
            pause = (pause * 2).min(RETRY_MOST);
        }
    })
}
