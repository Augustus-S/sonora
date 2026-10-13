//! Windows: the system mixes every stream at the rate set in Sound
//! settings, and only Sound settings can change that rate. It is cpal's default config, which
//! `native::offers` accepts before asking here, so nothing else is offered and nothing is ever
//! switched. cpal 0.17.1 lists only that rate anyway, but 0.17.3 lists every common rate and has
//! Windows convert the others on the way in, so this holds regardless of the cpal version.

use anyhow::Result;
use cpal::SupportedStreamConfig;

/// No rate but the mix rate, which the caller has compared already.
pub(super) fn offers(_device: &cpal::Device, _default: &SupportedStreamConfig, _rate: u32) -> bool {
    false
}

/// Nothing to do, since only the rate the mixer runs at is ever offered.
pub(super) fn switch(_device: &cpal::Device, _rate: u32) -> Result<()> {
    Ok(())
}
