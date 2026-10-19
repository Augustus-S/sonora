use gpui::prelude::*;
use gpui::{
    AnyElement, App, Div, Hsla, Window, div, linear_color_stop, linear_gradient, point, px, rgb,
    svg,
};
use state::Sonora;
use ui::{ActiveTheme as _, Hover};

use crate::shared::provider_logo;

/// The brand colours of the providers' marks.
const SPOTIFY: u32 = 0x1ed760;
const YOUTUBE: u32 = 0xff0033;
const DEEZER: u32 = 0xa238ff;
const APPLE: u32 = 0xfa2d48;
const SUBSONIC: u32 = 0xffa31a;
/// A chip holding one mark, its mark, and the room between two chips, in units.
const CHIP: f32 = 0.42;
const MARK: f32 = 0.21;
const GAP: f32 = 0.14;
/// Where the front row's top sits, and how far below it the back row starts, in units.
const FRONT_TOP: f32 = 0.13;
const BACK_DROP: f32 = 0.5;
/// The back row is smaller, fainter and softer than the front, so the two read as depths.
const BACK_SCALE: f32 = 0.78;
const BACK_FADE: f32 = 0.6;
const BACK_BLUR: f32 = 0.006;
/// How strongly a mark that is not signed in shows, against the signed-in one at full.
const RESTING: f32 = 0.62;
/// How fast each row drifts while hovered, in units a second. The nearer row moves faster.
const FRONT_DRIFT: f32 = 0.16;
const BACK_DRIFT: f32 = 0.09;
/// How far in from either side the rows fade out, in units.
const FADE: f32 = 0.3;
/// How many times the providers repeat along a row, enough to fill the widest tile.
const REPEATS: usize = 4;

/// The Providers tile's art: every provider's mark in its brand colour, on two staggered rows
/// that drift apart in opposite directions while hovered, with the signed-in one glowing.
pub(super) fn art(hover: Hover, window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let marks: Vec<(&'static str, bool)> = Sonora::global(cx)
        .session
        .read(cx)
        .providers()
        .map(|provider| (provider.slug, provider.active))
        .collect();
    let moving = super::moving(hover, cx);
    let wash = glow(cx);

    let front = Row {
        scale: 1.,
        shift: 0,
        drift: hover.clock * FRONT_DRIFT,
    };
    let back = Row {
        scale: BACK_SCALE,
        shift: 1,
        drift: -hover.clock * BACK_DRIFT,
    };
    let front = front
        .draw(&marks, unit, moving, window, cx)
        .top(px((unit * FRONT_TOP).round()));
    let back = back
        .draw(&marks, unit, moving, window, cx)
        .top(px((unit * (FRONT_TOP + BACK_DROP)).round()))
        .opacity(BACK_FADE)
        .blur(px(unit * BACK_BLUR));

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .child(super::halo(
            wash.opacity(0.22),
            unit * 0.3,
            unit * 0.1,
            unit * 1.6,
            unit * 0.4,
        ))
        .child(
            div()
                .absolute()
                .inset_0()
                .fade_sides(px(unit * FADE), px(unit * FADE))
                .child(back)
                .child(front),
        )
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the Providers tile glows in while hovered: the signed-in provider's brand colour,
/// or the accent while none is.
pub(super) fn glow(cx: &App) -> Hsla {
    let active = Sonora::global(cx)
        .session
        .read(cx)
        .providers()
        .find(|provider| provider.active)
        .map(|provider| provider.slug);
    match active {
        Some(slug) => brand(slug, cx),
        None => cx.theme().primary,
    }
}

/// One row of chips: how large it is against the front row, how many places along the providers
/// start so the rows stagger, and how far it has drifted sideways, in units.
struct Row {
    scale: f32,
    shift: usize,
    drift: f32,
}

impl Row {
    /// The row laid out from its left edge, wrapping round so it never runs out whichever way
    /// it drifts.
    fn draw(
        &self,
        marks: &[(&'static str, bool)],
        unit: f32,
        moving: bool,
        window: &Window,
        cx: &App,
    ) -> Div {
        let gap = (unit * GAP * self.scale).round();
        // The pitch is what layout spaces the chips by, so wrapping round lands on a chip.
        let pitch = (unit * CHIP * self.scale).round() + gap;
        let span = pitch * marks.len().max(1) as f32;
        // The row starts a whole span to the left, so drifting either way never bares its start.
        let offset = (self.drift * unit).rem_euclid(span) - span - pitch * 0.5 * self.shift as f32;
        let (left, fraction) = super::adrift(offset, moving, window);
        let count = marks.len() * REPEATS;
        // A layer holds only what is painted inside it, so the row reaches past its chips far
        // enough to carry the signed-in one's glow while it drifts.
        let room = (unit * CHIP * self.scale).round();

        div()
            .absolute()
            .left(left)
            .mt(px(-room))
            .py(px(room))
            .when(moving, |row| row.layer_translate(point(fraction, px(0.))))
            .flex()
            .gap(px(gap))
            .children((0..count).filter_map(|index| {
                let (slug, active) = *marks.get((index + self.shift) % marks.len().max(1))?;
                Some(chip(slug, active, unit * self.scale, cx))
            }))
    }
}

/// One provider's mark on a chip tinted in its brand colour, with a glow behind it when it is
/// the one signed in.
fn chip(slug: &'static str, active: bool, unit: f32, cx: &App) -> Div {
    let theme = cx.theme();
    let color = brand(slug, cx);
    let size = (unit * CHIP).round();
    let strength = match active {
        true => 1.,
        false => RESTING,
    };

    div()
        .relative()
        .flex_none()
        .size(px(size))
        .when(active, |chip| {
            chip.child(super::halo(
                color.opacity(0.55),
                size / 2.,
                size / 2.,
                size * 1.5,
                size * 0.35,
            ))
        })
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radius * 1.5)
                .bg(linear_gradient(
                    160.,
                    linear_color_stop(color.opacity(0.3 * strength), 0.),
                    linear_color_stop(color.opacity(0.08 * strength), 1.),
                ))
                .border_1()
                .border_color(color.opacity(0.36 * strength))
                .shadow_md()
                .child(
                    svg()
                        .path(icons::path(provider_logo(slug)))
                        .size(px((unit * MARK).round()))
                        .text_color(color.opacity(strength)),
                ),
        )
}

/// The brand colour of the provider `slug`. Tidal's mark is plain black or white, so it takes
/// the foreground.
pub(super) fn brand(slug: &str, cx: &App) -> Hsla {
    match slug {
        "spotify" => rgb(SPOTIFY).into(),
        "youtube" => rgb(YOUTUBE).into(),
        "deezer" => rgb(DEEZER).into(),
        "apple" => rgb(APPLE).into(),
        "subsonic" => rgb(SUBSONIC).into(),
        _ => cx.theme().foreground,
    }
}
