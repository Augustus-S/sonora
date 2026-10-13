//! Which output device Sonora plays on, and what the stream on it runs at. Every engine mixes into
//! one stream on the chosen device, or on the system's default when none is chosen or the chosen
//! one is not connected. A new choice reaches the engines through the same watch that notices the
//! default device changing.
//!
//! A device that would not open is remembered, so it is not tried again until it is chosen again
//! or leaves the system and comes back. Opening one can take CoreAudio ten seconds to give up on.
//! A device the system no longer lists is gone, which is not a failure: the default plays in its
//! place without trying it, and it is played on again once it is back.

pub(crate) mod output_rate;
#[cfg(target_os = "macos")]
mod output_rate_macos;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, PoisonError};

use anyhow::{Context as _, Result, bail};
use cpal::traits::{DeviceTrait as _, HostTrait as _};
use tokio::sync::watch;

/// The device chosen in settings, or `None` to follow the system's default.
static CHOSEN: Mutex<Option<Choice>> = Mutex::new(None);

/// Counts the choices made, so a stream or a failure tells which choice it answers.
static ROUND: AtomicU64 = AtomicU64::new(0);

/// The devices that would not open, most recent last.
static FAILED: Mutex<Vec<Failure>> = Mutex::new(Vec::new());

/// Moves on whenever a stream opens or closes, so a settings page can read `status` again.
static CHANGES: LazyLock<watch::Sender<()>> = LazyLock::new(|| watch::Sender::new(()));

/// An output device the system offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// cpal's id for the device, which is what `choose` takes and what settings store.
    pub id: String,
    pub name: String,
    /// Whether it is the system's default output right now.
    pub system: bool,
}

/// A device that would not open, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub id: String,
    pub error: String,
    /// Whether it is the saved device, which gave way to the system's default, rather than one
    /// picked just now, which leaves nothing playing.
    pub saved: bool,
    round: u64,
}

/// How the device chosen in one round turned out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Opened(Status),
    Failed(Failure),
}

/// A device chosen in settings, by cpal id.
#[derive(Clone)]
struct Choice {
    id: String,
    /// Whether the system's default plays in its place when it would not open, as for a saved
    /// device rather than one picked just now and not yet opened.
    fallback: bool,
}

/// The stream Sonora plays into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub id: String,
    pub name: String,
    /// The rate the device runs at, which every track is resampled to.
    pub rate: u32,
    /// The buffer Sonora asks the device for, in frames.
    pub buffer: u32,
    pub(crate) round: u64,
}

/// Every output device the system offers. This opens devices on some platforms, so call it off
/// the UI thread.
pub fn devices() -> Vec<Device> {
    let host = cpal::default_host();
    let system = host
        .default_output_device()
        .and_then(|device| device.id().ok());
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let id = device.id().ok()?;
            Some(Device {
                system: system.as_ref() == Some(&id),
                id: id.to_string(),
                name: name(&device),
            })
        })
        .collect()
}

/// Plays on the device with `id` from now on, or on the system's default when `id` is `None`, as
/// picked in settings. A device that would not open leaves nothing playing, with `failure` saying
/// why, rather than playing somewhere the user did not pick. Every device is tried afresh.
/// Returns the round to ask `outcome` about.
pub fn choose(id: Option<String>) -> u64 {
    set(id.map(|id| Choice {
        id,
        fallback: false,
    }))
}

/// Plays on the device with `id`, saved from an earlier run, or on the system's default when it
/// would not open or `id` is `None`.
pub fn restore(id: Option<String>) {
    set(id.map(|id| Choice { id, fallback: true }));
}

/// Treats the device picked in `round` as saved, as it is once it has opened: from then on it
/// gives way to the system's default when it will not open. A later choice is left as it is.
pub fn keep(round: u64) {
    if ROUND.load(Ordering::Acquire) != round {
        return;
    }
    if let Some(choice) = CHOSEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_mut()
    {
        choice.fallback = true;
    }
}

fn set(choice: Option<Choice>) -> u64 {
    *CHOSEN.lock().unwrap_or_else(PoisonError::into_inner) = choice;
    let round = ROUND.fetch_add(1, Ordering::AcqRel) + 1;
    failed().clear();
    announce();
    round
}

/// Whether the device chosen in `round` has opened or would not, or `None` while that is not
/// known yet. A later choice answers for it too.
pub fn outcome(round: u64) -> Option<Outcome> {
    if let Some(status) = status().filter(|status| status.round >= round) {
        return Some(Outcome::Opened(status));
    }
    failed()
        .iter()
        .rev()
        .find(|failure| failure.round >= round)
        .cloned()
        .map(Outcome::Failed)
}

/// The stream every engine plays into, if one is open.
pub fn status() -> Option<Status> {
    crate::audio::status()
}

/// The device that last would not open, and why, if one has not opened since.
pub fn failure() -> Option<Failure> {
    failed().last().cloned()
}

/// A watch that moves on whenever a stream opens or closes, or a device would not open.
pub fn changes() -> watch::Receiver<()> {
    CHANGES.subscribe()
}

/// Where the system lets someone change the device's sample rate, as a URL to open, or `None` on
/// a system that has no such place.
pub fn settings() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("file:///System/Applications/Utilities/Audio%20MIDI%20Setup.app")
    } else if cfg!(target_os = "windows") {
        Some("ms-settings:sound")
    } else {
        None
    }
}

/// The device to play on: the chosen one while it is connected, and the system's default
/// otherwise. A device that would not open is not offered again, and a chosen one gives way to
/// the default only when it was restored rather than picked.
pub(crate) fn wanted() -> Result<cpal::Device> {
    let host = cpal::default_host();
    forget_gone(&host);
    let chosen = CHOSEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(choice) = chosen
        && let Some(device) = choice.id.parse().ok().and_then(|id| host.device_by_id(&id))
    {
        if !refused(&choice.id) {
            return Ok(device);
        }
        if !choice.fallback {
            bail!("the chosen output device would not open");
        }
    }
    let device = host
        .default_output_device()
        .context("no audio output device")?;
    if refused(&ident(&device)) {
        bail!("the default output device would not open");
    }
    Ok(device)
}

/// Remembers that the device with `id` would not open, so `wanted` passes it over.
pub(crate) fn refuse(id: &str, error: &anyhow::Error, round: u64) {
    let saved = CHOSEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|choice| choice.fallback && choice.id == id);
    let mut failed = failed();
    failed.retain(|failure| failure.id != id);
    failed.push(Failure {
        id: id.to_owned(),
        error: format!("{error:#}"),
        saved,
        round,
    });
    drop(failed);
    announce();
}

/// Forgets any failure of the device with `id`, which opened.
pub(crate) fn opened(id: &str) {
    failed().retain(|failure| failure.id != id);
    announce();
}

/// The round of the latest choice.
pub(crate) fn round() -> u64 {
    ROUND.load(Ordering::Acquire)
}

/// cpal's id for `device` as a string, or `unknown` when it has none.
pub(crate) fn ident(device: &cpal::Device) -> String {
    device
        .id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| "unknown".to_owned())
}

/// Tells every `changes` watch that the stream opened or closed.
pub(crate) fn announce() {
    CHANGES.send_modify(|_| {});
}

/// The name the system shows for `device`, or its id when it has none.
pub(crate) fn name(device: &cpal::Device) -> String {
    match device.description() {
        Ok(description) => description.name().to_owned(),
        Err(_) => device
            .id()
            .map(|id| id.to_string())
            .unwrap_or_else(|_| "unknown".to_owned()),
    }
}

fn failed() -> std::sync::MutexGuard<'static, Vec<Failure>> {
    FAILED.lock().unwrap_or_else(PoisonError::into_inner)
}

fn refused(id: &str) -> bool {
    failed().iter().any(|failure| failure.id == id)
}

/// Forgets the failures of devices that have left the system, so one that comes back is tried
/// again.
fn forget_gone(host: &cpal::Host) {
    // the system is asked without the lock held, since `failure` reads it on the UI thread
    let ids: Vec<String> = failed().iter().map(|failure| failure.id.clone()).collect();
    let gone: Vec<String> = ids
        .into_iter()
        .filter(|id| {
            id.parse()
                .ok()
                .and_then(|id| host.device_by_id(&id))
                .is_none()
        })
        .collect();
    if !gone.is_empty() {
        failed().retain(|failure| !gone.contains(&failure.id));
    }
}
