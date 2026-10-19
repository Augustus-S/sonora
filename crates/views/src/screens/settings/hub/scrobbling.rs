use gpui::{AnyElement, App, Hsla, Window};
use ui::{ActiveTheme as _, Hover};

/// The Scrobbling tile's art, drawn for one frame of `hover`.
pub(super) fn art(hover: Hover, _window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    super::placeholder(theme.danger.opacity(0.4), theme.secondary, 190., hover)
}

/// The colour the Scrobbling tile glows in while hovered: a red like Last.fm's.
pub(super) fn glow(cx: &App) -> Hsla {
    cx.theme().danger
}
