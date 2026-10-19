use gpui::prelude::*;
use gpui::{Context, Entity, Pixels, Render, Window, div, px};
use router::{Destination, SettingsPage, navigate};
use ui::{ActiveTheme as _, Button, TabBar, VeilEdge, effects, veil};

use super::{Layout, SettingsView};
use crate::screens::equalizer::EqualizerView;

/// How far the rows are blurred where they pass under the header. One blur pass serves every
/// strip, so this is also the widest the haze gets.
const BLUR: Pixels = px(1.);

/// The search field and the pill bar of pages that float over every settings page in the
/// workspace. The workspace lays it over the content outside the page's transition, so a page
/// change fades the rows and nothing else. It measures itself so the pages start below it.
pub struct SettingsHeader {
    view: Entity<SettingsView>,
    /// The equalizer page, which starts below the header too and has to hear when it moves.
    equalizer: Entity<EqualizerView>,
    /// The page clicked last, kept free of the hover shade until the pointer leaves it, so the
    /// change of selection never flashes.
    calm: Option<SettingsPage>,
}

impl SettingsHeader {
    pub fn new(
        view: Entity<SettingsView>,
        equalizer: Entity<EqualizerView>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        cx.observe(&router::trail(cx), |_, _, cx| cx.notify())
            .detach();
        Self {
            view,
            equalizer,
            calm: None,
        }
    }
}

impl Render for SettingsHeader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = Layout::of(window, cx);
        let theme = *cx.theme();
        let view = self.view.read(cx);
        let search = view.search();
        // a search lights no page, since its results come from all of them
        let chosen = match view.searching() {
            true => None,
            false => router::trail(cx).read(cx).current().settings_page(),
        };
        let calm = self.calm;
        let pages = SettingsPage::ALL
            .into_iter()
            .filter(|page| page.parent().is_none());
        let shown = chosen.and_then(|chosen| pages.clone().position(|page| page == chosen));

        let tabs = TabBar::new("settings-pages")
            .max_w_full()
            .blurred()
            .reveal(shown)
            .items(pages.map(|page| {
                Button::new(page.id())
                    .label(i18n::lookup(page.key(), None))
                    .icon(page.icon())
                    .small()
                    .ghost()
                    .selected(chosen == Some(page))
                    .when(calm == Some(page), Button::hoverless)
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if !hovered && this.calm == Some(page) {
                            this.calm = None;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.calm = Some(page);
                        this.view.update(cx, |view, cx| view.clear_search(cx));
                        navigate(Destination::Settings(page), cx);
                    }))
            }));

        let view = self.view.clone();
        let equalizer = self.equalizer.clone();

        div()
            .relative()
            .flex()
            .justify_center()
            .px(layout.margin)
            // the veil is absolute and takes the header's own size, so the last child, the
            // column, is the one to measure
            .on_children_prepainted(move |bounds, _, cx| {
                let Some(height) = bounds.last().map(|bounds| bounds.size.height) else {
                    return;
                };
                if Layout::set_header(height, cx) {
                    view.update(cx, |_, cx| cx.notify());
                    equalizer.update(cx, |_, cx| cx.notify());
                }
            })
            // A see-through page has content of its own passing under the header and reads
            // worse without the haze, so only the flat fallback is dropped there, since a solid
            // band over a see-through page is a slab.
            .when(effects() || !theme.transparent, |header| {
                header.child(veil(
                    VeilEdge::Top,
                    layout.top,
                    BLUR,
                    theme.background,
                    None,
                    window,
                ))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .w_full()
                    .gap_2()
                    .py(theme.metrics.inset)
                    .child(div().w(layout.width).child(search))
                    .child(div().flex().justify_center().w(layout.width).child(tabs)),
            )
    }
}
