use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, AnyView, App, Bounds, Context, Entity, FocusHandle, Pixels, Render, SpringConfig,
    Window, div, point,
};
use input::WORKSPACE_CONTEXT;
use router::{Destination, SettingsPage, navigate};
use ui::{ActiveTheme as _, Beat, Dismiss, Entrance, Spring, Text, entering, eyebrow, snapped};

use crate::chrome::{Chrome, TitleBarOptions, ToastStack};
use crate::screens::equalizer::EqualizerView;
use crate::screens::settings::{Layout, SIDEBAR, SettingsView};
use crate::shared::confirm::Confirm;
use crate::shared::widevine::WidevinePrompt;
use crate::shells::Shell;

/// The spring the active entry's pill glides to a newly chosen entry on.
const GLIDE: SpringConfig = SpringConfig::new(420., 34., 1.);

/// One entry of the sidebar: a page, or the equalizer tucked under Sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Page(SettingsPage),
    Equalizer,
}

/// The sidebar's groups, each under its eyebrow, top to bottom.
const GROUPS: [(&str, &[Entry]); 5] = [
    (
        "settings-group-you",
        &[
            Entry::Page(SettingsPage::Providers),
            Entry::Page(SettingsPage::General),
        ],
    ),
    (
        "settings-group-look",
        &[
            Entry::Page(SettingsPage::Appearance),
            Entry::Page(SettingsPage::NowPlaying),
            Entry::Page(SettingsPage::Lyrics),
        ],
    ),
    (
        "settings-group-listen",
        &[
            Entry::Page(SettingsPage::Playback),
            Entry::Page(SettingsPage::Sound),
            Entry::Equalizer,
        ],
    ),
    (
        "settings-group-share",
        &[
            Entry::Page(SettingsPage::Discord),
            Entry::Page(SettingsPage::Scrobbling),
        ],
    ),
    ("settings-group-sonora", &[Entry::Page(SettingsPage::About)]),
];

impl Entry {
    /// The entry that stands for `destination`: the page itself, a drill-in's parent, or the
    /// equalizer.
    fn of(destination: &Destination) -> Option<Self> {
        match destination {
            Destination::Equalizer => Some(Self::Equalizer),
            destination => destination.settings_page().map(Self::Page),
        }
    }

    fn destination(self) -> Destination {
        match self {
            Self::Page(page) => Destination::Settings(page),
            Self::Equalizer => Destination::Equalizer,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Page(page) => page.key(),
            Self::Equalizer => "settings-equalizer",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Page(page) => page.id(),
            Self::Equalizer => "equalizer",
        }
    }
}

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

    /// Moves the pill towards the active entry and says where it is drawn, or `None` while
    /// nothing is lit. It glides on a spring and asks for frames only while it moves.
    fn pill(&mut self, active: Option<Entry>, window: &mut Window, cx: &App) -> Option<Pill> {
        let (top, height) = {
            let places = self.places.borrow();
            let entry = active?;
            places
                .iter()
                .find(|(place, ..)| *place == entry)
                .map(|(_, top, height)| (*top, *height))?
        };
        self.pill.aim(top.as_f32());
        let elapsed = self.beat.tick();
        match self.placed && !cx.reduce_motion() {
            true => self.pill.step(elapsed),
            false => self.pill.finish(),
        }
        self.placed = true;
        match self.pill.settled() {
            true => self.beat.stop(),
            false => window.request_animation_frame(),
        }
        Some(Pill {
            top,
            height,
            offset: Pixels::from(self.pill.at()) - top,
        })
    }

    /// The sidebar: the search field with its shortcut, then the pages in their groups, each
    /// under its eyebrow, with the active one on a pill.
    fn sidebar(
        &mut self,
        layout: Layout,
        left: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        // the pill reaches out past the text on the left only, so its right edge lines up with
        // the search field above
        let reach = theme.metrics.pad * 1.5;
        let view = self.view.read(cx);
        let search = view.search();
        // a search lights no page, since its results come from all of them
        let active = match view.searching() {
            true => None,
            false => Entry::of(&self.shown),
        };
        let pill = self.pill(active, window, cx);

        let mut entries = Vec::new();
        let mut children = Vec::new();
        for (index, (key, group)) in GROUPS.iter().enumerate() {
            entries.push(None);
            children.push(
                eyebrow(i18n::lookup(key, None), cx)
                    .when(index > 0, |heading| heading.mt_6())
                    .mb_1()
                    .into_any_element(),
            );
            for entry in group.iter().copied() {
                let lit = active == Some(entry);
                entries.push(Some(entry));
                children.push(
                    div()
                        .id(entry.id())
                        .flex()
                        .items_center()
                        .h(theme.metrics.control)
                        .ml(-reach)
                        .pl(reach)
                        .pr(reach)
                        .when(entry == Entry::Equalizer, |row| row.pl(reach * 2.))
                        .rounded(theme.radius)
                        .cursor_pointer()
                        .text_size(theme.text(Text::Body))
                        .map(|row| match lit {
                            true => row.text_color(theme.foreground),
                            false => row
                                .text_color(theme.muted_foreground)
                                .hover(|style| style.text_color(theme.foreground)),
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.view.update(cx, |view, cx| view.clear_search(cx));
                            navigate(entry.destination(), cx);
                        }))
                        .child(div().truncate().child(i18n::lookup(entry.key(), None)))
                        .into_any_element(),
                );
            }
        }

        let places = self.places.clone();
        let shell = cx.entity().downgrade();
        let list = div()
            .flex()
            .flex_col()
            .on_children_prepainted(move |bounds: Vec<Bounds<Pixels>>, window, cx| {
                let Some(first) = bounds.first() else {
                    return;
                };
                let origin = first.origin.y;
                let measured = entries
                    .iter()
                    .zip(&bounds)
                    .filter_map(|(entry, bounds)| {
                        entry.map(|entry| {
                            let top = snapped(bounds.origin.y - origin, window);
                            (entry, top, bounds.size.height)
                        })
                    })
                    .collect::<Vec<_>>();
                if *places.borrow() != measured {
                    *places.borrow_mut() = measured;
                    shell.update(cx, |_, cx| cx.notify()).ok();
                }
            })
            .children(children);

        div()
            .absolute()
            .top(layout.top)
            .left(left)
            .w(SIDEBAR)
            .flex()
            .flex_col()
            .child(div().flex_none().child(search))
            .child(
                div()
                    .relative()
                    .mt(layout.header - layout.top - theme.metrics.field)
                    .children(pill.map(|pill| {
                        div()
                            .absolute()
                            .top(pill.top)
                            .h(pill.height)
                            .left(-reach)
                            .right_0()
                            .rounded(theme.radius)
                            .bg(theme.secondary)
                            .when(pill.offset != Pixels::ZERO, |pill_box| {
                                pill_box.layer_translate(point(Pixels::ZERO, pill.offset))
                            })
                    }))
                    .child(list),
            )
            .into_any_element()
    }
}

/// Where the pill is drawn: laid out on the entry's whole-pixel place, and shifted from there by
/// a sub-pixel `offset` while it glides.
struct Pill {
    top: Pixels,
    height: Pixels,
    offset: Pixels,
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
        let layout = Layout::of(false, window, cx);
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
