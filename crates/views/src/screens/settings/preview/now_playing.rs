use gpui::{AnyElement, Context, Window};

use crate::screens::settings::SettingsView;

/// What the Now playing page's preview keeps between frames.
pub(in crate::screens::settings) struct NowPlayingPreview {}

impl NowPlayingPreview {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {}
    }
}

/// The Now playing page's preview, a miniature of the Now playing view.
/// `state` is this preview's own, kept on the view, and `None` leaves the page without one.
pub(in crate::screens::settings) fn preview(
    _state: &NowPlayingPreview,
    _view: &SettingsView,
    _window: &mut Window,
    _cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    None
}
