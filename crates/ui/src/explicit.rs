use crate::metrics::snapped;
use crate::theme::ActiveTheme as _;
use gpui::prelude::*;
use gpui::{App, Div, IntoElement, Pixels, RenderOnce, StyleRefinement, Window, div, rems};

/// The badge's side and the letter's size, in rems.
const SIDE: f32 = 1.;
const LETTER: f32 = 0.75;

/// The square "E" a track marked explicit carries. The letter sits at the optical centre of the
/// square, an equal gap above and below its capital, rather than where the font's line box puts
/// it, which leaves room for descenders the letter does not have.
#[derive(IntoElement)]
pub struct ExplicitBadge {
    base: Div,
}

impl ExplicitBadge {
    pub fn new() -> Self {
        Self { base: div() }
    }
}

impl Default for ExplicitBadge {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for ExplicitBadge {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for ExplicitBadge {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { mut base } = self;
        let theme = *cx.theme();
        let overrides = std::mem::take(base.style());
        let side = rems(SIDE).to_pixels(window.rem_size());

        let mut badge = base
            .size(rems(SIDE))
            .flex()
            .items_center()
            .justify_center()
            .text_size(rems(LETTER))
            .line_height(rems(SIDE))
            .text_color(theme.muted_foreground)
            .bg(theme.muted)
            .rounded_xs()
            .child(
                div()
                    .relative()
                    .top(capital_offset(side, window))
                    .child("E"),
            );
        badge.style().refine(&overrides);
        badge
    }
}

/// How far a capital letter in a line `side` tall has to move down to sit in the middle of it,
/// snapped to the device's pixels. The line box centres the font's ascent and descent, so a
/// capital, which has no descender, sits above the middle by the difference. The font's metrics
/// carry the descent negated where the laid out line has it positive, hence the `abs`.
fn capital_offset(side: Pixels, window: &Window) -> Pixels {
    let size = rems(LETTER).to_pixels(window.rem_size());
    let text = window.text_system();
    let font = text.resolve_font(&window.text_style().font());
    let ascent = text.ascent(font, size);
    let descent = text.descent(font, size).abs();
    let baseline = (side - ascent - descent) / 2. + ascent;
    let middle = baseline - text.cap_height(font, size) / 2.;
    snapped(side / 2. - middle, window)
}
