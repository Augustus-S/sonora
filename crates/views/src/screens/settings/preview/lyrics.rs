use gpui::{AnyElement, Context, Window};

use crate::screens::settings::SettingsView;

/// What the Lyrics page's preview keeps between frames.
pub(in crate::screens::settings) struct LyricsPreview {}

impl LyricsPreview {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {}
    }
}

/// The Lyrics page's preview, the lyrics drawn the way the panel and Now playing draw them.
/// `state` is this preview's own, kept on the view, and `None` leaves the page without a
/// preview column.
pub(in crate::screens::settings) fn preview(
    _state: &LyricsPreview,
    _view: &SettingsView,
    _window: &mut Window,
    _cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    None
}
