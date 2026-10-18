//! Which of cpal's Linux hosts Sonora plays through, and what each one's device list holds. cpal
//! picks PulseAudio when its socket answers, which PipeWire serves through `pipewire-pulse`, and
//! ALSA otherwise. A sound server lists its outputs by the names the desktop shows, while ALSA
//! lists every PCM its configuration defines.

use std::sync::{Arc, Mutex, PoisonError};

use cpal::traits::{DeviceTrait as _, HostTrait as _};

use super::{DeviceOption, output_alsa};

/// The PulseAudio host every caller shares while its connection to the server holds. A dropped
/// PulseAudio host never closes its connection, so a host made per call leaves one open each time.
static SERVER_HOST: Mutex<Option<Arc<cpal::Host>>> = Mutex::new(None);

/// The host Sonora plays through. A PulseAudio host is made once and shared, and made again once
/// its connection to the server is lost, while an ALSA host is made afresh, so a server that
/// starts later is still found.
pub(super) fn host() -> Arc<cpal::Host> {
    let mut shared = SERVER_HOST.lock().unwrap_or_else(PoisonError::into_inner);
    // a host whose connection is gone fails every call at once, without asking the server
    if let Some(host) = shared.as_ref()
        && host.output_devices().is_ok()
    {
        return host.clone();
    }
    let host = Arc::new(cpal::default_host());
    *shared = is_server(&host).then(|| host.clone());
    host
}

/// Whether `host` is a sound server, which mixes every app's streams and routes them to its
/// sinks, rather than ALSA, which opens the device itself.
pub(super) fn is_server(host: &cpal::Host) -> bool {
    host.id() == cpal::HostId::PulseAudio
}

/// Whether `device` belongs to a sound server's host.
pub(super) fn served(device: &cpal::Device) -> bool {
    device
        .id()
        .is_ok_and(|id| id.host() == cpal::HostId::PulseAudio)
}

/// `option`, made from `device`, as the device list shows it, or `None` when the list leaves it
/// out. ALSA's devices are tidied by `output_alsa::tidy`.
pub(super) fn listed(device: &cpal::Device, option: DeviceOption) -> Option<DeviceOption> {
    let id = device.id().ok()?;
    match id.host() {
        cpal::HostId::Alsa => output_alsa::tidy(id.id(), option),
        _ => Some(option),
    }
}

/// The id of the sink the server plays its default output on, as `devices` lists it, or `None`
/// on ALSA, whose default is left out of the list. The server names the sink behind its default
/// when asked for it, `pipewire-pulse` from PipeWire's own default.
pub(super) fn system_default(host: &cpal::Host) -> Option<String> {
    match host.id() {
        cpal::HostId::PulseAudio => Some(host.default_output_device()?.id().ok()?.to_string()),
        _ => None,
    }
}
