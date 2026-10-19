use gpui::prelude::*;
use gpui::{AnyElement, App, Div, Hsla, Window, div, point, px};
use state::Sonora;
use ui::{ActiveTheme as _, Artwork, Hover};

/// How many covers the tile cycles through at most.
const COVERS: usize = 6;
/// Each place a cover can take, front to back: its centre across the tile and its size, in
/// units, and how bright it shows.
const PLACES: [(f32, f32, f32); 3] = [(0.62, 0.7, 1.), (1.12, 0.54, 0.55), (1.5, 0.4, 0.)];
/// How far down the covers' centres sit, in units.
const MIDDLE: f32 = 0.44;
/// When the first cover moves on after the hover starts, and how often the next one follows,
/// in seconds.
const LEAD: f32 = 0.3;
const TRACK: f32 = 1.8;
/// How much larger the cover that is leaving grows as it fades into the one arriving under it.
const MERGE: f32 = 0.08;
/// How soft and how strong the glow the front cover casts is.
const GLOW_BLUR: f32 = 0.22;
const GLOW: f32 = 0.8;

/// The Playback tile's art: the playing cover with the next one in the queue behind it. While
/// hovered the next cover slides into the playing one's place as it fades away, the way a
/// gapless change runs one track into the next, and the queue moves on.
pub(super) fn art(hover: Hover, window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let covers = covers(cx);
    let moving = super::moving(hover, cx);
    let advance = super::stepped(hover.clock, LEAD, TRACK);
    let count = covers.len().max(1);
    let first = advance.floor() as usize;

    // The covers from the one fading out to the one coming in, drawn back to front.
    let shown: Vec<(f32, Option<String>)> = (first..first + 4)
        .map(|index| {
            let slot = index as f32 - advance;
            (slot, covers.get(index % count).cloned().flatten())
        })
        .filter(|(slot, _)| *slot > -1. && *slot < 2.)
        .collect();
    let front = shown
        .iter()
        .find(|(slot, _)| *slot > -0.5)
        .and_then(|(_, cover)| cover.clone());

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left(px((unit * (PLACES[0].0 - PLACES[0].1 * 0.8)).round()))
                .top(px((unit * (MIDDLE - PLACES[0].1 * 0.8)).round()))
                .opacity(GLOW)
                .blur(px(unit * GLOW_BLUR))
                .child(Artwork::new(front).size(px((unit * PLACES[0].1 * 1.6).round()))),
        )
        .children(
            shown
                .into_iter()
                .rev()
                .map(|(slot, cover)| place(slot, cover, unit, moving, window, cx)),
        )
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the Playback tile glows in while hovered: the tint of the playing cover.
pub(super) fn glow(cx: &App) -> Hsla {
    super::cover_tint(cx)
}

/// The covers the tile cycles through: the playing one, those queued after it and then the
/// history's, each cover once.
fn covers(cx: &App) -> Vec<Option<String>> {
    let sonora = Sonora::global(cx);
    let queue = sonora.queue.read(cx);
    let history = sonora.history.read(cx).tracks();
    // An album queues the same cover track after track, so each cover shows only once.
    let mut covers: Vec<Option<String>> = Vec::new();
    for cover in super::playing(cx)
        .into_iter()
        .chain(queue.upcoming().cloned())
        .chain(history.iter().cloned())
        .map(|track| track.cover)
    {
        if covers.len() == COVERS {
            break;
        }
        if !covers.contains(&cover) {
            covers.push(cover);
        }
    }
    covers
}

/// A cover at `slot`: 0 is the front, 1 the place behind it and 2 past the back, and the
/// fraction between them is a cover on its way forward. Below 0 a cover is fading out where it
/// stands, growing a little as the next merges into it.
fn place(
    slot: f32,
    cover: Option<String>,
    unit: f32,
    moving: bool,
    window: &Window,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let (center, size, light) = match slot < 0. {
        true => {
            let (center, size, _) = PLACES[0];
            (center, size * (1. - slot * MERGE), 1. + slot)
        }
        false => {
            let from = (slot.floor() as usize).min(PLACES.len() - 1);
            let to = (from + 1).min(PLACES.len() - 1);
            let along = slot - from as f32;
            let lerp = |a: f32, b: f32| a + (b - a) * along;
            (
                lerp(PLACES[from].0, PLACES[to].0),
                lerp(PLACES[from].1, PLACES[to].1),
                lerp(PLACES[from].2, PLACES[to].2),
            )
        }
    };
    let fade = match slot {
        slot if slot < 0. => (1. + slot).clamp(0., 1.),
        slot if slot > 1. => (2. - slot).clamp(0., 1.),
        _ => 1.,
    };
    // At rest a cover is laid out at its size on whole pixels. While moving it is laid out at
    // the front size and the compositor shrinks and shifts it to the fraction, so it never
    // steps from one pixel size to the next.
    let laid = match moving {
        true => (unit * PLACES[0].1).round(),
        false => (unit * size).round(),
    };
    let scale = unit * size / laid;
    let (left, fraction_x) = super::adrift(unit * center - laid / 2., moving, window);
    let (top, fraction_y) = super::adrift(unit * MIDDLE - laid / 2., moving, window);

    div()
        .absolute()
        .left(left)
        .top(top)
        .when(moving, |cover| {
            cover
                .layer_translate(point(fraction_x, fraction_y))
                .layer_scale(scale)
        })
        .size(px(laid))
        .opacity(fade)
        .rounded(theme.radius)
        .shadow_lg()
        .child(
            Artwork::new(cover)
                .size(px(laid))
                .corner_radius(theme.radius),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(theme.radius)
                .bg(theme.background.opacity((1. - light.clamp(0., 1.)) * 0.6)),
        )
}
