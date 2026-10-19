use gpui::prelude::*;
use gpui::{AnyElement, App, FontWeight, Hsla, Window, div, point, px, svg};
use router::{Destination, Screen};
use state::Sonora;
use ui::{ActiveTheme as _, Hover, Text, mix};

use crate::chrome::NAV;

/// How tall a row is, how far in from the left the rows start and how far down the first one
/// sits, in units.
const ROW: f32 = 0.3;
const INSET: f32 = 0.09;
const TOP: f32 = 0.08;
/// The icon on a row, the room inside the pill before it, and between it and the label, in
/// units.
const ICON: f32 = 0.15;
const PAD: f32 = 0.1;
const SPACE: f32 = 0.08;
/// How many rows the pill visits while hovered: the ones clear of the title.
const VISITS: usize = 3;
/// When the pill first moves after the hover starts, and how often it moves on, in seconds.
const LEAD: f32 = 0.35;
const DWELL: f32 = 1.1;
/// How far up from the bottom the rows fade out under the title, in units.
const FADE: f32 = 0.62;
/// How strongly the pill fills, and the glow under it.
const PILL: f32 = 0.09;
const PILL_GLOW: f32 = 0.12;

/// The General tile's art: the user's own sidebar entries drawn large, cropped by the tile, with
/// the page Sonora starts on lit on a pill. While hovered the pill glides from entry to entry.
pub(super) fn art(hover: Hover, window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let theme = *cx.theme();
    let settings = Sonora::global(cx).settings.read(cx);
    let start = Screen::from_id(settings.startup()).map(Screen::destination);
    let rows: Vec<(&'static str, &'static str, bool)> = NAV
        .iter()
        .filter(|(entry, _, _)| entry.is_none_or(|entry| settings.nav_shown(entry.id())))
        .map(|(entry, icon, destination)| {
            let key = entry.map_or("nav-settings", |entry| entry.key());
            let home = start.as_ref().is_some_and(|start| same(start, destination));
            (key, *icon, home)
        })
        .collect();
    let visits = VISITS.min(rows.len()).max(1);
    let home = rows
        .iter()
        .position(|(_, _, home)| *home)
        .unwrap_or(0)
        .min(visits - 1);

    // The pill steps through the first rows in turn from the one Sonora starts on, gliding back
    // up from the last.
    let steps = super::stepped(hover.clock, LEAD, DWELL);
    let taken = steps.floor();
    let from = (home + taken as usize) % visits;
    let to = (home + taken as usize + 1) % visits;
    let at = from as f32 + (to as f32 - from as f32) * (steps - taken);

    let row = unit * ROW;
    let top = (unit * TOP).round();
    let moving = super::moving(hover, cx);
    let (pill_top, fraction) = super::adrift(top + at * row, moving, window);
    let size = theme.text(Text::Title) * hover.scale;
    let width = (unit * (1.4 - INSET)).round();

    let list = div()
        .absolute()
        .inset_0()
        .fade_edges(px(0.), px(unit * FADE))
        .child(super::halo(
            theme.foreground.opacity(PILL_GLOW),
            unit * 0.55,
            top + at * row + row / 2.,
            unit * 1.1,
            unit * 0.3,
        ))
        .child(
            div()
                .absolute()
                .left(px((unit * INSET).round()))
                .top(pill_top)
                .when(moving, |pill| pill.layer_translate(point(px(0.), fraction)))
                .w(px(width))
                .h(px(row.round()))
                .rounded(theme.radius * hover.scale)
                .bg(theme.foreground.opacity(PILL))
                .border_1()
                .border_color(theme.foreground.opacity(PILL)),
        )
        .children(rows.iter().enumerate().map(|(index, (key, icon, _))| {
            let lit = (1. - (at - index as f32).abs()).clamp(0., 1.);
            let tint = mix(theme.muted_foreground.opacity(0.7), theme.foreground, lit);
            div()
                .absolute()
                .left(px((unit * (INSET + PAD)).round()))
                .top(px((top + index as f32 * row).round()))
                .h(px(row.round()))
                .flex()
                .items_center()
                .gap(px((unit * SPACE).round()))
                .whitespace_nowrap()
                .text_size(size)
                .font_weight(FontWeight::MEDIUM)
                .text_color(tint)
                .child(
                    svg()
                        .path(icons::path(icon))
                        .flex_none()
                        .size(px((unit * ICON).round()))
                        .text_color(tint),
                )
                .child(i18n::lookup(key, None))
        }));

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .child(list)
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the General tile glows in while hovered: the foreground.
pub(super) fn glow(cx: &App) -> Hsla {
    cx.theme().foreground
}

/// Whether two destinations are the same sidebar entry, so a start page inside the library
/// lights the library's row.
fn same(start: &Destination, row: &Destination) -> bool {
    std::mem::discriminant(start) == std::mem::discriminant(row)
}
