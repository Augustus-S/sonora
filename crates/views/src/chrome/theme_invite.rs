//! The question before a theme from a `sonora://theme/` link is installed: a miniature of the
//! window in its colours, its name and author, and Install or Cancel. `state::Links` holds the
//! offer and does the installing; this only draws it.

use gpui::prelude::*;
use gpui::{Context, Entity, FocusHandle, FontWeight, Render, Window, div};
use i18n::t;
use state::{Links, Sonora};
use ui::{
    ActiveTheme as _, Button, Dismiss, FORM_CONTEXT, Look, Modal, Submit, Text, Theme, ThemeKind,
};

use crate::shared::theme_miniature::{CARD, THUMB, sliced};

/// Asks whether to install the theme a link carried. It shows for as long as `Links` holds an
/// offer, wherever the listener is in the app, and either answer hands the offer back.
pub(crate) struct ThemeInvite {
    links: Entity<Links>,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    shown: bool,
}

impl ThemeInvite {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let links = Sonora::global(cx).links.clone();
        cx.observe(&links, |_, _, cx| cx.notify()).detach();
        Self {
            links,
            focus: cx.focus_handle(),
            restore: None,
            shown: false,
        }
    }

    fn decline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release(window, cx);
        self.links.update(cx, |links, cx| links.decline_theme(cx));
    }

    /// Installs the theme and fades the window into it, the way choosing a card does.
    fn install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release(window, cx);
        if !self.links.update(cx, |links, cx| links.install_theme(cx)) {
            return;
        }
        let (look, overrides) = {
            let settings = Sonora::global(cx).settings.read(cx);
            (settings.look(), settings.theme_overrides())
        };
        Theme::fade(
            Look {
                kind: ThemeKind::Dark,
                ..look
            },
            &overrides,
            cx,
        );
    }

    fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = self.restore.take() {
            window.focus(&focus, cx);
        }
        self.shown = false;
    }
}

impl Render for ThemeInvite {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(variant) = self
            .links
            .read(cx)
            .theme_offer()
            .map(|offer| offer.variant.clone())
        else {
            if self.shown {
                self.release(window, cx);
            }
            return div().into_any_element();
        };
        if !self.shown {
            self.restore = window.focused(cx);
            window.focus(&self.focus, cx);
            self.shown = true;
        }

        let theme = *cx.theme();
        let preview = div()
            .flex()
            .items_center()
            .gap(theme.metrics.pad)
            .child(
                div()
                    .flex_none()
                    .w(CARD)
                    .h(THUMB)
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .child(sliced(&[variant.palette()], theme.radius)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_size(theme.text(Text::Body))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(variant.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(t!("theme-install-author", author = &variant.author)),
                    ),
            );

        div()
            .absolute()
            .inset_0()
            .key_context(FORM_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                cx.stop_propagation();
                this.decline(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                cx.stop_propagation();
                this.install(window, cx);
            }))
            .child(
                Modal::new(
                    "theme-install",
                    t!("theme-install-title", name = &variant.title),
                )
                .detail(t!("theme-install-detail"))
                .child(preview)
                .action(
                    Button::new("decline-theme")
                        .ghost()
                        .label(t!("common-cancel"))
                        .on_click(cx.listener(|this, _, window, cx| this.decline(window, cx))),
                )
                .action(
                    Button::new("install-theme")
                        .primary()
                        .label(t!("theme-install-confirm"))
                        .on_click(cx.listener(|this, _, window, cx| this.install(window, cx))),
                )
                .on_dismiss(cx.listener(|this, _, window, cx| this.decline(window, cx))),
            )
            .into_any_element()
    }
}
