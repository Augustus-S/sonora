use gpui::{AnyElement, App, Hsla, Window};
use ui::{ActiveTheme as _, Hover};

/// The Now playing tile's art, drawn for one frame of `hover`.
pub(super) fn art(hover: Hover, _window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    super::placeholder(
        theme.progress_bar.opacity(0.6),
        theme.background,
        200.,
        hover,
    )
}

/// The colour the Now playing tile glows in while hovered: the tint of the playing cover.
pub(super) fn glow(cx: &App) -> Hsla {
    super::tint(cx)
}
