//! The settings takeover's sidebar: the search field, then every page under its group's eyebrow
//! with a muted icon before its label, and the active one on a pill. Nothing in it moves, so a
//! page change repaints the old and new entries in their final colours on the same frame.

use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, Hsla, Pixels, Stateful, Svg, div, px, svg};
use router::{Destination, SettingsPage, navigate};
use ui::{ActiveTheme as _, Panel, Side, Text, Theme, eyebrow};

use super::SettingsShell;
use crate::screens::settings::{Layout, SIDEBAR};

/// The icon before a page's label.
const ICON: Pixels = px(16.);

/// One entry of the sidebar, standing for a page and the drill-ins under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry(SettingsPage);

/// The sidebar's groups, each under its eyebrow, top to bottom.
const GROUPS: [(&str, &[Entry]); 5] = [
    (
        "settings-group-you",
        &[Entry(SettingsPage::Providers), Entry(SettingsPage::General)],
    ),
    (
        "settings-group-look",
        &[
            Entry(SettingsPage::Appearance),
            Entry(SettingsPage::NowPlaying),
            Entry(SettingsPage::Lyrics),
        ],
    ),
    (
        "settings-group-listen",
        &[Entry(SettingsPage::Playback), Entry(SettingsPage::Sound)],
    ),
    (
        "settings-group-share",
        &[
            Entry(SettingsPage::Discord),
            Entry(SettingsPage::Scrobbling),
        ],
    ),
    ("settings-group-sonora", &[Entry(SettingsPage::About)]),
];

impl Entry {
    /// The entry that stands for `destination`: the page itself, or the parent of a drill-in
    /// such as the equalizer under Sound.
    fn of(destination: &Destination) -> Option<Self> {
        destination.settings_page().map(Self)
    }

    fn destination(self) -> Destination {
        Destination::Settings(self.0)
    }

    fn id(self) -> &'static str {
        self.0.id()
    }

    fn icon(self) -> &'static str {
        self.0.icon()
    }

    /// The entry's row. An `active` row sits on the pill in the foreground and keeps its
    /// colours under the pointer, and any other is muted until hovered.
    fn row(self, active: bool, theme: &Theme) -> Stateful<Div> {
        let ink = match active {
            true => theme.foreground,
            false => theme.muted_foreground,
        };
        let hover = (!active).then_some(theme.foreground);

        div()
            .id(self.id())
            .group(self.id())
            .flex()
            .items_center()
            .gap(theme.metrics.pad * 1.25)
            .h(theme.metrics.control)
            .px(theme.metrics.pad * 1.25)
            .rounded(theme.radius)
            .cursor_pointer()
            .text_size(theme.text(Text::Body))
            .text_color(ink)
            .when(active, |row| row.bg(theme.secondary))
            .when(!active, |row| {
                row.hover(|style| {
                    style
                        .text_color(theme.foreground)
                        .bg(theme.secondary.opacity(0.5))
                })
            })
            .child(icon(self, ink, hover))
            .child(div().truncate().child(i18n::lookup(self.0.key(), None)))
    }
}

impl SettingsShell {
    /// The sidebar against the window's left edge: the search field with its shortcut, then the
    /// pages in their groups, each under its eyebrow, with the active one on a pill.
    pub(super) fn sidebar(&mut self, layout: Layout, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let view = self.view.read(cx);
        let search = view.search();
        // a search lights no page, since its results come from all of them
        let active = match view.searching() {
            true => None,
            false => Entry::of(&self.shown),
        };

        let mut children = Vec::new();
        for (index, (key, group)) in GROUPS.iter().enumerate() {
            children.push(
                eyebrow(i18n::lookup(key, None), cx)
                    .px(theme.metrics.pad * 1.25)
                    .when(index > 0, |heading| heading.mt_5())
                    .mb_1()
                    .into_any_element(),
            );
            for entry in group.iter().copied() {
                children.push(
                    entry
                        .row(active == Some(entry), &theme)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.view.update(cx, |view, cx| view.clear_search(cx));
                            navigate(entry.destination(), cx);
                        }))
                        .into_any_element(),
                );
            }
        }

        Panel::new("settings-sidebar", Side::Left, SIDEBAR)
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .when(!theme.transparent, |panel| panel.bg(theme.sidebar))
            .border_color(theme.sidebar_border)
            .child(
                div()
                    .flex_none()
                    .px(theme.metrics.pad * 2.)
                    .pt(layout.top)
                    .child(search),
            )
            .child(
                div()
                    .id("settings-sidebar-pages")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .px(theme.metrics.pad * 2.)
                    .pt(theme.metrics.inset)
                    .pb(theme.metrics.inset)
                    .children(children),
            )
            .into_any_element()
    }
}

/// The icon of `entry` in `color`, turning `hover` while the pointer is on its row.
fn icon(entry: Entry, color: Hsla, hover: Option<Hsla>) -> Svg {
    svg()
        .path(icons::path(entry.icon()))
        .flex_none()
        .size(ICON)
        .text_color(color)
        .when_some(hover, |icon, hover| {
            icon.group_hover(entry.id(), move |style| style.text_color(hover))
        })
}
