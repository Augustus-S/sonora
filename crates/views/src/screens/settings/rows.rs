use gpui::prelude::*;
use gpui::{AnyElement, App, Div, ElementId, SharedString, Window, div, px, svg};
use ui::{ActiveTheme as _, Button, Perch, Room, Tipped as _};

use super::SettingsView;
use crate::chrome::Chrome;

/// How large the info mark beside a hinted label draws.
const HINT: gpui::Pixels = px(14.);

/// One row of a group: the label at the start, an info mark beside it when the setting needs a
/// word of explanation, and the control at the end. It stands one list row tall and pads itself
/// to the group's inset, so rows stacked in a surface run edge to edge with nothing between.
#[derive(IntoElement)]
pub(super) struct Row {
    title: SharedString,
    hint: Option<&'static str>,
    control: AnyElement,
}

impl Row {
    pub(super) fn new(title: SharedString, control: AnyElement) -> Self {
        Self {
            title,
            hint: None,
            control,
        }
    }

    /// Puts an info mark beside the label that shows the Fluent string `key` when hovered.
    pub(super) fn hint(mut self, key: &'static str) -> Self {
        self.hint = Some(key);
        self
    }
}

impl RenderOnce for Row {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            title,
            hint,
            control,
        } = self;
        let theme = *cx.theme();

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .min_h(theme.metrics.list_row)
            .px_5()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_1p5()
                    .child(div().min_w_0().truncate().child(title))
                    .children(hint.map(|key| {
                        div()
                            .id(ElementId::Name(SharedString::from(key)))
                            .flex_none()
                            .tip(key, Perch::Pointer)
                            .child(
                                svg()
                                    .path(icons::path("icons/info.svg"))
                                    .size(HINT)
                                    .text_color(theme.muted_foreground),
                            )
                    })),
            )
            .child(div().flex_none().child(control))
    }
}

impl SettingsView {
    /// A plain row: the label and the control.
    pub(super) fn row(&self, title: SharedString, control: AnyElement) -> AnyElement {
        Row::new(title, control).into_any_element()
    }

    /// A row with an info mark beside its label, showing the Fluent string `hint` when hovered.
    pub(super) fn hinted(
        &self,
        title: SharedString,
        hint: &'static str,
        control: AnyElement,
    ) -> AnyElement {
        Row::new(title, control).hint(hint).into_any_element()
    }
}

/// A row that opens a page of its own: the label, the value chosen there in muted text, and a
/// chevron. The whole row is the button.
pub(super) fn drill(
    id: impl Into<ElementId>,
    title: SharedString,
    value: Option<SharedString>,
    cx: &App,
) -> Button {
    whole(id, title, value, "icons/chevron-right.svg", cx)
}

/// A row that opens something outside Sonora, a page in the browser or a file, marked so.
pub(super) fn outward(
    id: impl Into<ElementId>,
    title: SharedString,
    value: Option<SharedString>,
    cx: &App,
) -> Button {
    whole(id, title, value, "icons/external-link.svg", cx)
}

/// A row that is one button from edge to edge, ending in `trailing`.
fn whole(
    id: impl Into<ElementId>,
    title: SharedString,
    value: Option<SharedString>,
    trailing: &'static str,
    cx: &App,
) -> Button {
    let theme = cx.theme();

    Button::new(id)
        .ghost()
        .label(title)
        .when_some(value, Button::detail)
        .trailing(trailing)
        .w_full()
        .h(theme.metrics.list_row)
        .px_5()
        .gap_3()
        .rounded_none()
}

/// The grid option cards stand in: `columns` across where the page is wide, fewer as it narrows.
pub(super) fn grid(columns: u16, window: &Window, cx: &App) -> Div {
    let columns = match Chrome::room(window, cx) {
        Room::Tight => columns.min(2),
        Room::Snug => columns.min(3),
        Room::Roomy => columns.min(4),
        Room::Wide | Room::Vast => columns,
    };
    div().grid().grid_cols(columns.max(1)).gap_3()
}
