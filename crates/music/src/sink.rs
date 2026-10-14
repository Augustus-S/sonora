//! The output queue every engine feeds. A decoder runs on its own thread and hands finished
//! samples to `Paced`, which appends them to the shared rodio output and blocks the writer once
//! enough is queued. Nothing here decodes or reaches the network, so the audio thread never
//! waits on either.

use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use ardftsrc::RealtimeResampler;
use rodio::buffer::SamplesBuffer;
use rodio::{ChannelCount, SampleRate, Source};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::audio::Chain;
use crate::output::{self, Output};

/// How many packets stay queued before a write blocks. This is what paces the decoder to real
/// time; deeper rides out network hiccups, but the decoder's position runs this far ahead of
/// what is heard.
const QUEUED_CHUNKS: usize = 26;
/// About how many frames a resampled packet comes out as, before conversion, so a converted queue
/// paces like one of the packets that went in.
const PIECE_FRAMES: u64 = 1024;
/// How long a blocked write sleeps between looks at the queue.
const DRAIN_POLL: Duration = Duration::from_millis(10);
/// How often a watch looks for an output device to come back once none could be opened.
const WATCH_POLL: Duration = Duration::from_secs(1);
// TEMPORARY, remove before merging: one packet in this many logs how long it waited in the queue.
const QUEUE_SAMPLE: u64 = 50;
static QUEUE_SAMPLED: AtomicU64 = AtomicU64::new(0);

/// What the cue does with the next write. It rides in an `AtomicU8`, so it converts at that
/// boundary and stays a type everywhere else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Admit {
    /// Writes pass through and nothing is reported.
    Open,
    /// A clear was asked for and the engine has not announced the new track or position yet,
    /// so every write is still the old one and is dropped.
    Cleared,
    /// The engine announced the new track or position; the next write is its first audio.
    Armed,
}

impl Admit {
    fn of(value: u8) -> Self {
        match value {
            1 => Self::Cleared,
            2 => Self::Armed,
            _ => Self::Open,
        }
    }
}

/// The output device went away, another one is wanted, or its rate changed. The engine reopens
/// rather than carrying on into silence or into a conversion.
#[derive(Debug)]
pub struct Gone;

impl std::fmt::Display for Gone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "audio output changed")
    }
}

impl std::error::Error for Gone {}

/// The queue's link to the event stream. `clear` retires every packet queued so far and refuses
/// the ones a decoder still writes from the old position, so the old audio stops when the user
/// acts. `arm` lets writes through again and has the first one reported, which is how a provider
/// marks audio from the new track or position rather than a decoder merely being ready.
#[derive(Clone)]
pub struct Cue {
    state: Arc<AtomicU8>,
    generation: Arc<AtomicU32>,
    /// Samples that have reached the output since the last clear.
    played: Arc<AtomicU64>,
    written: UnboundedSender<()>,
}

impl Cue {
    pub fn new() -> (Self, UnboundedReceiver<()>) {
        let (written, receiver) = unbounded_channel();
        let cue = Self {
            state: Arc::default(),
            generation: Arc::default(),
            played: Arc::default(),
            written,
        };
        (cue, receiver)
    }

    /// Moves the queue on a generation. Every chunk from before ends at its next sample, and
    /// nothing here touches rodio's own bookkeeping, which does not survive a clear that races
    /// a chunk ending on its own.
    pub fn clear(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.state.store(Admit::Cleared as u8, Ordering::Relaxed);
        self.played.store(0, Ordering::Relaxed);
    }

    /// Lets writes through again and has the next one reported.
    pub fn arm(&self) {
        self.state.store(Admit::Armed as u8, Ordering::Relaxed);
    }

    /// Whether a clear is still waiting for the engine to announce where it went.
    pub fn cleared(&self) -> bool {
        self.state() == Admit::Cleared
    }

    fn state(&self) -> Admit {
        Admit::of(self.state.load(Ordering::Relaxed))
    }

    /// Samples heard since the last clear, across every channel.
    pub fn played(&self) -> u64 {
        self.played.load(Ordering::Relaxed)
    }

    /// Decides a write: refused after a clear, reported when armed, plain otherwise. A clear
    /// that lands during the armed write wins, since that packet predates the seek behind it.
    pub(crate) fn admit(&self) -> bool {
        match self.state() {
            Admit::Cleared => false,
            Admit::Armed => {
                let armed = self
                    .state
                    .compare_exchange(
                        Admit::Armed as u8,
                        Admit::Open as u8,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok();
                if armed {
                    self.written.send(()).ok();
                }
                armed
            }
            Admit::Open => true,
        }
    }
}

/// One packet in the output queue. It ends as soon as the cue moves on a generation and counts
/// itself out when dropped, whichever way it went, so `live` is the number still queued.
struct Chunk {
    samples: SamplesBuffer,
    born: u32,
    generation: Arc<AtomicU32>,
    played: Arc<AtomicU64>,
    live: Arc<AtomicUsize>,
    // TEMPORARY, remove before merging: when the packet was queued, until its first pull.
    queued: Option<Instant>,
}

impl Chunk {
    fn new(samples: SamplesBuffer, cue: &Cue, live: &Arc<AtomicUsize>) -> Self {
        live.fetch_add(1, Ordering::Relaxed);
        Self {
            samples,
            born: cue.generation.load(Ordering::Relaxed),
            generation: cue.generation.clone(),
            played: cue.played.clone(),
            live: live.clone(),
            queued: Some(Instant::now()),
        }
    }
}

impl Iterator for Chunk {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.generation.load(Ordering::Relaxed) != self.born {
            return None;
        }
        let sample = self.samples.next()?;
        self.played.fetch_add(1, Ordering::Relaxed);
        // TEMPORARY, remove before merging
        if let Some(queued) = self.queued.take()
            && QUEUE_SAMPLED
                .fetch_add(1, Ordering::Relaxed)
                .is_multiple_of(QUEUE_SAMPLE)
        {
            let length = self.samples.total_duration().unwrap_or_default();
            log::debug!(
                "sink: packet waited {:.1} ms in the queue ({} queued, {:.1} ms each)",
                queued.elapsed().as_secs_f64() * 1000.,
                self.live.load(Ordering::Relaxed),
                length.as_secs_f64() * 1000.,
            );
        }
        Some(sample)
    }
}

impl Source for Chunk {
    fn current_span_len(&self) -> Option<usize> {
        self.samples.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.samples.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.samples.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.samples.total_duration()
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::Relaxed);
    }
}

/// The shared rodio output with a paced queue in front of it. A write blocks once enough is
/// queued, which keeps a decoder in step with playback. A write on an output that has to reopen
/// reports it gone; the output also says so on `changed` itself, playing or not.
pub struct Paced {
    output: Output,
    cue: Cue,
    /// Chunks appended and not yet played out or retired.
    live: Arc<AtomicUsize>,
    changed: UnboundedSender<()>,
    /// Resamples packets as they are written, for a writer whose packets keep a rate of their
    /// own while the device runs at another.
    converter: Option<Converter>,
}

impl Paced {
    /// Claims the output device, paused until the caller starts it, for packets the caller
    /// resamples to its `rate`.
    pub fn open(cue: Cue, chain: Chain, changed: UnboundedSender<()>) -> Result<Self> {
        let output = Output::open(chain, changed.clone())?;
        Ok(Self::paused(output, cue, changed))
    }

    /// Claims the output device for packets that keep `rate` and `channels` of their own, which
    /// are resampled as they are written when the device runs at another rate. A resampler that
    /// cannot start leaves the conversion to the mixer.
    pub fn converting(
        cue: Cue,
        chain: Chain,
        changed: UnboundedSender<()>,
        rate: u32,
        channels: u16,
    ) -> Result<Self> {
        let output = Output::open(chain, changed.clone())?;
        let mut paced = Self::paused(output, cue, changed);
        let device = paced.rate();
        if rate != device {
            let generation = paced.cue.generation.load(Ordering::Relaxed);
            match Converter::new(rate, device, channels, generation) {
                Ok(converter) => {
                    log::info!("sink: resampling {rate} Hz to {device} Hz as it is written");
                    paced.converter = Some(converter);
                }
                Err(error) => log::error!("sink: {error:#}, leaving the conversion to the mixer"),
            }
        }
        Ok(paced)
    }

    fn paused(output: Output, cue: Cue, changed: UnboundedSender<()>) -> Self {
        output.sink().pause();
        Self {
            output,
            cue,
            live: Arc::default(),
            changed,
            converter: None,
        }
    }

    pub fn play(&mut self) -> Result<(), Gone> {
        if self.output.failed() {
            return Err(self.disconnected());
        }
        self.output.sink().play();
        Ok(())
    }

    pub fn pause(&self) {
        self.output.sink().pause();
    }

    pub fn set_volume(&self, gain: f32) {
        self.output.set_volume(gain);
    }

    /// Queues one packet. A packet the cue refuses is dropped, so a decoder that has been
    /// retired makes no sound while it winds down. This never blocks: a caller with nothing
    /// else to do waits through `drain`, one with commands to answer waits through `full`.
    pub fn write(&mut self, samples: SamplesBuffer) -> Result<(), Gone> {
        if self.output.failed() {
            return Err(self.disconnected());
        }
        if !self.cue.admit() {
            return Ok(());
        }
        let Some(converter) = &mut self.converter else {
            self.append(samples);
            return Ok(());
        };

        // what it holds from before a clear is the old position, which is not heard
        let generation = self.cue.generation.load(Ordering::Relaxed);
        if converter.generation != generation {
            converter.reset(generation);
        }
        for piece in converter.push(samples) {
            self.append(piece);
        }
        Ok(())
    }

    /// Queues whatever the resampler still holds, as when the writer stops, so the last of it is
    /// heard. The next write primes it afresh, so a resume is a splice rather than a seamless
    /// join. What it holds from before a clear is dropped instead.
    pub fn flush(&mut self) {
        let Some(converter) = &mut self.converter else {
            return;
        };
        let generation = self.cue.generation.load(Ordering::Relaxed);
        if self.cue.cleared() || converter.generation != generation {
            converter.reset(generation);
            return;
        }
        for piece in converter.flush() {
            self.append(piece);
        }
    }

    fn append(&self, samples: SamplesBuffer) {
        self.output
            .sink()
            .append(Chunk::new(samples, &self.cue, &self.live));
    }

    /// Whether the queue holds all it should, so the next packet can wait.
    pub fn full(&self) -> bool {
        self.live.load(Ordering::Relaxed) > QUEUED_CHUNKS
    }

    /// The rate the device runs at. A packet at any other rate is resampled before it is heard.
    pub fn rate(&self) -> u32 {
        self.output.rate()
    }

    /// Waits until there is room for another packet.
    pub fn drain(&mut self) -> Result<(), Gone> {
        while self.full() {
            if self.output.failed() {
                return Err(self.disconnected());
            }
            std::thread::sleep(DRAIN_POLL);
        }
        Ok(())
    }

    /// How long a full queue takes to play out. This is how far a decoder runs ahead of what
    /// is heard, so it is also the longest a paused writer waits before it sees a command.
    pub const fn poll() -> Duration {
        DRAIN_POLL
    }

    /// Tells the engine the output is gone.
    pub fn disconnected(&self) -> Gone {
        self.changed.send(()).ok();
        Gone
    }
}

/// Packets resampled to the device's rate as they are written, for a writer whose packets keep a
/// rate of their own, which is Spotify's. librespot decodes faster than real time, so the resampler
/// primes from real audio within a few packets, and everything queued is at the device's rate
/// already: a pause or a clear acts on it at once. It uses ardftsrc's good preset, which holds less
/// than high's, since what it holds is how far librespot's position runs ahead of the sound.
struct Converter {
    resampler: RealtimeResampler,
    channels: u16,
    rate: u32,
    /// Samples per queued piece.
    piece: usize,
    /// The cue generation the audio it holds belongs to.
    generation: u32,
    input: Vec<f64>,
    output: Vec<f64>,
}

impl Converter {
    fn new(from: u32, to: u32, channels: u16, generation: u32) -> Result<Self> {
        let config = ardftsrc::PRESET_GOOD
            .with_input_rate(from as usize)
            .with_output_rate(to as usize)
            .with_channels(usize::from(channels));
        let resampler = RealtimeResampler::new(config).context("cannot start the resampler")?;
        let frames = (PIECE_FRAMES * u64::from(to)).div_ceil(u64::from(from.max(1)));
        Ok(Self {
            resampler,
            channels,
            rate: to,
            piece: frames as usize * usize::from(channels),
            generation,
            input: Vec::new(),
            output: Vec::new(),
        })
    }

    /// Takes one packet in and hands back what is ready, in pieces about a packet long. The rest
    /// waits for more input or `flush`.
    fn push(&mut self, samples: SamplesBuffer) -> Vec<SamplesBuffer> {
        self.input.clear();
        self.input.extend(samples.map(f64::from));
        if let Err(error) = self.resampler.write_samples(&self.input) {
            log::error!("sink: cannot resample a packet: {error}");
            self.resampler.reset();
            return Vec::new();
        }
        self.read(false)
    }

    /// Ends the stream so far and hands back everything it held, then starts afresh.
    fn flush(&mut self) -> Vec<SamplesBuffer> {
        let pieces = match self.resampler.finalize() {
            Ok(()) => self.read(true),
            Err(error) => {
                log::error!("sink: cannot flush the resampler: {error}");
                Vec::new()
            }
        };
        self.resampler.reset();
        pieces
    }

    /// Drops what it holds, which belongs to an older `generation`.
    fn reset(&mut self, generation: u32) {
        self.resampler.reset();
        self.generation = generation;
    }

    /// Reads whole pieces, and with `all` the short one left at the end too.
    fn read(&mut self, all: bool) -> Vec<SamplesBuffer> {
        let mut pieces = Vec::new();
        loop {
            let ready = self.resampler.num_samples_ready();
            if ready == 0 || (ready < self.piece && !all) {
                return pieces;
            }
            self.output.resize(ready.min(self.piece), 0.);
            let read = match self.resampler.read_samples(&mut self.output) {
                Some(read) if read > 0 => read,
                _ => return pieces,
            };
            let samples: Vec<f32> = self.output[..read].iter().map(|&s| s as f32).collect();
            if let Some(piece) = packet(&samples, self.channels, self.rate) {
                pieces.push(piece);
            }
        }
    }
}

/// Waits for an output device to come back and then tells `notify`, which the engine takes the
/// way it takes the device changing: it reopens the output and reloads where it was.
/// This is what an engine that could not open a device at all waits on. A machine whose only
/// device has gone, a Bluetooth headset that dropped its link, has nothing to reopen on and
/// nothing else asks the system again, so without the watch playback stays silent until the app
/// restarts. The watch ends when the engine listening on `notify` is gone.
pub fn watch_for_output(notify: UnboundedSender<()>) {
    std::thread::spawn(move || {
        while !notify.is_closed() {
            std::thread::sleep(WATCH_POLL);
            if !output::output_available() {
                continue;
            }
            log::info!("sink: an output device is available again, reopening");
            notify.send(()).ok();
            return;
        }
    });
}

/// A packet of interleaved samples at one rate, ready for the queue.
pub fn packet(samples: &[f32], channels: u16, rate: u32) -> Option<SamplesBuffer> {
    Some(SamplesBuffer::new(
        NonZero::new(channels)?,
        NonZero::new(rate)?,
        samples,
    ))
}
