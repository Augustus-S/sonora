use std::f32::consts::TAU;
use std::time::Instant;

use gpui::prelude::*;
use gpui::{AnyElement, App, Hsla, Window, div, point, px};
use state::{PlaybackState, Sonora};
use ui::{ActiveTheme as _, Artwork, Hover, Levels, Visualizer, mix};

use crate::shared::ambient::{Ambient, BLOBS};

/// Each blob of the field: its centre as fractions of the tile, its size in units, the seconds
/// one turn of its drift takes, and its phase.
const FIELD: [(f32, f32, f32, f32, f32); BLOBS] = [
    (0.18, 0.2, 1.3, 9., 0.),
    (0.85, 0.15, 1.1, 7., 1.7),
    (0.55, 0.62, 1.5, 11., 3.4),
    (0.1, 0.85, 1., 8., 5.1),
    (0.9, 0.8, 0.9, 10., 2.5),
];
/// How far a blob drifts from its place while hovered, in units, and how soft the field is.
const DRIFT: f32 = 0.16;
const SOFT: f32 = 0.28;
/// How much brighter than the fullscreen field the tile's field is, since the tile's own shade
/// darkens its lower half again.
const FIELD_LIFT: f32 = 1.3;
/// The cover, centred across the tile, and how far down its top sits, in units.
const COVER: f32 = 0.6;
const COVER_TOP: f32 = 0.11;
/// How tall the visualizer reaches, in units.
const REACH: f32 = 0.8;
/// How many bands the drawn spectrum has while nothing plays.
const BANDS: usize = 24;
/// How long the drawn spectrum takes to close most of the gap to the live one, in seconds.
const SETTLE: f32 = 0.05;

/// The Now playing tile's art: the ambient field from the playing cover with the cover over it
/// and the visualizer along the bottom. While hovered the field drifts and the spectrum moves,
/// the live one while a track plays and a drawn one otherwise.
pub(super) fn art(hover: Hover, window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let theme = *cx.theme();
    let track = super::playing(cx);
    let cover = track.as_ref().and_then(|track| track.cover.clone());
    let palette = cover
        .as_deref()
        .and_then(|cover| ui::cover_palette(cover, cx));
    let mut lit = theme;
    if theme.tint.is_none() {
        lit.tint = palette.and_then(|palette| palette.primary);
        lit.tint_secondary = palette.and_then(|palette| palette.secondary);
    }
    let colors = Ambient::colors(&lit).map(|color| Hsla {
        l: (color.l * FIELD_LIFT).min(0.7),
        ..color
    });
    let style = Sonora::global(cx).settings.read(cx).visualizer_style();
    let moving = super::moving(hover, cx);
    let levels = levels(hover, window, cx);
    let line = mix(colors[1], theme.foreground, 0.75);

    let field =
        div().absolute().inset_0().bg(theme.background).child(
            div().absolute().inset_0().blur(px(unit * SOFT)).children(
                FIELD.iter().zip(colors).map(|(spec, color)| {
                    let (x, y, size, period, phase) = *spec;
                    let spin = TAU * hover.clock / period + phase;
                    let dx = unit * DRIFT * spin.sin();
                    let dy = unit * DRIFT * (spin * 0.8).cos();
                    let size = unit * size;
                    let (left, fraction_x) = super::adrift(-size / 2. + dx, moving, window);
                    let (top, fraction_y) = super::adrift(-size / 2. + dy, moving, window);
                    div()
                        .absolute()
                        .left(gpui::relative(x))
                        .top(gpui::relative(y))
                        .child(
                            div()
                                .absolute()
                                .left(left)
                                .top(top)
                                .when(moving, |blob| {
                                    blob.layer_translate(point(fraction_x, fraction_y))
                                })
                                .size(px(size))
                                .rounded_full()
                                .bg(color),
                        )
                }),
            ),
        );
    let cover_size = (unit * COVER).round();

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .child(field)
        .when(style.shown(), |art| {
            art.child(
                div().absolute().left_0().right_0().bottom_0().child(
                    Visualizer::new(levels, px(unit * REACH))
                        .style_kind(style)
                        .tint(line)
                        .behind(colors[3])
                        .w_full(),
                ),
            )
        })
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px((unit * COVER_TOP).round()))
                .flex()
                .justify_center()
                .child(
                    div().rounded(theme.radius).shadow_lg().child(
                        Artwork::new(cover)
                            .size(px(cover_size))
                            .corner_radius(theme.radius),
                    ),
                ),
        )
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the Now playing tile glows in while hovered: the tint of the playing cover.
pub(super) fn glow(cx: &App) -> Hsla {
    super::cover_tint(cx)
}

/// The spectrum drawn this frame, eased towards the live one while a track plays and the tile
/// is hovered, and towards a drawn one otherwise, so neither change jumps.
fn levels(hover: Hover, window: &mut Window, cx: &mut App) -> Levels {
    let playback = Sonora::global(cx).playback.read(cx);
    let live = match (hover.hovered, playback.state()) {
        (true, PlaybackState::Playing) => playback.spectrum(cx),
        _ => None,
    };
    let target = match live {
        Some(spectrum) => Levels {
            left: spectrum.left(),
            right: spectrum.right(),
        },
        None => drawn(hover.clock),
    };
    if !super::moving(hover, cx) {
        return target;
    }

    let eased = window.use_keyed_state("hub-now-playing-levels", cx, |_, _| Eased {
        levels: target.clone(),
        at: Instant::now(),
    });
    eased.update(cx, |eased, _| {
        let now = Instant::now();
        let step = now.duration_since(eased.at).as_secs_f32();
        eased.at = now;
        let rate = 1. - (-step / SETTLE).exp();
        ease(&mut eased.levels.left, &target.left, rate);
        ease(&mut eased.levels.right, &target.right, rate);
        eased.levels.clone()
    })
}

/// The spectrum drawn at rest and while nothing plays: strongest in the low middle, each band
/// swelling on its own slow beat as `clock` runs.
fn drawn(clock: f32) -> Levels {
    let channel = |offset: f32| {
        (0..BANDS)
            .map(|band| {
                let place = band as f32 / (BANDS - 1) as f32;
                let shape = 0.25 + 0.6 * (-((place - 0.3) / 0.35).powi(2)).exp();
                let beat = 0.5
                    + 0.5 * (clock * TAU * (0.6 + place * 0.9) + band as f32 * 1.3 + offset).sin();
                (shape * (0.6 + 0.4 * beat)).clamp(0., 1.)
            })
            .collect()
    };
    Levels {
        left: channel(0.),
        right: channel(1.1),
    }
}

/// The spectrum the tile last drew and when, so the next frame eases on from it.
struct Eased {
    levels: Levels,
    at: Instant,
}

/// Walks `shown` a share of the way towards `target`, adopting it outright when the band count
/// changes.
fn ease(shown: &mut Vec<f32>, target: &[f32], rate: f32) {
    if shown.len() != target.len() {
        *shown = target.to_vec();
        return;
    }
    for (shown, target) in shown.iter_mut().zip(target) {
        *shown += (target - *shown) * rate;
    }
}
