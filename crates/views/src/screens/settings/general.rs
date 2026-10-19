use gpui::prelude::*;
use gpui::{AnyElement, App, Context, SharedString};
use i18n::{Language, t};
use router::{NavEntry, Screen};
use ui::{Button, MenuItem, Pace, Picker, Rounding, Saver, Stillness, Switch};

use super::{Group, LANGUAGES, SettingsView, Slot};
use crate::shared::popups::matches_query;

const STARTUP: &str = "startup";
const ENTRIES: &str = "entries";
const MOTION: &str = "motion";
const PACE: &str = "pace";
const SAVER: &str = "saver";
#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
const WINDOW_ROUNDING: &str = "window-rounding";

impl SettingsView {
    pub(super) fn general_groups(&self, cx: &App) -> Vec<Group> {
        vec![
            Group::new("settings-group-startup", "icons/house.svg").rows([
                Slot::Startup,
                Slot::Entries,
                Slot::Language,
            ]),
            Group::new("settings-group-window", "icons/panel-left-open.svg")
                .rows([Slot::Tray, Slot::TrayIcon])
                .rows(self.decoration_slots(cx)),
            Group::new("settings-group-motion", "icons/loader-circle.svg").rows([
                Slot::Motion,
                Slot::Pace,
                Slot::Saver,
            ]),
            Group::new("settings-group-context-menus", "icons/list.svg").rows([Slot::AdaptiveMenu]),
        ]
    }

    /// The rows about the window's own frame, which differ by platform.
    #[allow(
        unused_variables,
        reason = "cx is only read on Linux and FreeBSD, where rounding follows the decorations"
    )]
    fn decoration_slots(&self, cx: &App) -> Vec<Slot> {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let mut slots = vec![Slot::ServerSideDecorations, Slot::Side, Slot::TrafficLights];
        // Server-side decorations put the compositor in charge of the frame, so window
        // rounding is only ever this app's call with client-side ones.
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if !self.settings.read(cx).server_side_decorations() {
            slots.push(Slot::WindowRounding);
        }
        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "macos")))]
        let slots = vec![
            Slot::Decorations,
            Slot::Side,
            Slot::TrafficLights,
            Slot::WindowRounding,
        ];
        #[cfg(target_os = "macos")]
        let slots = vec![Slot::Decorations, Slot::WindowRounding];
        slots
    }

    pub(super) fn startup_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = Screen::from_id(self.settings.read(cx).startup()).unwrap_or(Screen::Home);
        let current = i18n::lookup(chosen.key(), None);
        let guest = !self.session.read(cx).authenticated();

        let picker = Picker::new(STARTUP, &self.popovers, current)
            .width(Picker::NARROW)
            .items(Screen::ALL.map(|screen| {
                let item = MenuItem::new(screen.id(), i18n::lookup(screen.key(), None))
                    .selected(screen == chosen);
                match guest && screen.needs_account() {
                    true => item.disabled().tooltip("settings-startup-no-guest"),
                    false => item.on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_startup(screen.id(), cx));
                        cx.notify();
                    })),
                }
            }));

        self.row(t!("settings-startup"), picker.into_any_element())
    }

    pub(super) fn entries_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let picker = Picker::new(ENTRIES, &self.popovers, t!("settings-entries-pick"))
            .width(Picker::REGULAR)
            .sticky()
            .items(NavEntry::ALL.map(|entry| {
                let shown = self.settings.read(cx).nav_shown(entry.id());

                MenuItem::new(entry.id(), i18n::lookup(entry.key(), None))
                    .selected(shown)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings.update(cx, |settings, cx| {
                            settings.set_nav_shown(entry.id(), !shown, cx)
                        });
                        cx.notify();
                    }))
            }));

        self.row(t!("settings-entries"), picker.into_any_element())
    }

    pub(super) fn language_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).language().to_owned();
        let current = match Language::from_id(&chosen) {
            Some(language) => SharedString::from(language.label()),
            None => t!("settings-language-system"),
        };

        let asked = self.languages.query();
        let entries = std::iter::once((i18n::AUTO, t!("settings-language-system")))
            .chain(
                Language::ALL
                    .into_iter()
                    .map(|language| (language.id(), SharedString::from(language.label()))),
            )
            .filter(|(id, label)| matches_query(id, label, &asked))
            .collect::<Vec<_>>();
        let barren = entries.is_empty();
        let count = entries.len();
        let cursor = self.languages.cursor(count);
        let submitted = entries.clone();

        let picker = Picker::new(LANGUAGES, &self.popovers, current)
            .width(Picker::WIDE)
            .menu(self.languages.menu("languages-menu", Picker::WIDE))
            .items(entries.into_iter().enumerate().map(|(place, (id, label))| {
                MenuItem::new(id, label)
                    .selected(place == cursor)
                    .checked(chosen == id)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_language(id, cx));
                        this.popovers.close();
                        cx.notify();
                    }))
            }))
            .when(barren, |picker| {
                picker
                    .item(MenuItem::new("language-empty", t!("settings-language-none")).disabled())
            });
        let picker = self.languages.controls(
            picker,
            count,
            move |this, place, _, cx| {
                let Some((id, _)) = submitted.get(place) else {
                    return;
                };
                this.settings
                    .update(cx, |settings, cx| settings.set_language(*id, cx));
                this.popovers.close();
                cx.notify();
            },
            cx,
        );

        self.row(t!("settings-language"), picker.into_any_element())
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub(super) fn server_side_decorations_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled = self.settings.read(cx).server_side_decorations();

        self.row(
            t!("settings-server-side-decorations"),
            Switch::new("server-side-decorations", enabled)
                .on_click(cx.listener(move |this, _, window, cx| {
                    let decorations = this.settings.update(cx, |settings, cx| {
                        settings.set_server_side_decorations(!enabled, cx);
                        settings.window_decorations()
                    });
                    window.request_decorations(decorations);
                }))
                .into_any_element(),
        )
    }

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    pub(super) fn decorations_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).window_controls();

        self.row(
            t!("settings-window-controls"),
            Switch::new("window-controls", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_window_controls(!on, cx));
                }))
                .into_any_element(),
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn traffic_light_controls_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).traffic_light_controls();

        self.row(
            t!("settings-traffic-light-controls"),
            Switch::new("traffic-light-controls", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_traffic_light_controls(!on, cx)
                    });
                }))
                .into_any_element(),
        )
    }

    #[cfg(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "macos"
    ))]
    pub(super) fn window_rounding_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings.read(cx).window_rounding();

        let picker = Picker::new(WINDOW_ROUNDING, &self.popovers, current.label())
            .width(Picker::NARROW)
            // AppKit has no square window, so macOS offers only the corners it can draw.
            .items(
                Rounding::ALL
                    .into_iter()
                    .filter(|rounding| {
                        cfg!(not(target_os = "macos")) || *rounding != Rounding::Square
                    })
                    .map(|rounding| {
                        MenuItem::new(rounding.id(), rounding.label())
                            .selected(current == rounding)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings.update(cx, |settings, cx| {
                                    settings.set_window_rounding(rounding, cx)
                                });
                                cx.notify();
                            }))
                    }),
            );

        self.row(t!("settings-window-rounding"), picker.into_any_element())
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn side_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings.read(cx);
        let left = settings.controls_on_left();
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let shown = !settings.server_side_decorations();
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let shown = settings.window_controls();

        self.row(
            t!("settings-controls-side"),
            Button::new("controls-side")
                .label(match left {
                    true => t!("common-left"),
                    false => t!("common-right"),
                })
                .small()
                .outline()
                .disabled(!shown)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_controls_on_left(!left, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn motion_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings.read(cx).stillness();

        let picker = Picker::new(MOTION, &self.popovers, current.label())
            .width(Picker::NARROW)
            .items(Stillness::ALL.into_iter().map(|stillness| {
                MenuItem::new(stillness.id(), stillness.label())
                    .selected(current == stillness)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings.update(cx, |settings, cx| {
                            settings.set_stillness(stillness, cx);
                        });
                        cx.notify();
                    }))
            }));

        self.row(t!("settings-motion"), picker.into_any_element())
    }

    pub(super) fn pace_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings.read(cx).pace();

        let picker = Picker::new(PACE, &self.popovers, current.label())
            .width(Picker::NARROW)
            .items(Pace::ALL.into_iter().map(|pace| {
                MenuItem::new(pace.id(), pace.label())
                    .selected(current == pace)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_pace(pace, cx));
                        cx.notify();
                    }))
            }));

        self.row(t!("settings-pace"), picker.into_any_element())
    }

    pub(super) fn saver_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings.read(cx).saver();

        let picker = Picker::new(SAVER, &self.popovers, current.label())
            .width(Picker::NARROW)
            .items(Saver::ALL.into_iter().map(|saver| {
                MenuItem::new(saver.id(), saver.label())
                    .selected(current == saver)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_saver(saver, cx));
                        cx.notify();
                    }))
            }));

        self.hinted(
            t!("settings-saver"),
            "settings-saver-detail",
            picker.into_any_element(),
        )
    }

    pub(super) fn adaptive_menu_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).adaptive_menu();

        self.hinted(
            t!("settings-adaptive-menu"),
            "settings-adaptive-menu-detail",
            Switch::new("adaptive-menu", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_adaptive_menu(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn tray_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).close_to_tray();

        self.row(
            t!("settings-close-to-tray"),
            Switch::new("close-to-tray", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_close_to_tray(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn tray_icon_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).tray_icon();

        self.row(
            t!("settings-tray-icon"),
            Switch::new("tray-icon", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_tray_icon(!on, cx));
                }))
                .into_any_element(),
        )
    }
}
