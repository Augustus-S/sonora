use gpui::{AnyElement, Context, Window};

use crate::screens::settings::SettingsView;

/// What the Sound page preview keeps between frames.
pub(in crate::screens::settings) struct SoundStage {}

impl SoundStage {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {}
    }
}

/// The preview at the top of the Sound page,
/// showing the signal path from the source to the device.
/// `state` is this preview's own, kept on the view. `None` leaves the page without one.
pub(in crate::screens::settings) fn stage(
    _state: &SoundStage,
    _view: &SettingsView,
    _window: &mut Window,
    _cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    None
}
