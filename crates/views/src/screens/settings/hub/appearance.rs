use gpui::{AnyElement, App, Hsla, Window};
use ui::{ActiveTheme as _, Hover};

/// The Appearance tile's art, drawn for one frame of `hover`.
pub(super) fn art(hover: Hover, _window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    super::placeholder(theme.primary.opacity(0.7), theme.selection, 110., hover)
}

/// The colour the Appearance tile glows in while hovered: the accent.
pub(super) fn glow(cx: &App) -> Hsla {
    cx.theme().primary
}
