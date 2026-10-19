//! A few lines of lyrics drawn by the lyrics sheet's own renderer: the synced lyrics of the
//! track playing, or sample lines with one in Japanese while there are none. They show twice
//! side by side, as the side panel draws them and as Now playing does, each at its own text size.

use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Div, Entity, Hsla, Pixels, Window, div};
use music::{LyricsLine, LyricsWord, RomanizedText, Voice, WritingSystem};
use state::{LyricsState, PlaybackState, RomanizationScripts, Sonora};
use ui::{ActiveTheme as _, Text, Theme};

use super::Tick;
use crate::chrome::{VerseLook, verse, verse_sizes};
use crate::screens::settings::{Layout, SettingsView};
use crate::shared::ambient::Ambient;

/// How tall the two halves stand for the width of the whole preview.
const ASPECT: f32 = 0.4;
/// How many lines show either side of the one being sung.
const AROUND: usize = 2;
/// How far down the preview the line being sung starts, as a share of its height.
const ANCHOR: f32 = 0.36;
/// The haze curve of the sheet, from a line's distance to the one being sung.
const HAZE: f32 = 0.45;
/// How often the sheet is redrawn while a word sweeps, as the side panel does it.
const SWEEP: Duration = Duration::from_nanos(1_000_000_000 / 45);
/// How often the lines are looked at again while a track plays without a sweep.
const FOLLOW: Duration = Duration::from_millis(250);
/// How long the sample's sung line takes, and how long it rests sung before it starts again.
const SUNG: Duration = Duration::from_millis(3200);
const REST: Duration = Duration::from_millis(1600);
/// Where the sweep of the sample stands when nothing moves.
const STILL: f32 = 0.6;
/// How far the sheet fades in from its top and bottom edges, in verse sizes, as the panel does.
const VERSE_FADE: f32 = 1.25;
/// How far apart the sample's lines start, and which of them is being sung.
const SPACING: Duration = Duration::from_secs(4);
const SUNG_LINE: usize = 2;
/// The sample's lines by their Fluent keys, and the one in Japanese with its romanization.
const SAMPLE: [&str; 5] = [
    "settings-lyrics-sample-first",
    "settings-lyrics-sample-second",
    "settings-lyrics-sample-japanese",
    "settings-lyrics-sample-fourth",
    "settings-lyrics-sample-fifth",
];
const ROMANIZED: &str = "settings-lyrics-sample-romanized";

/// What the Lyrics page's preview keeps between frames.
pub(in crate::screens::settings) struct LyricsPreview {
    /// When the sample's sweep started.
    started: Instant,
    /// Redraws the lines as they are sung.
    tick: Tick,
}

/// The lyrics settings both halves of the preview follow.
#[derive(Clone, Copy)]
struct Look {
    karaoke: bool,
    blur: bool,
    scripts: Option<RomanizationScripts>,
}

/// One half of the preview: the verse and lane sizes it sets, the colour behind its lines, the
/// ambient field over that colour if any, and the Fluent key that names it.
struct Half {
    sizes: (Pixels, Pixels),
    behind: Hsla,
    field: Option<Entity<Ambient>>,
    key: &'static str,
}

/// The lines on show and where the track stands in them.
struct Shown<'a> {
    lines: &'a [LyricsLine],
    /// The line being sung, if any.
    active: Option<usize>,
    /// The line the few on show gather around.
    center: usize,
    position: Duration,
}

impl LyricsPreview {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {
            started: Instant::now(),
            tick: Tick::default(),
        }
    }
}

/// The Lyrics page's preview: the lines as the side panel draws them beside the lines as Now
/// playing draws them, each at its own text size and each named underneath.
pub(in crate::screens::settings) fn preview(
    state: &LyricsPreview,
    view: &SettingsView,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    let theme = *cx.theme();
    let width = Layout::of(window, cx).width;
    let height = (width * ASPECT).round();
    let animates = ui::motion::animates(cx);

    let settings = view.settings.read(cx);
    let look = Look {
        karaoke: settings.karaoke_lyrics(),
        blur: settings.blur_lyrics(),
        scripts: settings
            .romanized_lyrics()
            .then(|| settings.romanization_scripts()),
    };
    let scales = (
        settings.panel_lyrics_scale(),
        settings.fullscreen_lyrics_scale(),
    );
    let ambient = settings.ambient();

    let playback = view.playback.read(cx);
    let playing = matches!(playback.state(), PlaybackState::Playing);
    let lyrics = Sonora::global(cx).lyrics.read(cx);
    let synced = match (lyrics.state(), lyrics.current().map(|hit| &hit.lyrics)) {
        (LyricsState::Ready, Some(music::Lyrics::Synced { lines })) if !lines.is_empty() => {
            Some(lines.clone())
        }
        _ => None,
    };
    let (lines, active, position, moving) = match synced {
        Some(lines) => {
            let position = playback.live_position();
            let active = music::lyrics::active(&lines, position);
            (lines, active, position, playing)
        }
        None => {
            let lines = sample();
            let sweep = match animates && look.karaoke {
                true => {
                    let cycle = (SUNG + REST).as_secs_f32();
                    Duration::from_secs_f32(state.started.elapsed().as_secs_f32() % cycle)
                }
                false => SUNG.mul_f32(STILL),
            };
            let position = lines[SUNG_LINE].start + sweep;
            (
                lines.into(),
                Some(SUNG_LINE),
                position,
                animates && look.karaoke,
            )
        }
    };
    if moving {
        let sweeping = active.is_some_and(|index| lines[index].worded());
        let wait = match look.karaoke && animates && sweeping {
            true => SWEEP,
            false => FOLLOW,
        };
        state.tick.after(wait, window, cx);
    }
    let center = active.unwrap_or_else(|| {
        lines
            .iter()
            .position(|line| line.start > position)
            .unwrap_or(lines.len() - 1)
    });
    let shown = Shown {
        lines: &lines,
        active,
        center,
        position,
    };

    let field = ambient.then(|| {
        let cover = view.previews.playing_or_picked(cx).large_cover(cx);
        view.previews.ambient(cover, cx)
    });
    let panel = Half {
        sizes: verse_sizes(true, scales.0, &theme),
        behind: theme.sidebar,
        field: None,
        key: "settings-lyrics-preview-panel",
    };
    let now_playing = Half {
        sizes: verse_sizes(false, scales.1, &theme),
        behind: theme.background,
        field,
        key: "settings-lyrics-preview-now-playing",
    };

    Some(
        div()
            .w_full()
            .flex()
            .gap_3()
            .child(half(panel, &shown, look, height, cx))
            .child(half(now_playing, &shown, look, height, cx))
            .into_any_element(),
    )
}

/// One half of the preview, as `half` describes it, framed at `height` and named under its frame.
fn half(half: Half, shown: &Shown, look: Look, height: Pixels, cx: &App) -> Div {
    let theme = *cx.theme();
    let sheet = div()
        .relative()
        .w_full()
        .h(height)
        .bg(half.behind)
        .children(half.field)
        .child(lines(shown, look, half.sizes, height, theme));

    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .child(super::framed(sheet, cx))
        .child(
            div()
                .max_w_full()
                .truncate()
                .text_size(theme.text(Text::Label))
                .text_color(theme.muted_foreground)
                .child(i18n::lookup(half.key, None)),
        )
}

/// A few lines around the one being sung, drawn the way the sheet draws them at the verse and
/// lane sizes in `sizes`, over the whole of a parent `height` tall. The one being sung starts at
/// `ANCHOR` of the height whatever the sizes, so a large text size never pushes it out of view.
fn lines(shown: &Shown, look: Look, sizes: (Pixels, Pixels), height: Pixels, theme: Theme) -> Div {
    let (size, lane) = sizes;
    let first = shown.center.saturating_sub(AROUND);
    let last = (shown.center + AROUND + 1).min(shown.lines.len());
    let verse_at = |index: usize| {
        let distance = index.abs_diff(shown.center) as f32;
        let haze = match look.blur && Some(index) != shown.active {
            true => (distance / AROUND as f32).min(1.).powf(HAZE),
            false => 0.,
        };
        verse(
            &shown.lines[index],
            VerseLook {
                active: Some(index) == shown.active,
                size,
                lane,
                karaoke: look.karaoke,
                scripts: look.scripts,
                haze,
            },
            shown.position,
            theme,
        )
    };
    let fade = size * VERSE_FADE;
    let anchor = height * ANCHOR;

    div()
        .absolute()
        .inset_0()
        .when(ui::effects(), |this| this.fade_edges(fade, fade))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(height - anchor)
                .pb_4()
                .px(theme.metrics.inset)
                .flex()
                .flex_col()
                .gap_4()
                .children((first..shown.center).map(verse_at)),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(anchor)
                .px(theme.metrics.inset)
                .flex()
                .flex_col()
                .gap_4()
                .children((shown.center..last).map(verse_at)),
        )
}

/// The sample lines, timed so the third is being sung and the ones before it are over.
fn sample() -> Vec<LyricsLine> {
    SAMPLE
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let text = i18n::lookup(key, None).to_string();
            let start = SPACING * index as u32;
            let romanized = (index == SUNG_LINE).then(|| RomanizedText {
                text: i18n::lookup(ROMANIZED, None).to_string(),
                writing_system: WritingSystem::Japanese,
            });
            LyricsLine {
                start,
                end: Some(start + SUNG),
                words: Some(timed(&text, start)),
                text,
                romanized,
                secondary: Vec::new(),
                voice: Voice::Lead,
            }
        })
        .collect()
}

/// Splits a line into words sung one after another across `SUNG`, by its spaces where it has
/// them and letter by letter where it has none, each word taking time for its length.
fn timed(text: &str, start: Duration) -> Vec<LyricsWord> {
    let words: Vec<String> = match text.contains(char::is_whitespace) {
        true => text.split_whitespace().map(str::to_owned).collect(),
        false => text.chars().map(String::from).collect(),
    };
    let letters = words
        .iter()
        .map(|word| word.chars().count())
        .sum::<usize>()
        .max(1);
    let mut at = start;
    words
        .into_iter()
        .map(|word| {
            let length = SUNG.mul_f32(word.chars().count() as f32 / letters as f32);
            let timed = LyricsWord {
                start: at,
                end: at + length,
                text: word,
            };
            at += length;
            timed
        })
        .collect()
}
