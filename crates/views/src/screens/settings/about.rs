use super::*;

impl SettingsView {
    pub(super) fn updates_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).check_updates();

        self.row(
            t!("settings-check-updates"),
            t!("settings-check-updates-detail"),
            muted,
            small,
            Switch::new("check-updates", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_check_updates(!on, cx));
                }))
                .into_any_element(),
        )
    }

    /// The About row that opens a copy of the current log file in whatever the system reads text
    /// with. The button is disabled when the platform names no state or cache folder to log into.
    pub(super) fn log_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let path = state::log_file();

        self.row(
            t!("settings-log"),
            t!("settings-log-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            Button::new("open-log")
                .label(t!("settings-log-open"))
                .small()
                .outline()
                .disabled(path.is_none())
                .on_click(move |_, _, _| {
                    let Some(path) = &path else { return };
                    if let Err(error) = snapshot(path).and_then(open::that_detached) {
                        log::warn!(
                            "settings: cannot open a copy of {}: {error}",
                            path.display()
                        );
                    }
                })
                .into_any_element(),
        )
    }

    pub(super) fn version_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();

        self.row(
            t!("settings-version"),
            t!("settings-version-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            div().child(VERSION).into_any_element(),
        )
    }

    pub(super) fn license_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();

        self.row(
            t!("settings-license"),
            t!("settings-license-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            Button::new("license")
                .label(t!("settings-license-view"))
                .small()
                .outline()
                .icon("icons/link.svg")
                .on_click(|_, _, cx| cx.open_url(LICENSE_URL))
                .into_any_element(),
        )
    }

    pub(super) fn source_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();

        self.row(
            t!("settings-source"),
            t!("settings-source-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            Button::new("source")
                .label(t!("settings-source-view"))
                .small()
                .outline()
                .icon("icons/link.svg")
                .on_click(|_, _, cx| cx.open_url(SOURCE_URL))
                .into_any_element(),
        )
    }

    pub(super) fn notice(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .text_color(theme.muted_foreground)
            .text_size(theme.text(Text::Small))
            .child(t!("settings-notice"))
    }

    pub(super) fn team(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        InfoCard::new(t!("settings-team")).flex_none().child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .children(MEMBERS.into_iter().enumerate().map(|(index, member)| {
                    div()
                        .id(("team-member", index))
                        .flex()
                        .items_center()
                        .gap_3()
                        .px(theme.metrics.pad)
                        .py(theme.metrics.pad / 2.)
                        .rounded(theme.radius)
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.secondary_hover))
                        .on_click(move |_, _, cx| cx.open_url(member.profile))
                        .child(Avatar::new(Some(member.avatar)).size(theme.metrics.thumb))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(div().font_weight(FontWeight::MEDIUM).child(member.login))
                                .child(
                                    div()
                                        .text_size(theme.text(Text::Small))
                                        .text_color(theme.muted_foreground)
                                        .child(t!("settings-team-github")),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(theme.text(Text::Small))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.muted_foreground)
                                .child(member.role.label()),
                        )
                })),
        )
    }
}

/// Notepad on Windows refuses a file that Sonora still writes to, so the button opens a copy
/// `File::open` shares write access, `fs::copy` documents no share mode
pub(super) fn snapshot(log: &Path) -> io::Result<PathBuf> {
    let copy = log.with_file_name("sonora.copy.log");
    io::copy(&mut File::open(log)?, &mut File::create(&copy)?)?;
    Ok(copy)
}
