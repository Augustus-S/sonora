use gpui::{App, Entity, SharedString};
use music::equalizer::Preset;
use state::{AppSettings, Playback, equalizer_profile_label};
use ui::{MenuItem, Picker, Popovers};

/// The equalizer profiles to choose from, showing the one that plays. Choosing one plays it.
pub(crate) fn profile_picker(
    key: &'static str,
    popovers: &Popovers,
    playback: &Entity<Playback>,
    settings: &Entity<AppSettings>,
    cx: &App,
) -> Picker {
    let settings = settings.read(cx);
    let chosen = settings.equalizer_profile();
    let names: Vec<SharedString> = settings
        .equalizer_profiles()
        .map(equalizer_profile_label)
        .collect();
    let current = names.get(chosen).cloned().unwrap_or_default();
    Picker::new(key, popovers, current).items(names.into_iter().enumerate().map(|(index, name)| {
        let playback = playback.clone();
        MenuItem::new(("equalizer-profile", index), name)
            .selected(index == chosen)
            .on_click(move |_, _, cx| {
                playback.update(cx, |playback, cx| {
                    playback.select_equalizer_profile(index, cx)
                });
            })
    }))
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
