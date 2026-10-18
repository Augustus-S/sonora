use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, Render, Window, div, svg};
use i18n::t;
use state::{OutputLevel, Playback};
use ui::{ActiveTheme as _, Button, LevelMeter, Text};

use super::rows::figure;

/// Below this an overload rounds to no tenth of a decibel at all.
const SLIGHT_OVERLOAD: f64 = 0.05;
/// How wide the level meter is, in cover sizes.
const METER_WIDTH: f32 = 1.2;

/// The strip under the graph: the output's level meter, then the clip warning and its fix while
/// the output overloads. The screen places it as a cached view at a fixed height, so the
/// meter's reads and the warning coming and going redraw only the strip and never move the page.
pub(super) struct OutputMeter {
    playback: Entity<Playback>,
    level: Entity<OutputLevel>,
    /// The warning as last drawn, so a playback notify that left it as it was draws nothing.
    warning: Option<Warning>,
}

/// The clip warning as drawn: the over in tenths of a decibel, and whether lowering the preamp
/// is offered.
#[derive(Clone, Copy, PartialEq)]
struct Warning {
    db: f64,
    fixable: bool,
}

impl OutputMeter {
    pub(super) fn new(playback: Entity<Playback>, cx: &mut Context<Self>) -> Self {
        let level = playback.read(cx).output_level();
        cx.observe(&level, |_, _, cx| cx.notify()).detach();
        cx.observe(&playback, |this, _, cx| {
            let warning = this.warning(cx);
            if warning != this.warning {
                this.warning = warning;
                cx.notify();
            }
        })
        .detach();
        let mut meter = Self {
            playback,
            level,
            warning: None,
        };
        meter.warning = meter.warning(cx);
        meter
    }

    /// The warning playback asks for right now, with the fix left out when the equalizer is off
    /// or the preamp is as low as it goes.
    fn warning(&self, cx: &Context<Self>) -> Option<Warning> {
        let playback = self.playback.read(cx);
        Some(Warning {
            db: playback.overload()?,
            fixable: playback.equalizer() && playback.lowered_preamp().is_some(),
        })
    }
}

impl Render for OutputMeter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let level = self.level.read(cx);
        let size = theme.text(Text::Small);

        div()
            .size_full()
            .flex()
            .items_center()
            .gap_3()
            .overflow_hidden()
            .text_size(size)
            .child(
                div()
                    .flex_none()
                    .text_color(theme.muted_foreground)
                    .child(t!("equalizer-level")),
            )
            .child(
                LevelMeter::new(level.level_db())
                    .peak(level.peak_db())
                    .w(theme.metrics.cover * METER_WIDTH),
            )
            .when_some(self.warning, |this, warning| {
                this.child(
                    svg()
                        .path(icons::path("icons/triangle-alert.svg"))
                        .size(size)
                        .flex_none()
                        .text_color(theme.danger),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.danger)
                        .child(match warning.db < SLIGHT_OVERLOAD {
                            true => t!("equalizer-overload-slight"),
                            false => t!("equalizer-overload", db = figure(warning.db, 1)),
                        }),
                )
                .when(warning.fixable, |this| {
                    this.child(
                        Button::new("equalizer-lower-preamp")
                            .small()
                            .outline()
                            .flex_none()
                            .label(t!("equalizer-lower-preamp"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.playback
                                    .update(cx, |playback, cx| playback.lower_preamp(cx))
                            })),
                    )
                })
            })
    }
}
