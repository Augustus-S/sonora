use std::rc::Rc;

use gpui::{App, Entity, SharedString};
use i18n::t;
use music::equalizer::Preset;
use state::{AppSettings, Equalizer, Sound};
use ui::{ActiveTheme as _, MenuItem, Picker, Popovers};

/// How tall a sound menu grows before it scrolls, in cover sizes.
pub(crate) const SOUND_MENU_HEIGHT: f32 = 2.4;

/// One picker for every sound the equalizer can play: the built-in presets, the curve someone
/// shaped from one, and the named profiles. Choosing any of them turns the equalizer on.
///
/// A preset loads into the profile with no name, so choosing one never overwrites a named
/// profile. That profile shows as the preset it matches, or as Custom once it was edited.
pub(crate) fn sound_picker(
    key: &'static str,
    popovers: &Popovers,
    equalizer: &Entity<Equalizer>,
    settings: &Entity<AppSettings>,
    cx: &App,
) -> Picker {
    let height = cx.theme().metrics.cover * SOUND_MENU_HEIGHT;
    let settings = settings.read(cx);
    let playing = settings.equalizer_profile();
    let scratch = settings.equalizer_scratch();
    let shaped = scratch
        .and_then(|index| settings.equalizer_profile_curve(index))
        .map(|curve| Preset::matching(&curve));
    let on_scratch = scratch == Some(playing);
    let custom = matches!(shaped, Some(None));

    let current = match (on_scratch, shaped.flatten()) {
        (true, Some(preset)) => i18n::lookup(preset_key(preset), None),
        (true, None) => t!("settings-equalizer-custom"),
        (false, _) => settings
            .equalizer_profiles()
            .nth(playing)
            .map(|name| SharedString::from(name.to_owned()))
            .unwrap_or_default(),
    };

    let custom_item = custom.then(|| {
        let equalizer = equalizer.clone();
        let index = scratch.unwrap_or_default();
        MenuItem::new("equalizer-sound-custom", t!("settings-equalizer-custom"))
            .selected(on_scratch)
            .on_click(move |_, _, cx| {
                equalizer.update(cx, |equalizer, cx| equalizer.play_profile(index, cx));
            })
    });
    let equalizer = equalizer.clone();
    let sounds = sound_items(
        settings,
        |sound| match sound {
            Sound::Preset(preset) => on_scratch && shaped == Some(Some(*preset)),
            Sound::Profile(name) => !on_scratch && settings.equalizer_profile_name() == name,
        },
        move |sound, cx| {
            equalizer.update(cx, |equalizer, cx| equalizer.play_sound(sound, cx));
        },
    );

    Picker::new(key, popovers, current)
        .scroll_after(height)
        .items(custom_item)
        .items(sounds)
}

/// A menu item for each built-in preset, then one for each named profile after a divider. The
/// unnamed profile is left out. `selected` marks an item and `pick` runs when one is clicked.
pub(crate) fn sound_items(
    settings: &AppSettings,
    selected: impl Fn(&Sound) -> bool,
    pick: impl Fn(&Sound, &mut App) + 'static,
) -> Vec<MenuItem> {
    let pick = Rc::new(pick);
    let item = |id: gpui::ElementId, sound: Sound| {
        let pick = pick.clone();
        MenuItem::new(id, sound_label(&sound))
            .selected(selected(&sound))
            .on_click(move |_, _, cx| pick(&sound, cx))
    };
    let presets = Preset::ALL.map(|preset| item(preset.id().into(), Sound::Preset(preset)));
    let named: Vec<_> = settings
        .equalizer_profiles()
        .enumerate()
        .filter(|(_, name)| !name.is_empty())
        .map(|(index, name)| {
            item(
                ("equalizer-profile", index).into(),
                Sound::Profile(name.to_owned()),
            )
        })
        .collect();
    let divider = (!named.is_empty()).then(|| MenuItem::separator("equalizer-sound-profiles"));
    presets.into_iter().chain(divider).chain(named).collect()
}

/// What a sound is called: a preset's name in the interface's language, or a profile's own.
pub(crate) fn sound_label(sound: &Sound) -> SharedString {
    match sound {
        Sound::Preset(preset) => i18n::lookup(preset_key(*preset), None),
        Sound::Profile(name) => SharedString::from(name.clone()),
    }
}

/// The Fluent key naming a built-in preset.
pub(crate) fn preset_key(preset: Preset) -> &'static str {
    match preset {
        Preset::Flat => "settings-equalizer-flat",
        Preset::BassBoost => "settings-equalizer-bass-boost",
        Preset::BassReducer => "settings-equalizer-bass-reducer",
        Preset::TrebleBoost => "settings-equalizer-treble-boost",
        Preset::Vocal => "settings-equalizer-vocal",
        Preset::Rock => "settings-equalizer-rock",
        Preset::Pop => "settings-equalizer-pop",
        Preset::Jazz => "settings-equalizer-jazz",
        Preset::Classical => "settings-equalizer-classical",
        Preset::Electronic => "settings-equalizer-electronic",
        Preset::Acoustic => "settings-equalizer-acoustic",
        Preset::Loudness => "settings-equalizer-loudness",
    }
}
