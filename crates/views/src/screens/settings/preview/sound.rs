use gpui::{AnyElement, Context, Window};

use crate::screens::settings::SettingsView;

/// What the Sound page's preview keeps between frames.
pub(in crate::screens::settings) struct SoundPreview {}

impl SoundPreview {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {}
    }
}

/// The Sound page's preview, the path the signal takes from the source to the device.
/// `state` is this preview's own, kept on the view, and `None` leaves the page without one.
pub(in crate::screens::settings) fn preview(
    _state: &SoundPreview,
    _view: &SettingsView,
    _window: &mut Window,
    _cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    None
}
