use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Hsla, PathBuilder, Pixels, Point, Window, canvas, div,
    linear_color_stop, linear_gradient, point, px,
};
use ui::{ActiveTheme as _, Hover, Theme, ThemeKind};

/// How wide a band is, how far its foot leans back from its head, and how wide the accent
/// stripe along its leading edge is, in units.
const BAND: f32 = 0.27;
const LEAN: f32 = 0.42;
const STRIPE: f32 = 0.035;
/// How far the bands start to the left of the tile, in units, so the lean never bares a corner.
const BEFORE: f32 = 0.6;
/// How fast the bands slide while hovered, in units a second.
const SLIDE: f32 = 0.22;
/// How soft the glow the stripes cast over their neighbours is, in units, and how strong.
const GLOW_BLUR: f32 = 0.09;
const GLOW: f32 = 0.85;
/// How much darker each band grows towards its foot.
const SHADE: f32 = 0.82;
/// How many times the themes repeat across the tile, enough to fill the widest one.
const REPEATS: usize = 3;

/// The Appearance tile's art: every built-in theme as a tall leaning band of its background with
/// its accent along the leading edge. While hovered the bands slide sideways without end.
pub(super) fn art(hover: Hover, window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let themes: Vec<(Hsla, Hsla)> = ThemeKind::ALL
        .into_iter()
        .filter(|kind| *kind != ThemeKind::System)
        .map(Theme::for_kind)
        .map(|theme| (theme.background, theme.primary))
        .collect();
    let width = (unit * BAND).round();
    let span = width * themes.len() as f32;
    let moving = super::moving(hover, cx);
    let offset = (hover.clock * SLIDE * unit).rem_euclid(span) - (unit * BEFORE).round() - span;
    let (left, fraction) = super::adrift(offset, moving, window);
    let lean = (unit * LEAN).round();
    let stripe = (unit * STRIPE).max(2.).round();

    let bands = {
        let themes = themes.clone();
        move |bounds: Bounds<Pixels>, window: &mut Window| {
            for (index, (background, _)) in themes
                .iter()
                .cycle()
                .take(themes.len() * REPEATS)
                .enumerate()
            {
                let x = bounds.origin.x + px(index as f32 * width);
                band(bounds, x, px(width), px(lean), *background, window);
            }
        }
    };
    let stripes = move |bounds: Bounds<Pixels>, window: &mut Window, glow: bool| {
        for (index, (_, accent)) in themes
            .iter()
            .cycle()
            .take(themes.len() * REPEATS)
            .enumerate()
        {
            let x = bounds.origin.x + px(index as f32 * width);
            let color = match glow {
                true => accent.opacity(GLOW),
                false => *accent,
            };
            let corners = slant(bounds, x, px(stripe), px(lean));
            fill(&corners, color, window);
        }
    };
    let glows = stripes.clone();
    let layer = |child: AnyElement| {
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(left)
            .w(px(span * REPEATS as f32))
            .when(moving, |layer| {
                layer.layer_translate(point(fraction, px(0.)))
            })
            .child(child)
    };

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .child(layer(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| bands(bounds, window),
            )
            .absolute()
            .inset_0()
            .into_any_element(),
        ))
        .child(layer(
            div()
                .absolute()
                .inset_0()
                .blur(px(unit * GLOW_BLUR))
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| glows(bounds, window, true),
                    )
                    .size_full(),
                )
                .into_any_element(),
        ))
        .child(layer(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| stripes(bounds, window, false),
            )
            .absolute()
            .inset_0()
            .into_any_element(),
        ))
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the Appearance tile glows in while hovered: the accent.
pub(super) fn glow(cx: &App) -> Hsla {
    cx.theme().primary
}

/// One band of `color`, `width` wide with its head at `x`, darkening towards its foot.
fn band(
    bounds: Bounds<Pixels>,
    x: Pixels,
    width: Pixels,
    lean: Pixels,
    color: Hsla,
    window: &mut Window,
) {
    let corners = slant(bounds, x, width, lean);
    let foot = Hsla {
        l: color.l * SHADE,
        ..color
    };
    let mut builder = PathBuilder::fill();
    builder.add_polygon(&corners, true);
    if let Ok(path) = builder.build() {
        window.paint_path(
            path,
            linear_gradient(
                180.,
                linear_color_stop(color, 0.),
                linear_color_stop(foot, 1.),
            ),
        );
    }
}

/// The four corners of a parallelogram `width` wide whose head starts at `x` on the top of
/// `bounds` and whose foot sits `lean` further left on the bottom.
fn slant(bounds: Bounds<Pixels>, x: Pixels, width: Pixels, lean: Pixels) -> [Point<Pixels>; 4] {
    let top = bounds.origin.y;
    let bottom = bounds.origin.y + bounds.size.height;
    [
        point(x + lean, top),
        point(x + lean + width, top),
        point(x + width, bottom),
        point(x, bottom),
    ]
}

/// Fills the polygon `corners` with `color`.
fn fill(corners: &[Point<Pixels>], color: Hsla, window: &mut Window) {
    let mut builder = PathBuilder::fill();
    builder.add_polygon(corners, true);
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}
