use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div, px, svg};
use i18n::t;
use state::SYSTEM_FONT;
use ui::{
    ActiveTheme as _, Button, Look, MAX_FONT, MAX_TRANSPARENCY, MIN_FONT, MenuItem, Picker,
    Rounding, Scrubber, Switch, Theme, ThemeKind,
};

use super::{Group, SettingsView, Slot, TYPEFACES};
use crate::shared::popups::matches_query;

const THEMES: &str = "themes";
const PACKS: &str = "packs";
const CORNERS: &str = "corners";
const TYPEFACE_LIMIT: usize = 200;
const TYPEFACE_LEAD: usize = 2;
/// How many faces are previewed before a row has been measured.
const TYPEFACE_GUESS: usize = 24;
/// How many faces are loaded a frame.
const TYPEFACE_BATCH: usize = 3;

impl SettingsView {
    pub(super) fn appearance_groups(&self) -> Vec<Group> {
        vec![
            Group::new("settings-theme").rows([Slot::Theme, Slot::Adaptive]),
            Group::new("settings-icons").rows([Slot::Icons]),
            Group::new("settings-corners").rows([Slot::Corners]),
            Group::new("settings-group-text").rows([Slot::Typeface, Slot::Font]),
            Group::new("settings-group-window").rows(
                [Slot::Opacity]
                    .into_iter()
                    .chain(ui::WINDOW_BLUR.then_some(Slot::WindowBlur))
                    .chain([Slot::Blur]),
            ),
        ]
    }

    /// The look the theme is drawn from now, with the tint the adaptive theme put on it.
    pub(super) fn look(&self, cx: &Context<Self>) -> Look {
        Look {
            tint: cx.theme().tint,
            tint_secondary: cx.theme().tint_secondary,
            ..self.settings.read(cx).look()
        }
    }

    pub(super) fn typeface_entries(&self) -> Vec<SharedString> {
        let asked = self.typefaces.query();
        let installed = self.installed.as_deref().unwrap_or_default();

        std::iter::once(SharedString::from(SYSTEM_FONT))
            .chain(installed.iter().cloned())
            .filter(|name| {
                let label = match name.as_ref() == SYSTEM_FONT {
                    true => t!("settings-typeface-system"),
                    false => name.clone(),
                };
                matches_query(name, &label, &asked)
            })
            .take(TYPEFACE_LIMIT)
            .collect()
    }

    pub(super) fn typeface_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).font().to_owned();
        let bundled = chosen == SYSTEM_FONT;
        let current = match bundled {
            true => t!("settings-typeface-system"),
            false => SharedString::from(chosen.clone()),
        };

        let picking = self.popovers.shows(TYPEFACES);
        let asked = self.typefaces.query();
        let installed = self.installed.as_deref().unwrap_or_default();

        let mut entries = Vec::new();
        if picking {
            entries.push((
                SharedString::from(SYSTEM_FONT),
                t!("settings-typeface-system"),
            ));
            entries.extend(installed.iter().map(|name| (name.clone(), name.clone())));
            entries = entries
                .into_iter()
                .filter(|(id, label)| matches_query(id, label, &asked))
                .take(TYPEFACE_LIMIT)
                .collect();
        }

        let barren = picking && entries.is_empty();
        let count = entries.len();
        let cursor = self.typefaces.cursor(count);
        let submitted = entries
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();

        let mut items = Vec::new();
        let mut waiting = false;

        if picking {
            let scroll = self.typefaces.scroll(cx);
            let row = scroll
                .bounds_for_item(0)
                .map(|item| item.size.height)
                .filter(|height| *height > px(0.));
            let first = row.map_or(cursor, |row| {
                ((-scroll.offset().y) / row).floor().max(0.) as usize
            });
            let shown = row.map_or(TYPEFACE_GUESS, |row| {
                (self.typefaces.height() / row).ceil() as usize
            });
            let previewed = first.saturating_sub(TYPEFACE_LEAD)..first + shown + TYPEFACE_LEAD;

            let mut budget = TYPEFACE_BATCH;
            let mut faced = self.typeface_faced.borrow_mut();

            // forget the faces that scrolled out of view, so scrolling back spends
            // the per-frame budget on them again rather than facing them all at once
            faced.retain(|name| {
                entries
                    .iter()
                    .enumerate()
                    .any(|(place, (id, _))| id == name && previewed.contains(&place))
            });

            items = entries
                .into_iter()
                .enumerate()
                .map(|(place, (id, label))| {
                    let name = id.clone();
                    let preview = name.clone();
                    let wanted = name.as_ref() != SYSTEM_FONT && previewed.contains(&place);
                    let shows = match (wanted, faced.contains(&name)) {
                        (false, _) => false,
                        (true, true) => true,
                        (true, false) => match budget {
                            0 => {
                                waiting = true;
                                false
                            }
                            _ => {
                                budget -= 1;
                                faced.insert(name.clone());
                                true
                            }
                        },
                    };
                    MenuItem::new(id, label)
                        .selected(place == cursor)
                        .checked(chosen == name.as_ref())
                        .when(shows, |item| item.face(preview))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let name = name.to_string();
                            this.settings
                                .update(cx, |settings, cx| settings.set_font(name, cx));
                            this.popovers.close();
                            cx.notify();
                        }))
                })
                .collect::<Vec<_>>();
            drop(faced);
        }

        if waiting {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                this.update(cx, |_, cx| cx.notify()).ok();
            })
            .detach();
        }

        let picker = Picker::new(TYPEFACES, &self.popovers, current)
            .width(Picker::WIDE)
            .menu(self.typefaces.menu("typefaces-menu", Picker::WIDE))
            .items(items)
            .when(self.loading_fonts, |picker| {
                picker.item(
                    MenuItem::new("typeface-loading", t!("settings-typeface-loading")).disabled(),
                )
            })
            .when(barren && !self.loading_fonts, |picker| {
                picker
                    .item(MenuItem::new("typeface-empty", t!("settings-typeface-none")).disabled())
            });

        let keys = self.typefaces.controls(
            picker,
            count,
            move |this, place, _, cx| {
                let Some(name) = submitted.get(place) else {
                    return;
                };
                this.settings
                    .update(cx, |settings, cx| settings.set_font(name.to_string(), cx));
                this.popovers.close();
                cx.notify();
            },
            cx,
        );

        self.row(t!("settings-typeface"), keys.into_any_element())
    }

    pub(super) fn corners_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides();

        let picker = Picker::new(CORNERS, &self.popovers, look.rounding.label())
            .width(Picker::NARROW)
            .items(Rounding::ALL.into_iter().map(|rounding| {
                let overrides = overrides.clone();
                MenuItem::new(rounding.id(), rounding.label())
                    .selected(look.rounding == rounding)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings.update(cx, |settings, cx| {
                            settings.set_rounding(rounding.id(), cx);
                        });
                        Theme::set(Look { rounding, ..look }, &overrides, cx);
                        cx.notify();
                    }))
            }));

        self.row(t!("settings-corners"), picker.into_any_element())
    }

    /// Turns every frosted treatment in the app on or off at once: the menus, the fields, the
    /// floating panels and the bands the chrome lays over the page. A backdrop blur is the
    /// priciest thing the renderer does per frame, which is the whole reason it is a choice.
    pub(super) fn blur_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides();

        self.row(
            t!("settings-blur"),
            Switch::new("blur", look.blur)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let blur = !look.blur;
                    this.settings
                        .update(cx, |settings, cx| settings.set_blur(blur, cx));
                    Theme::set(Look { blur, ..look }, &overrides, cx);
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// Asks the platform to blur the desktop behind a see-through window. It needs something to
    /// show through, so the switch is off and disabled while the window is opaque.
    pub(super) fn blur_window_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides();
        let opaque = !look.transparent;

        self.row(
            t!("settings-blur-window"),
            Switch::new("blur-window", look.blur_window && !opaque)
                .disabled(opaque)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let blur_window = !look.blur_window;
                    this.settings
                        .update(cx, |settings, cx| settings.set_blur_window(blur_window, cx));
                    Theme::set(
                        Look {
                            blur_window,
                            ..look
                        },
                        &overrides,
                        cx,
                    );
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    pub(super) fn font_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides();

        let step = move |id: &'static str, label: &'static str, delta: f32| {
            let overrides = overrides.clone();
            let wanted = (look.font + delta).clamp(MIN_FONT, MAX_FONT);

            Button::new(id)
                .label(label)
                .small()
                .outline()
                .disabled(wanted == look.font)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_font_size(wanted, cx));
                    Theme::set(
                        Look {
                            font: wanted,
                            ..look
                        },
                        &overrides,
                        cx,
                    );
                    cx.notify();
                }))
        };

        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .child(step("font-smaller", "−", -1.))
            .child(div().child(t!("settings-font-value", size = look.font.round() as i64)))
            .child(step("font-larger", "+", 1.));

        self.row(t!("settings-font"), actions.into_any_element())
    }

    pub(super) fn theme_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = self.look(cx);
        let (selected, adaptive, custom) = {
            let settings = self.settings.read(cx);
            (
                settings.theme().to_owned(),
                settings.adaptive_theme(),
                settings
                    .custom_themes()
                    .map(|(id, name)| (id.to_owned(), name.to_owned()))
                    .collect::<Vec<_>>(),
            )
        };
        let built_in = ThemeKind::ALL
            .into_iter()
            .find(|kind| kind.id() == selected);
        let custom_selected = custom.iter().any(|(id, _)| id == &selected);
        let current = custom
            .iter()
            .find(|(id, _)| id == &selected)
            .map(|(_, name)| SharedString::from(name.clone()))
            .or_else(|| built_in.map(ThemeKind::label))
            .unwrap_or_else(|| t!("theme-unavailable", name = selected.clone()));

        let mut items = ThemeKind::ALL
            .into_iter()
            .map(|kind| {
                let item = MenuItem::new(kind.id(), kind.label()).selected(selected == kind.id());
                match adaptive
                    && !matches!(kind, ThemeKind::System | ThemeKind::Dark | ThemeKind::Light)
                {
                    true => item.disabled().tooltip("settings-theme-unavailable"),
                    false => item.on_click(cx.listener(move |this, _, _, cx| {
                        let overrides = this.settings.update(cx, |settings, cx| {
                            settings.set_theme(kind.id(), cx);
                            settings.theme_overrides()
                        });
                        Theme::fade(Look { kind, ..look }, &overrides, cx);
                        cx.notify();
                    })),
                }
            })
            .collect::<Vec<_>>();

        if !custom.is_empty() || built_in.is_none() {
            items.push(MenuItem::separator(("custom-themes", 0usize)));
        }
        items.extend(custom.into_iter().map(|(id, name)| {
            let item = MenuItem::new(format!("custom-theme:{id}"), name).selected(selected == id);
            match adaptive {
                true => item.disabled().tooltip("settings-theme-unavailable"),
                false => item.on_click(cx.listener(move |this, _, _, cx| {
                    let overrides = this.settings.update(cx, |settings, cx| {
                        settings.set_theme(id.clone(), cx);
                        settings.theme_overrides()
                    });
                    Theme::fade(
                        Look {
                            kind: ThemeKind::Dark,
                            ..look
                        },
                        &overrides,
                        cx,
                    );
                    cx.notify();
                })),
            }
        }));
        if built_in.is_none() && !custom_selected {
            items.push(
                MenuItem::new(
                    "unavailable-theme",
                    t!("theme-unavailable", name = selected.clone()),
                )
                .selected(true)
                .disabled(),
            );
        }

        let picker = Picker::new(THEMES, &self.popovers, current)
            .width(Picker::REGULAR)
            .items(items);

        let settings = self.settings.clone();
        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("open-theme-folder")
                    .label(t!("settings-theme-folder"))
                    .small()
                    .outline()
                    .on_click(move |_, _, cx| {
                        let path =
                            settings.update(cx, |settings, _| settings.ensure_themes_directory());
                        if let Err(error) = open::that_detached(&path) {
                            log::warn!("settings: cannot open {}: {error}", path.display());
                        }
                    }),
            )
            .child(picker);

        self.row(t!("settings-theme"), actions.into_any_element())
    }

    pub(super) fn icons_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let chosen = self.settings.read(cx).icons().to_owned();
        let current = icons::pack(&chosen).unwrap_or_else(icons::active);

        let picker = Picker::new(PACKS, &self.popovers, current.title())
            .width(Picker::REGULAR)
            .items(icons::packs().map(|pack| {
                MenuItem::new(pack.id, pack.title())
                    .selected(pack.id == current.id)
                    .detail(samples(pack, theme.muted_foreground))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_icons(pack.id, cx));
                        cx.notify();
                    }))
            }));

        self.row(t!("settings-icons"), picker.into_any_element())
    }

    pub(super) fn opacity_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides();
        let transparency = match look.transparent {
            true => look.transparency,
            false => 0.,
        };
        let value = 1. - transparency / MAX_TRANSPARENCY;
        let percent = ((1. - transparency) * 100.).round() as i64;

        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div().w(theme.metrics.cover).child(
                    Scrubber::new(&self.opacity, value)
                        .colors(theme.progress_bar, theme.muted, theme.foreground)
                        .on_move(cx.listener(move |this, fraction: &f32, _, cx| {
                            let transparency = (1. - *fraction) * MAX_TRANSPARENCY;
                            let transparent = transparency > 0.;
                            this.settings.update(cx, |settings, cx| {
                                settings.set_transparent(transparent, cx);
                                settings.set_transparency(transparency, cx);
                            });
                            Theme::set(
                                Look {
                                    transparent,
                                    transparency,
                                    ..look
                                },
                                &overrides,
                                cx,
                            );
                        })),
                ),
            )
            .child(
                div()
                    .flex_none()
                    .w(theme.metrics.control * 1.5)
                    .whitespace_nowrap()
                    .text_right()
                    .child(t!("settings-opacity-value", percent = percent)),
            );

        self.row(t!("settings-opacity"), control.into_any_element())
    }

    pub(super) fn adaptive_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let (on, neutral) = {
            let settings = self.settings.read(cx);
            let theme = settings.theme();
            (
                settings.adaptive_theme(),
                [ThemeKind::System, ThemeKind::Dark, ThemeKind::Light]
                    .into_iter()
                    .any(|kind| kind.id() == theme),
            )
        };
        let look = self.look(cx);

        self.hinted(
            t!("settings-adaptive"),
            "settings-adaptive-detail",
            Switch::new("adaptive-theme", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let adaptive = !on;
                    let reset = adaptive && !neutral;
                    let kind = match reset {
                        true => ThemeKind::Dark,
                        false => look.kind,
                    };
                    let overrides = this.settings.update(cx, |settings, cx| {
                        settings.set_adaptive_theme(adaptive, cx);
                        if reset {
                            settings.set_theme(kind.id(), cx);
                        }
                        settings.theme_overrides()
                    });
                    if reset {
                        Theme::fade(Look { kind, ..look }, &overrides, cx);
                    }
                }))
                .into_any_element(),
        )
    }
}

pub(super) fn samples(pack: &'static icons::Pack, tint: gpui::Hsla) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .children(icons::SAMPLES.iter().map(|name| {
            svg()
                .path(icons::shown(pack, name))
                .size(px(14.))
                .flex_none()
                .text_color(tint)
        }))
}

pub(super) fn usable_fonts(text_system: std::sync::Arc<gpui::TextSystem>) -> Vec<SharedString> {
    let mut names = text_system.all_font_names();
    names.sort_unstable();
    names.dedup();

    names
        .into_iter()
        .filter(|name| !name.starts_with('.'))
        .map(SharedString::from)
        .collect()
}
