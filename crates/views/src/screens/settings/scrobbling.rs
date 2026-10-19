use super::*;

impl SettingsView {
    /// One row per scrobbling service, in the order `music::scrobble` lists them.
    pub(super) fn scrobble_slots(&self, cx: &App) -> Vec<Slot> {
        let services = self.scrobbling.read(cx).rows().len();
        (0..services).map(Slot::Scrobble).collect()
    }

    pub(super) fn scrobble_row(&self, index: usize, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);

        let row = &self.scrobbling.read(cx).rows()[index];
        let service = row.id();
        let link = row.link();
        let linked = row.linked();
        let linking = row.linking();
        let enabled = row.enabled();
        let title = i18n::lookup(&format!("settings-{service}"), None);
        let detail = match row.state() {
            ScrobbleState::Off => t!("settings-scrobble-off"),
            ScrobbleState::Linking => t!("settings-scrobble-waiting"),
            ScrobbleState::On(name) => match name.is_empty() {
                true => t!("settings-scrobble-on"),
                false => t!("settings-scrobble-as", name = name.as_ref()),
            },
            ScrobbleState::Failed(key) => i18n::lookup(key, None),
        };

        let action = match linked {
            true => div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Switch::new(
                        SharedString::from(format!("scrobble-on-{service}")),
                        enabled,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.scrobbling.update(cx, |scrobbling, cx| {
                            scrobbling.set_enabled(service, !enabled, cx)
                        });
                    })),
                )
                .child(
                    Button::new(SharedString::from(format!("scrobble-unlink-{service}")))
                        .label(t!("settings-scrobble-disconnect"))
                        .small()
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.scrobbling
                                .update(cx, |scrobbling, cx| scrobbling.disconnect(service, cx));
                        })),
                )
                .into_any_element(),
            false => match linking {
                true => Button::new(SharedString::from(format!("scrobble-cancel-{service}")))
                    .label(t!("common-cancel"))
                    .small()
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.scrobbling
                            .update(cx, |scrobbling, cx| scrobbling.cancel(cx));
                    }))
                    .into_any_element(),
                false => Button::new(SharedString::from(format!("scrobble-link-{service}")))
                    .label(t!("settings-scrobble-connect"))
                    .small()
                    .outline()
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.open_scrobble(service, link, cx)),
                    )
                    .into_any_element(),
            },
        };

        self.row(title, detail, muted, small, action)
    }

    /// Starts a link. A service with nothing to type goes straight to the browser; the rest put
    /// up the dialog with whatever was stored last time already in the fields.
    pub(super) fn open_scrobble(
        &mut self,
        service: &'static str,
        link: Link,
        cx: &mut Context<Self>,
    ) {
        let account = self.settings.read(cx).account(service);
        let (first, second) = match link {
            Link::Browser => {
                return self.scrobbling.update(cx, |scrobbling, cx| {
                    scrobbling.connect(service, Secret::None, cx)
                });
            }
            Link::Keys => (
                Field {
                    hint: "settings-scrobble-key",
                    value: account.key,
                    masked: false,
                },
                Some(Field {
                    hint: "settings-scrobble-secret",
                    value: account.secret,
                    masked: true,
                }),
            ),
            Link::Token => (
                Field {
                    hint: "settings-scrobble-token",
                    value: account.session,
                    masked: true,
                },
                None,
            ),
            Link::Server => (
                Field {
                    hint: "settings-scrobble-server",
                    value: account.server,
                    masked: false,
                },
                Some(Field {
                    hint: "settings-scrobble-key",
                    value: account.session,
                    masked: true,
                }),
            ),
        };

        self.scrobble_first.update(cx, |input, cx| {
            input.set_hint(first.hint, cx);
            input.set_masked(first.masked, cx);
            input.set_text(first.value, cx);
        });
        if let Some(second) = second {
            self.scrobble_second.update(cx, |input, cx| {
                input.set_hint(second.hint, cx);
                input.set_masked(second.masked, cx);
                input.set_text(second.value, cx);
            });
        }
        self.scrobble_prompt = Some(service);
        cx.notify();
    }

    pub(super) fn link_scrobble(
        &mut self,
        service: &'static str,
        link: Link,
        cx: &mut Context<Self>,
    ) {
        let first = self.scrobble_first.read(cx).text().to_string();
        let second = self.scrobble_second.read(cx).text().to_string();
        let secret = match link {
            Link::Browser => Secret::None,
            Link::Keys => Secret::Keys {
                key: first,
                secret: second,
            },
            Link::Token => Secret::Token(first),
            Link::Server => Secret::Server {
                url: first,
                key: second,
            },
        };

        self.scrobble_prompt = None;
        self.scrobbling
            .update(cx, |scrobbling, cx| scrobbling.connect(service, secret, cx));
    }

    pub(super) fn scrobble_modal(
        &self,
        service: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let row = self
            .scrobbling
            .read(cx)
            .rows()
            .iter()
            .find(|row| row.id() == service);
        let link = row.map(|row| row.link()).unwrap_or(Link::Token);
        let signup = row.and_then(|row| row.signup());
        let title = i18n::lookup(&format!("settings-{service}"), None);
        let request = match link {
            Link::Token => "settings-scrobble-token-request",
            _ => "settings-scrobble-request",
        };

        Modal::new(
            "settings-scrobble-prompt",
            t!("settings-scrobble-title", service = title.as_ref()),
        )
        .w(px(560.))
        .detail(i18n::lookup(&format!("settings-{service}-detail"), None))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(self.scrobble_first.clone())
                .when(link != Link::Token, |this| {
                    this.child(self.scrobble_second.clone())
                }),
        )
        .when_some(signup, |this, url| {
            this.action(
                Button::new("settings-scrobble-request")
                    .ghost()
                    .label(i18n::lookup(request, None))
                    .on_click(move |_, _, cx| cx.open_url(url)),
            )
        })
        .action(
            Button::new("settings-scrobble-cancel")
                .ghost()
                .label(t!("common-cancel"))
                .on_click(cx.listener(|this, _, _, cx| this.close_scrobble(cx))),
        )
        .action(
            Button::new("settings-scrobble-submit")
                .primary()
                .label(t!("settings-scrobble-connect"))
                .on_click(cx.listener(move |this, _, _, cx| this.link_scrobble(service, link, cx))),
        )
        .on_dismiss(cx.listener(|this, _, _, cx| this.close_scrobble(cx)))
    }

    pub(super) fn close_scrobble(&mut self, cx: &mut Context<Self>) {
        self.scrobble_prompt = None;
        cx.notify();
    }
}
