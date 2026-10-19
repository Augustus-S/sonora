use gpui::prelude::*;
use gpui::{AnyElement, App, FontWeight, Hsla, SharedString, Window, div, point, px, relative};
use state::{LyricsState, Sonora};
use ui::{ActiveTheme as _, Hover, Text, mix};

/// The lines the tile sings when the playing track has none of its own.
const SAMPLE: [&str; 3] = [
    "settings-hub-lyrics-first",
    "settings-hub-lyrics-second",
    "settings-hub-lyrics-third",
];
/// How far apart the lines are, where the first one sits and how far in from the left, in
/// units.
const PITCH: f32 = 0.26;
const TOP: f32 = 0.06;
const INSET: f32 = 0.12;
/// How soft and how faint a line away from the middle is.
const BLUR: f32 = 0.022;
const FAINT: f32 = 0.38;
/// How long the karaoke takes to cross a line and how long a line has before the next one
/// steps up, in seconds.
const SWEEP: f32 = 1.6;
const LINE: f32 = 2.4;
/// How light and how vivid the sung words are at least, and how soft their glow is, in units.
const SUNG_LIGHT: f32 = 0.72;
const SUNG_SATURATION: f32 = 0.75;
const SUNG_GLOW: f32 = 0.09;
const SUNG_GLOW_STRENGTH: f32 = 0.35;
/// How soft the front of the sweep is, in units.
const FRONT: f32 = 0.12;
/// How far in from the right edge the lines fade out, in units.
const FADE: f32 = 0.35;

/// The Lyrics tile's art: three lines of the playing track's lyrics, or a sample, the middle one
/// bright and the others soft. While hovered a karaoke sweep crosses the bright line and then
/// the lines step up.
pub(super) fn art(hover: Hover, window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let theme = *cx.theme();
    let (lines, middle) = lines(cx);
    let tint = super::cover_tint(cx);
    let sung_color = Hsla {
        l: tint.l.max(SUNG_LIGHT),
        s: tint.s.max(SUNG_SATURATION),
        ..tint
    };
    let moving = super::moving(hover, cx);

    // Each line takes `LINE` seconds: the sweep crosses it, then the lines step up one.
    let scroll = super::stepped(hover.clock, SWEEP, LINE);
    let singing = (hover.clock / LINE).floor();
    let sweep = ((hover.clock - singing * LINE) / SWEEP).clamp(0., 1.);
    let pitch = unit * PITCH;
    let top = unit * TOP;
    let first = scroll.floor() as usize;
    let size = theme.text(Text::Large) * hover.scale;
    let count = lines.len().max(1);

    let rows = (first..first + 4).map(|index| {
        let slot = index as f32 - scroll;
        let bright = (1. - (slot - 1.).abs()).clamp(0., 1.);
        let text = lines[(middle + count - 1 + index) % count].clone();
        let sung = match (index as f32 - 1.).total_cmp(&singing) {
            std::cmp::Ordering::Less => 1.,
            std::cmp::Ordering::Equal => sweep,
            std::cmp::Ordering::Greater => 0.,
        };
        // A layer holds only what is painted inside it, so a line reaches a line's height
        // above and below its text to carry the glow of the sung words while it moves.
        let (y, fraction) = super::adrift(top + (slot - 1.) * pitch, moving, window);
        div()
            .absolute()
            .left(px((unit * INSET).round()))
            .top(y)
            .when(moving, |line| line.layer_translate(point(px(0.), fraction)))
            .h(px((pitch * 3.).round()))
            .flex()
            .items_center()
            .opacity(FAINT + (1. - FAINT) * bright)
            .when(bright < 1., |line| {
                line.blur(px(unit * BLUR * (1. - bright)))
            })
            .child(
                div()
                    .relative()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_color(mix(theme.muted_foreground, theme.foreground, bright))
                    .when(sung > 0. && bright > 0., |line| {
                        line.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .bottom_0()
                                .w(relative(sung))
                                .rounded_full()
                                .bg(sung_color.opacity(SUNG_GLOW_STRENGTH * bright))
                                .blur(px(unit * SUNG_GLOW)),
                        )
                    })
                    .child(text.clone())
                    .when(sung > 0. && bright > 0., |line| {
                        // The sung words are a copy of the line cut to how far the sweep has
                        // come, its front softened so the colour washes over the words.
                        let front = unit * FRONT * ((1. - sung) * 4.).min(1.);
                        line.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .bottom_0()
                                .w(relative(sung))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .when(front > 0., |sung| sung.fade_sides(px(0.), px(front)))
                                .opacity(bright)
                                .text_color(sung_color)
                                .child(text),
                        )
                    }),
            )
    });

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .child(super::halo(
            tint.opacity(0.28),
            unit * 0.45,
            top + pitch * 1.5,
            unit * 1.5,
            unit * 0.4,
        ))
        .child(
            div()
                .absolute()
                .inset_0()
                .text_size(size)
                .font_weight(FontWeight::BOLD)
                .fade_sides(px(0.), px(unit * FADE))
                .children(rows),
        )
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the Lyrics tile glows in while hovered: the tint of the playing cover.
pub(super) fn glow(cx: &App) -> Hsla {
    super::cover_tint(cx)
}

/// The lines the tile sings and which of them starts in the middle: the playing track's
/// lyrics from the line playing now, its plain lyrics from the top, or the sample.
fn lines(cx: &App) -> (Vec<SharedString>, usize) {
    let sonora = Sonora::global(cx);
    let lyrics = sonora.lyrics.read(cx);
    let found = match lyrics.state() {
        LyricsState::Ready => lyrics.current(),
        _ => None,
    };
    let sung: Option<(Vec<SharedString>, usize)> = found.map(|hit| match &hit.lyrics {
        music::Lyrics::Synced { lines } => {
            let position = sonora.playback.read(cx).position();
            let playing = music::lyrics::active(lines, position).unwrap_or(0);
            let words: Vec<(usize, SharedString)> = lines
                .iter()
                .enumerate()
                .filter(|(_, line)| !line.text.trim().is_empty())
                .map(|(index, line)| (index, SharedString::from(line.text.trim().to_owned())))
                .collect();
            let middle = words
                .iter()
                .rposition(|(index, _)| *index <= playing)
                .unwrap_or(0);
            (words.into_iter().map(|(_, text)| text).collect(), middle)
        }
        music::Lyrics::Plain { text, .. } => (
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| SharedString::from(line.to_owned()))
                .collect(),
            1,
        ),
    });
    match sung {
        Some((lines, middle)) if lines.len() >= 3 => {
            let middle = middle.clamp(1, lines.len() - 2);
            (lines, middle)
        }
        _ => (
            SAMPLE.iter().map(|key| i18n::lookup(key, None)).collect(),
            1,
        ),
    }
}
