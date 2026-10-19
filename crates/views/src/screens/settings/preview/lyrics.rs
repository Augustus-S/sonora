//! A few lines of lyrics drawn by the lyrics sheet's own renderer: the synced lyrics of the
//! track playing, or sample lines with one in Japanese while there are none. A switch in the
//! corner picks whether they show as the side panel draws them or as Now playing does, which
//! is also which of the two text sizes they take.

use std::cell::Cell;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{AnyElement, Context, Window, div};
use i18n::t;
use music::{LyricsLine, LyricsWord, RomanizedText, Voice, WritingSystem};
use state::{LyricsState, PlaybackState, Sonora};
use ui::{ActiveTheme as _, Button, TabBar};

use super::Tick;
use crate::chrome::{VerseLook, verse, verse_sizes};
use crate::screens::settings::{Layout, SettingsView};

/// How tall the preview stands for its width.
const ASPECT: f32 = 0.5;
/// How many lines show either side of the one being sung.
const AROUND: usize = 2;
/// How much of the preview's width the lines take from its left, about the share the side
/// panel and Now playing's lyrics column take of a window, which keeps them clear of the switch.
const REACH: f32 = 0.64;
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
    /// Whether the lines show as the side panel draws them, or as Now playing does.
    panel: Cell<bool>,
    /// When the sample's sweep started.
    started: Instant,
    /// Redraws the lines as they are sung.
    tick: Tick,
}

impl LyricsPreview {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {
            panel: Cell::new(true),
            started: Instant::now(),
            tick: Tick::default(),
        }
    }
}

/// The Lyrics page's preview, a few lines around the one being sung as the panel or Now playing
/// draws them, with the switch between the two in the corner.
pub(in crate::screens::settings) fn preview(
    state: &LyricsPreview,
    view: &SettingsView,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    let theme = *cx.theme();
    let width = Layout::of(window, cx).width;
    let height = (width * ASPECT).round();
    let panel = state.panel.get();
    let animates = ui::motion::animates(cx);

    let settings = view.settings.read(cx);
    let karaoke = settings.karaoke_lyrics();
    let blur = settings.blur_lyrics();
    let scripts = settings
        .romanized_lyrics()
        .then(|| settings.romanization_scripts());
    let scale = match panel {
        true => settings.panel_lyrics_scale(),
        false => settings.fullscreen_lyrics_scale(),
    };
    let ambient = settings.ambient();
    let (size, lane) = verse_sizes(panel, scale, &theme);

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
            let sweep = match animates && karaoke {
                true => {
                    let cycle = (SUNG + REST).as_secs_f32();
                    Duration::from_secs_f32(state.started.elapsed().as_secs_f32() % cycle)
                }
                false => SUNG.mul_f32(STILL),
            };
            let position = lines[SUNG_LINE].start + sweep;
            (lines.into(), Some(SUNG_LINE), position, animates && karaoke)
        }
    };
    if moving {
        let sweeping = active.is_some_and(|index| lines[index].worded());
        let wait = match karaoke && animates && sweeping {
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
    let first = center.saturating_sub(AROUND);
    let last = (center + AROUND + 1).min(lines.len());
    let shown = (first..last).map(|index| {
        let distance = index.abs_diff(center) as f32;
        let haze = match blur && Some(index) != active {
            true => (distance / AROUND as f32).min(1.).powf(HAZE),
            false => 0.,
        };
        verse(
            &lines[index],
            VerseLook {
                active: Some(index) == active,
                size,
                lane,
                karaoke,
                scripts,
                haze,
            },
            position,
            theme,
        )
    });

    let field = (!panel && ambient).then(|| {
        let cover = view.previews.playing_or_picked(cx).large_cover(cx);
        view.previews.ambient(cover, cx)
    });
    let fade = size * VERSE_FADE;
    let switch = TabBar::new("lyrics-preview-mode").items([
        Button::new("lyrics-preview-panel")
            .label(t!("settings-lyrics-preview-panel"))
            .small()
            .ghost()
            .selected(panel)
            .on_click(cx.listener(|this, _, _, cx| {
                this.previews.lyrics.panel.set(true);
                cx.notify();
            })),
        Button::new("lyrics-preview-now-playing")
            .label(t!("settings-lyrics-preview-now-playing"))
            .small()
            .ghost()
            .selected(!panel)
            .on_click(cx.listener(|this, _, _, cx| {
                this.previews.lyrics.panel.set(false);
                cx.notify();
            })),
    ]);

    Some(
        div()
            .relative()
            .w_full()
            .h(height)
            .overflow_hidden()
            .bg(match panel {
                true => theme.sidebar,
                false => theme.background,
            })
            .children(field)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .px(theme.metrics.inset)
                    .pr(width * (1. - REACH))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_4()
                    .when(ui::effects(), |this| this.fade_edges(fade, fade))
                    .children(shown),
            )
            .child(
                div()
                    .absolute()
                    .top(theme.metrics.pad)
                    .right(theme.metrics.pad)
                    .child(switch),
            )
            .into_any_element(),
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
