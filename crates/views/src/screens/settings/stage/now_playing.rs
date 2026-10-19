use gpui::{AnyElement, Context, Window};

use crate::screens::settings::SettingsView;

/// What the Now playing page preview keeps between frames.
pub(in crate::screens::settings) struct NowPlayingStage {}

impl NowPlayingStage {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {}
    }
}

/// The preview at the top of the Now playing page,
/// showing the ambient background and the visualizer.
/// `state` is this preview's own, kept on the view. `None` leaves the page without one.
pub(in crate::screens::settings) fn stage(
    _state: &NowPlayingStage,
    _view: &SettingsView,
    _window: &mut Window,
    _cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    None
}
