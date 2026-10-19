//! A still miniature of the Now playing view for the track on show: the ambient field from its
//! cover, the cover, title and artist, and the visualizer along the bottom, each following the
//! page's settings as the real view does. It takes no pointer input, so it has no controls.

use std::f32::consts::TAU;
use std::time::Instant;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, Pixels, Window, div, px};
use state::PlaybackState;
use ui::{ActiveTheme as _, Artwork, Levels, Text, Visualizer};

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
/// How far the cover and its lines sit above the middle, as a share of the visualizer's reach.
const LIFT: f32 = 0.5;
/// How much of the miniature's width the title and artist may take.
const META: f32 = 0.6;
/// How far the bands fall off toward the treble, and how far apart in phase the two channels sway.
const FALL: f32 = 0.8;
const STEREO: f32 = 1.3;

/// What the Now playing page's preview keeps between frames.
pub(in crate::screens::settings) struct NowPlayingPreview {
    /// Eases the live spectrum, and stops once the preview is no longer drawn.
    drive: VisualizerDrive,
    /// When the synthetic spectrum started swaying.
    started: Instant,
}

impl NowPlayingPreview {
    pub(in crate::screens::settings) fn new(_cx: &mut Context<SettingsView>) -> Self {
        Self {
            drive: VisualizerDrive::leased(),
            started: Instant::now(),
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

    let playback = view.playback.read(cx);
    let playing = matches!(playback.state(), PlaybackState::Playing);
    let volume = playback.volume_gain();
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
    let side = (height * COVER).round();
    let corner = (theme.radius - px(1.)).max(Pixels::ZERO);

    Some(
        super::framed(
            div()
                .relative()
                .w_full()
                .h(height)
                .overflow_hidden()
                .bg(theme.background)
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
                ),
            cx,
        )
        .into_any_element(),
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
