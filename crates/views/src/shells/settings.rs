mod sidebar;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyView, App, Context, Entity, FocusHandle, Pixels, Render, Window, div};
use input::WORKSPACE_CONTEXT;
use router::{Destination, SettingsPage};
use ui::{ActiveTheme as _, Beat, Dismiss, Entrance, Spring, entering};

use crate::chrome::{Chrome, TitleBarOptions, ToastStack};
use crate::screens::equalizer::EqualizerView;
use crate::screens::settings::{Layout, SettingsView};
use crate::shared::confirm::Confirm;
use crate::shared::widevine::WidevinePrompt;
use crate::shells::Shell;
use sidebar::{Entry, GLIDE, Light};

/// The settings takeover. It fills the window while a settings page is open, with no app
/// sidebar, player bar or history, and lays out like a documentation site: one container centred
/// in the window holding the sidebar of pages, and the page beside it with its own header and
/// preview. The shell draws the sidebar over the page, which fills the shell and leaves the
/// sidebar's place free. Closing returns to the hub.
pub(crate) struct SettingsShell {
    view: Entity<SettingsView>,
    equalizer: Entity<EqualizerView>,
    confirm: Entity<Confirm>,
    widevine: Entity<WidevinePrompt>,
    toasts: Entity<ToastStack>,
    /// The settings destination on show, a page or the equalizer.
    shown: Destination,
    /// The entrance the page plays as another page replaces it.
    transition: Option<Entrance>,
    /// Where the active entry's pill is, as a distance from the top of the sidebar's list.
    pill: Spring,
    beat: Beat,
    /// Whether the pill has landed anywhere yet. The first place it learns it jumps to.
    placed: bool,
    /// Where each entry stands in the sidebar's list, its top and height, as the last layout put
    /// it. The pill reads its target from here.
    places: Rc<RefCell<Vec<(Entry, Pixels, Pixels)>>>,
    /// The fade between the icon of the entry last on show and the one on show now.
    light: Light,
    focus: FocusHandle,
}

impl SettingsShell {
    pub fn new(
        view: Entity<SettingsView>,
        equalizer: Entity<EqualizerView>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&view, |_, _, cx| cx.notify()).detach();

        Self {
            view,
            equalizer,
            confirm: Confirm::entity(cx),
            widevine: cx.new(WidevinePrompt::new),
            toasts: cx.new(ToastStack::new),
            shown: Destination::Settings(SettingsPage::Providers),
            transition: None,
            pill: Spring::new(GLIDE),
            beat: Beat::default(),
            placed: false,
            places: Rc::default(),
            light: Light::default(),
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

    /// Plays the page entrance over the content and preview as one page replaces another. The
    /// sidebar stays still, and only its pill moves.
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
    /// clears. Returns false when there was nothing left to close, and the root returns to the
    /// hub.
    fn escape(&mut self, cx: &mut Context<Self>) -> bool {
        match self.shown {
            Destination::Equalizer => self.equalizer.update(cx, |view, _| view.escape()),
            _ => self.view.update(cx, |view, cx| view.escape(cx)),
        }
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
        let layout = Layout::of(&self.shown, window, cx);
        // the pages lay their rows and cards out for the width of the content column
        let viewport = window.viewport_size().width;
        Chrome::publish(viewport - layout.width, Pixels::ZERO, Pixels::ZERO, cx);
        let hidden = self.hidden(window, cx);
        let page = match self.shown {
            Destination::Equalizer => self.equalizer.clone().into_any_element(),
            _ => self.view.clone().into_any_element(),
        };
        let sidebar = layout
            .sidebar
            .map(|left| self.sidebar(layout, left, window, cx));

        div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .bg(cx.theme().background)
            .key_context(super::shell_context(WORKSPACE_CONTEXT))
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                if this.escape(cx) {
                    cx.stop_propagation();
                }
            }))
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
            .children(sidebar)
            // the toasts hang from the bottom edge, where the player bar would be
            .child(div().relative().w_full().child(self.toasts.clone()))
            .child(self.confirm.clone())
            .child(self.widevine.clone())
    }
}
