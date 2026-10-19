use gpui::prelude::*;
use gpui::{AnyElement, App, Div, FontWeight, Hsla, Window, div, point, px, rgb, svg};
use music::Track;
use state::Sonora;
use ui::{ActiveTheme as _, Artwork, Hover, Text};

/// Last.fm's brand red.
const LASTFM: u32 = 0xd51007;
/// How many tracks of the history the feed shows before it comes round again.
const TRACKS: usize = 8;
/// How far apart the rows are, the cover on each, and the room left of the covers and between
/// a cover and its words, in units.
const PITCH: f32 = 0.36;
const COVER: f32 = 0.27;
const INSET: f32 = 0.13;
const SPACE: f32 = 0.08;
/// The mark a scrobbled row carries, in units.
const MARK: f32 = 0.1;
/// How far the feed has already scrolled at rest, in rows, so the tile opens on a row cut by
/// its top edge as a feed does.
const START: f32 = 0.35;
/// How fast the feed scrolls up while hovered, in units a second.
const SCROLL: f32 = 0.12;
/// How far in from the top the feed fades out, and from the bottom, where it gives way to the
/// tile's title, in units.
const FADE_TOP: f32 = 0.22;
const FADE_BOTTOM: f32 = 0.6;

/// The Scrobbling tile's art: the listening history as a feed of covers and titles, each marked
/// in Last.fm's red once a service is linked. While hovered the feed scrolls up slowly.
pub(super) fn art(hover: Hover, window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let sonora = Sonora::global(cx);
    let linked = sonora
        .scrobbling
        .read(cx)
        .rows()
        .iter()
        .any(|row| row.linked());
    let mut tracks: Vec<Track> = Vec::new();
    for track in sonora.history.read(cx).tracks() {
        if tracks.len() == TRACKS {
            break;
        }
        if !tracks.iter().any(|seen| seen.id == track.id) {
            tracks.push(track.clone());
        }
    }
    if tracks.is_empty() {
        tracks.extend(super::playing(cx));
    }
    let pitch = (unit * PITCH).round();
    let span = pitch * tracks.len().max(1) as f32;
    let moving = super::moving(hover, cx);
    let offset = (pitch * START + hover.clock * SCROLL * unit).rem_euclid(span);
    let (top, fraction) = super::adrift(-offset, moving, window);
    let red: Hsla = rgb(LASTFM).into();

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .when(linked, |art| {
            art.child(super::halo(
                red.opacity(0.3),
                unit * 1.4,
                unit * 0.1,
                unit * 1.3,
                unit * 0.4,
            ))
        })
        .child(
            div()
                .absolute()
                .inset_0()
                .fade_edges(px(unit * FADE_TOP), px(unit * FADE_BOTTOM))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(top)
                        .when(moving, |feed| feed.layer_translate(point(px(0.), fraction)))
                        .flex()
                        .flex_col()
                        .children(
                            tracks
                                .iter()
                                .cycle()
                                .take(tracks.len() * 3)
                                .map(|track| row(track, linked, unit, hover, cx)),
                        ),
                ),
        )
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the Scrobbling tile glows in while hovered: Last.fm's red once a service is
/// linked, and the foreground before.
pub(super) fn glow(cx: &App) -> Hsla {
    let linked = Sonora::global(cx)
        .scrobbling
        .read(cx)
        .rows()
        .iter()
        .any(|row| row.linked());
    match linked {
        true => rgb(LASTFM).into(),
        false => cx.theme().foreground,
    }
}

/// One row of the feed: the cover, the title over the artist, and Last.fm's mark once linked.
fn row(track: &Track, linked: bool, unit: f32, hover: Hover, cx: &App) -> Div {
    let theme = cx.theme();
    let cover = (unit * COVER).round();

    div()
        .flex_none()
        .h(px((unit * PITCH).round()))
        .flex()
        .items_center()
        .gap(px((unit * SPACE).round()))
        .pl(px((unit * INSET).round()))
        .pr(px((unit * INSET).round()))
        .child(
            Artwork::new(track.cover.clone())
                .size(px(cover))
                .corner_radius(theme.radius.min(px(cover / 4.))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(theme.text(Text::Body) * hover.scale)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.foreground)
                        .child(track.name.clone()),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(theme.text(Text::Small) * hover.scale)
                        .text_color(theme.muted_foreground)
                        .child(track.artists.clone()),
                ),
        )
        .when(linked, |row| {
            row.child(
                svg()
                    .flex_none()
                    .path(icons::path("icons/check.svg"))
                    .size(px((unit * MARK).round()))
                    .text_color(rgb(LASTFM)),
            )
        })
}
