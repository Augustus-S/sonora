use gpui::{AnyElement, App, Hsla, Window, rgb};
use ui::{ActiveTheme as _, Hover};

/// Discord's brand blurple.
const BLURPLE: u32 = 0x5865f2;

/// The Discord tile's art, drawn for one frame of `hover`.
pub(super) fn art(hover: Hover, _window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    super::placeholder(theme.primary.opacity(0.45), theme.secondary, 145., hover)
}

/// The colour the Discord tile glows in while hovered: Discord's own blurple.
pub(super) fn glow(_cx: &App) -> Hsla {
    rgb(BLURPLE).into()
}
