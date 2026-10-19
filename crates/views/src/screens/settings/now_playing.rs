use super::*;

impl SettingsView {
    /// The fullscreen background sampled from the cover. It carries the cover's hues itself,
    /// so fullscreen tints from the artwork with the adaptive theme off as well.
    pub(super) fn ambient_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).ambient();

        self.row(
            t!("settings-ambient"),
            t!("settings-ambient-detail"),
            muted,
            small,
            Switch::new("ambient", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_ambient(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn ambient_motion_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).ambient_motion();

        self.row(
            t!("settings-ambient-motion"),
            t!("settings-ambient-motion-detail"),
            muted,
            small,
            Switch::new("ambient-motion", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_ambient_motion(!on, cx));
                }))
                .into_any_element(),
        )
    }

    /// How the spectrum is drawn behind the fullscreen artwork, off included.
    pub(super) fn visualizer_style_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let chosen = self.settings.read(cx).visualizer_style();

        let picker = Picker::new(VISUALIZER_STYLE, &self.popovers, chosen.label())
            .width(Picker::NARROW)
            .items(VisualizerStyle::ALL.map(|style| {
                MenuItem::new(style.id(), style.label())
                    .selected(style == chosen)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_visualizer_style(style, cx));
                        cx.notify();
                    }))
            }));

        self.row(
            t!("settings-visualizer"),
            t!("settings-visualizer-detail"),
            muted,
            small,
            picker.into_any_element(),
        )
    }

    pub(super) fn visualizer_absolute_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).visualizer_absolute();

        self.row(
            t!("settings-visualizer-absolute"),
            t!("settings-visualizer-absolute-detail"),
            muted,
            small,
            Switch::new("visualizer-absolute", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_visualizer_absolute(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn fullscreen_controls_autohide_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let chosen = self.settings.read(cx).fullscreen_controls_autohide();

        let picker = Picker::new(
            FULLSCREEN_CONTROLS_AUTOHIDE,
            &self.popovers,
            i18n::lookup(chosen.key(), None),
        )
        .width(Picker::NARROW)
        .items(FullscreenControlsAutohide::ALL.map(|fca| {
            MenuItem::new(fca.id(), i18n::lookup(fca.key(), None))
                .selected(fca == chosen)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_fullscreen_controls_autohide(fca, cx)
                    });
                    cx.notify();
                }))
        }));

        self.row(
            t!("settings-fullscreen-controls-autohide"),
            t!("settings-fullscreen-controls-autohide-detail"),
            muted,
            small,
            picker.into_any_element(),
        )
    }

    pub(super) fn os_fullscreen_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).os_fullscreen();

        self.row(
            t!("settings-os-fullscreen"),
            t!("settings-os-fullscreen-detail"),
            muted,
            small,
            Switch::new("os-fullscreen", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_os_fullscreen(!on, cx));
                }))
                .into_any_element(),
        )
    }
}
