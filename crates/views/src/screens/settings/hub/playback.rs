use gpui::{AnyElement, App, Hsla, Window};
use ui::{ActiveTheme as _, Hover};

/// The Playback tile's art, drawn for one frame of `hover`.
pub(super) fn art(hover: Hover, _window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    super::placeholder(theme.primary_hover.opacity(0.55), theme.muted, 120., hover)
}

/// The colour the Playback tile glows in while hovered: the tint of the playing cover.
pub(super) fn glow(cx: &App) -> Hsla {
    super::tint(cx)
}
