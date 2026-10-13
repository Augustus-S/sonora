//! Which rates an output device plays as they are. Switching the hardware to a track's rate is
//! fine; a system mixer or a driver converting it is not, so a rate that would only be converted
//! is turned down here and Sonora resamples it with ardftsrc instead. Each platform module answers
//! the same two questions, `offers` and `switch`, for the rates the device does not run at already.

use anyhow::Result;
use cpal::SupportedStreamConfig;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;

/// Whether `device` plays `rate` without anything converting it, either because it runs at that
/// rate already or because its hardware can be switched to it. `default` is its current config.
pub(crate) fn offers(device: &cpal::Device, default: &SupportedStreamConfig, rate: u32) -> bool {
    default.sample_rate() == rate || platform::offers(device, default, rate)
}

/// Puts the hardware at `rate` where the platform leaves that to the caller. Only called for a
/// rate `offers` accepted. An error leaves the device where it was.
pub(crate) fn switch(device: &cpal::Device, rate: u32) -> Result<()> {
    platform::switch(device, rate)
}

/// The device's config at `rate`, in the channels and sample format of `default`.
pub(crate) fn at(default: &SupportedStreamConfig, rate: u32) -> SupportedStreamConfig {
    SupportedStreamConfig::new(
        default.channels(),
        rate,
        *default.buffer_size(),
        default.sample_format(),
    )
}
