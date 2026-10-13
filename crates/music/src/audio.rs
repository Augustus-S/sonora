use std::num::NonZero;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::thread;
use std::time::Duration;

use anyhow::{Context as _, Result};
use ardftsrc::RodioResampler;
use cpal::traits::DeviceTrait;
use rodio::source::SeekError;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};
use tokio::sync::mpsc::UnboundedSender;

use crate::equalizer::{Equalized, Equalizer};
use crate::output::{self, output_rate};
use crate::spectrum::{Spectrum, Tap};

pub const RAMP: Duration = Duration::from_millis(25);
/// The buffer asked of the device, in frames, as far as the device allows it.
const BUFFER_FRAMES: u32 = 1024;
/// How often an open stream asks the system whether its device is still the one wanted and
/// still runs at the stream's rate.
const DEVICE_POLL: Duration = Duration::from_millis(500);

/// The device stream every output in the process mixes into, so the sound server sees Sonora as
/// one client however many engines are running. It runs at the rate the device runs at and
/// closes when the last output on it drops.
static SHARED: Mutex<Weak<Device>> = Mutex::new(Weak::new());

/// Held while a stream opens.
static OPENING: Mutex<()> = Mutex::new(());

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
pub(crate) struct Device {
    id: String,
    name: String,
    rate: u32,
    /// The buffer asked of the device, in frames.
    buffer: u32,
    failed: Arc<AtomicBool>,
    /// Set once the device is no longer the one wanted or runs at another rate, so no output
    /// joins the stream again.
    stale: AtomicBool,
    /// Where each output on the stream hears that it has to reopen.
    listeners: Mutex<Vec<UnboundedSender<()>>>,
    stream: MixerDeviceSink,
}

impl Device {
    /// The shared stream on the device `output::wanted` names, at the rate it runs at. A stream
    /// left on a device that is no longer wanted, or that runs at another rate by now, stays with
    /// the outputs still using it until they reopen, which `watch` tells them to. A device that
    /// would not open is remembered, and the one `wanted` names after it, if any, is tried once.
    fn shared() -> Result<Arc<Self>> {
        // Opens take turns, so two engines starting together share one stream, and only they
        // wait: `status` reads `SHARED`, which is never held across an open, since CoreAudio can
        // take ten seconds to give up on a device.
        let _opening = OPENING.lock().unwrap_or_else(PoisonError::into_inner);
        match Self::refusing(output::wanted()?) {
            Ok(open) => Ok(open),
            Err(error) => match output::wanted() {
                Ok(device) => Self::refusing(device),
                Err(_) => Err(error),
            },
        }
    }

    /// The shared stream on `device`, opened now whatever is wanted, as a device picked in
    /// settings is tried before the engines move onto it. A device that will not open is not
    /// remembered.
    pub(crate) fn open_now(device: cpal::Device) -> Result<Arc<Self>> {
        let _opening = OPENING.lock().unwrap_or_else(PoisonError::into_inner);
        Self::join(device)
    }

    /// `join`, remembering a device that would not open so `output::wanted` passes it over.
    fn refusing(device: cpal::Device) -> Result<Arc<Self>> {
        let id = output::ident(&device);
        Self::join(device).inspect_err(|error| output::refuse(&id, error))
    }

    /// The open stream on `device`, or a new one.
    fn join(device: cpal::Device) -> Result<Arc<Self>> {
        let id = output::ident(&device);
        let current = SHARED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .upgrade();
        if let Some(open) = current
            .filter(|open| open.id == id && open.live() && !output_rate::moved(&device, open.rate))
        {
            return Ok(open);
        }

        let open = Arc::new(Self::open(device, id.clone())?);
        *SHARED.lock().unwrap_or_else(PoisonError::into_inner) = Arc::downgrade(&open);
        watch(Arc::downgrade(&open));
        output::opened(&id);
        Ok(open)
    }

    /// Opens the device at the rate it runs at, so nothing between here and the hardware
    /// converts what Sonora plays.
    fn open(device: cpal::Device, id: String) -> Result<Self> {
        let default = device
            .default_output_config()
            .map_err(|error| anyhow::anyhow!("cannot read the output config: {error}"))?;
        let config = output_rate::config(&device, default);
        let name = output::name(&device);
        let failed = Arc::new(AtomicBool::new(false));
        let (stream, buffer) = open_stream(device, &id, &config, &failed)?;
        Ok(Self {
            id,
            name,
            rate: config.sample_rate(),
            buffer,
            failed,
            stale: AtomicBool::new(false),
            listeners: Mutex::new(Vec::new()),
            stream,
        })
    }

    /// Whether the stream has reported no error and its device is still the one wanted, at the
    /// rate the stream runs at.
    fn live(&self) -> bool {
        !self.failed.load(Ordering::Acquire) && !self.stale.load(Ordering::Acquire)
    }

    /// Whether the stream has to reopen: it failed, another device is wanted, no device is left,
    /// or the device runs at another rate by now.
    fn outdated(&self) -> bool {
        if self.failed.load(Ordering::Acquire) {
            return true;
        }
        let Ok(device) = output::wanted() else {
            return true;
        };
        let id = output::ident(&device);
        (id != "unknown" && id != self.id) || output_rate::moved(&device, self.rate)
    }

    /// Has `changed` hear when the stream has to reopen.
    fn listen(&self, changed: UnboundedSender<()>) {
        let mut listeners = self
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        listeners.retain(|listener| !listener.is_closed());
        listeners.push(changed);
    }

    /// Marks the stream stale and tells every output on it to reopen.
    fn retire(&self) {
        self.stale.store(true, Ordering::Release);
        let listeners = self
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for listener in listeners.iter() {
            listener.send(()).ok();
        }
        drop(listeners);
        output::announce();
    }

    /// Puts one engine's chain into the mixer.
    fn add(&self, source: impl Source + Send + 'static) {
        self.stream.mixer().add(source);
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        output::announce();
    }
}

/// One engine's player on the shared device stream. Dropping it ends the player's source in the
/// mixer, and the stream closes once no output is left on it.
pub struct Output {
    chain: Chain,
    sink: Arc<rodio::Player>,
    device: Arc<Device>,
}

impl Output {
    /// Joins the shared stream for packets resampled to its rate already, and runs every sample
    /// through the equalizer and then the volume ramp before it reaches the mixer. `changed`
    /// hears when the stream has to reopen, playing or not.
    pub fn open(chain: Chain, changed: UnboundedSender<()>) -> Result<Self> {
        let device = Device::shared()?;
        device.listen(changed);
        let sink = attach(&chain, &device);
        Ok(Self {
            chain,
            sink: Arc::new(sink),
            device,
        })
    }

    pub fn sink(&self) -> &Arc<rodio::Player> {
        &self.sink
    }

    /// The rate the device runs at, which every packet is resampled to before it is heard.
    pub fn rate(&self) -> u32 {
        self.device.rate
    }

    pub fn set_volume(&self, gain: f32) {
        self.chain.volume.set(gain);
    }

    /// Whether the stream reported an error or has to reopen, so what this output holds can no
    /// longer be heard as it should. Every output on the stream sees it.
    pub fn failed(&self) -> bool {
        !self.device.live()
    }
}

/// The stream every engine plays into, if one is open.
pub(crate) fn status() -> Option<output::Status> {
    let device = SHARED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .upgrade()?;
    Some(describe(&device))
}

/// What a stream runs at, as a settings page shows it.
pub(crate) fn describe(device: &Device) -> output::Status {
    output::Status {
        id: device.id.clone(),
        name: device.name.clone(),
        rate: device.rate,
        buffer: device.buffer,
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

/// A track's decoder converted to `rate`, the device's, by ardftsrc's high preset. A decoder
/// already at that rate passes through untouched, sample for sample. The engine converts as it
/// decodes, ahead of the queue, so a pause, a clear or a position read acts on audio already at
/// the device's rate rather than on what the resampler still holds. Fast start primes it from the
/// decoder at once, so the first samples out are the track rather than silence.
pub fn resampled<S: Source>(source: S, rate: u32) -> Result<RodioResampler<S>> {
    let native = source.sample_rate().get();
    let config = ardftsrc::PRESET_HIGH
        .with_input_rate(native as usize)
        .with_output_rate(rate as usize)
        .with_channels(usize::from(source.channels().get()))
        .with_rodio_fast_start(true);
    if native != rate {
        log::info!("sink: resampling {native} Hz to {rate} Hz");
    }
    RodioResampler::new(source, config).context("cannot start the resampler")
}

/// Whether there is an output device to play on. A device can be missing for as long as it
/// takes a headset to reconnect, so the engines ask again rather than give up.
pub fn available() -> bool {
    output::wanted().is_ok()
}

/// Opens the stream on `device` in `config`, flagging `failed` when the device reports an error.
/// Returns the stream and the buffer it asked for, in frames.
fn open_stream(
    device: cpal::Device,
    id: &str,
    config: &cpal::SupportedStreamConfig,
    failed: &Arc<AtomicBool>,
) -> Result<(MixerDeviceSink, u32)> {
    let frames = match *config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => BUFFER_FRAMES.clamp(min, max.max(min)),
        cpal::SupportedBufferSize::Unknown => BUFFER_FRAMES,
    };
    log::info!(
        "sink: using {} at {} Hz, {} channels, {}, {frames} frames",
        id,
        config.sample_rate(),
        config.channels(),
        config.sample_format()
    );

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
    Ok((stream, frames))
}

/// Looks at `device` every `DEVICE_POLL` for as long as it is open, playing or not, and retires
/// it once it is outdated, so its outputs reopen and a settings page reads the new stream.
fn watch(device: Weak<Device>) {
    let spawned = thread::Builder::new()
        .name("output-watch".to_owned())
        .spawn(move || {
            loop {
                thread::sleep(DEVICE_POLL);
                let Some(device) = device.upgrade() else {
                    return;
                };
                if device.outdated() {
                    log::info!("sink: {} changed, reopening", device.id);
                    device.retire();
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        log::warn!("sink: cannot watch the output device: {error}");
    }
}

/// A fresh player whose chain plays into `device`.
fn attach(chain: &Chain, device: &Device) -> rodio::Player {
    let (sink, source) = rodio::Player::new();
    play(chain, device, source);
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
