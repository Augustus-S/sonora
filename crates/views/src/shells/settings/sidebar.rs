//! The settings takeover's sidebar: the search field, then every page under its group's eyebrow
//! with a muted icon before its label, and the active one on a gliding pill.

use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Context, Div, Hsla, Pixels, SpringConfig, Stateful, Svg, Window, div,
    point, px, svg,
};
use router::{Destination, SettingsPage, navigate};
use ui::{ActiveTheme as _, Text, Theme, css_ease_out, eyebrow, mix, snapped};

use super::SettingsShell;
use crate::screens::settings::{Layout, SIDEBAR};

/// The spring the active entry's pill glides to a newly chosen entry on.
pub(super) const GLIDE: SpringConfig = SpringConfig::new(420., 34., 1.);
/// How long the icon of a newly chosen entry takes to light, and the last one's to go out.
const LIGHT: Duration = Duration::from_millis(180);
/// The icon before a page's label, and before the equalizer nested under Sound.
const ICON: Pixels = px(16.);
const NESTED_ICON: Pixels = px(14.);

/// One entry of the sidebar: a page, or the equalizer tucked under Sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Entry {
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

/// Where the pill is drawn: laid out on the entry's whole-pixel place, and shifted from there by
/// a sub-pixel `offset` while it glides.
struct Pill {
    top: Pixels,
    height: Pixels,
    offset: Pixels,
}

/// The fade that lights the icon of a newly chosen entry and puts out the last one's.
#[derive(Debug, Default)]
pub(super) struct Light {
    now: Option<Entry>,
    before: Option<Entry>,
    since: Option<Instant>,
}

impl Entry {
    /// The entry that stands for `destination`: the page itself, a drill-in's parent, or the
    /// equalizer.
    pub(super) fn of(destination: &Destination) -> Option<Self> {
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

    fn icon(self) -> &'static str {
        match self {
            Self::Page(SettingsPage::Providers | SettingsPage::MusicFolders) => {
                "icons/user-round.svg"
            }
            Self::Page(SettingsPage::General) => "icons/settings.svg",
            Self::Page(SettingsPage::Appearance) => "icons/palette.svg",
            Self::Page(SettingsPage::NowPlaying) => "icons/maximize.svg",
            Self::Page(SettingsPage::Lyrics | SettingsPage::LyricsSources) => "icons/mic-vocal.svg",
            Self::Page(SettingsPage::Playback) => "icons/play.svg",
            Self::Page(SettingsPage::Sound) => "icons/volume-2.svg",
            Self::Equalizer => "icons/sliders-horizontal.svg",
            Self::Page(SettingsPage::Discord) => "icons/radio.svg",
            Self::Page(SettingsPage::Scrobbling) => "icons/list-music.svg",
            Self::Page(SettingsPage::About) => "icons/info.svg",
        }
    }

    fn nested(self) -> bool {
        self == Self::Equalizer
    }

    /// The entry's row, lit by `lit` from 0 to 1 as it becomes the page on show. The pill
    /// reaches `reach` past the column on the left, so the row does too. An `active` row keeps
    /// its colours under the pointer.
    fn row(self, lit: f32, active: bool, reach: Pixels, theme: &Theme) -> Stateful<Div> {
        let gap = theme.metrics.pad * 1.25;
        let ink = mix(theme.muted_foreground, theme.foreground, lit);
        let hover = (!active).then_some(theme.foreground);
        let size = match self.nested() {
            true => NESTED_ICON,
            false => ICON,
        };

        div()
            .id(self.id())
            .group(self.id())
            .flex()
            .items_center()
            .gap(gap)
            .h(theme.metrics.control)
            .ml(-reach)
            .pl(reach)
            .pr(reach)
            // the nested equalizer starts where its parent's label does
            .when(self.nested(), |row| row.pl(reach + ICON + gap))
            .rounded(theme.radius)
            .cursor_pointer()
            .text_size(theme.text(Text::Body))
            .text_color(ink)
            .when(!active, |row| {
                row.hover(|style| style.text_color(theme.foreground))
            })
            .child(icon(self, size, ink, hover))
            .child(div().truncate().child(i18n::lookup(self.key(), None)))
    }
}

impl Light {
    /// Follows the active entry and says how far the change to it has gone, from 0 to 1 eased.
    /// It asks for the next frame while the fade runs, and lands at once under reduced motion.
    fn follow(&mut self, active: Option<Entry>, window: &mut Window, cx: &App) -> f32 {
        if active != self.now {
            self.before = self.now;
            self.now = active;
            self.since = Some(Instant::now());
        }
        let Some(since) = self.since.filter(|_| !cx.reduce_motion()) else {
            return 1.;
        };
        let progress = since.elapsed().as_secs_f32() / LIGHT.as_secs_f32();
        if progress >= 1. {
            self.since = None;
            return 1.;
        }
        window.request_animation_frame();
        css_ease_out(progress)
    }

    /// How lit `entry` is at `progress` through the change: rising if it is the one now chosen,
    /// falling if it was the one before.
    fn level(&self, entry: Entry, progress: f32) -> f32 {
        match (Some(entry) == self.now, Some(entry) == self.before) {
            (true, _) => progress,
            (false, true) => 1. - progress,
            (false, false) => 0.,
        }
    }
}

impl SettingsShell {
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
    pub(super) fn sidebar(
        &mut self,
        layout: Layout,
        left: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        // the pill reaches out past the icons on the left only, so its right edge lines up with
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
        let progress = self.light.follow(active, window, cx);

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
                let lit = self.light.level(entry, progress);
                entries.push(Some(entry));
                children.push(
                    entry
                        .row(lit, active == Some(entry), reach, &theme)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.view.update(cx, |view, cx| view.clear_search(cx));
                            navigate(entry.destination(), cx);
                        }))
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

/// The icon of `entry` at `size` in `color`, turning `hover` while the pointer is on its row.
fn icon(entry: Entry, size: Pixels, color: Hsla, hover: Option<Hsla>) -> Svg {
    svg()
        .path(icons::path(entry.icon()))
        .flex_none()
        .size(size)
        .text_color(color)
        .when_some(hover, |icon, hover| {
            icon.group_hover(entry.id(), move |style| style.text_color(hover))
        })
}
