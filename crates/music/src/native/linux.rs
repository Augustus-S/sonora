//! cpal's ALSA host, which serves Linux and the BSDs. cpal's ranges are trusted, and the host sets
//! the rate itself as it opens the stream.
//!
//! TODO: Right now, cpal 0.17 opens ALSA's `default` device, which on most desktops is PipeWire's ALSA plugin.
//! The plugin takes any rate and PipeWire converts it to the graph's rate unseen,
//! and cpal's default config is 44.1 kHz whenever the range allows it, whatever the graph runs at.
//! So every rate looks native here and none of them are. This is a big problem!
//!
//! When sonora moves to rodio 0.23 and cpal 0.18:
//!
//! - Turn on cpal's `pipewire` feature and open the PipeWire host when its socket is reachable,
//!   keeping ALSA for systems without it. Check the Flatpak's read-only `xdg-run/pipewire-0` is
//!   enough to connect.
//! - That host reports `clock.allowed-rates` as its supported configs and the graph's `clock.rate`
//!   as its default, so `offers` below becomes exact as it is written.
//! - PipeWire only switches the graph to a rate in `clock.allowed-rates`, and not while another
//!   stream holds it at a different one. cpal's streams set `node.latency` but not `node.rate`, so
//!   check on a PipeWire machine whether the graph follows a stream's rate. If it does not, `switch`
//!   has to ask for it.
//! - After opening, confirm the graph really runs at the stream's rate, and fall back to the graph's
//!   rate when it does not, since PipeWire would otherwise convert the stream unseen.

use anyhow::Result;
use cpal::SupportedStreamConfig;
use cpal::traits::DeviceTrait as _;

/// Whether one of cpal's ranges in the default's channels and sample format holds `rate`.
pub(super) fn offers(device: &cpal::Device, default: &SupportedStreamConfig, rate: u32) -> bool {
    device.supported_output_configs().is_ok_and(|mut ranges| {
        ranges.any(|range| {
            range.channels() == default.channels()
                && range.sample_format() == default.sample_format()
                && range.try_with_sample_rate(rate).is_some()
        })
    })
}

/// Nothing to do: the host opens the hardware at the stream's rate by itself.
pub(super) fn switch(_device: &cpal::Device, _rate: u32) -> Result<()> {
    Ok(())
}
