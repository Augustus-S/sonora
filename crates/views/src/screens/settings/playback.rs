use super::*;

impl SettingsView {
    pub(super) fn playback_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.playback.read(cx).normalisation();

        self.row(
            t!("settings-normalisation"),
            t!("settings-normalisation-detail"),
            muted,
            small,
            Switch::new("normalisation", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.playback
                        .update(cx, |playback, cx| playback.set_normalisation(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn gapless_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.playback.read(cx).gapless();

        self.row(
            t!("settings-gapless"),
            t!("settings-gapless-detail"),
            muted,
            small,
            Switch::new("gapless", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.playback
                        .update(cx, |playback, cx| playback.set_gapless(!on, cx));
                }))
                .into_any_element(),
        )
    }

    /// The stream level picker, offering only the levels the signed-in service serves, each with
    /// what it streams as. A saved level the service lacks shows as the one it plays instead, and
    /// a service with one level states it without a picker.
    pub(super) fn stream_quality_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let offers = self.session.read(cx).capabilities().qualities;
        let current = self.playback.read(cx).quality().within(offers);

        let picker = Picker::new(
            STREAM_QUALITIES,
            &self.popovers,
            i18n::lookup(quality_key(current), None),
        )
        .width(Picker::WIDE)
        .items(offers.iter().map(|offer| {
            let quality = offer.quality;
            MenuItem::new(quality.id(), i18n::lookup(quality_key(quality), None))
                .selected(quality == current)
                .detail(
                    div()
                        .text_color(muted)
                        .text_size(small)
                        .child(encoding_label(offer)),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.playback
                        .update(cx, |playback, cx| playback.set_quality(quality, cx));
                    cx.notify();
                }))
        }));

        // one level is nothing to choose between, so it is stated rather than offered
        let (detail, control) = match offers {
            [only] => (
                t!("settings-stream-quality-fixed-detail"),
                div()
                    .text_color(muted)
                    .text_size(small)
                    .child(encoding_label(only))
                    .into_any_element(),
            ),
            _ => (
                t!("settings-stream-quality-detail"),
                picker.into_any_element(),
            ),
        };
        self.row(t!("settings-stream-quality"), detail, muted, small, control)
    }

    pub(super) fn stream_format_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let transcode = self.playback.read(cx).transcode();

        let picker = Picker::new(
            STREAM_FORMATS,
            &self.popovers,
            i18n::lookup(codec_key(transcode.codec), None),
        )
        .width(Picker::WIDE)
        .items(
            std::iter::once(None)
                .chain(Codec::ALL.map(Some))
                .map(|codec| {
                    MenuItem::new(
                        codec.map_or("original", Codec::id),
                        i18n::lookup(codec_key(codec), None),
                    )
                    .selected(codec == transcode.codec)
                    .detail(
                        div()
                            .text_color(muted)
                            .text_size(small)
                            .child(i18n::lookup(codec_detail_key(codec), None)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.playback.update(cx, |playback, cx| {
                            playback.set_transcode(Transcode { codec, ..transcode }, cx)
                        });
                        cx.notify();
                    }))
                }),
        );

        self.row(
            t!("settings-stream-format"),
            t!("settings-stream-format-detail"),
            muted,
            small,
            picker.into_any_element(),
        )
    }

    pub(super) fn stream_bitrate_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let transcode = self.playback.read(cx).transcode();

        let picker = Picker::new(
            STREAM_BITRATES,
            &self.popovers,
            bitrate_label(transcode.bitrate),
        )
        .width(Picker::REGULAR)
        .items(
            std::iter::once(None)
                .chain(Transcode::BITRATES.map(Some))
                .map(|bitrate| {
                    let id = bitrate.map_or_else(|| "unlimited".into(), |kbps| kbps.to_string());
                    MenuItem::new(SharedString::from(id), bitrate_label(bitrate))
                        .selected(bitrate == transcode.bitrate)
                        .detail(
                            div()
                                .text_color(muted)
                                .text_size(small)
                                .child(bitrate_detail(bitrate)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.playback.update(cx, |playback, cx| {
                                playback.set_transcode(
                                    Transcode {
                                        bitrate,
                                        ..transcode
                                    },
                                    cx,
                                )
                            });
                            cx.notify();
                        }))
                }),
        );

        self.row(
            t!("settings-stream-bitrate"),
            t!("settings-stream-bitrate-detail"),
            muted,
            small,
            picker.into_any_element(),
        )
    }

    pub(super) fn stay_awake_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).stay_awake();

        self.row(
            t!("settings-stay-awake"),
            t!("settings-stay-awake-detail"),
            muted,
            small,
            Switch::new("stay-awake", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_stay_awake(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn sleep_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).sleep_timer();

        let picker = Picker::plain(SLEEP, &self.popovers, t!("settings-sleep-configure"))
            .width(Picker::REGULAR)
            .selected(self.playback.read(cx).sleep().is_some())
            .items(match self.popovers.shows(SLEEP) {
                true => vec![self.sleep_dial(cx)],
                false => Vec::new(),
            });
        let action = div()
            .flex()
            .items_center()
            .gap_2()
            .when(on, |this| this.child(picker))
            .child(
                Switch::new("sleep-timer", on).on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_sleep_timer(!on, cx));
                    if on {
                        this.popovers.close();
                        this.playback
                            .update(cx, |playback, cx| playback.set_sleep(None, cx));
                    }
                })),
            );

        self.row(
            t!("settings-sleep"),
            t!("settings-sleep-detail"),
            muted,
            small,
            action.into_any_element(),
        )
    }

    /// The slider that arms the timer: off at the left, end of track at the right, minutes in
    /// between, with the quarter hours widened.
    pub(super) fn sleep_dial(&self, cx: &mut Context<Self>) -> MenuItem {
        let theme = *cx.theme();
        let current = self
            .pending_sleep
            .unwrap_or_else(|| self.playback.read(cx).sleep());

        let dial = div()
            .flex()
            .flex_col()
            .w_full()
            .gap_2()
            .py_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .text_size(theme.text(Text::Small))
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(t!("settings-sleep")),
                    )
                    .child(sleep_label(current)),
            )
            .child(
                Scrubber::new(&self.sleep, sleep_slot(current) as f32 / SLEEP_LAST as f32)
                    .colors(
                        theme.progress_bar,
                        theme.muted_foreground.opacity(0.3),
                        theme.foreground,
                    )
                    .on_move(cx.listener(|this, fraction: &f32, _, cx| {
                        this.pending_sleep = Some(sleep_at_fraction(*fraction));
                        cx.notify();
                    }))
                    .on_release(cx.listener(|this, _: &MouseUpEvent, _, cx| {
                        let Some(sleep) = this.pending_sleep.take() else {
                            return;
                        };
                        this.playback
                            .update(cx, |playback, cx| playback.set_sleep(sleep, cx));
                    })),
            );

        MenuItem::new("sleep-dial", "").content(dial)
    }

    /// The Widevine module row, which only appears while the current provider is one whose
    /// tracks need the module and this build has a host for one. Sonora uses a browser's copy
    /// when one is here and otherwise offers Google's download, so the row says where that
    /// stands. The download is offered by hand whenever Google's copy is not the one in use,
    /// because a browser's copy can be one this host cannot open. The explanation wraps below
    /// the title rather than truncating, to the height `widevine_height` measured.
    pub(super) fn widevine_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let state = self.drm.read(cx).state().clone();
        let (detail, note) = widevine_copy(&state);
        let offerable = matches!(
            state,
            CdmState::Declined | CdmState::Missing | CdmState::Ready(Origin::Installed)
        );
        let removable = matches!(state, CdmState::Ready(Origin::Fetched));
        let title = t!("settings-widevine");
        let detail = i18n::lookup(detail, None);

        let action = div()
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .text_color(muted)
            .text_size(small)
            .child(i18n::lookup(note, None))
            .when(offerable, |row| {
                row.child(
                    Button::new("fetch-widevine")
                        .label(t!("settings-widevine-fetch"))
                        .small()
                        .outline()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.drm.update(cx, |drm, cx| drm.download(cx));
                        })),
                )
            })
            .when(removable, |row| {
                row.child(
                    Button::new("uninstall-widevine")
                        .label(t!("settings-widevine-uninstall"))
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let drm = this.drm.clone();
                            Confirm::ask(
                                Kind::Widevine,
                                move |cx| drm.update(cx, |drm, cx| drm.uninstall(cx)),
                                cx,
                            );
                        })),
                )
            });

        let element = div()
            .flex()
            .flex_col()
            .gap_1()
            .py_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .h(widevine_head(&theme))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .line_height(relative(LEADING))
                            .child(title.clone()),
                    )
                    .child(action),
            )
            .child(
                div()
                    .overflow_hidden()
                    .line_height(relative(LEADING))
                    .text_color(muted)
                    .text_size(small)
                    .child(detail.clone()),
            )
            .into_any_element();

        Setting {
            title,
            detail,
            element,
        }
    }
}

/// What the Widevine row says: the detail key for its state, and the note beside it.
/// Shared by the row and its search entry.
pub(super) fn widevine_copy(state: &CdmState) -> (&'static str, &'static str) {
    match state {
        CdmState::Looking => ("settings-widevine-detail", "settings-widevine-looking"),
        CdmState::Ready(Origin::Configured) => {
            ("settings-widevine-detail", "settings-widevine-configured")
        }
        CdmState::Ready(Origin::Installed) => {
            ("settings-widevine-detail", "settings-widevine-installed")
        }
        CdmState::Ready(Origin::Fetched) => {
            ("settings-widevine-detail", "settings-widevine-fetched")
        }
        CdmState::Wanted | CdmState::Offered(_) => {
            ("settings-widevine-none", "settings-widevine-asking")
        }
        CdmState::Offering => ("settings-widevine-none", "settings-widevine-fetching"),
        CdmState::Installing => ("settings-widevine-none", "settings-widevine-installing"),
        CdmState::Declined | CdmState::Missing => {
            ("settings-widevine-none", "settings-widevine-missing")
        }
    }
}

/// How tall the Widevine row's top line stands: the title, or the small button beside it
/// when that is taller.
pub(super) fn widevine_head(theme: &Theme) -> Pixels {
    line(theme, Text::Body).max(theme.metrics.control_small)
}

pub(super) fn sleep_slot(sleep: Option<Sleep>) -> usize {
    match sleep {
        None => 0,
        Some(Sleep::EndOfTrack) => SLEEP_LAST,
        Some(Sleep::After(after)) => minute_slot(after.as_secs() / 60),
    }
}

pub(super) fn sleep_at_fraction(fraction: f32) -> Option<Sleep> {
    let slot = (fraction.clamp(0., 1.) * SLEEP_LAST as f32).round() as usize;
    match slot {
        0 => None,
        SLEEP_LAST => Some(Sleep::EndOfTrack),
        slot => Some(Sleep::After(Duration::from_secs(slot_minute(slot) * 60))),
    }
}

pub(super) fn minute_slot(minutes: u64) -> usize {
    let minutes = minutes.clamp(1, SLEEP_MAX_MINUTES);
    let earlier_magnets = SLEEP_MAGNETS
        .iter()
        .filter(|magnet| **magnet < minutes)
        .count();
    let width = match SLEEP_MAGNETS.contains(&minutes) {
        true => SLEEP_MAGNET_WEIGHT,
        false => 1,
    };
    minutes as usize + earlier_magnets * (SLEEP_MAGNET_WEIGHT - 1) + (width - 1) / 2
}

pub(super) fn slot_minute(slot: usize) -> u64 {
    let mut first = 1;
    for minute in 1..=SLEEP_MAX_MINUTES {
        let width = match SLEEP_MAGNETS.contains(&minute) {
            true => SLEEP_MAGNET_WEIGHT,
            false => 1,
        };
        if slot < first + width {
            return minute;
        }
        first += width;
    }
    SLEEP_MAX_MINUTES
}

pub(super) fn sleep_label(sleep: Option<Sleep>) -> SharedString {
    match sleep {
        Some(Sleep::EndOfTrack) => t!("settings-sleep-end-of-track"),
        Some(Sleep::After(after)) => t!("settings-sleep-minutes", count = after.as_secs() / 60),
        None => t!("settings-sleep-off"),
    }
}

/// The Fluent key naming a stream format, where `None` keeps the format the server sends.
pub(super) fn quality_key(quality: Quality) -> &'static str {
    match quality {
        Quality::HiRes => "settings-stream-quality-hi-res",
        Quality::Lossless => "settings-stream-quality-lossless",
        Quality::High => "settings-stream-quality-high",
        Quality::Normal => "settings-stream-quality-normal",
        Quality::Low => "settings-stream-quality-low",
    }
}

/// What a level streams as, such as "FLAC, up to 24-bit 192 kHz" or "AAC, 320 kbps".
pub(super) fn encoding_label(offer: &Offer) -> SharedString {
    let lossy = |codec: &str, kbps: u32| match offer.up_to {
        true => t!(
            "settings-stream-quality-lossy-up-to",
            codec = codec.to_owned(),
            kbps = i64::from(kbps)
        ),
        false => t!(
            "settings-stream-quality-lossy",
            codec = codec.to_owned(),
            kbps = i64::from(kbps)
        ),
    };
    match offer.encoding {
        Encoding::Flac { depth, rate } => {
            // 44100 reads as 44.1 and 192000 as 192
            let khz = match rate % 1000 {
                0 => (rate / 1000).to_string(),
                _ => format!("{:.1}", f64::from(rate) / 1000.0),
            };
            match offer.up_to {
                true => t!(
                    "settings-stream-quality-flac-up-to",
                    depth = i64::from(depth),
                    rate = khz
                ),
                false => t!(
                    "settings-stream-quality-flac",
                    depth = i64::from(depth),
                    rate = khz
                ),
            }
        }
        Encoding::Aac { kbps } => lossy("AAC", kbps),
        Encoding::Vorbis { kbps } => lossy("Ogg Vorbis", kbps),
        Encoding::Mp3 { kbps } => lossy("MP3", kbps),
        Encoding::Opus { kbps } => lossy("Opus", kbps),
        // No service offers these. They name a local file's encoding in the signal path.
        Encoding::Alac { .. } => t!("signal-path-codec-alac"),
        Encoding::Pcm { .. } => t!("signal-path-codec-pcm"),
        Encoding::Other => t!("signal-path-codec-other"),
    }
}

pub(super) fn codec_key(codec: Option<Codec>) -> &'static str {
    match codec {
        None => "settings-stream-format-original",
        Some(Codec::Mp3) => "settings-stream-format-mp3",
        Some(Codec::Opus) => "settings-stream-format-opus",
        Some(Codec::Aac) => "settings-stream-format-aac",
    }
}

/// What choosing a format does, said under it in the picker.
pub(super) fn codec_detail_key(codec: Option<Codec>) -> &'static str {
    match codec {
        None => "settings-stream-format-original-detail",
        Some(Codec::Mp3) => "settings-stream-format-mp3-detail",
        Some(Codec::Opus) => "settings-stream-format-opus-detail",
        Some(Codec::Aac) => "settings-stream-format-aac-detail",
    }
}

pub(super) fn bitrate_label(bitrate: Option<u32>) -> SharedString {
    match bitrate {
        Some(kbps) => t!("settings-stream-bitrate-value", kbps = i64::from(kbps)),
        None => t!("settings-stream-bitrate-unlimited"),
    }
}

/// About how much data an hour at a bitrate ceiling takes, said under it in the picker.
pub(super) fn bitrate_detail(bitrate: Option<u32>) -> SharedString {
    match bitrate {
        // kilobits a second over an hour, in megabytes
        Some(kbps) => t!(
            "settings-stream-bitrate-hourly",
            megabytes = i64::from(kbps * 3600 / 8 / 1000)
        ),
        None => t!("settings-stream-bitrate-unlimited-detail"),
    }
}
