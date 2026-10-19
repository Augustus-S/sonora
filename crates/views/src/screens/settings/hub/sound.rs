use std::f32::consts::TAU;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Hsla, PathBuilder, Pixels, Point, Window, canvas, div,
    linear_color_stop, linear_gradient, point, px,
};
use music::equalizer::{Curve, Filter};
use state::Sonora;
use ui::{ActiveTheme as _, Hover};

/// The rate the curve is read at.
const RATE: u32 = 48_000;
/// The lowest frequency drawn and how many decades the tile spans from it.
const LOWEST: f64 = 20.;
const DECADES: f64 = 3.;
/// How many points the curve is read at across the tile.
const POINTS: usize = 96;
/// The frequencies the faint grid marks, an octave apart.
const OCTAVES: [f64; 10] = [
    31.5, 63., 125., 250., 500., 1_000., 2_000., 4_000., 8_000., 16_000.,
];
/// How far up the tile the 0 dB line sits, as a fraction of its height, and how many decibels
/// reach a unit up or down from it.
const LEVEL: f32 = 0.4;
const SPAN_DB: f32 = 16.;
/// How far each band sways around its value while hovered, in decibels, and how long one sway
/// takes at the slowest, in seconds.
const SWAY_DB: f64 = 2.2;
const SWAY: f32 = 3.2;
/// How wide a band's handle on the curve is, in pixels.
const NODE: f32 = 6.;
/// The line, the glow under it and how soft that glow is, in units.
const LINE: f32 = 2.;
const GLOW_LINE: f32 = 7.;
const GLOW_BLUR: f32 = 0.08;
/// How strongly the grid, the 0 dB line and the fill under the curve show.
const GRID: f32 = 0.06;
const ZERO: f32 = 0.14;
const FILL: f32 = 0.22;
/// How faint the curve is while the equalizer is off.
const BYPASSED: f32 = 0.45;

/// The Sound tile's art: the equalizer's own curve as a glowing line over a faint grid of
/// octaves. While hovered the curve breathes as each band sways gently around its value.
pub(super) fn art(hover: Hover, _window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let theme = *cx.theme();
    let equalizer = Sonora::global(cx).equalizer.read(cx);
    let on = equalizer.enabled();
    let curve = swayed(equalizer.curve(), hover.clock);
    let response = curve.response(RATE);
    let samples: Vec<(f32, f32)> = (0..POINTS)
        .map(|index| {
            let along = index as f64 / (POINTS - 1) as f64;
            let frequency = LOWEST * 10f64.powf(along * DECADES);
            (along as f32, response.db(frequency) as f32)
        })
        .collect();
    let strength = match on {
        true => 1.,
        false => BYPASSED,
    };
    let tint = super::cover_tint(cx);
    let accent = Hsla {
        l: tint.l.max(0.6),
        s: tint.s.max(0.7),
        ..tint
    };
    let line = theme.foreground.opacity(strength);
    let glow = accent.opacity(strength);
    let fill = accent.opacity(FILL * strength);
    let grid = theme.foreground.opacity(GRID);
    let zero = theme.foreground.opacity(ZERO);
    let reach = unit / SPAN_DB;

    // Each band's handle sits on the curve at the band's frequency, as the equalizer draws it.
    let nodes: Vec<f32> = curve
        .filters()
        .iter()
        .filter(|filter| filter.enabled())
        .map(|filter| across(filter.frequency()))
        .filter(|along| (0. ..=1.).contains(along))
        .collect();
    let node = theme.foreground.opacity(strength);
    let plotted = samples.clone();
    let glowing = samples.clone();
    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .child(super::halo(
            accent.opacity(0.18 * strength),
            unit * 1.8,
            unit * 0.4,
            unit * 2.4,
            unit * 0.5,
        ))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    for frequency in OCTAVES {
                        let x = bounds.origin.x + bounds.size.width * across(frequency);
                        stroke(
                            &[point(x, bounds.origin.y), point(x, bounds.bottom())],
                            px(1.),
                            grid,
                            window,
                        );
                    }
                    let y = bounds.origin.y + bounds.size.height * LEVEL;
                    stroke(
                        &[point(bounds.origin.x, y), point(bounds.right(), y)],
                        px(1.),
                        zero,
                        window,
                    );
                    let points = project(&samples, bounds, reach);
                    area(&points, bounds, fill, window);
                },
            )
            .absolute()
            .inset_0(),
        )
        .child(
            div().absolute().inset_0().blur(px(unit * GLOW_BLUR)).child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let points = project(&glowing, bounds, reach);
                        stroke(&points, px(GLOW_LINE), glow, window);
                    },
                )
                .size_full(),
            ),
        )
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let points = project(&plotted, bounds, reach);
                    stroke(&points, px(LINE), line, window);
                    for along in &nodes {
                        let at = (along * (POINTS - 1) as f32).round() as usize;
                        let Some(point) = points.get(at) else {
                            continue;
                        };
                        let radius = px(NODE / 2.);
                        window.paint_quad(
                            gpui::fill(
                                Bounds::new(
                                    *point - gpui::point(radius, radius),
                                    gpui::size(radius * 2., radius * 2.),
                                ),
                                node,
                            )
                            .corner_radii(radius),
                        );
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the Sound tile glows in while hovered: the foreground.
pub(super) fn glow(cx: &App) -> Hsla {
    cx.theme().foreground
}

/// `curve` with each filter's gain swayed around its value as `clock` runs, every band on its
/// own beat. At rest the sway is already partway, so even a flat curve has a little life.
fn swayed(curve: &Curve, clock: f32) -> Curve {
    let filters: Vec<Filter> = curve
        .filters()
        .iter()
        .enumerate()
        .map(|(index, filter)| {
            let beat = TAU * clock / (SWAY * (1. + index as f32 * 0.17)) + index as f32 * 1.9;
            let gain = filter.gain() + SWAY_DB * f64::from(beat.sin());
            Filter::new(
                filter.kind(),
                filter.frequency(),
                gain,
                filter.width(),
                filter.enabled(),
            )
            .unwrap_or(*filter)
        })
        .collect();
    Curve::new(curve.preamp(), filters).unwrap_or_else(|_| curve.clone())
}

/// Where across the tile `frequency` falls, from 0 at the left edge to 1 at the right.
fn across(frequency: f64) -> f32 {
    ((frequency / LOWEST).log10() / DECADES) as f32
}

/// The samples, each a place across the tile and a gain, as points in `bounds`, `reach` pixels
/// a decibel from the 0 dB line.
fn project(samples: &[(f32, f32)], bounds: Bounds<Pixels>, reach: f32) -> Vec<Point<Pixels>> {
    let zero = bounds.origin.y + bounds.size.height * LEVEL;
    samples
        .iter()
        .map(|(along, db)| {
            point(
                bounds.origin.x + bounds.size.width * *along,
                zero - px(db * reach),
            )
        })
        .collect()
}

/// Strokes the line through `points`, `width` wide, in `color`.
fn stroke(points: &[Point<Pixels>], width: Pixels, color: Hsla, window: &mut Window) {
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    let mut builder = PathBuilder::stroke(width);
    builder.move_to(*first);
    for point in rest {
        builder.line_to(*point);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Fills under the line through `points` down to the bottom of `bounds`, fading as it falls.
fn area(points: &[Point<Pixels>], bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return;
    };
    let mut builder = PathBuilder::fill();
    builder.move_to(*first);
    for point in &points[1..] {
        builder.line_to(*point);
    }
    builder.line_to(point(last.x, bounds.bottom()));
    builder.line_to(point(first.x, bounds.bottom()));
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(
            path,
            linear_gradient(
                180.,
                linear_color_stop(color, 0.),
                linear_color_stop(color.opacity(0.), 0.75),
            ),
        );
    }
}
