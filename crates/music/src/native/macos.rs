//! CoreAudio's own view of an output device: the nominal rates its hardware runs at, and switching
//! between them. cpal merges those rates into one range and never switches them for an output, so
//! the device is asked directly.

use std::ptr::{NonNull, null};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use cpal::SupportedStreamConfig;
use cpal::traits::DeviceTrait as _;
use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectPropertySelector, AudioObjectSetPropertyData,
    kAudioDevicePropertyAvailableNominalSampleRates, kAudioDevicePropertyDeviceUID,
    kAudioDevicePropertyNominalSampleRate, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
};
use objc2_core_audio_types::AudioValueRange;
use objc2_core_foundation::{CFRetained, CFString};

/// How long a switch may take to be confirmed, as long as cpal waits when it switches an input.
const SWITCH_TIMEOUT: Duration = Duration::from_secs(1);
/// How often a switch is looked for while it is pending.
const SWITCH_POLL: Duration = Duration::from_millis(5);

/// Whether `rate` is one of the hardware's nominal rates. A device CoreAudio cannot find or answer
/// for offers none, so a track is never left to a hidden conversion.
pub(super) fn offers(device: &cpal::Device, _default: &SupportedStreamConfig, rate: u32) -> bool {
    let Some(object) = object(device) else {
        return false;
    };
    let Some(ranges) =
        list::<AudioValueRange>(object, kAudioDevicePropertyAvailableNominalSampleRates)
    else {
        return false;
    };
    let rate = f64::from(rate);
    ranges
        .iter()
        .any(|range| range.mMinimum <= rate && rate <= range.mMaximum)
}

/// Sets the hardware's nominal rate and waits until the device reports it. This changes the rate
/// for every app on the device, which then has its own audio converted instead.
pub(super) fn switch(device: &cpal::Device, rate: u32) -> Result<()> {
    let Some(object) = object(device) else {
        bail!("cannot find the device in CoreAudio");
    };
    let rate = f64::from(rate);
    if nominal(object) == Some(rate) {
        return Ok(());
    }

    let address = address(kAudioDevicePropertyNominalSampleRate);
    // SAFETY: the nominal rate is a single f64, and the size passed is that of `rate`.
    let status = unsafe {
        AudioObjectSetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            size_of::<f64>() as u32,
            NonNull::from(&rate).cast(),
        )
    };
    if status != 0 {
        bail!("CoreAudio refused the rate (OSStatus {status})");
    }

    let start = Instant::now();
    while start.elapsed() < SWITCH_TIMEOUT {
        if nominal(object) == Some(rate) {
            return Ok(());
        }
        std::thread::sleep(SWITCH_POLL);
    }
    bail!("the device did not confirm the switch")
}

/// The CoreAudio object behind a cpal device, matched on the UID cpal reports as its id. Walking
/// every device rather than translating the UID keeps this working on macOS 11.
fn object(device: &cpal::Device) -> Option<AudioObjectID> {
    let uid = device.id().ok()?.1;
    list::<AudioObjectID>(
        kAudioObjectSystemObject as AudioObjectID,
        kAudioHardwarePropertyDevices,
    )?
    .into_iter()
    .find(|&object| uid_of(object).as_deref() == Some(uid.as_str()))
}

fn uid_of(object: AudioObjectID) -> Option<String> {
    let address = address(kAudioDevicePropertyDeviceUID);
    let mut uid: *const CFString = null();
    let mut size = size_of::<*const CFString>() as u32;
    // SAFETY: the property is one CFString pointer, and `size` is the size of `uid`.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::from(&mut uid).cast(),
        )
    };
    if status != 0 {
        return None;
    }
    // SAFETY: CoreAudio hands the UID over retained, so `CFRetained` takes that reference and
    // releases it.
    let uid = unsafe { CFRetained::from_raw(NonNull::new(uid.cast_mut())?) };
    Some(uid.to_string())
}

fn nominal(object: AudioObjectID) -> Option<f64> {
    list::<f64>(object, kAudioDevicePropertyNominalSampleRate)?
        .first()
        .copied()
}

/// Reads a property made of `T`s, which covers one-value properties too.
fn list<T: Copy>(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Option<Vec<T>> {
    let address = address(selector);
    let mut size = 0u32;
    // SAFETY: this only writes the property's size in bytes into `size`.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
        )
    };
    if status != 0 {
        return None;
    }

    let mut items = Vec::<T>::with_capacity(size as usize / size_of::<T>());
    // SAFETY: `items` has room for `size` bytes, and CoreAudio writes at most that many and
    // reports how many it wrote back into `size`.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::new(items.as_mut_ptr())?.cast(),
        )
    };
    if status != 0 {
        return None;
    }
    // SAFETY: the call initialised `size` bytes from the start of `items`, which is this many
    // whole `T`s.
    unsafe { items.set_len(size as usize / size_of::<T>()) };
    Some(items)
}

fn address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}
