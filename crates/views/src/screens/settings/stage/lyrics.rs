use gpui::{AnyElement, Context, Window};

use crate::screens::settings::SettingsView;

/// What the Lyrics page preview keeps between frames.
pub(in crate::screens::settings) struct LyricsStage {}

impl LyricsStage {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {}
    }
}

/// The preview at the top of the Lyrics page,
/// showing the karaoke, blur, romanization and text size settings.
/// `state` is this preview's own, kept on the view. `None` leaves the page without one.
pub(in crate::screens::settings) fn stage(
    _state: &LyricsStage,
    _view: &SettingsView,
    _window: &mut Window,
    _cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    None
}
