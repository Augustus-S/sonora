use gpui::{AnyElement, App, Hsla, Window};
use ui::{ActiveTheme as _, Hover};

/// The Sound tile's art, drawn for one frame of `hover`.
pub(super) fn art(hover: Hover, _window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    super::placeholder(theme.selection, theme.background, 170., hover)
}

/// The colour the Sound tile glows in while hovered: the foreground.
pub(super) fn glow(cx: &App) -> Hsla {
    cx.theme().foreground
}
