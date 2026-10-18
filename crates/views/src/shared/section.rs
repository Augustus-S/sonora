use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Empty, Entity, Pixels, Render, StyleRefinement, Window, div};

type Build = Box<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A block of a screen drawn as a cached view of its own, so a redraw of the screen around it
/// reuses what the block painted last. A cached view is laid out at the size its parent gives,
/// so the block measures its content on every draw and the screen places it at that height.
pub(crate) struct Section {
    build: Build,
    /// How tall the content stood when it was last drawn, unknown until then.
    height: Option<Pixels>,
    /// Whether the content changed since it was last drawn. The next frame then lays it out in
    /// the screen's flow, at whatever height it has now, rather than at the one measured before.
    stale: bool,
}

impl Section {
    /// A section of `screen` that `build` fills, drawn again whenever `screen` notifies. A
    /// redraw of `screen` only because something inside it changed leaves the section cached.
    pub(crate) fn new<V: 'static>(
        screen: &Entity<V>,
        build: impl Fn(&mut V, &mut Window, &mut Context<V>) -> AnyElement + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(screen, |this, _, cx| this.invalidate(cx))
            .detach();
        let screen = screen.downgrade();
        Self {
            build: Box::new(move |window, cx| {
                screen
                    .update(cx, |screen, cx| build(screen, window, cx))
                    .unwrap_or_else(|_| Empty.into_any_element())
            }),
            height: None,
            stale: true,
        }
    }

    /// The section as a child of its screen: cached at the height it measured, or laid out in
    /// the flow while that height is unknown or its content changed.
    pub(crate) fn place(section: &Entity<Self>, cx: &App) -> AnyElement {
        let this = section.read(cx);
        match (this.stale, this.height) {
            (false, Some(height)) => section
                .clone()
                .cached(StyleRefinement::default().w_full().h(height))
                .into_any_element(),
            _ => section.clone().into_any_element(),
        }
    }

    fn invalidate(&mut self, cx: &mut Context<Self>) {
        self.stale = true;
        cx.notify();
    }
}

impl Render for Section {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.stale = false;
        let section = cx.weak_entity();
        div()
            .w_full()
            .child((self.build)(window, cx))
            .on_children_prepainted(move |bounds, window, cx| {
                let height = bounds
                    .first()
                    .map_or(Pixels::ZERO, |bounds| bounds.size.height);
                let changed = section
                    .update(cx, |this, _| this.height.replace(height) != Some(height))
                    .unwrap_or(false);
                // A notify while drawing is dropped, so the new height takes effect on the
                // next frame.
                if changed {
                    let section = section.clone();
                    window.defer(cx, move |_, cx| {
                        section.update(cx, Section::invalidate).ok();
                    });
                }
            })
    }
}
