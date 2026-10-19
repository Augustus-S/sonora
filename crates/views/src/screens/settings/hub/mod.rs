//! The settings hub: the search field over a grid of tiles, one per settings page, inside the
//! normal workspace. Each tile's art lives in a file of its own here, as a function of the hover
//! that draws it, and a function that names the colour its glow takes.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, Hsla, Pixels, Render, ScrollHandle, Window, div,
    linear_color_stop, linear_gradient, px,
};
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
    pub fn new(view: Entity<SettingsView>, cx: &mut Context<Self>) -> Self {
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
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
