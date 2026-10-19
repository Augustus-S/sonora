//! The live previews at the top of the pages where seeing a setting beats reading it. Each page
//! with one has a file here holding the preview's own state and the function that draws it. The
//! page puts whatever that function returns in the shared frame, so a preview only fills it.

use gpui::prelude::*;
use gpui::{AnyElement, App, div};
use ui::{ActiveTheme as _, InfoCard};

use super::SettingsView;

pub(super) mod appearance;
pub(super) mod discord;
pub(super) mod lyrics;
pub(super) mod now_playing;
pub(super) mod sound;

/// The state every preview keeps between frames, one field per preview, each owned by its file.
pub(super) struct Stages {
    pub(super) appearance: appearance::AppearanceStage,
    pub(super) now_playing: now_playing::NowPlayingStage,
    pub(super) lyrics: lyrics::LyricsStage,
    pub(super) sound: sound::SoundStage,
    pub(super) discord: discord::DiscordStage,
}

impl Stages {
    pub(super) fn new(cx: &mut gpui::Context<SettingsView>) -> Self {
        Self {
            appearance: appearance::AppearanceStage::new(cx),
            now_playing: now_playing::NowPlayingStage::new(cx),
            lyrics: lyrics::LyricsStage::new(cx),
            sound: sound::SoundStage::new(cx),
            discord: discord::DiscordStage::new(cx),
        }
    }
}

/// The frame a preview sits in: the group surface, as tall as two covers, clipping what it
/// holds to its rounded edge. The preview fills it whole.
pub(super) fn frame(preview: AnyElement, cx: &App) -> AnyElement {
    let theme = cx.theme();

    InfoCard::new("")
        .untitled()
        .flush()
        .relative()
        .flex_none()
        .w_full()
        .h(theme.metrics.cover * 2.)
        .py_0()
        .child(div().relative().size_full().child(preview))
        .into_any_element()
}
