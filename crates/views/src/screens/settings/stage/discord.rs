use gpui::{AnyElement, Context, Window};

use crate::screens::settings::SettingsView;

/// What the Discord page preview keeps between frames.
pub(in crate::screens::settings) struct DiscordStage {}

impl DiscordStage {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {}
    }
}

/// The preview at the top of the Discord page,
/// showing the status card the Discord settings shape.
/// `state` is this preview's own, kept on the view. `None` leaves the page without one.
pub(in crate::screens::settings) fn stage(
    _state: &DiscordStage,
    _view: &SettingsView,
    _window: &mut Window,
    _cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    None
}
