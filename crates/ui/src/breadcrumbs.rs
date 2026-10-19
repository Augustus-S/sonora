use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, FontWeight, Pixels, SharedString, Stateful, StyleRefinement,
    Window, div, px, svg,
};

use crate::theme::ActiveTheme as _;

/// How large the chevron between two crumbs draws.
const CHEVRON: Pixels = px(14.);

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One step of a trail: its label and what clicking it does.
struct Crumb {
    label: SharedString,
    on_click: Option<Click>,
}

/// A trail of the places above the one on show, `Settings › Sound › Equalizer`. Earlier crumbs
/// are muted and turn foreground on hover, and the last one is the place itself, in the
/// foreground and semibold, and never clickable.
#[derive(IntoElement)]
pub struct Breadcrumbs {
    base: Stateful<Div>,
    crumbs: Vec<Crumb>,
}

impl Breadcrumbs {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id),
            crumbs: Vec::new(),
        }
    }

    /// A crumb that goes somewhere when it is clicked.
    pub fn link(
        mut self,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.crumbs.push(Crumb {
            label: label.into(),
            on_click: Some(Box::new(handler)),
        });
        self
    }

    /// A crumb that only names a place, the last one usually.
    pub fn item(mut self, label: impl Into<SharedString>) -> Self {
        self.crumbs.push(Crumb {
            label: label.into(),
            on_click: None,
        });
        self
    }
}

impl Styled for Breadcrumbs {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Breadcrumbs {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { mut base, crumbs } = self;
        let theme = *cx.theme();
        let last = crumbs.len().saturating_sub(1);
        let foreground = theme.foreground;

        let children = crumbs
            .into_iter()
            .enumerate()
            .flat_map(|(index, crumb)| {
                let separator = (index > 0).then(|| {
                    svg()
                        .path(icons::path("icons/chevron-right.svg"))
                        .size(CHEVRON)
                        .flex_none()
                        .text_color(theme.muted_foreground)
                        .into_any_element()
                });
                let label = div()
                    .id(index)
                    .flex_none()
                    .whitespace_nowrap()
                    .map(|label| match index == last {
                        true => label
                            .text_color(theme.foreground)
                            .font_weight(FontWeight::SEMIBOLD),
                        false => label.text_color(theme.muted_foreground),
                    })
                    .when_some(
                        crumb.on_click.filter(|_| index != last),
                        |label, handler| {
                            label
                                .cursor_pointer()
                                .hover(move |style| style.text_color(foreground))
                                .on_click(move |event, window, cx| handler(event, window, cx))
                        },
                    )
                    .child(crumb.label)
                    .into_any_element();
                separator.into_iter().chain(std::iter::once(label))
            })
            .collect::<Vec<_>>();

        let overrides = std::mem::take(base.style());
        let mut trail = base
            .flex()
            .items_center()
            .gap_1p5()
            .min_w_0()
            .overflow_hidden()
            .children(children);

        trail.style().refine(&overrides);
        trail
    }
}
