//! How equalizer profiles are stored in the settings file and edited through `AppSettings`.
//! Every reader here is lenient, so a hand-edited mistake drops one value instead of making the
//! whole file unreadable.

use std::collections::HashSet;

use anyhow::{Context as _, Result};
use gpui::{Context, SharedString};
use music::equalizer::{Curve, Filter, Kind, MAX_FILTERS, PREAMP_RANGE, Preamp, Preset, Width};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use super::{AppSettings, Values};

/// One stored equalizer curve, in plain numbers and strings, so a hand-edited mistake drops one
/// filter instead of making the file unreadable. The name is empty for the profile Sonora made.
#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct Profile {
    name: String,
    preamp: StoredPreamp,
    filters: Vec<StoredFilter>,
    /// The output devices this profile plays on. A device is named by one profile at most.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    outputs: Vec<BoundOutput>,
    /// Whether this profile plays on every output no profile names. One profile at most.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    other_outputs: bool,
}

/// An output device a profile is bound to: its stable id, and its name when it was bound, so
/// a binding to a device that is not connected can still be shown.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundOutput {
    pub id: String,
    pub name: String,
}

/// `"auto"` or a number of decibels.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum StoredPreamp {
    Decibels(f64),
    Word(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct StoredFilter {
    kind: String,
    frequency: f64,
    gain: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    q: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    octaves: Option<f64>,
    enabled: bool,
}

impl Values {
    /// Settles the equalizer values for use: one profile when the file had none, built from the
    /// band gains an older Sonora saved when it had those, each output bound once, and the
    /// chosen profile in range.
    pub(super) fn settle_equalizer(&mut self) {
        if self.equalizer_profiles.is_empty() {
            let gains: Vec<f64> = self
                .equalizer_bands
                .iter()
                .map(|gain| f64::from(*gain))
                .collect();
            self.equalizer_profiles
                .push(Profile::of(&Curve::bands(&gains)));
        }
        self.equalizer_bands = Vec::new();
        let mut bound = HashSet::new();
        let mut other = false;
        for profile in &mut self.equalizer_profiles {
            let before = profile.outputs.len();
            profile
                .outputs
                .retain(|output| bound.insert(output.id.clone()));
            if profile.outputs.len() < before {
                log::warn!(
                    "settings: an output named by two equalizer profiles stays with the first; the second binding is dropped the next time settings are saved"
                );
            }
            if profile.other_outputs && std::mem::replace(&mut other, true) {
                log::warn!(
                    "settings: more than one equalizer profile plays on other outputs, keeping the first; the rest are cleared the next time settings are saved"
                );
                profile.other_outputs = false;
            }
        }
        self.equalizer_profile = self
            .equalizer_profile
            .min(self.equalizer_profiles.len() - 1);
    }
}

impl Profile {
    fn of(curve: &Curve) -> Self {
        let mut profile = Self::default();
        profile.store(curve);
        profile
    }

    /// Every profile in the file's list that is an object, a filter that does not read dropped
    /// and logged and a preamp that does not read taken as automatic.
    pub(super) fn read_all<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Self>, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let serde_json::Value::Array(items) = value else {
            log::warn!(
                "settings: equalizer_profiles is not a list, starting over with one; the stored value is replaced the next time settings are saved"
            );
            return Ok(Vec::new());
        };
        Ok(items.into_iter().filter_map(Self::read).collect())
    }

    fn read(value: serde_json::Value) -> Option<Self> {
        let serde_json::Value::Object(mut fields) = value else {
            log::warn!(
                "settings: dropping an equalizer profile that is not an object; it is gone the next time settings are saved"
            );
            return None;
        };
        let mut profile = Self::default();
        if let Some(name) = profile_field(&mut fields, "name") {
            profile.name = name;
        }
        match profile_field(&mut fields, "preamp") {
            Some(StoredPreamp::Decibels(db)) if !PREAMP_RANGE.contains(&db) => log::warn!(
                "settings: equalizer preamp {db} dB is outside {PREAMP_RANGE:?}, taking auto; the stored value is replaced the next time settings are saved"
            ),
            Some(StoredPreamp::Word(word)) if word != "auto" => log::warn!(
                "settings: equalizer preamp {word:?} is not a number nor auto, taking auto; the stored value is replaced the next time settings are saved"
            ),
            Some(preamp) => profile.preamp = preamp,
            None => {}
        }
        let filters: Vec<serde_json::Value> =
            profile_field(&mut fields, "filters").unwrap_or_default();
        profile.filters = filters
            .into_iter()
            .filter_map(|filter| {
                serde_json::from_value::<StoredFilter>(filter)
                    .map_err(anyhow::Error::from)
                    .and_then(|filter| filter.parse())
                    .map(|filter| StoredFilter::of(&filter))
                    .map_err(|error| {
                        log::warn!(
                            "settings: dropping an equalizer filter, gone the next time settings are saved: {error:#}"
                        );
                    })
                    .ok()
            })
            .collect();
        let outputs: Vec<serde_json::Value> =
            profile_field(&mut fields, "outputs").unwrap_or_default();
        profile.outputs = outputs
            .into_iter()
            .filter_map(|output| match serde_json::from_value::<BoundOutput>(output) {
                Ok(output) if !output.id.is_empty() => Some(output),
                _ => {
                    log::warn!(
                        "settings: dropping an equalizer profile's output that has no id and name, gone the next time settings are saved"
                    );
                    None
                }
            })
            .collect();
        profile.other_outputs = profile_field(&mut fields, "other_outputs").unwrap_or(false);
        if profile.filters.len() > MAX_FILTERS {
            log::warn!(
                "settings: dropping {} equalizer filters past the first {MAX_FILTERS}",
                profile.filters.len() - MAX_FILTERS
            );
            profile.filters.truncate(MAX_FILTERS);
        }
        Some(profile)
    }

    fn store(&mut self, curve: &Curve) {
        self.preamp = match curve.preamp() {
            Preamp::Auto => StoredPreamp::default(),
            Preamp::Manual(db) => StoredPreamp::Decibels(db),
        };
        self.filters = curve.filters().iter().map(StoredFilter::of).collect();
    }

    /// The curve as stored. `read` already dropped, and logged, what does not make a valid
    /// curve, so this only converts.
    fn curve(&self) -> Curve {
        let preamp = match self.preamp {
            StoredPreamp::Decibels(db) if PREAMP_RANGE.contains(&db) => Preamp::Manual(db),
            _ => Preamp::Auto,
        };
        let filters = self
            .filters
            .iter()
            .filter_map(|filter| filter.parse().ok())
            .take(MAX_FILTERS)
            .collect();
        Curve::new(preamp, filters).expect("the preamp and every filter were checked")
    }
}

impl Default for StoredPreamp {
    fn default() -> Self {
        Self::Word("auto".to_owned())
    }
}

impl StoredFilter {
    fn of(filter: &Filter) -> Self {
        let (q, octaves) = match filter.width() {
            Width::Q(q) => (Some(q), None),
            Width::Octaves(octaves) => (None, Some(octaves)),
        };
        Self {
            kind: filter.kind().id().to_owned(),
            frequency: filter.frequency(),
            gain: filter.gain(),
            q,
            octaves,
            enabled: filter.enabled(),
        }
    }

    fn parse(&self) -> Result<Filter> {
        let kind = Kind::from_id(&self.kind)
            .with_context(|| format!("unknown filter kind {:?}", self.kind))?;
        let width = match (self.q, self.octaves) {
            (Some(q), _) => Width::Q(q),
            (None, Some(octaves)) => Width::Octaves(octaves),
            (None, None) => anyhow::bail!("a filter needs a q or a width in octaves"),
        };
        Ok(Filter::new(
            kind,
            self.frequency,
            self.gain,
            width,
            self.enabled,
        )?)
    }
}

impl Default for StoredFilter {
    fn default() -> Self {
        Self {
            kind: Kind::Peak.id().to_owned(),
            frequency: 1_000.,
            gain: 0.,
            q: None,
            octaves: None,
            enabled: true,
        }
    }
}

impl AppSettings {
    pub fn equalizer(&self) -> bool {
        self.values.equalizer
    }

    /// The curve of the profile that plays, with any filter out of range dropped. Flat when
    /// there is no profile.
    pub fn equalizer_curve(&self) -> Curve {
        self.values
            .equalizer_profiles
            .get(self.values.equalizer_profile)
            .map_or_else(|| Preset::Flat.curve(), Profile::curve)
    }

    /// The curve profile `index` holds, with any filter out of range dropped.
    pub fn equalizer_profile_curve(&self, index: usize) -> Option<Curve> {
        self.values
            .equalizer_profiles
            .get(index)
            .map(Profile::curve)
    }

    pub fn set_equalizer(&mut self, on: bool, cx: &mut Context<Self>) {
        self.values.equalizer = on;
        self.schedule_save(cx);
    }

    pub fn set_equalizer_advanced(&mut self, advanced: bool, cx: &mut Context<Self>) {
        self.values.equalizer_advanced = advanced;
        self.schedule_save(cx);
    }

    /// The names of the equalizer profiles in order, empty for the one Sonora made.
    pub fn equalizer_profiles(&self) -> impl Iterator<Item = &str> {
        self.values
            .equalizer_profiles
            .iter()
            .map(|profile| profile.name.as_str())
    }

    /// Which of `equalizer_profiles` plays.
    pub fn equalizer_profile(&self) -> usize {
        self.values.equalizer_profile
    }

    pub fn equalizer_advanced(&self) -> bool {
        self.values.equalizer_advanced
    }

    /// The name of the profile that plays, empty for the one Sonora made.
    pub fn equalizer_profile_name(&self) -> &str {
        self.values
            .equalizer_profiles
            .get(self.values.equalizer_profile)
            .map_or("", |profile| profile.name.as_str())
    }

    /// Makes profile `index` the one that plays. An index past the end is ignored.
    pub fn select_equalizer_profile(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.values.equalizer_profiles.len() {
            self.values.equalizer_profile = index;
            self.schedule_save(cx);
        }
    }

    /// Where the profile with no name sits, the one presets load into, if there is one.
    pub fn equalizer_scratch(&self) -> Option<usize> {
        self.values
            .equalizer_profiles
            .iter()
            .position(|profile| profile.name.is_empty())
    }

    /// Makes the profile with no name the one that plays, adding it holding `curve` when there
    /// is none, so a preset never lands on a profile someone named.
    pub fn select_equalizer_scratch(&mut self, curve: &Curve, cx: &mut Context<Self>) {
        match self.equalizer_scratch() {
            Some(index) => self.values.equalizer_profile = index,
            None => {
                let values = &mut self.values;
                values.equalizer_profile = values.equalizer_profiles.len();
                values.equalizer_profiles.push(Profile::of(curve));
            }
        }
        self.schedule_save(cx);
    }

    /// Adds a profile after the others and makes it the one that plays. Its name is `name`
    /// trimmed, numbered when another profile shows it already. An empty name stays empty so
    /// the label is read at render, unless a profile with no name exists, which keeps that one
    /// the only one presets load into.
    pub fn add_equalizer_profile(&mut self, name: &str, curve: &Curve, cx: &mut Context<Self>) {
        let name = match (name.trim(), self.equalizer_scratch()) {
            ("", None) => String::new(),
            ("", Some(_)) => self.free_profile_name(&equalizer_profile_label(""), None),
            (name, _) => self.free_profile_name(name, None),
        };
        let values = &mut self.values;
        values.equalizer_profile = values.equalizer_profiles.len();
        values.equalizer_profiles.push(Profile {
            name,
            ..Profile::of(curve)
        });
        self.schedule_save(cx);
    }

    /// Renames profile `index` to `name` trimmed, numbered when another profile shows it
    /// already, while it is still the profile named `from`. Returns false for an empty name.
    pub fn rename_equalizer_profile(
        &mut self,
        index: usize,
        from: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(profile) = self.values.equalizer_profiles.get(index) else {
            return true;
        };
        if profile.name != from || equalizer_profile_label(&profile.name) == name {
            return true;
        }
        let name = self.free_profile_name(name, Some(index));
        self.values.equalizer_profiles[index].name = name;
        self.schedule_save(cx);
        true
    }

    /// Removes profile `index`, unless it is the only one or no longer the profile named
    /// `name`. When it was the one playing, its neighbour plays.
    pub fn remove_equalizer_profile(&mut self, index: usize, name: &str, cx: &mut Context<Self>) {
        let values = &mut self.values;
        let named = values
            .equalizer_profiles
            .get(index)
            .is_some_and(|profile| profile.name == name);
        if !named || values.equalizer_profiles.len() < 2 {
            return;
        }
        values.equalizer_profiles.remove(index);
        if index < values.equalizer_profile {
            values.equalizer_profile -= 1;
        }
        values.equalizer_profile = values
            .equalizer_profile
            .min(values.equalizer_profiles.len() - 1);
        self.schedule_save(cx);
    }

    /// `name`, or `name` with the lowest number after it that leaves it unlike every profile's
    /// shown name, profile `except` left out.
    fn free_profile_name(&self, name: &str, except: Option<usize>) -> String {
        let taken = |candidate: &str| {
            self.values
                .equalizer_profiles
                .iter()
                .enumerate()
                .any(|(index, profile)| {
                    Some(index) != except && equalizer_profile_label(&profile.name) == candidate
                })
        };
        (1..)
            .map(|number| match number {
                1 => name.to_owned(),
                number => format!("{name} {number}"),
            })
            .find(|candidate| !taken(candidate))
            .unwrap_or_else(|| name.to_owned())
    }

    /// The profile that plays on output `id`: the one bound to it, else the one for other
    /// outputs. `None` when neither is set, so the playing profile stays.
    pub fn equalizer_profile_for_output(&self, id: &str) -> Option<usize> {
        self.equalizer_profile_bound_to(id)
            .or_else(|| self.equalizer_other_outputs())
    }

    /// The profile bound to output `id`, if one is.
    pub fn equalizer_profile_bound_to(&self, id: &str) -> Option<usize> {
        self.values
            .equalizer_profiles
            .iter()
            .position(|profile| profile.outputs.iter().any(|output| output.id == id))
    }

    /// The profile that plays on outputs no profile names, if one does.
    pub fn equalizer_other_outputs(&self) -> Option<usize> {
        self.values
            .equalizer_profiles
            .iter()
            .position(|profile| profile.other_outputs)
    }

    /// Binds `output` to profile `index`, taking it from any other, or unbinds it with `None`.
    pub fn bind_equalizer_output(
        &mut self,
        output: BoundOutput,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        for profile in &mut self.values.equalizer_profiles {
            profile.outputs.retain(|bound| bound.id != output.id);
        }
        if let Some(profile) = index.and_then(|index| self.values.equalizer_profiles.get_mut(index))
        {
            profile.outputs.push(output);
        }
        self.schedule_save(cx);
    }

    /// Makes profile `index` the one for outputs no profile names, or none with `None`.
    pub fn set_equalizer_other_outputs(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        for (at, profile) in self.values.equalizer_profiles.iter_mut().enumerate() {
            profile.other_outputs = Some(at) == index;
        }
        self.schedule_save(cx);
    }

    /// Stores `curve` as the profile that plays, keeping its name.
    pub fn set_equalizer_curve(&mut self, curve: &Curve, cx: &mut Context<Self>) {
        let values = &mut self.values;
        match values.equalizer_profiles.get_mut(values.equalizer_profile) {
            Some(profile) => profile.store(curve),
            None => {
                values.equalizer_profile = values.equalizer_profiles.len();
                values.equalizer_profiles.push(Profile::of(curve));
            }
        }
        self.schedule_save(cx);
    }
}

/// The name a profile shows: its stored name, or the interface's label for the empty one
/// Sonora made.
pub fn equalizer_profile_label(stored: &str) -> SharedString {
    match stored.is_empty() {
        true => i18n::t!("equalizer-profile-default"),
        false => SharedString::from(stored.to_owned()),
    }
}

/// The band gains an older Sonora saved: a band that is not a number reads as 0 dB, so the
/// bands after it keep their places, and anything but a list reads as none.
pub(super) fn read_gains<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<f32>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    let serde_json::Value::Array(gains) = value else {
        log::warn!(
            "settings: equalizer_bands {value} is not a list, ignoring it; the key is dropped the next time settings are saved"
        );
        return Ok(Vec::new());
    };
    Ok(gains
        .iter()
        .map(|gain| match gain.as_f64() {
            Some(gain) => gain as f32,
            None => {
                log::warn!(
                    "settings: equalizer band {gain} is not a number, taking 0 dB; equalizer_bands is dropped the next time settings are saved"
                );
                0.
            }
        })
        .collect())
}

/// Field `key` of a stored equalizer profile, or `None` with a warning when it is there but does
/// not read as a `T`, so one mistyped field never drops the profile.
fn profile_field<T: DeserializeOwned>(
    fields: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<T> {
    serde_json::from_value(fields.remove(key)?)
        .map_err(|error| {
            log::warn!(
                "settings: an equalizer profile's {key} does not read, taking the default; the stored value is replaced the next time settings are saved: {error}"
            );
        })
        .ok()
}

/// The profile index as stored: any number clamped at zero, and anything else as zero, so a
/// hand-edited mistake picks the first profile instead of making the file unreadable.
pub(super) fn read_index<'de, D: Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    if !value.is_number() {
        log::warn!(
            "settings: equalizer_profile {value} is not a number, taking the first; the stored value is replaced the next time settings are saved"
        );
    }
    Ok(value.as_f64().unwrap_or(0.).max(0.) as usize)
}
