//! Where the settings takeover puts things. The sidebar stands against the window's left edge,
//! one content column is centred in the room right of it, and the close button waits beside the
//! column's top right corner. The shell draws the sidebar and the close button and the page draws
//! the column, so both work the places out here from the same window and land on the same pixels.

use gpui::prelude::*;
use gpui::{AnyElement, App, Div, Entity, Pixels, SharedString, Window, div, px};
use router::{Destination, SettingsPage};
use ui::{ActiveTheme as _, Button, Scrollbar, Scroller, effects, heading};

/// How wide the sidebar stands, its padding included.
pub(crate) const SIDEBAR: Pixels = px(248.);
/// How wide the content column grows at most.
const COLUMN: Pixels = px(640.);

/// The places of the takeover's parts in a window, as distances from its left and top edges.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout {
    /// Where the content column starts and how wide it is.
    pub(crate) column: Pixels,
    pub(crate) width: Pixels,
    /// Where the close button starts.
    pub(crate) close: Pixels,
    /// How far down the sidebar's search field, the page title and the close button start.
    pub(crate) top: Pixels,
}

impl Layout {
    /// The places for the window. The column is centred in the room right of the sidebar
    /// unless that would push the close button off the window, and then it moves left as far
    /// as the sidebar's margin allows before it narrows.
    pub(crate) fn of(window: &Window, cx: &App) -> Self {
        let metrics = cx.theme().metrics;
        let margin = metrics.inset;
        let gap = metrics.inset;
        let viewport = window.viewport_size().width;
        // the room the close button keeps on the right of the column
        let tools = gap + metrics.control + margin;
        let room = (viewport - SIDEBAR - margin - tools).max(Pixels::ZERO);
        let width = room.min(COLUMN);
        let centred = SIDEBAR + ((viewport - SIDEBAR - width) / 2.).round();
        let column = centred.min(viewport - tools - width).max(SIDEBAR + margin);

        Self {
            column,
            width,
            close: column + width + gap,
            top: metrics.inset * 1.5,
        }
    }

    /// Fades the scrolling content out across the band above the title, so rows rising past it
    /// dissolve rather than meet the window's edge at a line.
    pub(crate) fn fade<E: Styled>(&self, element: E) -> E {
        match effects() {
            true => element.fade_edges(self.top, Pixels::ZERO),
            false => element,
        }
    }
}

/// The top of a page: the back link to the page a drill-in opened from, and the page's title or
/// `named` in its place, standing as tall as the sidebar's search field so the two share a
/// centre line.
fn title(destination: &Destination, named: Option<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    let (parent, page) = match destination {
        Destination::Equalizer => (Some(SettingsPage::Sound), "settings-equalizer"),
        Destination::Settings(page) => (page.parent(), page.key()),
        _ => (None, "nav-settings"),
    };

    div()
        .flex()
        .flex_col()
        .items_start()
        .children(parent.map(|parent| {
            Button::new("settings-back")
                .ghost()
                .small()
                .icon("icons/chevron-left.svg")
                .label(i18n::lookup(parent.key(), None))
                .tint(theme.muted_foreground)
                .ml(-theme.metrics.pad)
                .on_click(move |_, _, cx| router::up(Destination::Settings(parent), cx))
        }))
        .child(
            div()
                .flex()
                .items_center()
                .h(theme.metrics.field)
                .child(heading(
                    named.unwrap_or_else(|| i18n::lookup(page, None)),
                    cx,
                )),
        )
}

/// Lays a takeover page out in its column: the title, then `preview` framed across the column
/// when the page has one, then `body`, all scrolling together. `destination` names the page for
/// its title unless `named` gives one. The page fills the shell, which draws the sidebar and the
/// close button over it.
#[allow(
    clippy::too_many_arguments,
    reason = "every page hands its whole frame over at once"
)]
pub(crate) fn frame(
    destination: &Destination,
    named: Option<SharedString>,
    id: &'static str,
    scrollbar: &Entity<Scrollbar>,
    body: impl IntoElement,
    preview: Option<AnyElement>,
    window: &Window,
    cx: &mut App,
) -> Div {
    let layout = Layout::of(window, cx);
    let theme = *cx.theme();
    scrollbar.update(cx, |bar, cx| {
        bar.set_track_top(layout.top, cx);
    });
    let preview = preview.map(|preview| {
        div()
            .w_full()
            .overflow_hidden()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .child(preview)
    });

    div().relative().size_full().child(
        div().absolute().inset_0().child(
            layout.fade(Scroller::new(id, scrollbar)).child(
                div()
                    .flex()
                    .flex_col()
                    .ml(layout.column)
                    .w(layout.width)
                    .pt(layout.top)
                    .pb(theme.metrics.inset * 2.)
                    .child(title(destination, named, cx).mb(theme.metrics.inset))
                    .children(preview.map(|preview| preview.mb(super::GROUPS)))
                    .child(body),
            ),
        ),
    )
}
