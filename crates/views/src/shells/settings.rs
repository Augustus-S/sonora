use gpui::prelude::*;
use gpui::{AnyView, App, Context, Entity, FocusHandle, FontWeight, Pixels, Render, SharedString};
use gpui::{Window, div, px};
use i18n::t;
use input::WORKSPACE_CONTEXT;
use router::{Destination, SettingsPage, navigate};
use state::{AppSettings, Playback, Sonora};
use ui::{ActiveTheme as _, Button, Card, Dismiss, Entrance, LEADING, Panel, Side, Text, entering};

use crate::chrome::{Chrome, TitleBarOptions, ToastStack, sidebar_width};
use crate::screens::equalizer::EqualizerView;
use crate::screens::settings::SettingsView;
use crate::shared::confirm::Confirm;
use crate::shared::nav::nav_row;
use crate::shared::transport;
use crate::shared::widevine::WidevinePrompt;
use crate::shells::Shell;

/// The settings shell. It takes the whole window while settings is open, with the rail of pages
/// on the left and the open page beside it, and has no sidebar, player bar or history of its own.
/// The equalizer is one of its pages, and closing returns to where the user was before.
pub(crate) struct SettingsShell {
    view: Entity<SettingsView>,
    equalizer: Entity<EqualizerView>,
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    confirm: Entity<Confirm>,
    widevine: Entity<WidevinePrompt>,
    toasts: Entity<ToastStack>,
    /// The settings destination on show, a page or the equalizer.
    shown: Destination,
    /// The entrance the page plays as another page replaces it.
    transition: Option<Entrance>,
    focus: FocusHandle,
}

impl SettingsShell {
    pub fn new(
        view: Entity<SettingsView>,
        equalizer: Entity<EqualizerView>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();

        Self {
            view,
            equalizer,
            playback,
            settings,
            confirm: Confirm::entity(cx),
            widevine: cx.new(WidevinePrompt::new),
            toasts: cx.new(ToastStack::new),
            shown: Destination::Settings(SettingsPage::Accounts),
            transition: None,
            focus: cx.focus_handle(),
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    /// Follows the route. The equalizer reads its meters only while it is the page on show, so
    /// every move tells it, settings or not.
    pub fn show(&mut self, destination: &Destination, cx: &mut Context<Self>) {
        let equalizer = matches!(destination, Destination::Equalizer);
        self.equalizer
            .update(cx, |view, cx| view.set_visible(equalizer, cx));
        if !destination.in_settings() {
            return;
        }
        if let Destination::Settings(page) = destination {
            let page = *page;
            self.view.update(cx, |view, cx| view.select(page, cx));
        }
        self.shown = destination.clone();
        cx.notify();
    }

    /// Plays the page entrance over the page area as one page replaces another.
    pub fn reveal_page(&mut self, cx: &mut Context<Self>) {
        self.transition = match cx.reduce_motion() {
            true => None,
            false => Some(Entrance::start()),
        };
        cx.notify();
    }

    /// How much of the page its entrance still hides, asking for the next frame while it runs.
    fn hidden(&mut self, window: &mut Window, cx: &Context<Self>) -> f32 {
        let Some(transition) = self.transition else {
            return 0.;
        };
        if cx.reduce_motion() || !transition.running() {
            self.transition = None;
            return 0.;
        }
        window.request_animation_frame();
        transition.hidden()
    }

    /// Answers escape inside settings: an open menu or dialog closes first, then the search
    /// clears. Returns false when there was nothing left to close, and the root closes settings.
    fn escape(&mut self, cx: &mut Context<Self>) -> bool {
        match self.shown {
            Destination::Equalizer => self.equalizer.update(cx, |view, _| view.escape()),
            _ => self.view.update(cx, |view, cx| view.escape(cx)),
        }
    }

    /// The rail: the heading with the close button, the search field, the pages in their
    /// clusters, and the playing track pinned to the bottom.
    fn rail(&self, window: &Window, cx: &mut Context<Self>) -> Panel {
        let theme = *cx.theme();
        let accent = theme.sidebar_accent;
        let view = self.view.read(cx);
        let searching = view.searching();
        let search = view.search();
        let current = self.shown.settings_page();
        // the heading stands as tall as the page title and as far down, so the two line up
        let title = px((theme.text(Text::Title) / px(1.) * LEADING).round());
        let lead = (theme.metrics.inset * 1.5 - window.rem_size() * 0.75).max(Pixels::ZERO);
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let radius = crate::chrome::window_radius(self.settings.read(cx), cx, window);
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let radius: Option<Pixels> = None;

        let rows = SettingsPage::RAIL.map(|page| {
            // a search lights no page, since its results come from all of them
            let active = !searching && current == Some(page);
            let tint = match active {
                true => theme.foreground,
                false => theme.muted_foreground,
            };
            nav_row(page.id(), page.key(), tint, accent)
                .icon(page.icon())
                .w_full()
                .when(page.opens_cluster(), |row| row.mt(theme.metrics.pad))
                .when(active, |row| row.bg(accent))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.view.update(cx, |view, cx| view.clear_search(cx));
                    navigate(Destination::Settings(page), cx);
                }))
        });

        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap_2()
            .h(title.max(theme.metrics.control))
            .mt(lead)
            .pl_2()
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(theme.text(Text::Large))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t!("nav-settings")),
            )
            .child(
                Button::new("settings-close")
                    .ghost()
                    .icon("icons/x.svg")
                    .size_8()
                    .px_0()
                    .tooltip("settings-close")
                    .on_click(|_, _, cx| router::close_settings(cx)),
            );

        let chip = self.playback.read(cx).track().map(|track| {
            Card::new(
                "settings-now-playing",
                SharedString::from(track.name.clone()),
            )
            .cover(track.cover.clone())
            .art(theme.metrics.thumb)
            .meta(SharedString::from(track.artists.clone()))
            .flat()
            .h(theme.metrics.list_row)
            .trailing(transport::toggle(&self.playback, false, false, cx))
        });

        Panel::new(
            "settings-rail",
            Side::Left,
            sidebar_width(self.settings.read(cx)),
        )
        .when(!theme.transparent, |this| this.bg(theme.sidebar))
        .border_color(theme.sidebar_border)
        .when_some(radius, |this, radius| this.rounded_bl(radius))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap_1()
                .p_3()
                .overflow_hidden()
                .child(header)
                .child(div().flex_none().mb(theme.metrics.pad).child(search))
                .children(rows),
        )
        .children(chip.map(|chip| div().flex_none().p_3().child(chip)))
    }
}

impl Shell for SettingsShell {
    fn title_bar(&self, _content: Option<AnyView>, _cx: &App) -> TitleBarOptions {
        TitleBarOptions {
            navigation: false,
            sidebar_open: false,
            sidebar_right: None,
            offset: Pixels::ZERO,
            border: false,
            content: None,
            transparent: false,
        }
    }
}

impl Render for SettingsShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rail = sidebar_width(self.settings.read(cx));
        Chrome::publish(rail, Pixels::ZERO, Pixels::ZERO, cx);
        let hidden = self.hidden(window, cx);
        let page = match self.shown {
            Destination::Equalizer => self.equalizer.clone().into_any_element(),
            _ => self.view.clone().into_any_element(),
        };

        div()
            .relative()
            .flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .key_context(super::shell_context(WORKSPACE_CONTEXT))
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                if this.escape(cx) {
                    cx.stop_propagation();
                }
            }))
            .child(self.rail(window, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .when(hidden > 0., |this| this.overflow_hidden())
                            .map(|this| entering(this, hidden))
                            .child(page),
                    )
                    // the toasts hang from the bottom edge, where the player bar would be
                    .child(div().relative().w_full().child(self.toasts.clone())),
            )
            .child(self.confirm.clone())
            .child(self.widevine.clone())
    }
}
