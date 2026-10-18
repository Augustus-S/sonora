//! Which of cpal's Linux hosts Sonora plays through, and what each one's device list holds. cpal
//! picks PipeWire when its socket answers, then PulseAudio, then ALSA. A sound server lists its
//! outputs by the names the desktop shows, while ALSA lists every PCM its configuration defines.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait as _, HostTrait as _};
use pipewire::context::ContextRc;
use pipewire::core::PW_ID_CORE;
use pipewire::main_loop::MainLoopRc;
use pipewire::metadata::{Metadata, MetadataListener};
use pipewire::spa::utils::result::AsyncSeq;
use pipewire::types::ObjectType;

use super::{DeviceOption, output_alsa};

/// The device cpal's PipeWire host offers for playing into whatever the server's default sink
/// is. The picker offers that as "System default", so it is not listed again.
const PIPEWIRE_DEFAULT_SINK: &str = "sink_default";

/// How long PipeWire is given to say which sink is its default.
const ANSWER: Duration = Duration::from_secs(1);

/// The metadata key that holds the sink PipeWire plays its default output on, as JSON such as
/// `{"name": "alsa_output.usb-…"}`.
const DEFAULT_SINK_KEY: &str = "default.audio.sink";

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
/// a device of its own, so PipeWire's metadata is read for the sink behind it.
pub(super) fn system_default(host: &cpal::Host) -> Option<String> {
    match host.id() {
        cpal::HostId::PipeWire => {
            let sink = pipewire_default_sink()?;
            Some(cpal::DeviceId::new(cpal::HostId::PipeWire, &sink).to_string())
        }
        cpal::HostId::PulseAudio => Some(host.default_output_device()?.id().ok()?.to_string()),
        _ => None,
    }
}

/// The node name of the sink PipeWire plays its default output on, from its `default` metadata,
/// or `None` when PipeWire does not say within `ANSWER`. Blocks while it waits.
fn pipewire_default_sink() -> Option<String> {
    // libpipewire counts its users, so this one never lets go and cpal's never pull it away
    pipewire::init();
    let mainloop = MainLoopRc::new(None).ok()?;
    let context = ContextRc::new(&mainloop, None).ok()?;
    let core = context.connect_rc(None).ok()?;
    let registry = core.get_registry_rc().ok()?;

    let sink: Rc<RefCell<Option<String>>> = Rc::default();
    let pending: Rc<RefCell<Vec<AsyncSeq>>> = Rc::default();
    let bound: Rc<RefCell<Vec<(Metadata, MetadataListener)>>> = Rc::default();
    // the server answers this once it has listed every object, the metadata among them
    pending.borrow_mut().push(core.sync(0).ok()?);

    let _done = core
        .add_listener_local()
        .done({
            let pending = pending.clone();
            let mainloop = mainloop.clone();
            move |id, seq| {
                if id != PW_ID_CORE {
                    return;
                }
                let mut pending = pending.borrow_mut();
                pending.retain(|waiting| *waiting != seq);
                if pending.is_empty() {
                    mainloop.quit();
                }
            }
        })
        .register();
    let _globals = registry
        .add_listener_local()
        .global({
            let registry = registry.clone();
            let core = core.clone();
            let sink = sink.clone();
            let pending = pending.clone();
            let bound = bound.clone();
            move |global| {
                let named_default = global
                    .props
                    .is_some_and(|props| props.get("metadata.name") == Some("default"));
                if global.type_ != ObjectType::Metadata || !named_default {
                    return;
                }
                let Ok(metadata) = registry.bind::<Metadata, _>(global) else {
                    return;
                };
                let sink = sink.clone();
                let listener = metadata
                    .add_listener_local()
                    .property(move |_, key, _, value| {
                        if key == Some(DEFAULT_SINK_KEY) {
                            *sink.borrow_mut() = value.and_then(sink_name);
                        }
                        0
                    })
                    .register();
                // answered once the metadata has sent its properties
                if let Ok(seq) = core.sync(0) {
                    pending.borrow_mut().push(seq);
                }
                bound.borrow_mut().push((metadata, listener));
            }
        })
        .register();

    let (cancel, cancelled) = mpsc::channel::<()>();
    let (expire, expired) = pipewire::channel::channel::<()>();
    let _expiry = expired.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });
    thread::spawn(move || {
        if cancelled.recv_timeout(ANSWER).is_err() {
            expire.send(()).ok();
        }
    });
    mainloop.run();
    cancel.send(()).ok();
    sink.take()
}

/// The sink's name in a `default.audio.sink` value.
fn sink_name(value: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(value).ok()?;
    value.get("name")?.as_str().map(str::to_owned)
}
