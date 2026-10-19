use super::*;

impl SettingsView {
    pub(super) fn panel_lyrics_size_row(&self, cx: &mut Context<Self>) -> Setting {
        let scale = self.settings.read(cx).panel_lyrics_scale();

        self.lyrics_size_row(
            "panel-lyrics-size",
            "settings-panel-lyrics-size",
            "settings-panel-lyrics-size-detail",
            scale,
            |settings, scale, cx| settings.set_panel_lyrics_scale(scale, cx),
            cx,
        )
    }

    pub(super) fn fullscreen_lyrics_size_row(&self, cx: &mut Context<Self>) -> Setting {
        let scale = self.settings.read(cx).fullscreen_lyrics_scale();

        self.lyrics_size_row(
            "fullscreen-lyrics-size",
            "settings-fullscreen-lyrics-size",
            "settings-fullscreen-lyrics-size-detail",
            scale,
            |settings, scale, cx| settings.set_fullscreen_lyrics_scale(scale, cx),
            cx,
        )
    }

    pub(super) fn lyrics_size_row(
        &self,
        id: &'static str,
        title: &'static str,
        detail: &'static str,
        scale: f32,
        apply: fn(&mut AppSettings, f32, &mut Context<AppSettings>),
        cx: &mut Context<Self>,
    ) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);

        let step = move |suffix: &'static str, label: &'static str, delta: f32| {
            let wanted = (scale + delta).clamp(MIN_LYRICS_SCALE, MAX_LYRICS_SCALE);

            Button::new(SharedString::from(format!("{id}-{suffix}")))
                .label(label)
                .small()
                .outline()
                .disabled(wanted == scale)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| apply(settings, wanted, cx));
                    cx.notify();
                }))
        };

        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .child(step("smaller", "\u{2212}", -0.1))
            .child(div().child(t!(
                "settings-lyrics-size-value",
                size = (scale * 100.).round() as i64
            )))
            .child(step("larger", "+", 0.1));

        self.row(
            i18n::lookup(title, None),
            i18n::lookup(detail, None),
            muted,
            small,
            actions.into_any_element(),
        )
    }

    pub(super) fn lyrics_for_local_files_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).lyrics_for_local_files();

        self.row(
            t!("settings-lyrics-for-local-files"),
            t!("settings-lyrics-for-local-files-detail"),
            muted,
            small,
            Switch::new("lyrics-for-local-files", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_lyrics_for_local_files(!on, cx)
                    });
                }))
                .into_any_element(),
        )
    }

    pub(super) fn lyrics_providers_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let settings = self.settings.read(cx);
        let count = LYRICS_SOURCES
            .iter()
            .filter(|provider| settings.lyrics_provider_enabled(provider))
            .count();
        let picker = Picker::new(
            "lyrics-providers",
            &self.popovers,
            t!("settings-lyrics-providers-selected", count = count),
        )
        .sticky()
        .items(LYRICS_SOURCES.map(|provider| {
            MenuItem::new(provider, source_name(provider))
                .selected(settings.lyrics_provider_enabled(provider))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        let enabled = settings.lyrics_provider_enabled(provider);
                        settings.set_lyrics_provider(provider, !enabled, cx);
                    });
                }))
        }));
        self.row(
            t!("settings-lyrics-providers"),
            t!("settings-lyrics-providers-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            picker.into_any_element(),
        )
    }

    /// The enabled lyrics sources in the order ties between them are settled. Each entry can be
    /// dragged anywhere in the list or stepped with its arrows.
    pub(super) fn lyrics_priority_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let order = self.settings.read(cx).enabled_lyrics_provider_priority();
        let count = order.len();
        let entries: Vec<AnyElement> = order
            .into_iter()
            .enumerate()
            .map(|(index, provider)| self.lyrics_source_entry(index, provider, count, cx))
            .collect();
        let Setting {
            title,
            detail,
            element: header,
        } = self.row(
            t!("settings-lyrics-priority"),
            t!("settings-lyrics-priority-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            div().into_any_element(),
        );
        let list = div()
            .id("lyrics-priority")
            .flex()
            .flex_col()
            .pb_2()
            .children(entries)
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<DraggedSource>, _, cx| {
                    if !event.bounds.contains(&event.event.position)
                        && this.lyrics_drop_gap.take().is_some()
                    {
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(|this, dragged: &DraggedSource, _, cx| {
                let Some(gap) = this.lyrics_drop_gap.take() else {
                    return;
                };
                this.settings.update(cx, |settings, cx| {
                    settings.place_lyrics_provider(&dragged.provider, gap, cx)
                });
                cx.notify();
            }));
        Setting {
            title,
            detail,
            element: div()
                .flex()
                .flex_col()
                .gap_1()
                .child(header)
                .child(list)
                .into_any_element(),
        }
    }

    /// One source in the priority list: its muted position, its name and the arrows that step it.
    pub(super) fn lyrics_source_entry(
        &self,
        index: usize,
        provider: String,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let name = source_name(&provider);
        let edge = match self.lyrics_drop_gap {
            Some(gap) if gap == index => Some(ui::Edge::Above),
            Some(gap) if gap == count && index + 1 == count => Some(ui::Edge::Below),
            _ => None,
        };
        let arrows = [true, false].map(|up| {
            let moved = provider.clone();
            let (id, icon, tooltip, blocked) = match up {
                true => (
                    "lyrics-priority-up",
                    "icons/chevron-up.svg",
                    "settings-lyrics-priority-up",
                    index == 0,
                ),
                false => (
                    "lyrics-priority-down",
                    "icons/chevron-down.svg",
                    "settings-lyrics-priority-down",
                    index + 1 == count,
                ),
            };
            Button::new((id, index))
                .icon(icons::path(icon))
                .small()
                .ghost()
                .tooltip(tooltip)
                .disabled(blocked)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.move_lyrics_provider(&moved, up, cx)
                    });
                }))
        });
        let dragged = DraggedSource {
            provider,
            label: name.clone(),
            position: Point::default(),
        };
        div()
            .id(("lyrics-source", index))
            .relative()
            .h(theme.metrics.row)
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex_none()
                    .font_features(tabular())
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from((index + 1).to_string())),
            )
            .child(div().flex_1().min_w_0().truncate().child(name))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .children(arrows),
            )
            .on_drag(dragged, |dragged, position, _, cx| {
                cx.new(|_| DraggedSource {
                    position,
                    ..dragged.clone()
                })
            })
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<DraggedSource>, _, cx| {
                    // The entries touch, so only the one under the cursor answers, and the list
                    // clears the gap once the cursor leaves it.
                    if !event.bounds.contains(&event.event.position) {
                        return;
                    }
                    let Some(gap) = drop_gap(event.bounds, event.event.position, index) else {
                        return;
                    };
                    let order = this.settings.read(cx).enabled_lyrics_provider_priority();
                    let from = order
                        .iter()
                        .position(|name| *name == event.drag(cx).provider);
                    let gap = Some(gap)
                        .filter(|gap| from.is_some_and(|from| *gap != from && *gap != from + 1));
                    if this.lyrics_drop_gap != gap {
                        this.lyrics_drop_gap = gap;
                        cx.notify();
                    }
                }),
            )
            .when_some(edge, |entry, edge| entry.child(drop_marker(edge, cx)))
            .into_any_element()
    }

    pub(super) fn karaoke_lyrics_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).karaoke_lyrics();

        self.row(
            t!("settings-karaoke-lyrics"),
            t!("settings-karaoke-lyrics-detail"),
            muted,
            small,
            Switch::new("karaoke-lyrics", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_karaoke_lyrics(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn blur_lyrics_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).blur_lyrics();

        self.row(
            t!("settings-blur-lyrics"),
            t!("settings-blur-lyrics-detail"),
            muted,
            small,
            Switch::new("blur-lyrics", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_blur_lyrics(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn romanized_lyrics_row(&self, cx: &mut Context<Self>) -> Setting {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let settings = self.settings.read(cx);
        let on = settings.romanized_lyrics();
        let scripts = settings.romanization_scripts();
        let picker = Picker::new(
            "romanization-scripts",
            &self.popovers,
            t!("settings-romanization-writing-systems"),
        )
        .width(Picker::REGULAR)
        .sticky()
        .items(WritingSystem::ALL.map(|writing_system| {
            let (id, label) = romanization_script_copy(writing_system);
            let selected = scripts.contains(writing_system);
            MenuItem::new(id, i18n::lookup(label, None))
                .selected(selected)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_romanization_script(writing_system, !selected, cx);
                    });
                }))
        }));
        let action = div()
            .flex()
            .items_center()
            .gap_2()
            .when(on, |this| this.child(picker))
            .child(Switch::new("romanized-lyrics", on).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_romanized_lyrics(!on, cx);
                    });
                },
            )));

        self.row(
            t!("settings-romanized-lyrics"),
            t!("settings-romanized-lyrics-detail"),
            muted,
            small,
            action.into_any_element(),
        )
    }
}

pub(super) fn romanization_script_copy(
    writing_system: WritingSystem,
) -> (&'static str, &'static str) {
    match writing_system {
        WritingSystem::Japanese => ("romanization-japanese", "settings-romanization-japanese"),
        WritingSystem::Chinese => ("romanization-chinese", "settings-romanization-chinese"),
        WritingSystem::Korean => ("romanization-korean", "settings-romanization-korean"),
        WritingSystem::Cyrillic => ("romanization-cyrillic", "settings-romanization-cyrillic"),
        WritingSystem::Greek => ("romanization-greek", "settings-romanization-greek"),
        WritingSystem::Arabic => ("romanization-arabic", "settings-romanization-arabic"),
        WritingSystem::Other => ("romanization-other", "settings-romanization-other"),
    }
}

impl Render for DraggedSource {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Ghost::new(self.position, self.label.clone())
    }
}

/// The name a lyrics source goes by in the settings, falling back to the provider's own name.
pub(super) fn source_name(provider: &str) -> SharedString {
    let key = match provider {
        music::lyrics::LOCAL => "settings-lyrics-provider-local",
        "Subsonic" => "settings-lyrics-provider-subsonic",
        "Spotify" => "settings-lyrics-provider-spotify",
        "YouTube Music" => "settings-lyrics-provider-youtube",
        "Apple Music" => "settings-lyrics-provider-apple-music",
        "Musixmatch" => "settings-lyrics-provider-musixmatch",
        "LrcLib" => "settings-lyrics-provider-lrclib",
        "Kugou" => "settings-lyrics-provider-kugou",
        "NetEase" => "settings-lyrics-provider-netease",
        _ => return SharedString::from(provider.to_owned()),
    };
    i18n::lookup(key, None)
}
