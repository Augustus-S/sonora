//! The config an output device is opened in, and whether it still runs at the rate a stream was
//! opened at. Sonora opens the device at the rate it really runs at and resamples every track to it
//! with ardftsrc, so nothing between Sonora and the hardware converts it again. On Linux a rate
//! picked in settings is asked for instead, when the device offers it.
//!
//! On Windows the default config is the shared-mode engine's mix format, and the engine
//! invalidates a stream whose format changes, which fails it. On macOS the default config's rate
//! is the hardware's nominal rate, but a stream whose rate changes is converted rather than told,
//! so the watch has to look for the change itself.
//!
//! TODO: On Linux, cpal 0.17 opens ALSA's `default` device, which on most desktops is PipeWire's
//! ALSA plugin. Its default config is 44.1 kHz whatever the graph runs at, and PipeWire converts to
//! the graph's rate unseen, so `config` guesses 48 kHz, PipeWire's and PulseAudio's default, which
//! is right on most desktops and leaves one hidden conversion on the rest. When Sonora moves to
//! rodio 0.23 and cpal 0.18, open cpal's PipeWire host when its socket is reachable (checking the
//! Flatpak's read-only `xdg-run/pipewire-0` is enough), whose default config is the graph's
//! `clock.rate`, drop the guess, and have `moved` watch that rate.

use cpal::SupportedStreamConfig;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use cpal::traits::DeviceTrait as _;

#[cfg(target_os = "macos")]
use super::output_rate_macos as macos;

/// The rate a sound server on Linux most likely runs its graph at.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const SERVER_RATE: u32 = 48_000;

/// The rates a device can be asked for in settings, from the lowest any music is published at to
/// the highest any is mastered at.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const RATES: [u32; 7] = [32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

/// The config to open `device` in, given `default`, its default one. That is the device's real
/// rate on macOS and Windows, so `wanted` is not asked for there.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn config(
    _device: &cpal::Device,
    default: SupportedStreamConfig,
    _wanted: Option<u32>,
) -> SupportedStreamConfig {
    default
}

/// The config to open `device` in, given `default`, its default one: the same at `wanted` when
/// the device offers it in the default's channels and sample format, otherwise at 48 kHz, since
/// cpal's default is 44.1 kHz whatever the sound server runs at.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn config(
    device: &cpal::Device,
    default: SupportedStreamConfig,
    wanted: Option<u32>,
) -> SupportedStreamConfig {
    wanted
        .and_then(|rate| at_rate(device, &default, rate))
        .or_else(|| at_rate(device, &default, SERVER_RATE))
        .unwrap_or(default)
}

/// The rates in `RATES` that `device` can be opened at in `config`'s channels and sample format,
/// lowest first. That is only the rate it runs at on macOS and Windows, where the system sets it.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn rates(_device: &cpal::Device, config: &SupportedStreamConfig) -> Vec<u32> {
    vec![config.sample_rate()]
}

/// The rates in `RATES` that `device` can be opened at in `config`'s channels and sample format,
/// with the rate `config` runs at, lowest first.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn rates(device: &cpal::Device, config: &SupportedStreamConfig) -> Vec<u32> {
    let Ok(ranges) = device.supported_output_configs() else {
        return vec![config.sample_rate()];
    };
    let ranges: Vec<_> = ranges
        .filter(|range| {
            range.channels() == config.channels() && range.sample_format() == config.sample_format()
        })
        .collect();
    let mut rates: Vec<u32> = RATES
        .into_iter()
        .filter(|&rate| {
            ranges
                .iter()
                .any(|range| (range.min_sample_rate()..=range.max_sample_rate()).contains(&rate))
        })
        .collect();
    // a device running at a rate off the list still shows it, between its neighbours
    if !rates.contains(&config.sample_rate()) {
        rates.push(config.sample_rate());
        rates.sort_unstable();
    }
    rates
}

/// Whether `device` has moved off `rate` since a stream opened at it, where the platform would
/// convert the stream rather than fail it.
#[cfg(target_os = "macos")]
pub(crate) fn moved(device: &cpal::Device, rate: u32) -> bool {
    macos::rate(device).is_some_and(|now| now != rate)
}

/// Whether `device` has moved off `rate` since a stream opened at it, where the platform would
/// convert the stream rather than fail it.
#[cfg(not(target_os = "macos"))]
pub(crate) fn moved(_device: &cpal::Device, _rate: u32) -> bool {
    false
}

/// `like` at `rate`, if the device offers that rate in its channels and sample format.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn at_rate(
    device: &cpal::Device,
    like: &SupportedStreamConfig,
    rate: u32,
) -> Option<SupportedStreamConfig> {
    if like.sample_rate() == rate {
        return Some(like.clone());
    }
    device
        .supported_output_configs()
        .ok()?
        .filter(|range| {
            range.channels() == like.channels() && range.sample_format() == like.sample_format()
        })
        .find_map(|range| range.try_with_sample_rate(rate))
}
