//! A miniature of the Now playing view for the track on show: the ambient field from its cover,
//! the cover, title and artist, the visualizer along the bottom and the controls over it, each
//! following the page's settings as the real view does.

use std::cell::Cell;
use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Div, FontWeight, Hsla, Pixels, Window, div, linear_color_stop,
    linear_gradient, px, relative, svg,
};
use state::{FullscreenControlsAutohide, PlaybackState};
use ui::{ActiveTheme as _, Artwork, Levels, Motion, Text, Visualizer, ease_in_out_cubic, tabular};

use super::Tick;
use crate::screens::settings::{Layout, SettingsView};
use crate::shared::visualizer::VisualizerDrive;

/// How tall the miniature stands for its width, the shape of a window.
const ASPECT: f32 = 0.5;
/// How much of the miniature's height the cover takes.
const COVER: f32 = 0.46;
/// How much of the miniature's height the visualizer reaches up at most.
const SPECTRUM: f32 = 0.34;
/// How many bands the synthetic spectrum has while nothing plays.
const BANDS: usize = 32;
/// How fast the swells travel along the synthetic bands, in turns a second, and how many swells
/// fit across them, for the slow one and the quick one laid over it.
const SWELLS: [(f32, f32); 2] = [(0.05, 1.3), (0.11, 3.1)];
/// How far the synthetic bands rise and fall with each swell, as a share of their level.
const SWAY: [f32; 2] = [0.28, 0.12];
/// The synthetic level at the lowest band and at the highest.
const BASS: f32 = 0.42;
const TREBLE: f32 = 0.1;
/// The progress bar's height and the thumb on it.
const BAR: Pixels = px(3.);
const THUMB: Pixels = px(8.);
/// How far into the track the progress shows for a track that is not playing.
const PLAYED: f32 = 0.38;
/// How far the cover and its lines sit above the middle, as a share of the visualizer's reach.
const LIFT: f32 = 0.5;
/// How much of the miniature's width the title and artist may take.
const META: f32 = 0.6;
/// How much wider than the cover the progress row is.
const SEEK: f32 = 1.3;
/// How strong the progress bar's track is against the foreground.
const TRACK: f32 = 0.15;
/// How far the bands fall off toward the treble, and how far apart in phase the two channels sway.
const FALL: f32 = 0.8;
const STEREO: f32 = 1.3;
/// How much of the miniature's height the band that holds the controls clear of the visualizer
/// takes, and how much of the page colour it lays down at the bottom edge. The full view blurs
/// this band, which a settings page cannot, since its scroller fades its own edges.
const VEIL: f32 = 0.34;
const VEIL_STRENGTH: f32 = 0.8;

/// What the Now playing page's preview keeps between frames.
pub(in crate::screens::settings) struct NowPlayingPreview {
    /// Eases the live spectrum, and stops once the preview is no longer drawn.
    drive: VisualizerDrive,
    /// When the synthetic spectrum started swaying.
    started: Instant,
    /// Whether the pointer is over the miniature, and since when, which shows the controls
    /// while they hide on their own.
    hovered: Cell<bool>,
    hovered_at: Cell<Option<Instant>>,
    /// Moves the progress on while the track plays and the controls show.
    tick: Tick,
}

impl NowPlayingPreview {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {
            drive: VisualizerDrive::leased(),
            started: Instant::now(),
            hovered: Cell::new(false),
            hovered_at: Cell::new(None),
            tick: Tick::default(),
        }
    }

    /// How far the controls have faded in, from 0 hidden to 1 shown.
    fn controls(
        &self,
        autohide: FullscreenControlsAutohide,
        window: &mut Window,
        animates: bool,
    ) -> f32 {
        let hovered = self.hovered.get();
        match autohide {
            FullscreenControlsAutohide::AlwaysShown => 1.,
            FullscreenControlsAutohide::AlwaysHidden => 0.,
            FullscreenControlsAutohide::Automatic => {
                let progress = match (animates, self.hovered_at.get()) {
                    (true, Some(at)) => {
                        let span = Motion::Quick.span().as_secs_f32().max(f32::EPSILON);
                        (at.elapsed().as_secs_f32() / span).min(1.)
                    }
                    _ => 1.,
                };
                if progress < 1. {
                    window.request_animation_frame();
                }
                let eased = ease_in_out_cubic(progress);
                match hovered {
                    true => eased,
                    false => 1. - eased,
                }
            }
        }
    }
}

/// The Now playing page's preview, a miniature of the Now playing view for the track playing,
/// or for one from history while nothing is loaded.
pub(in crate::screens::settings) fn preview(
    state: &NowPlayingPreview,
    view: &SettingsView,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    let theme = *cx.theme();
    let width = Layout::of(window, cx).width;
    let height = (width * ASPECT).round();
    let sample = view.previews.playing_or_picked(cx);
    let cover = sample.large_cover(cx);
    let animates = ui::motion::animates(cx);

    let settings = view.settings.read(cx);
    let ambient = settings.ambient();
    let style = settings.visualizer_style();
    let absolute = settings.visualizer_absolute();
    let autohide = settings.fullscreen_controls_autohide();

    let playback = view.playback.read(cx);
    let playing = matches!(playback.state(), PlaybackState::Playing);
    let volume = playback.volume_gain();
    let loaded = playback.track().is_some();
    let position = match loaded {
        true => playback.live_position(),
        false => sample.duration.mul_f32(PLAYED),
    };
    let spectrum = (style.shown() && playing)
        .then(|| playback.spectrum(cx))
        .flatten();
    let levels = match spectrum {
        Some(spectrum) => {
            state.drive.show(cx.entity_id(), spectrum, window);
            state.drive.levels()
        }
        None => {
            let elapsed = match animates && style.shown() {
                true => {
                    window.request_animation_frame();
                    state.started.elapsed().as_secs_f32()
                }
                false => 0.,
            };
            let gain = match absolute {
                true => 1.,
                false => volume,
            };
            synthetic(elapsed, gain)
        }
    };

    let field = ambient.then(|| view.previews.ambient(cover.clone(), cx));
    let tint = field
        .as_ref()
        .and_then(|field| field.read(cx).tint())
        .map(|tint| theme.primary_of(tint));
    let shown = state.controls(autohide, window, animates);
    if shown > 0. && playing {
        state.tick.after(Duration::from_secs(1), window, cx);
    }

    let side = (height * COVER).round();
    let corner = (theme.radius - px(1.)).max(Pixels::ZERO);

    Some(
        div()
            .id("now-playing-preview")
            .relative()
            .w_full()
            .h(height)
            .overflow_hidden()
            .bg(theme.background)
            .on_hover(cx.listener(|this, over: &bool, _, cx| {
                let preview = &this.previews.now_playing;
                if preview.hovered.replace(*over) != *over {
                    preview.hovered_at.set(Some(Instant::now()));
                    cx.notify();
                }
            }))
            .children(field)
            .when(style.shown(), |this| {
                this.child(
                    Visualizer::new(levels, (height * SPECTRUM).round())
                        .style_kind(style)
                        .when_some(tint, |this, tint| this.tint(tint))
                        .corner_radius(corner)
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0(),
                )
            })
            .when(style.shown() && shown > 0., |this| {
                let band = (height * VEIL).round();
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(band)
                        .when(shown < 1., |this| this.opacity(shown))
                        .rounded_b(corner)
                        .bg(linear_gradient(
                            0.,
                            linear_color_stop(theme.background.opacity(VEIL_STRENGTH), 0.),
                            linear_color_stop(theme.background.opacity(0.), 1.),
                        )),
                )
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .pb(height * SPECTRUM * LIFT)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(Artwork::new(cover).size(side).corner_radius(theme.radius))
                    .child(
                        div()
                            .mt_2()
                            .max_w(width * META)
                            .truncate()
                            .text_size(theme.text(Text::Body))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(sample.title.clone()),
                    )
                    .child(
                        div()
                            .max_w(width * META)
                            .truncate()
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .child(sample.artist.clone()),
                    ),
            )
            .when(shown > 0., |this| {
                this.child(
                    controls(
                        position,
                        sample.duration,
                        playing,
                        side * SEEK,
                        tint.unwrap_or(theme.primary),
                        cx,
                    )
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(theme.metrics.pad * 1.5)
                    .when(shown < 1., |this| this.opacity(shown)),
                )
            })
            .into_any_element(),
    )
}

/// The controls at the bottom of the view, in miniature: the progress with its times, then the
/// transport.
fn controls(
    position: Duration,
    total: Duration,
    playing: bool,
    wide: Pixels,
    fill: Hsla,
    cx: &gpui::App,
) -> Div {
    let theme = cx.theme();
    let size = theme.text(Text::Small);
    let share = match total.is_zero() {
        true => 0.,
        false => (position.as_secs_f32() / total.as_secs_f32()).clamp(0., 1.),
    };
    let time = |value: Duration| {
        div()
            .flex_none()
            .text_size(theme.text(Text::Tiny))
            .text_color(theme.muted_foreground)
            .font_features(tabular())
            .child(ui::clock(value))
    };
    let icon = |path: &'static str, strong: bool| {
        svg()
            .path(icons::path(path))
            .size(size)
            .flex_none()
            .text_color(match strong {
                true => theme.foreground,
                false => theme.muted_foreground,
            })
    };

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(wide)
                .flex()
                .items_center()
                .gap_2()
                .child(time(position))
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .h(BAR)
                        .rounded_full()
                        .bg(theme.foreground.opacity(TRACK))
                        .child(div().h_full().w(relative(share)).rounded_full().bg(fill))
                        .child(
                            div()
                                .absolute()
                                .top((BAR - THUMB) / 2.)
                                .left(relative(share))
                                .ml(-THUMB / 2.)
                                .size(THUMB)
                                .rounded_full()
                                .bg(theme.foreground),
                        ),
                )
                .child(time(total)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_5()
                .child(icon("icons/shuffle.svg", false))
                .child(icon("icons/skip-back.svg", true))
                .child(icon(
                    match playing {
                        true => "icons/pause.svg",
                        false => "icons/play.svg",
                    },
                    true,
                ))
                .child(icon("icons/skip-forward.svg", true))
                .child(icon("icons/repeat.svg", false)),
        )
}

/// A calm spectrum for when nothing plays: louder in the bass and falling off toward the
/// treble, each band swaying slowly at its own pace, all scaled by `gain`.
fn synthetic(elapsed: f32, gain: f32) -> Levels {
    let band = |index: usize, phase: f32| {
        let along = index as f32 / (BANDS - 1) as f32;
        let level = TREBLE + (BASS - TREBLE) * (1. - along).powf(FALL);
        let swell = SWELLS
            .iter()
            .zip(SWAY)
            .map(|(&(pace, count), depth)| {
                depth * (TAU * (pace * elapsed + count * along) + phase).sin()
            })
            .sum::<f32>();
        (level * (1. + swell) * gain).clamp(0., 1.)
    };

    Levels {
        left: (0..BANDS).map(|index| band(index, 0.)).collect(),
        right: (0..BANDS).map(|index| band(index, STEREO)).collect(),
    }
}
