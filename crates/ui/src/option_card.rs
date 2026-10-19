use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, Interactivity, MouseButton, Pixels,
    SharedString, Stateful, StyleRefinement, Window, div, px,
};

use crate::metrics::Text;
use crate::theme::ActiveTheme as _;
use crate::tooltip::{Perch, Tipped as _};

/// How thick the ring around the chosen card draws.
const RING: Pixels = px(2.);
/// The gap between the chosen card's thumbnail and its ring, left in the page colour.
const RING_GAP: Pixels = px(2.);
/// How far a card that cannot be chosen fades.
const FADED: f32 = 0.45;

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One option of a visual choice: a thumbnail that shows what the option looks like, its name
/// under it, and a ring around the one that is chosen. Cards stand side by side in a grid, one
/// per option, and fill the column they are given.
#[derive(IntoElement)]
pub struct OptionCard {
    base: Stateful<Div>,
    thumb: Option<AnyElement>,
    label: Option<SharedString>,
    detail: Option<SharedString>,
    height: Option<Pixels>,
    selected: bool,
    disabled: bool,
    dashed: bool,
    tooltip: Option<SharedString>,
    on_click: Option<Click>,
}

impl OptionCard {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id),
            thumb: None,
            label: None,
            detail: None,
            height: None,
            selected: false,
            disabled: false,
            dashed: false,
            tooltip: None,
            on_click: None,
        }
    }

    /// What the thumbnail shows. It fills the thumbnail and is cut to its corners.
    pub fn thumb(mut self, thumb: impl IntoElement) -> Self {
        self.thumb = Some(thumb.into_any_element());
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// A second, smaller line under the label, such as the bitrate a quality streams at.
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// How tall the thumbnail stands, one and a half list rows unless set.
    pub fn thumb_height(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Draws the thumbnail's edge dashed, for a card that adds or opens something rather than
    /// choosing an option.
    pub fn dashed(mut self) -> Self {
        self.dashed = true;
        self
    }

    /// The Fluent string `key`, shown when the card is hovered. A disabled card keeps it, so
    /// it can say why it cannot be chosen.
    pub fn tooltip(mut self, key: impl Into<SharedString>) -> Self {
        self.tooltip = Some(key.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Styled for OptionCard {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for OptionCard {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for OptionCard {}

impl RenderOnce for OptionCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            thumb,
            label,
            detail,
            height,
            selected,
            disabled,
            dashed,
            tooltip,
            on_click,
        } = self;

        let theme = *cx.theme();
        let height = height.unwrap_or(theme.metrics.list_row * 1.5);
        let interactive = !disabled && on_click.is_some();
        let reach = RING + RING_GAP;
        let overrides = std::mem::take(base.style());

        let frame = div()
            .relative()
            .w_full()
            .h(height)
            .child(
                div()
                    .size_full()
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .border_1()
                    .when(dashed, |this| this.border_dashed())
                    .border_color(theme.border)
                    .bg(theme.secondary.opacity(0.45))
                    .when(interactive && !selected, |this| {
                        this.hover(|style| style.border_color(theme.muted_foreground))
                    })
                    .children(thumb),
            )
            .when(selected, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(-reach)
                        .left(-reach)
                        .right(-reach)
                        .bottom(-reach)
                        .rounded(theme.radius + reach)
                        .border_2()
                        .border_color(theme.foreground),
                )
            });

        let mut card = base
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .min_w_0()
            .when(disabled, |this| this.opacity(FADED))
            .when(interactive, |this| this.cursor_pointer())
            .when_some(tooltip, |this, key| this.tip(key, Perch::Pointer))
            .child(frame)
            .when_some(label, |this, label| {
                this.child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(theme.text(Text::Label))
                        .text_color(match selected {
                            true => theme.foreground,
                            false => theme.muted_foreground,
                        })
                        .when(selected, |this| this.font_weight(FontWeight::MEDIUM))
                        .child(label),
                )
            })
            .when_some(detail, |this, detail| {
                this.child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(theme.text(Text::Small))
                        .text_color(theme.muted_foreground)
                        .child(detail),
                )
            })
            .when(interactive, |this| {
                this.when_some(on_click, |this, handler| {
                    this.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |event, window, cx| handler(event, window, cx))
                })
            });

        card.style().refine(&overrides);
        card
    }
}
