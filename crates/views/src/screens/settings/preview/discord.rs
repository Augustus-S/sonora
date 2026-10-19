use gpui::{AnyElement, Context, Window};

use crate::screens::settings::SettingsView;

/// What the Discord page's preview keeps between frames.
pub(in crate::screens::settings) struct DiscordPreview {}

impl DiscordPreview {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {}
    }
}

/// The Discord page's preview, the card Discord shows for the playing track.
/// `state` is this preview's own, kept on the view, and `None` leaves the page without one.
pub(in crate::screens::settings) fn preview(
    _state: &DiscordPreview,
    _view: &SettingsView,
    _window: &mut Window,
    _cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    None
}
