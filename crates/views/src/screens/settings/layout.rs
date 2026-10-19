//! Where the settings takeover puts its columns. One container sits centred in the window and
//! holds the sidebar, the content column and, on the pages that have one, the preview column.
//! The shell draws the sidebar and the page draws the rest, so both work the places out here
//! from the same window and land on the same pixels.

use gpui::prelude::*;
use gpui::{AnyElement, App, Div, Entity, Pixels, Window, div, px};
use router::{Destination, SettingsPage};
use ui::{ActiveTheme as _, Breadcrumbs, Scrollbar, Scroller, VeilEdge, effects, veil};

/// How wide the sidebar stands.
pub(crate) const SIDEBAR: Pixels = px(220.);
/// How wide the content column grows at most.
const COLUMN: Pixels = px(640.);
/// How wide the preview column stands beside the content.
const PREVIEW: Pixels = px(380.);
/// The narrowest the content column may get with a preview beside it. Narrower than this and
/// the preview moves into the column, above the settings.
const NARROWEST: Pixels = px(440.);
/// How far the rows are blurred where they pass under the header.
const HEADER_BLUR: Pixels = px(1.);

/// The places of the takeover's columns in a window, as distances from its left edge.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout {
    /// Where the sidebar starts, or `None` when the window is too narrow to keep one.
    pub(crate) sidebar: Option<Pixels>,
    /// Where the content column starts and how wide it is.
    pub(crate) column: Pixels,
    pub(crate) width: Pixels,
    /// Where the preview column starts, when it stands beside the content.
    pub(crate) preview: Option<Pixels>,
    /// How far down the sidebar's search field and the header's trail start.
    pub(crate) top: Pixels,
    /// How tall the band the header floats in is. The content rests below it.
    pub(crate) header: Pixels,
    /// Where the scrolling content is cut off, through the middle of the trail. Rows rising past
    /// the header dissolve on the way up and are gone by this line, so none ever sits behind
    /// the trail's text.
    pub(crate) cut: Pixels,
}

impl Layout {
    /// The places for a page that has a preview to show, or none.
    pub(crate) fn of(preview: bool, window: &Window, cx: &App) -> Self {
        let metrics = cx.theme().metrics;
        let gap = metrics.inset * 2.;
        let margin = metrics.inset;
        let viewport = window.viewport_size().width;
        let room = (viewport - margin * 2.).max(Pixels::ZERO);
        let full = SIDEBAR + gap + COLUMN + gap + PREVIEW;
        let container = room.min(full);
        let start = ((viewport - container) / 2.).round();
        let sided = container - SIDEBAR - gap >= NARROWEST;
        let (sidebar, region) = match sided {
            true => (Some(start), start + SIDEBAR + gap),
            false => (None, start),
        };
        let space = start + container - region;
        let beside = preview && space - gap - PREVIEW >= NARROWEST;
        let (column, width, preview) = match beside {
            true => {
                let width = (space - gap - PREVIEW).min(COLUMN);
                (region, width, Some(region + width + gap))
            }
            false => {
                let width = space.min(COLUMN);
                (region + ((space - width) / 2.).round(), width, None)
            }
        };
        let top = metrics.pad * 2.;
        let header = top + metrics.field + metrics.inset * 1.5;

        Self {
            sidebar,
            column,
            width,
            preview,
            top,
            header,
            cut: top + (metrics.field / 2.).round(),
        }
    }

    /// How wide the preview column stands.
    pub(crate) fn preview_width(&self) -> Pixels {
        PREVIEW
    }

    /// How far below the cut the content rests, which is how far it dissolves across as it
    /// rises past the header.
    pub(crate) fn lead(&self) -> Pixels {
        self.header - self.cut
    }

    /// Fades the scrolling content out from the header's bottom up to the cut, so the rows
    /// dissolve as they pass beneath the trail rather than meeting the blur at a line. At rest
    /// the first row sits just below the fade, whole.
    pub(crate) fn fade<E: Styled>(&self, element: E) -> E {
        match effects() {
            true => element.fade_edges(self.lead(), Pixels::ZERO),
            false => element,
        }
    }
}

/// The header floating over the content column: the trail to the page on show over a band that
/// blurs and fades the rows passing under it. It fills the top of the page and paints over it.
pub(crate) fn header(
    destination: &Destination,
    layout: Layout,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();

    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(layout.header)
        // The haze follows the window: a see-through page has content of its own passing under
        // the header and reads worse without it. Only the flat fallback is dropped there, since
        // a solid band over a see-through page is a slab.
        .when(effects() || !theme.transparent, |band| {
            band.child(veil(
                VeilEdge::Top,
                layout.header,
                HEADER_BLUR,
                theme.background,
                None,
                window,
            ))
        })
        .child(
            div()
                .absolute()
                .top(layout.top)
                .left(layout.column)
                .w(layout.width)
                .h(theme.metrics.field)
                .flex()
                .items_center()
                .child(trail(destination)),
        )
        .into_any_element()
}

/// The trail to `destination`: Settings, which leaves for the hub, then the page a drill-in
/// opened from, then the page itself.
fn trail(destination: &Destination) -> Breadcrumbs {
    let settings = Breadcrumbs::new("settings-trail")
        .link(i18n::lookup("nav-settings", None), |_, _, cx| {
            router::close_settings(cx)
        });
    let (parent, page) = match destination {
        Destination::Equalizer => (Some(SettingsPage::Sound), "settings-equalizer"),
        Destination::Settings(page) => (page.parent(), page.key()),
        _ => (None, "nav-settings"),
    };

    settings
        .when_some(parent, |trail, parent| {
            trail.link(i18n::lookup(parent.key(), None), move |_, _, cx| {
                router::up(Destination::Settings(parent), cx)
            })
        })
        .item(i18n::lookup(page, None))
}

/// Lays a takeover page out in the window: `body` scrolling in the content column beneath the
/// floating header, and `preview` standing still beside it, or at the top of the column when the
/// window is too narrow to keep it beside. `destination` names the page for the header's trail.
/// The page fills the shell, which draws the sidebar over it.
pub(crate) fn frame(
    destination: &Destination,
    id: &'static str,
    scrollbar: &Entity<Scrollbar>,
    body: impl IntoElement,
    preview: Option<AnyElement>,
    window: &Window,
    cx: &mut App,
) -> Div {
    let layout = Layout::of(preview.is_some(), window, cx);
    let theme = *cx.theme();
    scrollbar.update(cx, |bar, cx| {
        bar.set_track_top(layout.lead(), cx);
    });
    let (beside, above) = match layout.preview {
        Some(left) => (preview.map(|preview| (left, preview)), None),
        None => (None, preview),
    };
    let header = header(destination, layout, window, cx);

    div()
        .relative()
        .size_full()
        .child(
            div()
                .absolute()
                .top(layout.cut)
                .left_0()
                .right_0()
                .bottom_0()
                .child(
                    layout.fade(Scroller::new(id, scrollbar)).child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(super::GROUPS)
                            .ml(layout.column)
                            .w(layout.width)
                            .pt(layout.lead())
                            .pb(theme.metrics.inset * 2.)
                            .children(above)
                            .child(body),
                    ),
                ),
        )
        .children(beside.map(|(left, preview)| {
            div()
                .absolute()
                .top(layout.header)
                .left(left)
                .w(layout.preview_width())
                .child(preview)
        }))
        .child(header)
}
