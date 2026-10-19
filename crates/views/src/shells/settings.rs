mod sidebar;

use gpui::prelude::*;
use gpui::{AnyView, App, Context, Entity, FocusHandle, Pixels, Render, Window, div};
use input::{SETTINGS_CONTEXT, WORKSPACE_CONTEXT};
use router::{Destination, SettingsPage};
use ui::{ActiveTheme as _, Button, Dismiss, Entrance, Text, entering};

use crate::chrome::{Chrome, TitleBarOptions, ToastStack};
use crate::screens::equalizer::EqualizerView;
use crate::screens::settings::{Layout, SettingsView};
use crate::shared::confirm::Confirm;
use crate::shared::widevine::WidevinePrompt;
use crate::shells::Shell;

/// The settings takeover. It fills the window while a settings page is open, with no app
/// sidebar, player bar or history, and lays out like Discord's settings: a still sidebar of
/// pages against the window's left edge, the page in one column centred in the room beside it,
/// and a close button by the column's top right corner. The page fills the shell and the shell
/// draws the sidebar and the close button over it. Closing returns to where settings opened from.
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
    /// Whether a settings destination was on show after the last move, so the view hears when
    /// settings open and close.
    open: bool,
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
            open: false,
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
        let open = destination.in_settings();
        if open != self.open {
            self.open = open;
            self.view.update(cx, |view, _| view.set_open(open));
        }
        if !open {
            return;
        }
        if let Destination::Settings(page) = destination {
            let page = *page;
            self.view.update(cx, |view, cx| view.select(page, cx));
        }
        self.shown = destination.clone();
        cx.notify();
    }

    /// Plays the page entrance over the content as one page replaces another. The sidebar and
    /// the close button stay still.
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
    /// clears. Returns false when there was nothing left to close, and settings close instead.
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
            border: true,
            content: None,
            transparent: false,
        }
    }
}

impl Render for SettingsShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = Layout::of(window, cx);
        // the pages lay their rows and cards out for the width of the content column
        let viewport = window.viewport_size().width;
        Chrome::publish(viewport - layout.width, Pixels::ZERO, Pixels::ZERO, cx);
        let hidden = self.hidden(window, cx);
        let page = match self.shown {
            Destination::Equalizer => self.equalizer.clone().into_any_element(),
            _ => self.view.clone().into_any_element(),
        };
        let sidebar = self.sidebar(layout, cx);

        div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .bg(cx.theme().background)
            .key_context(super::shell_context(&[WORKSPACE_CONTEXT, SETTINGS_CONTEXT]))
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                if !this.escape(cx) {
                    router::close_settings(cx);
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
            .child(sidebar)
            .child(close(layout, cx))
            // the toasts hang from the bottom edge, where the player bar would be
            .child(div().relative().w_full().child(self.toasts.clone()))
            .child(self.confirm.clone())
            .child(self.widevine.clone())
    }
}

/// The close button by the column's top right corner, an outlined cross with the key that
/// does the same written small under it.
fn close(layout: Layout, cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .absolute()
        .top(layout.top)
        .left(layout.close)
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .child(
            Button::new("settings-close")
                .outline()
                .icon("icons/x.svg")
                .size(theme.metrics.control)
                .px_0()
                .rounded(theme.radius)
                .tooltip("settings-close")
                .on_click(|_, _, cx| router::close_settings(cx)),
        )
        .child(
            div()
                .text_size(theme.text(Text::Tiny))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child(i18n::lookup("settings-close-key", None)),
        )
}
