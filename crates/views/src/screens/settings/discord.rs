use gpui::prelude::*;
use gpui::{AnyElement, App, Context};
use i18n::t;
use state::DiscordName;
use ui::{MenuItem, Picker, Switch};

use super::{Group, SettingsView, Slot};

const DISCORD_NAME: &str = "discord-name";
const DISCORD_BUTTONS: &str = "discord-buttons";

impl SettingsView {
    /// The Discord page. While the status is off only the master switch and the local files
    /// row stand, since the rest shapes a status nobody sees.
    pub(super) fn discord_groups(&self, cx: &App) -> Vec<Group> {
        let mut groups = vec![Group::untitled().rows([Slot::Discord])];
        if self.settings.read(cx).discord_presence() {
            groups.push(Group::new("settings-group-status").rows([
                Slot::DiscordName,
                Slot::DiscordShowPaused,
                Slot::DiscordBadge,
                Slot::DiscordAnonymous,
            ]));
            groups.push(Group::new("settings-discord-buttons").rows([Slot::DiscordButtons]));
        }
        groups.push(Group::new("settings-group-local-files").rows([Slot::ArtworkForLocal]));
        groups
    }

    pub(super) fn discord_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).discord_presence();

        self.row(
            t!("settings-discord"),
            Switch::new("discord", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_discord_presence(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn discord_name_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).discord_name();

        let picker = Picker::new(
            DISCORD_NAME,
            &self.popovers,
            i18n::lookup(chosen.key(), None),
        )
        .width(Picker::NARROW)
        .items(DiscordName::ALL.map(|name| {
            MenuItem::new(name.id(), i18n::lookup(name.key(), None))
                .selected(name == chosen)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_discord_name(name, cx));
                    cx.notify();
                }))
        }));

        self.row(t!("settings-discord-name"), picker.into_any_element())
    }

    pub(super) fn discord_show_paused_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).discord_show_paused();

        self.row(
            t!("settings-discord-show-paused"),
            Switch::new("discord-show-paused", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_discord_show_paused(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn discord_badge_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).discord_badge();

        self.row(
            t!("settings-discord-badge"),
            Switch::new("discord-badge", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_discord_badge(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn artwork_for_local_files_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).artwork_for_local_files();

        self.row(
            t!("settings-artwork-for-local-files"),
            Switch::new("artwork-for-local-files", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_artwork_for_local_files(!on, cx)
                    });
                }))
                .into_any_element(),
        )
    }

    pub(super) fn discord_anonymous_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).discord_without_details();

        self.row(
            t!("settings-discord-anonymous"),
            Switch::new("discord-anonymous", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_discord_without_details(!on, cx)
                    });
                }))
                .into_any_element(),
        )
    }

    pub(super) fn discord_buttons_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings.read(cx);
        let sonora = settings.discord_sonora_button();
        let provider = settings.discord_provider_button();

        let picker = Picker::new(
            DISCORD_BUTTONS,
            &self.popovers,
            t!("settings-discord-buttons-pick"),
        )
        .width(Picker::NARROW)
        .sticky()
        .item(
            MenuItem::new(
                "discord-button-provider",
                t!("settings-discord-name-provider"),
            )
            .selected(provider)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.settings.update(cx, |settings, cx| {
                    settings.set_discord_provider_button(!provider, cx)
                });
                cx.notify();
            })),
        )
        .item(
            MenuItem::new("discord-button-sonora", t!("settings-discord-name-sonora"))
                .selected(sonora)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_discord_sonora_button(!sonora, cx)
                    });
                    cx.notify();
                })),
        );

        self.row(t!("settings-discord-buttons"), picker.into_any_element())
    }
}
