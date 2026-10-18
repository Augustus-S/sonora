use gpui::{App, Entity, SharedString};
use i18n::t;
use music::equalizer::Preset;
use state::{AppSettings, Playback};
use ui::{ActiveTheme as _, MenuItem, Picker, Popovers};

/// How tall the sound menu grows before it scrolls, in cover sizes.
const MENU_HEIGHT: f32 = 2.4;

/// One picker for every sound the equalizer can play: the built-in presets, the curve someone
/// shaped from one, and the named profiles. Choosing any of them turns the equalizer on.
///
/// A preset loads into the profile with no name, so choosing one never overwrites a named
/// profile. That profile shows as the preset it matches, or as Custom once it was edited.
pub(crate) fn sound_picker(
    key: &'static str,
    popovers: &Popovers,
    playback: &Entity<Playback>,
    settings: &Entity<AppSettings>,
    cx: &App,
) -> Picker {
    let height = cx.theme().metrics.cover * MENU_HEIGHT;
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
        let playback = playback.clone();
        let index = scratch.unwrap_or_default();
        MenuItem::new("equalizer-sound-custom", t!("settings-equalizer-custom"))
            .selected(on_scratch)
            .on_click(move |_, _, cx| play_profile(&playback, index, cx))
    });
    let presets = Preset::ALL.map(|preset| {
        let playback = playback.clone();
        MenuItem::new(preset.id(), i18n::lookup(preset_key(preset), None))
            .selected(on_scratch && shaped == Some(Some(preset)))
            .on_click(move |_, _, cx| {
                playback.update(cx, |playback, cx| {
                    playback.play_equalizer_preset(preset, cx)
                });
            })
    });
    let named: Vec<_> = settings
        .equalizer_profiles()
        .enumerate()
        .filter(|(_, name)| !name.is_empty())
        .map(|(index, name)| {
            let playback = playback.clone();
            MenuItem::new(
                ("equalizer-profile", index),
                SharedString::from(name.to_owned()),
            )
            .selected(index == playing)
            .on_click(move |_, _, cx| play_profile(&playback, index, cx))
        })
        .collect();
    let divider = (!named.is_empty()).then(|| MenuItem::separator("equalizer-sound-profiles"));

    Picker::new(key, popovers, current)
        .scroll_after(height)
        .items(custom_item)
        .items(presets)
        .items(divider)
        .items(named)
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

/// Plays stored profile `index` and turns the equalizer on.
fn play_profile(playback: &Entity<Playback>, index: usize, cx: &mut App) {
    playback.update(cx, |playback, cx| {
        playback.select_equalizer_profile(index, cx);
        if !playback.equalizer() {
            playback.set_equalizer(true, cx);
        }
    });
}
