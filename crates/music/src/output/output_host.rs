//! Which of cpal's Linux hosts Sonora plays through, and what each one's device list holds. cpal
//! picks PipeWire when its socket answers, then PulseAudio, then ALSA. A sound server lists its
//! outputs by the names the desktop shows, while ALSA lists every PCM its configuration defines.

use cpal::traits::{DeviceTrait as _, HostTrait as _};

use super::{DeviceOption, output_alsa};

/// The device cpal's PipeWire host offers for playing into whatever the server's default sink
/// is. The picker offers that as "System default", so it is not listed again.
const PIPEWIRE_DEFAULT_SINK: &str = "sink_default";

/// Whether `host` is a sound server, which mixes every app's streams and routes them to its
/// sinks, rather than ALSA, which opens the device itself.
pub(super) fn is_server(host: &cpal::Host) -> bool {
    matches!(host.id(), cpal::HostId::PipeWire | cpal::HostId::PulseAudio)
}

/// Whether `device` belongs to a sound server's host.
pub(super) fn served(device: &cpal::Device) -> bool {
    device
        .id()
        .is_ok_and(|id| matches!(id.host(), cpal::HostId::PipeWire | cpal::HostId::PulseAudio))
}

/// `option`, made from `device`, as the device list shows it, or `None` when the list leaves it
/// out. PipeWire lists every node that takes audio, so only its sinks are kept, and ALSA's
/// devices are tidied by `output_alsa::tidy`.
pub(super) fn listed(device: &cpal::Device, option: DeviceOption) -> Option<DeviceOption> {
    let id = device.id().ok()?;
    match id.host() {
        // a sink is the one node cpal offers both ways, since it can be recorded from as well
        cpal::HostId::PipeWire => (id.id() != PIPEWIRE_DEFAULT_SINK
            && device.supports_input()
            && device.supports_output())
        .then_some(option),
        cpal::HostId::Alsa => output_alsa::tidy(id.id(), option),
        _ => Some(option),
    }
}

/// The id of the sink the server plays its default output on, as `devices` lists it, or `None`
/// on ALSA, whose default is left out of the list. cpal's PipeWire host only knows the default as
/// a device of its own, so the server's PulseAudio side, which names sinks the same, is asked.
pub(super) fn system_default(host: &cpal::Host) -> Option<String> {
    match host.id() {
        cpal::HostId::PipeWire => {
            let pulse = cpal::host_from_id(cpal::HostId::PulseAudio).ok()?;
            let sink = pulse.default_output_device()?.id().ok()?;
            Some(cpal::DeviceId::new(cpal::HostId::PipeWire, sink.id()).to_string())
        }
        cpal::HostId::PulseAudio => Some(host.default_output_device()?.id().ok()?.to_string()),
        _ => None,
    }
}
