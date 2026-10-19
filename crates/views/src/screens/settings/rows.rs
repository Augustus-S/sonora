use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Div, ElementId, SharedString, Window, div};
use ui::{ActiveTheme as _, Button, Room, TabBar};

use super::SettingsView;
use crate::chrome::Chrome;

/// What a segmented control does with the option picked in it.
type Pick<T> = Rc<dyn Fn(&T, &mut Window, &mut App)>;

/// One row of a group: the label at the start and the control at the end. It stands one list
/// row tall with no inset of its own, so its label lines up with the group's heading and rows
/// stack with nothing between.
#[derive(IntoElement)]
pub(crate) struct Row {
    title: SharedString,
    control: AnyElement,
}

impl Row {
    pub(crate) fn new(title: SharedString, control: AnyElement) -> Self {
        Self { title, control }
    }
}

impl RenderOnce for Row {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { title, control } = self;
        let theme = *cx.theme();

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .min_h(theme.metrics.list_row)
            .child(div().flex_1().min_w_0().truncate().child(title))
            .child(div().flex_none().child(control))
    }
}

impl SettingsView {
    /// A plain row: the label and the control.
    pub(super) fn row(&self, title: SharedString, control: AnyElement) -> AnyElement {
        Row::new(title, control).into_any_element()
    }

    /// A row whose choice is drawn as option cards: the label on a row of its own, then the
    /// cards across the column under it.
    pub(super) fn stacked(&self, title: SharedString, cards: impl IntoElement) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .pb_3()
            .child(Row::new(title, div().into_any_element()))
            .child(cards)
            .into_any_element()
    }
}

/// A choice between a few options as one segmented control, each option a segment with its
/// label and the `chosen` one lit. `pick` hears the option a click lands on.
pub(super) fn segmented<T: Copy + PartialEq + 'static>(
    id: &'static str,
    options: impl IntoIterator<Item = (T, SharedString)>,
    chosen: T,
    pick: impl Fn(&T, &mut Window, &mut App) + 'static,
) -> TabBar {
    let pick: Pick<T> = Rc::new(pick);

    TabBar::new(id)
        .flex_none()
        .items(
            options
                .into_iter()
                .enumerate()
                .map(|(index, (option, label))| {
                    let pick = pick.clone();
                    Button::new((id, index))
                        .label(label)
                        .small()
                        .ghost()
                        .selected(option == chosen)
                        .on_click(move |_, window, cx| pick(&option, window, cx))
                }),
        )
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

/// A row that is one button from edge to edge, ending in `trailing`. Its hover reaches a little
/// past the column on both sides, so the label still lines up with the rows around it.
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
        .h(theme.metrics.list_row)
        .mx_neg_3()
        .px_3()
        .gap_3()
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
