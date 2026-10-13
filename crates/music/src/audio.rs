use std::num::NonZero;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use anyhow::{Context as _, Result};
use ardftsrc::RodioResampler;
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::buffer::SamplesBuffer;
use rodio::source::SeekError;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};

use crate::equalizer::{Equalized, Equalizer};
use crate::native;
use crate::spectrum::{Spectrum, Tap};

pub const RAMP: Duration = Duration::from_millis(25);
const BUFFER: Duration = Duration::from_millis(50);

/// The device stream every output in the process mixes into, so the sound server sees Sonora as
/// one client however many engines are running. It runs at the rate of whatever played last and
/// closes when the last output on it drops.
static SHARED: Mutex<Weak<Device>> = Mutex::new(Weak::new());

#[derive(Clone)]
pub struct Volume(Arc<AtomicU32>);

impl Volume {
    pub fn new(gain: f32) -> Self {
        Self(Arc::new(AtomicU32::new(gain.to_bits())))
    }

    pub fn set(&self, gain: f32) {
        self.0.store(gain.to_bits(), Ordering::Relaxed);
    }

    fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// What sits between the queue and the device: the equalizer, then the volume ramp, with the
/// spectrum tap listening at the end. Every engine builds one and hands it to the output.
pub struct Chain {
    pub volume: Volume,
    pub equalizer: Equalizer,
    pub spectrum: Spectrum,
}

/// One open stream on an output device, with the mixer the engines' chains play into.
struct Device {
    id: String,
    rate: u32,
    /// Rates the device turned down that this stream plays in place of, resampled, so a track
    /// at one of them does not reopen it.
    standing_in: Mutex<Vec<u32>>,
    failed: Arc<AtomicBool>,
    /// Taken when an output reopens the device at another rate, which closes the stream under
    /// every output still on it.
    stream: Mutex<Option<MixerDeviceSink>>,
}

impl Device {
    /// The shared stream on the default device, for `rate` when one is given. A rate the device
    /// plays as it is gets a stream at that rate, switching the hardware where that is up to
    /// Sonora. Any other rate plays on whatever the device runs at, resampled. A new stream
    /// replaces the old one on the same device only when the rate it runs at has to change. A
    /// stream left on a device that is no longer the default stays with the outputs still using
    /// it until they reopen.
    fn shared(rate: Option<u32>) -> Result<Arc<Self>> {
        let device = cpal::default_host()
            .default_output_device()
            .context("no audio output device")?;
        let id = ident(&device);

        let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
        let current = shared.upgrade().filter(|open| open.id == id && open.live());
        if let Some(open) = &current
            && rate.is_none_or(|rate| open.fits(rate))
        {
            return Ok(open.clone());
        }

        let default = device
            .default_output_config()
            .map_err(|error| anyhow::anyhow!("cannot read the output config: {error}"))?;
        let native = rate.filter(|&rate| native::offers(&device, &default, rate));
        if let Some(open) = &current
            && let Some(rate) = rate
            && native.is_none()
            && open.rate == default.sample_rate()
        {
            log::info!("sink: {id} does not play {rate} Hz as is, resampling instead");
            open.stand_in(rate);
            return Ok(open.clone());
        }
        if let Some(open) = shared.upgrade().filter(|open| open.id == id) {
            open.close();
        }

        let open = Arc::new(Self::open(device, id, &default, native, rate)?);
        *shared = Arc::downgrade(&open);
        Ok(open)
    }

    /// Opens the device at `native` when it plays that rate as it is, switching the hardware
    /// first, and at `default` otherwise. A native stream that will not open, as when another
    /// app holds the device, is retried at whatever rate the device runs at by then.
    fn open(
        device: cpal::Device,
        id: String,
        default: &cpal::SupportedStreamConfig,
        native: Option<u32>,
        wanted: Option<u32>,
    ) -> Result<Self> {
        let config = match native {
            Some(rate) => match native::switch(&device, rate) {
                Ok(()) => native::at(default, rate),
                Err(error) => {
                    log::warn!(
                        "sink: cannot switch {id} to {rate} Hz, resampling instead: {error:#}"
                    );
                    default.clone()
                }
            },
            None => {
                if let Some(rate) = wanted {
                    log::info!("sink: {id} does not play {rate} Hz as is, resampling instead");
                }
                default.clone()
            }
        };

        let failed = Arc::new(AtomicBool::new(false));
        let (stream, config) = match open_stream(device.clone(), &id, &config, &failed) {
            Ok(stream) => (stream, config),
            Err(error) if native.is_some() => {
                // the hardware may have switched before the open failed, so this asks again
                // rather than reusing `default`, which would leave the conversion hidden
                let current = device
                    .default_output_config()
                    .map_err(|error| anyhow::anyhow!("cannot read the output config: {error}"))?;
                log::warn!(
                    "sink: {error:#}, falling back to {} Hz",
                    current.sample_rate()
                );
                (open_stream(device, &id, &current, &failed)?, current)
            }
            Err(error) => return Err(error),
        };

        let rate = config.sample_rate();
        Ok(Self {
            id,
            rate,
            standing_in: Mutex::new(
                wanted
                    .filter(|&wanted| wanted != rate)
                    .into_iter()
                    .collect(),
            ),
            failed,
            stream: Mutex::new(Some(stream)),
        })
    }

    /// Whether a track at `rate` can play on this stream without a reopen: the stream runs at
    /// it, or it is a rate the device turned down and this stream plays in its place.
    fn fits(&self, rate: u32) -> bool {
        rate == self.rate || self.standing_in().contains(&rate)
    }

    /// Records that this stream plays `rate` in place of the device, resampled.
    fn stand_in(&self, rate: u32) {
        let mut standing_in = self.standing_in();
        if !standing_in.contains(&rate) {
            standing_in.push(rate);
        }
    }

    fn standing_in(&self) -> MutexGuard<'_, Vec<u32>> {
        self.standing_in
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether the stream is still open and has reported no error.
    fn live(&self) -> bool {
        !self.failed.load(Ordering::Acquire) && self.stream().is_some()
    }

    fn close(&self) {
        self.stream().take();
    }

    /// Puts one engine's chain into the mixer. A stream that has closed drops it.
    fn add(&self, source: impl Source + Send + 'static) {
        if let Some(stream) = self.stream().as_ref() {
            stream.mixer().add(source);
        }
    }

    fn stream(&self) -> MutexGuard<'_, Option<MixerDeviceSink>> {
        self.stream.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One engine's player on the shared device stream. Dropping it ends the player's source in the
/// mixer, and the stream closes once no output is left on it.
pub struct Output {
    chain: Chain,
    sink: Arc<rodio::Player>,
    device: Arc<Device>,
    /// Set when `fit` could not reopen the device, so the engine reports the output gone.
    broken: bool,
    /// Set for an output whose packets keep a rate of their own rather than one the engine
    /// resampled them to, which is Spotify's. `fit` resamples those in front of the mixer when
    /// the device runs at another rate.
    converting: Option<Converting>,
}

/// What an output that resamples in front of the mixer needs: the channels its packets come
/// in, and the cue generation whose moving on resets the resampler.
struct Converting {
    generation: Arc<AtomicU32>,
    channels: u16,
}

impl Output {
    /// Joins the stream on the default output device at whatever rate it runs and runs every
    /// sample through the equalizer and then the volume ramp before it reaches the mixer.
    pub fn open(chain: Chain) -> Result<Self> {
        let device = Device::shared(None)?;
        let sink = attach(&chain, &device, None);
        Ok(Self {
            chain,
            sink: Arc::new(sink),
            device,
            broken: false,
            converting: None,
        })
    }

    /// Has `fit` resample this output's packets, which come in `channels`, in front of the
    /// mixer whenever the device runs at another rate than they come at, resetting whenever
    /// `generation` moves on. Called once, before the first `fit`.
    pub fn converting(mut self, generation: Arc<AtomicU32>, channels: u16) -> Self {
        self.converting = Some(Converting {
            generation,
            channels,
        });
        self
    }

    pub fn sink(&self) -> &Arc<rodio::Player> {
        &self.sink
    }

    /// The rate the device runs at, which differs from what was asked for when the device turned
    /// that rate down.
    pub fn rate(&self) -> u32 {
        self.device.rate
    }

    /// Whether a track at `rate` can go out as the output stands, without `fit` reopening it.
    pub fn fits(&self, rate: u32) -> bool {
        self.device.live() && self.device.fits(rate)
    }

    /// Moves this output onto a shared stream at `rate` unless it `fits` already, keeping the
    /// player paused if it was. The stream reopens when it runs at another rate, which closes
    /// it under the other engines. Whatever this output had queued is dropped and any clone of
    /// `sink` goes stale, so callers do this before a track and take `sink` again when it
    /// returns true.
    pub fn fit(&mut self, rate: u32) -> Result<bool> {
        if self.fits(rate) {
            return Ok(false);
        }
        let paused = self.sink.is_paused();
        let device = match Device::shared(Some(rate)) {
            Ok(device) => device,
            Err(error) => {
                self.broken = true;
                return Err(error);
            }
        };
        let converting = self.converting.as_ref().filter(|_| device.rate != rate);
        let sink = attach(
            &self.chain,
            &device,
            converting.map(|converting| (converting, rate)),
        );
        if paused {
            sink.pause();
        }
        self.sink = Arc::new(sink);
        self.device = device;
        self.broken = false;
        Ok(true)
    }

    pub fn set_volume(&self, gain: f32) {
        self.chain.volume.set(gain);
    }

    /// Whether what this output holds can no longer be heard: the stream reported an error,
    /// another engine reopened it at another rate, or `fit` failed. Every output on the stream
    /// sees an error. A fresh track recovers through `fit`, a paused one has to be reloaded.
    pub fn failed(&self) -> bool {
        self.broken || !self.device.live()
    }

    /// Whether the system's default device is no longer the one this output plays on.
    pub fn changed(&self) -> bool {
        cpal::default_host()
            .default_output_device()
            .map(|device| ident(&device))
            .is_some_and(|device| device != "unknown" && device != self.device.id)
    }
}

pub struct SmoothGain<I> {
    input: I,
    volume: Volume,
    tap: Option<Tap>,
    /// Whether the tap hears the samples from before the gain. It is read once per frame so
    /// every channel of a frame agrees.
    unscaled: bool,

    current: f32,
    target: f32,
    step: f32,

    ramp: Duration,
    frames_left: u32,
    ramp_frames: u32,

    channel: u16,
    channels: u16,
    rate: u32,
}

impl<I: Source> SmoothGain<I> {
    pub fn new(input: I, volume: Volume, initial: f32, ramp: Duration) -> Self {
        Self {
            input,
            volume,
            tap: None,
            unscaled: false,
            current: initial,
            target: initial,
            step: 0.0,
            ramp,
            frames_left: 0,
            ramp_frames: 1,
            channel: 0,
            channels: 0,
            rate: 0,
        }
    }

    pub fn with_tap(mut self, tap: Tap) -> Self {
        self.tap = Some(tap);
        self
    }

    fn resync(&mut self) {
        let channels = self.input.channels().get();
        let rate = self.input.sample_rate().get();
        if channels == self.channels && rate == self.rate {
            return;
        }

        self.channels = channels;
        self.rate = rate;
        if let Some(tap) = &self.tap {
            tap.format(rate, channels);
        }
        self.ramp_frames = (self.ramp.as_secs_f64() * rate as f64).round().max(1.0) as u32;
        self.frames_left = self.frames_left.min(self.ramp_frames);
    }
}

impl<I: Source> Iterator for SmoothGain<I> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next()?;

        if self.channel == 0 {
            self.resync();
            self.unscaled = self.tap.as_ref().is_some_and(Tap::absolute);
            let requested = self.volume.get().max(0.0);

            if requested.to_bits() != self.target.to_bits() {
                self.target = requested;
                self.frames_left = self.ramp_frames;
                self.step = (self.target - self.current) / self.ramp_frames as f32;
            }

            if self.frames_left > 0 {
                self.current += self.step;
                self.frames_left -= 1;

                if self.frames_left == 0 {
                    self.current = self.target;
                }
            }
        }

        let output = sample * self.current;
        if let Some(tap) = self.tap.as_mut() {
            tap.push(match self.unscaled {
                true => sample,
                false => output,
            });
        }

        self.channel += 1;
        if self.channel >= self.channels {
            self.channel = 0;
        }

        Some(output)
    }
}

impl<I: Source> Source for SmoothGain<I> {
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    fn channels(&self) -> NonZero<u16> {
        self.input.channels()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.input.try_seek(position)
    }
}

pub struct Trimmed<I> {
    input: I,
    head: u64,
    body: Option<u64>,
    emitted: u64,
    primed: bool,
    lane: u64,
}

impl<I: Source> Trimmed<I> {
    pub fn new(input: I, skip: Duration, take: Option<Duration>) -> Self {
        let lane = (input.sample_rate().get() as u64) * (input.channels().get() as u64);
        let samples = |span: Duration| (span.as_secs_f64() * lane as f64).round() as u64;

        Self {
            head: samples(skip),
            body: take.map(samples),
            emitted: 0,
            primed: false,
            lane,
            input,
        }
    }

    fn offset(&self) -> Duration {
        Duration::from_secs_f64(self.head as f64 / self.lane as f64)
    }
}

impl<I: Source> Iterator for Trimmed<I> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.primed {
            self.primed = true;
            for _ in 0..self.head {
                self.input.next()?;
            }
        }
        if self.body.is_some_and(|body| self.emitted >= body) {
            return None;
        }

        let sample = self.input.next()?;
        self.emitted += 1;
        Some(sample)
    }
}

impl<I: Source> Source for Trimmed<I> {
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    fn channels(&self) -> NonZero<u16> {
        self.input.channels()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        match self.body {
            Some(body) => Some(Duration::from_secs_f64(body as f64 / self.lane as f64)),
            None => self
                .input
                .total_duration()
                .map(|whole| whole.saturating_sub(self.offset())),
        }
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.input.try_seek(position + self.offset())?;
        self.primed = true;
        self.emitted = (position.as_secs_f64() * self.lane as f64).round() as u64;
        Ok(())
    }
}

/// A track's decoder as the device hears it: untouched when the device runs at the track's own
/// rate, converted by ardftsrc's high preset when it runs at another. The engine converts as it
/// decodes, ahead of the queue, so a pause, a clear or a position read acts on audio already at
/// the device's rate rather than on what the resampler still holds.
pub enum Resampled<S: Source> {
    Native(S),
    Converted(Box<RodioResampler<S>>),
}

impl<S: Source> Resampled<S> {
    /// Converts to `rate` unless the decoder plays at that rate already or is converted already.
    /// Fast start primes the resampler from the decoder at once, so the first samples out are
    /// the track rather than silence. `Err` means the resampler failed after taking the decoder.
    pub fn to(self, rate: u32) -> Result<Self> {
        let Self::Native(source) = self else {
            return Ok(self);
        };
        let native = source.sample_rate().get();
        if native == rate {
            return Ok(Self::Native(source));
        }

        let config = ardftsrc::PRESET_HIGH
            .with_input_rate(native as usize)
            .with_output_rate(rate as usize)
            .with_channels(usize::from(source.channels().get()))
            .with_rodio_fast_start(true);
        log::info!("sink: resampling {native} Hz to {rate} Hz");
        RodioResampler::new(source, config)
            .map(|resampler| Self::Converted(Box::new(resampler)))
            .context("cannot start the resampler")
    }
}

impl<S: Source> Iterator for Resampled<S> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Native(source) => source.next(),
            Self::Converted(source) => source.next(),
        }
    }
}

impl<S: Source> Source for Resampled<S> {
    fn current_span_len(&self) -> Option<usize> {
        match self {
            Self::Native(source) => source.current_span_len(),
            Self::Converted(source) => source.current_span_len(),
        }
    }

    fn channels(&self) -> NonZero<u16> {
        match self {
            Self::Native(source) => source.channels(),
            Self::Converted(source) => source.channels(),
        }
    }

    fn sample_rate(&self) -> NonZero<u32> {
        match self {
            Self::Native(source) => source.sample_rate(),
            Self::Converted(source) => source.sample_rate(),
        }
    }

    fn total_duration(&self) -> Option<Duration> {
        match self {
            Self::Native(source) => source.total_duration(),
            Self::Converted(source) => source.total_duration(),
        }
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        match self {
            Self::Native(source) => source.try_seek(position),
            Self::Converted(source) => source.try_seek(position),
        }
    }
}

/// A player's packets resampled to the device's rate on their way into the mixer, for an output
/// whose packets keep a rate of their own. These cannot be read ahead, so it uses ardftsrc's
/// good preset, which holds about 100 ms rather than high's 300: a pause lands that much late.
/// A clear on the cue resets it, so nothing from before a seek or a skip is heard after it.
struct Converted<S: Source> {
    resampler: RodioResampler<S>,
    generation: Arc<AtomicU32>,
    seen: u32,
}

impl<S: Source> Converted<S> {
    /// Takes the player's current format as the resampler's input format. A packet in another
    /// one primes it again, which is why `attach` seeds the queue in the packets' format first.
    fn new(source: S, rate: u32, generation: Arc<AtomicU32>) -> Result<Self> {
        let config = ardftsrc::PRESET_GOOD
            .with_input_rate(source.sample_rate().get() as usize)
            .with_output_rate(rate as usize)
            .with_channels(usize::from(source.channels().get()))
            .with_rodio_fast_start(true);
        let seen = generation.load(Ordering::Relaxed);
        let resampler =
            RodioResampler::new(source, config).context("cannot start the resampler")?;
        Ok(Self {
            resampler,
            generation,
            seen,
        })
    }
}

impl<S: Source> Iterator for Converted<S> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let generation = self.generation.load(Ordering::Relaxed);
        if generation != self.seen {
            self.seen = generation;
            self.resampler.reset();
        }
        self.resampler.next()
    }
}

impl<S: Source> Source for Converted<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.resampler.current_span_len()
    }

    fn channels(&self) -> NonZero<u16> {
        self.resampler.channels()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.resampler.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.resampler.total_duration()
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.resampler.try_seek(position)
    }
}

/// Whether the system has a default output device to play on. A device can be missing for as
/// long as it takes a headset to reconnect, so the engines ask again rather than give up.
pub fn available() -> bool {
    cpal::default_host().default_output_device().is_some()
}

/// Opens the stream on `device` in `config`, flagging `failed` when the device reports an error.
fn open_stream(
    device: cpal::Device,
    id: &str,
    config: &cpal::SupportedStreamConfig,
    failed: &Arc<AtomicBool>,
) -> Result<MixerDeviceSink> {
    log::info!(
        "sink: using {} at {} Hz, {} channels, {}",
        id,
        config.sample_rate(),
        config.channels(),
        config.sample_format()
    );

    let frames = (BUFFER.as_secs_f64() * config.sample_rate() as f64).round() as u32;
    let stream_failed = failed.clone();
    let builder = DeviceSinkBuilder::default()
        .with_device(device)
        .with_config(&config.config())
        .with_buffer_size(cpal::BufferSize::Fixed(frames))
        .with_sample_format(config.sample_format())
        .with_error_callback(move |error| match error {
            cpal::StreamError::BufferUnderrun => log::debug!("sink: buffer underrun"),
            error => {
                log::warn!("sink: audio output failed: {error}");
                stream_failed.store(true, Ordering::Release);
            }
        });
    let mut stream = builder
        .open_stream()
        .map_err(|error| anyhow::anyhow!("cannot open the audio output: {error}"))?;
    stream.log_on_drop(false);
    Ok(stream)
}

fn ident(device: &cpal::Device) -> String {
    device
        .id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| "unknown".to_owned())
}

/// A fresh player whose chain plays into `device`, resampled to the device's rate first when
/// `converting` says how packets at the given rate come. A resampler that cannot start leaves
/// the conversion to the mixer.
fn attach(chain: &Chain, device: &Device, converting: Option<(&Converting, u32)>) -> rodio::Player {
    let (sink, source) = rodio::Player::new();
    let Some((converting, rate)) = converting else {
        play(chain, device, source);
        return sink;
    };
    // An empty queue reports a format of its own until the first packet, and a resampler that
    // meets a new format primes again by reading ahead. Reading ahead while the first packets
    // are still arriving would put silence after the first one. One silent frame in the
    // packets' format has it prime on silence now, before any audio, and never again.
    let channels = usize::from(converting.channels);
    if let (Some(count), Some(rate)) = (NonZero::new(converting.channels), NonZero::new(rate)) {
        sink.append(SamplesBuffer::new(count, rate, vec![0.0; channels]));
    }
    match Converted::new(source, device.rate, converting.generation.clone()) {
        Ok(converted) => play(chain, device, converted),
        Err(error) => {
            log::error!("sink: {error:#}, leaving the conversion to the mixer");
            return attach(chain, device, None);
        }
    }
    log::info!("sink: resampling into the mixer at {} Hz", device.rate);
    sink
}

/// Runs `source` through the equalizer and then the volume ramp, with the spectrum tap at the
/// end, into `device`'s mixer.
fn play(chain: &Chain, device: &Device, source: impl Source + Send + 'static) {
    let applied = chain.volume.get();
    let tap = chain.spectrum.attach();
    let equalized = Equalized::new(source, chain.equalizer.clone());
    device.add(SmoothGain::new(equalized, chain.volume.clone(), applied, RAMP).with_tap(tap));
}
