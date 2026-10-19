//! The settings hub: the search field over a grid of tiles, one per settings page, inside the
//! normal workspace. Each tile's art lives in a file of its own here, as a function of the hover
//! that draws it, and a function that names the colour its glow takes.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, Entity, Hsla, Pixels, Render, ScrollHandle, SpringConfig,
    SpringState, Window, div, linear_color_stop, linear_gradient, px,
};
use music::Track;
use router::{Destination, SettingsPage, navigate};
use state::Sonora;
use ui::{ActiveTheme as _, Dismiss, Hover, Room, Scrollbar, Scroller, Showcase};

use super::SettingsView;
use crate::chrome::Chrome;

mod about;
mod appearance;
mod discord;
mod general;
mod lyrics;
mod now_playing;
mod playback;
mod providers;
mod scrobbling;
mod sound;

/// How wide the grid grows at most, so the tiles stay pictures rather than banners on a wide
/// window.
const WIDTH: Pixels = px(1080.);
/// The spring a part of an illustration glides on when it steps from one place to the next.
const GLIDE: SpringConfig = SpringConfig::new(420., 34., 1.);
/// How strongly the key light from the top left brightens every illustration.
const SHEEN: f32 = 0.07;

/// What a tile's art draws for one frame of its hover.
type Art = fn(Hover, &mut Window, &mut App) -> AnyElement;
/// The colour a tile's glow takes.
type Glow = fn(&App) -> Hsla;

/// The hub view. It shares the settings search with the pages, so a query typed here finds the
/// same rows, and a hit opens its page scrolled to the row.
pub struct HubView {
    view: Entity<SettingsView>,
    scrollbar: Entity<Scrollbar>,
}

impl HubView {
    /// The hub redraws as the search is typed, as settings change and as the data its art draws
    /// changes. It does not follow the pages' view or playback as a whole, which redraw on every
    /// tick of playback.
    pub fn new(view: Entity<SettingsView>, cx: &mut Context<Self>) -> Self {
        follow_art(cx);
        let search = view.read(cx).search();
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        let me = cx.entity_id();

        Self {
            view,
            scrollbar: cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(me)),
        }
    }

    /// The grid of tiles: three columns from `Room::Wide` up, where Providers and Sound span two
    /// so the ten tiles fill four rows, two at `Room::Roomy`, and one below.
    fn tiles(&self, window: &Window, cx: &App) -> impl IntoElement {
        let columns: u16 = match Chrome::room(window, cx) {
            Room::Wide | Room::Vast => 3,
            Room::Roomy => 2,
            Room::Tight | Room::Snug => 1,
        };

        div()
            .grid()
            .grid_cols(columns)
            .gap_4()
            .children(SettingsPage::HUB.map(|page| {
                let (art, glow) = drawing(page);
                let wide =
                    columns == 3 && matches!(page, SettingsPage::Providers | SettingsPage::Sound);
                Showcase::new(page.id(), i18n::lookup(page.key(), None))
                    .glow(glow(cx))
                    .art(art)
                    .when(wide, |tile| tile.col_span(2))
                    .on_click(move |_, _, cx| navigate(Destination::Settings(page), cx))
            }))
    }
}

impl Render for HubView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let search = self.view.read(cx).search();
        let body = match self.view.read(cx).searching() {
            true => self
                .view
                .update(cx, |view, cx| view.results(window, cx))
                .into_any_element(),
            false => self.tiles(window, cx).into_any_element(),
        };

        Scroller::new("settings-hub", &self.scrollbar)
            .flex()
            .flex_col()
            .items_center()
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                if this.view.update(cx, |view, cx| view.escape(cx)) {
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.metrics.inset)
                    .w_full()
                    .max_w(WIDTH)
                    .p(theme.metrics.inset)
                    .child(search)
                    .child(body),
            )
    }
}

/// The art and the glow of the tile that opens `page`.
fn drawing(page: SettingsPage) -> (Art, Glow) {
    match page {
        SettingsPage::Providers => (providers::art, providers::glow),
        SettingsPage::General => (general::art, general::glow),
        SettingsPage::Appearance => (appearance::art, appearance::glow),
        SettingsPage::NowPlaying => (now_playing::art, now_playing::glow),
        SettingsPage::Lyrics | SettingsPage::LyricsSources => (lyrics::art, lyrics::glow),
        SettingsPage::Playback => (playback::art, playback::glow),
        SettingsPage::Sound => (sound::art, sound::glow),
        SettingsPage::Discord => (discord::art, discord::glow),
        SettingsPage::Scrobbling => (scrobbling::art, scrobbling::glow),
        SettingsPage::About => (about::art, about::glow),
        SettingsPage::MusicFolders => (providers::art, providers::glow),
    }
}

/// Redraws the hub when the data the tiles draw changes: the providers, the queue, the history,
/// the lyrics, the equalizer and the scrobbling links.
fn follow_art(cx: &mut Context<HubView>) {
    let sonora = Sonora::global(cx);
    let (session, queue, history) = (
        sonora.session.clone(),
        sonora.queue.clone(),
        sonora.history.clone(),
    );
    let (lyrics, equalizer, scrobbling) = (
        sonora.lyrics.clone(),
        sonora.equalizer.clone(),
        sonora.scrobbling.clone(),
    );
    cx.observe(&session, |_, _, cx| cx.notify()).detach();
    cx.observe(&queue, |_, _, cx| cx.notify()).detach();
    cx.observe(&history, |_, _, cx| cx.notify()).detach();
    cx.observe(&lyrics, |_, _, cx| cx.notify()).detach();
    cx.observe(&equalizer, |_, _, cx| cx.notify()).detach();
    cx.observe(&scrobbling, |_, _, cx| cx.notify()).detach();
}

/// The size every illustration is measured in: the cover size at the scale the art is laid out,
/// in pixels. A part of an illustration is a fraction of this, so the tiles keep their
/// proportions and land on the same pixels hovered or not.
fn unit(hover: Hover, cx: &App) -> f32 {
    cx.theme().metrics.cover / px(1.) * hover.scale
}

/// Whether an illustration is moving this frame. A moving part may sit between pixels, and a
/// still one is drawn on whole pixels.
fn moving(hover: Hover, cx: &App) -> bool {
    hover.hovered && !cx.reduce_motion()
}

/// Splits `position` into the whole pixel layout puts it on and the fraction left over, which
/// the compositor adds while `moving`, so a moving part glides rather than stepping from pixel to
/// pixel. At rest the fraction is dropped.
fn adrift(position: f32, moving: bool, window: &Window) -> (Pixels, Pixels) {
    let laid = ui::snapped(px(position), window);
    match moving {
        true => (laid, px(position) - laid),
        false => (laid, px(0.)),
    }
}

/// How far a part released `elapsed` seconds ago has glided from where it was to where it goes,
/// from 0 to 1, overshooting a little as the spring lands. Before its release it has not moved.
fn settle(elapsed: f32) -> f32 {
    match elapsed > 0. {
        true => {
            GLIDE
                .step(
                    SpringState {
                        position: 0.,
                        velocity: 0.,
                    },
                    1.,
                    elapsed,
                )
                .position
        }
        false => 0.,
    }
}

/// How many steps a sequence that steps every `period` seconds, the first `lead` seconds into
/// the hover, has taken by `clock`, with the step under way as the fraction. Each step glides
/// on `GLIDE`, so the count rises smoothly and rests on whole numbers between steps.
fn stepped(clock: f32, lead: f32, period: f32) -> f32 {
    let since = clock - lead;
    if since < 0. {
        return 0.;
    }
    let taken = (since / period).floor();
    taken + settle(since - taken * period)
}

/// The key light every illustration shares, a soft brightening from the top left corner, so the
/// tiles read as one family lit from one side.
fn sheen(cx: &App) -> Div {
    let theme = cx.theme();
    div().absolute().inset_0().bg(linear_gradient(
        135.,
        linear_color_stop(theme.foreground.opacity(SHEEN), 0.),
        linear_color_stop(theme.foreground.opacity(0.), 0.55),
    ))
}

/// A soft round glow of `color`, `size` across and blurred by `blur`, centred on `x`, `y`.
fn halo(color: Hsla, x: f32, y: f32, size: f32, blur: f32) -> Div {
    div()
        .absolute()
        .left(px(x - size / 2.))
        .top(px(y - size / 2.))
        .size(px(size))
        .rounded_full()
        .bg(color)
        .blur(px(blur))
}

/// The track playing, or the one the queue would resume, which is what the art that shows a
/// track draws.
fn playing(cx: &App) -> Option<Track> {
    let sonora = Sonora::global(cx);
    sonora
        .playback
        .read(cx)
        .track()
        .or_else(|| sonora.queue.read(cx).current())
        .cloned()
}

/// The colour the playing cover leads with: the theme's tint when the adaptive theme took one,
/// the cover's own leading hue once its art has been drawn, or the accent.
fn cover_tint(cx: &App) -> Hsla {
    if let Some(tint) = cx.theme().tint {
        return tint;
    }
    playing(cx)
        .and_then(|track| track.cover)
        .and_then(|cover| ui::cover_palette(&cover, cx))
        .and_then(|palette| palette.primary)
        .unwrap_or_else(|| tint(cx))
}

/// A stand-in for art still to be drawn: a wash from `from` to `to` across the tile, turning
/// slowly with the hover clock so the tile's motion can be judged.
fn placeholder(from: Hsla, to: Hsla, angle: f32, hover: Hover) -> AnyElement {
    div()
        .size_full()
        .bg(linear_gradient(
            (angle + hover.clock * 24.) % 360.,
            linear_color_stop(from, 0.),
            linear_color_stop(to, 1.),
        ))
        .into_any_element()
}

/// The theme's tint, the cover colour the adaptive theme took, or its primary without one.
fn tint(cx: &App) -> Hsla {
    let theme = cx.theme();
    theme.tint.unwrap_or(theme.primary)
}
