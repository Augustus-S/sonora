use super::*;

impl SettingsView {
    /// The device picker, whose "System default" follows whatever the system plays on.
    pub(super) fn output_device_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        self.row(
            t!("settings-output-device"),
            t!("settings-output-device-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            self.output_picker.clone().into_any_element(),
        )
    }

    /// Why nothing plays: the device to play on would not open. It stands in for the rate and
    /// buffer rows.
    pub(super) fn output_failure_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let (title, detail) = self.output_failure(cx);

        self.row(
            title,
            detail,
            theme.muted_foreground,
            theme.text(Text::Small),
            div().into_any_element(),
        )
    }

    /// The failure row's title and detail.
    pub(super) fn output_failure(&self, cx: &App) -> (SharedString, SharedString) {
        let outputs = self.outputs.read(cx);
        let error = outputs
            .failure()
            .map(|failure| failure.error.clone())
            .unwrap_or_default();
        (
            t!("settings-output-failed"),
            t!("settings-output-failed-detail", error = error),
        )
    }

    /// The rate the device runs at. Where Sonora can open the device at other rates it steps
    /// through them, otherwise it offers a way into the system's own settings where there is one.
    /// The row keeps its shape while the stream reopens, showing the rate it reopens at.
    pub(super) fn sample_rate_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let outputs = self.outputs.read(cx);
        let device = outputs.current();
        let current = outputs.rate(cx);
        let rates = device
            .map(|device| device.rates.clone())
            .unwrap_or_default();
        // while another device is being tried, the open stream is not the one that will play
        let locked = outputs.switching();
        let settings_uri = match rates.len() > 1 {
            true => None,
            false => {
                music::output::system_audio_settings_uri(device.map(|device| device.id.as_str()))
            }
        };
        let rate = current
            .map(|rate| t!("settings-sample-rate-value", khz = f64::from(rate) / 1_000.))
            .unwrap_or_else(|| t!("settings-output-idle"));
        let stepper = current.filter(|_| rates.len() > 1).map(|current| {
            output_stepper(
                "sample-rate",
                current,
                &rates,
                locked,
                Outputs::set_rate,
                cx,
            )
        });

        self.row(
            t!("settings-sample-rate"),
            t!("settings-sample-rate-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            div()
                .flex()
                .items_center()
                .gap_2()
                .map(|row| match stepper {
                    Some((lower, higher)) => row.child(lower).child(rate).child(higher),
                    None => row.child(rate),
                })
                .when_some(settings_uri, |row, uri| {
                    row.child(
                        Button::new("sample-rate-change")
                            .label(t!("settings-sample-rate-change"))
                            .small()
                            .outline()
                            .icon("icons/link.svg")
                            .on_click(move |_, _, cx| cx.open_url(&uri)),
                    )
                })
                .into_any_element(),
        )
    }

    /// The buffer Sonora asks the device for, in frames, with the buffers the device allows to
    /// step through. The row keeps its shape while the stream reopens, showing the buffer it
    /// reopens with.
    pub(super) fn buffer_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let outputs = self.outputs.read(cx);
        let current = outputs.buffer(cx);
        let buffers = outputs
            .current()
            .map(|device| device.buffers.clone())
            .unwrap_or_default();
        let locked = outputs.switching();
        let buffer = current
            .map(|frames| SharedString::from(frames.to_string()))
            .unwrap_or_else(|| t!("settings-output-idle"));
        let stepper = current.filter(|_| buffers.len() > 1).map(|current| {
            output_stepper("buffer", current, &buffers, locked, Outputs::set_buffer, cx)
        });

        self.row(
            t!("settings-buffer"),
            t!("settings-buffer-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            div()
                .flex()
                .items_center()
                .gap_2()
                .map(|row| match stepper {
                    Some((lower, higher)) => row.child(lower).child(buffer).child(higher),
                    None => row.child(buffer),
                })
                .into_any_element(),
        )
    }

    /// The equalizer switch, with a button beside it that opens the equalizer screen.
    pub(super) fn equalizer_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.equalizer.read(cx).enabled();

        self.row(
            t!("settings-equalizer"),
            t!("settings-equalizer-detail"),
            muted,
            small,
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("open-equalizer")
                        .label(t!("settings-equalizer-editor"))
                        .small()
                        .outline()
                        .on_click(|_, _, cx| navigate(Destination::Equalizer, cx)),
                )
                .child(
                    Switch::new("equalizer", on).on_click(cx.listener(move |this, _, _, cx| {
                        this.equalizer
                            .update(cx, |equalizer, cx| equalizer.set_enabled(!on, cx));
                    })),
                )
                .into_any_element(),
        )
    }

    /// The sound that plays, picked from the built-in presets, Custom and the named profiles.
    pub(super) fn equalizer_preset_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let picker = sound_picker(
            EQUALIZER_PRESETS,
            &self.popovers,
            &self.equalizer,
            &self.settings,
            cx,
        )
        .width(Picker::REGULAR);

        self.row(
            t!("settings-equalizer-preset"),
            t!("settings-equalizer-preset-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            picker.into_any_element(),
        )
    }
}

/// The minus and plus buttons beside an output setting's value, which `apply` the next lower or
/// higher of `options` than `current`. A button with nothing further that way is disabled, and
/// both are while `locked`.
pub(super) fn output_stepper(
    id: &'static str,
    current: u32,
    options: &[u32],
    locked: bool,
    apply: fn(&mut Outputs, u32, &mut Context<Outputs>),
    cx: &mut Context<SettingsView>,
) -> (Button, Button) {
    let lower = options
        .iter()
        .rev()
        .find(|&&option| option < current)
        .copied();
    let higher = options.iter().find(|&&option| option > current).copied();
    let step = |suffix: &'static str, label: &'static str, next: Option<u32>| {
        Button::new(SharedString::from(format!("{id}-{suffix}")))
            .label(label)
            .small()
            .outline()
            .disabled(locked || next.is_none())
            .on_click(cx.listener(move |this, _, _, cx| {
                let Some(next) = next else { return };
                this.outputs
                    .update(cx, |outputs, cx| apply(outputs, next, cx));
            }))
    };
    (
        step("lower", "\u{2212}", lower),
        step("higher", "+", higher),
    )
}
