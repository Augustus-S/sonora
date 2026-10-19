use gpui::{AnyElement, App, Hsla, Window};
use ui::{ActiveTheme as _, Hover};

/// The Lyrics tile's art, drawn for one frame of `hover`.
pub(super) fn art(hover: Hover, _window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    super::placeholder(theme.foreground.opacity(0.22), theme.secondary, 150., hover)
}

/// The colour the Lyrics tile glows in while hovered: the tint of the playing cover.
pub(super) fn glow(cx: &App) -> Hsla {
    super::tint(cx)
}
