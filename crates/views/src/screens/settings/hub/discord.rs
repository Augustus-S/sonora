use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Div, FontWeight, Hsla, SharedString, Window, div, linear_color_stop,
    linear_gradient, point, px, relative, rgb, svg,
};
use i18n::t;
use state::{DiscordName, Sonora};
use ui::{ActiveTheme as _, Artwork, Hover, Text};

use crate::shared::provider_logo;

/// Discord's brand blurple, and the deeper blue its wash falls to.
const BLURPLE: u32 = 0x5865f2;
const BLURPLE_DEEP: u32 = 0x2f3593;
/// Discord's dark palette: the profile card, its title text, its body text, its quiet text and
/// the empty part of a progress bar.
const CARD: u32 = 0x232428;
const HEADER: u32 = 0xf2f3f5;
const NORMAL: u32 = 0xdbdee1;
const QUIET: u32 = 0xb5bac1;
const TRACK: u32 = 0x4e5058;
/// The light from the top left of the wash, in Discord's own white.
const SHINE: u32 = 0xffffff;
/// Discord's radius on the card and on the art, at the card's scale, in units.
const CARD_RADIUS: f32 = 0.06;
const ART_RADIUS: f32 = 0.045;
/// The card, its padding, the art and the provider badge on the art, in units.
const CARD_WIDTH: f32 = 1.34;
const CARD_TOP: f32 = 0.09;
const PAD: f32 = 0.075;
const ART: f32 = 0.34;
const BADGE: f32 = 0.13;
const BADGE_RING: f32 = 0.018;
/// The progress bar's thickness and the room above it, in units.
const BAR: f32 = 0.025;
const BAR_GAP: f32 = 0.06;
/// How high the card lifts while hovered, in units.
const LIFT: f32 = 0.035;
/// How many times faster than playback the progress runs while hovered, so the bar visibly
/// moves in the moment the pointer rests on the tile.
const RUN: f32 = 8.;
/// How faint the card is while the status is switched off.
const OFF: f32 = 0.5;

/// The Discord tile's art: a small replica of the status Discord shows for the playing track,
/// on Discord's blurple. While hovered the card lifts and its progress bar runs.
pub(super) fn art(hover: Hover, window: &mut Window, cx: &mut App) -> AnyElement {
    let unit = super::unit(hover, cx);
    let shine: Hsla = rgb(SHINE).into();
    let lifted = unit * LIFT * hover.lift;
    let moving = hover.lift > 0. && hover.lift != 1.;
    let (top, fraction) = super::adrift(unit * CARD_TOP - lifted, moving, window);

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .bg(linear_gradient(
            160.,
            linear_color_stop(rgb(BLURPLE), 0.),
            linear_color_stop(rgb(BLURPLE_DEEP), 1.),
        ))
        .child(super::halo(
            shine.opacity(0.22),
            unit * 0.1,
            unit * 0.05,
            unit * 1.2,
            unit * 0.35,
        ))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(top)
                .when(moving, |card| card.layer_translate(point(px(0.), fraction)))
                .flex()
                .justify_center()
                .child(card(hover, unit, cx)),
        )
        .child(super::sheen(cx))
        .into_any_element()
}

/// The colour the Discord tile glows in while hovered: Discord's own blurple.
pub(super) fn glow(_cx: &App) -> Hsla {
    rgb(BLURPLE).into()
}

/// The status card as Discord draws it, following every Discord setting: the name after
/// "Listening to", the provider badge, the details left out, and the card faded while the
/// status is off.
fn card(hover: Hover, unit: f32, cx: &App) -> Div {
    let theme = cx.theme();
    let sonora = Sonora::global(cx);
    let settings = sonora.settings.read(cx);
    let track = super::playing(cx).or_else(|| sonora.history.read(cx).tracks().first().cloned());
    let provider = sonora
        .session
        .read(cx)
        .providers()
        .find(|provider| provider.active);
    let hidden = settings.discord_without_details();
    let badge = settings
        .discord_badge()
        .then(|| provider.as_ref().map(|provider| provider.slug))
        .flatten();
    let name: SharedString = match (settings.discord_name(), &track) {
        (DiscordName::Sonora, _) => t!("settings-discord-name-sonora"),
        (DiscordName::Provider, _) => provider.as_ref().map_or_else(
            || t!("settings-discord-name-sonora"),
            |provider| provider.name.into(),
        ),
        (DiscordName::Title, Some(track)) => track.name.clone().into(),
        (DiscordName::Artist, Some(track)) => track.artists.clone().into(),
        (DiscordName::ArtistTitle, Some(track)) => {
            format!("{} - {}", track.artists, track.name).into()
        }
        _ => t!("settings-discord-name-music"),
    };
    let header = match hidden {
        true => t!("discord-listening"),
        false => t!("settings-hub-discord-listening", name = name.to_string()),
    };
    let length = track
        .as_ref()
        .map_or(Duration::ZERO, |track| track.duration);
    let played = sonora.playback.read(cx).position() + Duration::from_secs_f32(hover.clock * RUN);
    let played = match length.is_zero() {
        true => Duration::ZERO,
        false => Duration::from_secs_f32(played.as_secs_f32() % length.as_secs_f32()),
    };
    let progress = match length.is_zero() {
        true => 0.,
        false => played.as_secs_f32() / length.as_secs_f32(),
    };
    let size = |step: Text| theme.text(step) * hover.scale;
    let art = (unit * ART).round();
    let on = settings.discord_presence();

    div()
        .w(px((unit * CARD_WIDTH).round()))
        .p(px((unit * PAD).round()))
        .flex()
        .flex_col()
        .gap(px((unit * PAD * 0.7).round()))
        .rounded(px(unit * CARD_RADIUS))
        .bg(rgb(CARD))
        .shadow_lg()
        .when(!on, |card| card.opacity(OFF))
        .text_color(rgb(NORMAL))
        .child(
            div()
                .text_size(size(Text::Tiny))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(HEADER))
                .truncate()
                .child(header),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px((unit * PAD).round()))
                .child(
                    div()
                        .relative()
                        .flex_none()
                        .size(px(art))
                        .child(match (hidden, &track) {
                            (false, Some(track)) => Artwork::new(track.cover.clone())
                                .size(px(art))
                                .corner_radius(px(unit * ART_RADIUS))
                                .into_any_element(),
                            _ => div()
                                .size(px(art))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(unit * ART_RADIUS))
                                .bg(rgb(BLURPLE))
                                .child(
                                    svg()
                                        .path(icons::path("icons/sonora.svg"))
                                        .size(px(art * 0.6))
                                        .text_color(rgb(SHINE)),
                                )
                                .into_any_element(),
                        })
                        .when_some(badge, |art, slug| {
                            let badge = (unit * BADGE).round();
                            let ring = (unit * BADGE_RING).max(1.).round();
                            art.child(
                                div()
                                    .absolute()
                                    .right(px(-badge / 3.))
                                    .bottom(px(-badge / 3.))
                                    .size(px(badge + ring * 2.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(rgb(CARD))
                                    .child(
                                        div()
                                            .size(px(badge))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(super::providers::brand(slug, cx))
                                            .child(
                                                svg()
                                                    .path(icons::path(provider_logo(slug)))
                                                    .size(px((badge * 0.62).round()))
                                                    .text_color(rgb(CARD)),
                                            ),
                                    ),
                            )
                        }),
                )
                .when(!hidden, |row| {
                    row.child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .gap(px((unit * 0.015).round()))
                            .children(track.as_ref().map(|track| {
                                div()
                                    .text_size(size(Text::Small))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(HEADER))
                                    .truncate()
                                    .child(track.name.clone())
                            }))
                            .children(track.as_ref().map(|track| {
                                div()
                                    .text_size(size(Text::Tiny))
                                    .text_color(rgb(QUIET))
                                    .truncate()
                                    .child(track.artists.clone())
                            })),
                    )
                }),
        )
        .when(!length.is_zero(), |card| {
            card.child(
                div()
                    .mt(px((unit * (BAR_GAP - PAD * 0.7)).max(0.).round()))
                    .flex()
                    .flex_col()
                    .gap(px((unit * 0.03).round()))
                    .child(
                        div()
                            .h(px((unit * BAR).max(2.).round()))
                            .rounded_full()
                            .bg(rgb(TRACK))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(progress))
                                    .rounded_full()
                                    .bg(rgb(NORMAL)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(size(Text::Tiny))
                            .text_color(rgb(QUIET))
                            .font_features(ui::tabular())
                            .child(ui::clock(played))
                            .child(ui::clock(length)),
                    ),
            )
        })
}
