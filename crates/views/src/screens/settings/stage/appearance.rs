use gpui::{AnyElement, Context, Window};

use crate::screens::settings::SettingsView;

/// What the Appearance page preview keeps between frames.
pub(in crate::screens::settings) struct AppearanceStage {}

impl AppearanceStage {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {}
    }
}

/// The preview at the top of the Appearance page,
/// showing the theme, corners, font and icon pack.
/// `state` is this preview's own, kept on the view. `None` leaves the page without one.
pub(in crate::screens::settings) fn stage(
    _state: &AppearanceStage,
    _view: &SettingsView,
    _window: &mut Window,
    _cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    None
}
