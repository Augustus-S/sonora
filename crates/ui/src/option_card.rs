use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, Interactivity, MouseButton, Pixels,
    SharedString, Stateful, StyleRefinement, Window, div, px,
};

use std::time::Duration;

use crate::metrics::Text;
use crate::motion::{HoverFade, mix};
use crate::theme::ActiveTheme as _;
use crate::tooltip::{Perch, Tipped as _};

/// How thick the ring around the chosen card draws.
const RING: Pixels = px(2.);
/// The gap between the chosen card's thumbnail and its ring, left in the page colour.
const RING_GAP: Pixels = px(2.);
/// How far a card that cannot be chosen fades.
const FADED: f32 = 0.45;
/// How strongly the page shows through a card's thumbnail at rest.
const FILL: f32 = 0.45;
/// How long a card takes to brighten under the pointer, and to settle back once it leaves.
const LIGHT: Duration = Duration::from_millis(140);

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One option of a visual choice: a thumbnail that shows what the option looks like, its name
/// under it, and a ring around the one that is chosen. Cards stand side by side in a grid, one
/// per option, and fill the column they are given. A card that can be chosen brightens its
/// edge and fill under the pointer with a short ease.
#[derive(IntoElement)]
pub struct OptionCard {
    id: ElementId,
    base: Stateful<Div>,
    thumb: Option<AnyElement>,
    label: Option<SharedString>,
    detail: Option<SharedString>,
    accessory: Option<AnyElement>,
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
        let id = id.into();
        Self {
            base: div().id(id.clone()),
            id,
            thumb: None,
            label: None,
            detail: None,
            accessory: None,
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

    /// A control under the label, such as a button that opens a menu of the option's variants.
    /// It handles its own clicks, so pressing it does not choose the card.
    pub fn accessory(mut self, accessory: impl IntoElement) -> Self {
        self.accessory = Some(accessory.into_any_element());
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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            mut base,
            thumb,
            label,
            detail,
            accessory,
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
        let fade = window.use_keyed_state((id, "hover"), cx, |_, _| HoverFade::new(LIGHT));
        let lit = match interactive && !selected {
            true => fade.update(cx, |fade, cx| fade.level(window, cx)),
            false => 0.,
        };
        let rest = theme.secondary.opacity(FILL);

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
                    .border_color(mix(theme.border, theme.muted_foreground, lit))
                    .bg(mix(rest, theme.secondary_hover, lit))
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
            .on_hover(move |hovered, window, cx| {
                fade.update(cx, |fade, _| fade.set(*hovered));
                window.refresh();
            })
            .when_some(tooltip, |this, key| this.tip(key, Perch::Pointer))
            .child(frame)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_0p5()
                    .max_w_full()
                    .when_some(label, |this, label| {
                        this.child(
                            div()
                                .max_w_full()
                                .truncate()
                                .text_size(theme.text(Text::Label))
                                .text_color(match selected {
                                    true => theme.foreground,
                                    false => mix(theme.muted_foreground, theme.foreground, lit),
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
                    .children(accessory),
            )
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
