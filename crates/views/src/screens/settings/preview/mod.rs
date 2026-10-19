//! The previews in the sticky column beside the pages where seeing a setting beats reading it.
//! Each page with one has a file here holding the preview's own state and the function that
//! draws it. The page puts whatever that function returns in the preview column, which stands
//! beside the content on a wide window and moves above it on a narrow one.

use super::SettingsView;

pub(super) mod discord;
pub(super) mod lyrics;
pub(super) mod now_playing;
pub(super) mod sound;

/// The state every preview keeps between frames, one field per preview, each owned by its file.
pub(super) struct Previews {
    pub(super) now_playing: now_playing::NowPlayingPreview,
    pub(super) lyrics: lyrics::LyricsPreview,
    pub(super) sound: sound::SoundPreview,
    pub(super) discord: discord::DiscordPreview,
}

impl Previews {
    pub(super) fn new(cx: &mut gpui::Context<SettingsView>) -> Self {
        Self {
            now_playing: now_playing::NowPlayingPreview::new(cx),
            lyrics: lyrics::LyricsPreview::new(cx),
            sound: sound::SoundPreview::new(cx),
            discord: discord::DiscordPreview::new(cx),
        }
    }
}
