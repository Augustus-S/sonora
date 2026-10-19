use std::f32::consts::TAU;

use gpui::prelude::*;
use gpui::{AnyElement, App, Hsla, Window, div, px, relative};
use ui::{ActiveTheme as _, Hover};

/// The app mark's disc, and where its centre sits down the tile, in units.
const DISC: f32 = 0.56;
const MIDDLE: f32 = 0.5;
/// The mark's five bars, as in the app icon: each one's place across the disc and its height,
/// as fractions of the disc, and how thick they are.
const BARS: [(f32, f32); 5] = [
    (0.208, 0.146),
    (0.354, 0.365),
    (0.5, 0.583),
    (0.646, 0.365),
    (0.792, 0.146),
];
const BAR: f32 = 0.034;
/// The glow behind the mark: the wide soft one and the tight bright one, each a size and a
/// blur in units and a strength.
const WIDE: (f32, f32, f32) = (1.7, 0.42, 0.42);
const TIGHT: (f32, f32, f32) = (0.85, 0.16, 0.5);
/// How long one breath of the glow takes, in seconds, and how far it swells.
const BREATH: f32 = 3.2;
const SWELL: f32 = 0.16;
/// How far each bar sways while hovered, as a share of its height, and how fast.
const SWAY: f32 = 0.14;
const SWAY_RATE: f32 = 1.1;
/// How quickly the sway grows from nothing once the hover starts, so the bars ease into it.
const RISE: f32 = 2.5;

/// The About tile's art: the Sonora mark centred over a radial glow of the tint. While hovered
/// the glow breathes and the mark's bars sway like the sound it is drawn from.
pub(super) fn art(hover: Hover, _window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let theme = *cx.theme();
    let tint = super::cover_tint(cx);
    let breath = 0.5 - 0.5 * (TAU * hover.clock / BREATH).cos();
    let disc = (unit * DISC).round();
    let center = unit * MIDDLE;
    let swell = 1. + SWELL * breath;
    let moving = super::moving(hover, cx);

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left(relative(0.5))
                .top_0()
                .child(super::halo(
                    tint.opacity(WIDE.2 * (0.85 + 0.15 * breath)),
                    0.,
                    center,
                    unit * WIDE.0,
                    unit * WIDE.1,
                ))
                .child(match moving {
                    true => super::halo(
                        tint.opacity(TIGHT.2),
                        0.,
                        center,
                        unit * TIGHT.0,
                        unit * TIGHT.1,
                    )
                    .layer_scale(swell),
                    false => super::halo(
                        tint.opacity(TIGHT.2),
                        0.,
                        center,
                        (unit * TIGHT.0 * swell).round(),
                        unit * TIGHT.1,
                    ),
                })
                .child(
                    div()
                        .absolute()
                        .left(px(-disc / 2.))
                        .top(px((center - disc / 2.).round()))
                        .size(px(disc))
                        .rounded_full()
                        .bg(theme.foreground)
                        .shadow_lg()
                        .children(BARS.iter().enumerate().map(|(index, (across, tall))| {
                            // The sway is still where the clock stopped once the pointer
                            // leaves, so a bar lays out at its swayed height at rest and is
                            // scaled as a layer only while it moves.
                            let sway = 1.
                                + SWAY
                                    * (1. - (-hover.clock * RISE).exp())
                                    * (TAU * SWAY_RATE * hover.clock - index as f32 * 0.9).sin();
                            let width = (disc * BAR).max(2.).round();
                            let height = match moving {
                                true => (disc * tall).round(),
                                false => (disc * tall * sway).round(),
                            };
                            div()
                                .absolute()
                                .left(px((disc * across - width / 2.).round()))
                                .top(px(((disc - height) / 2.).round()))
                                .w(px(width))
                                .h(px(height))
                                .when(moving, |bar| bar.layer_scale(sway))
                                .rounded_full()
                                .bg(theme.background)
                        })),
                ),
        )
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the About tile glows in while hovered: the tint of the playing cover.
pub(super) fn glow(cx: &App) -> Hsla {
    super::cover_tint(cx)
}
