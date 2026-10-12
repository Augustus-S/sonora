//! The Jam as the app shows it: the question before a shared link is joined, and the list of
//! people in the one the listener is in. Both are `state::Devices`' to answer; this only draws
//! them.

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Div, Entity, FocusHandle, Pixels, Point, Render, SharedString, Window, div,
};
use i18n::t;
use music::connect::{Jam, JamMember};
use state::{Devices, Sonora};
use ui::{
    ActiveTheme as _, Avatar, Button, Dismiss, FORM_CONTEXT, Initials, Menu, MenuItem, Modal,
    Picker, Popup, Submit, Text, Theme,
};

/// Asks whether to join the Jam a pasted link names. It shows for as long as `Devices` holds an
/// invite, wherever the listener is in the app, and either answer hands the invite back.
pub(crate) struct JamInvite {
    devices: Entity<Devices>,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    shown: bool,
}

impl JamInvite {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let devices = Sonora::global(cx).devices.clone();
        cx.observe(&devices, |_, _, cx| cx.notify()).detach();
        Self {
            devices,
            focus: cx.focus_handle(),
            restore: None,
            shown: false,
        }
    }

    fn decline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release(window, cx);
        self.devices.update(cx, |devices, cx| devices.dismiss(cx));
    }

    fn join(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release(window, cx);
        self.devices.update(cx, |devices, cx| devices.accept(cx));
    }

    fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = self.restore.take() {
            window.focus(&focus, cx);
        }
        self.shown = false;
    }
}

impl Render for JamInvite {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let asked = self.devices.read(cx).invite().map(|invite| {
            (
                host_name(&invite.jam).map(str::to_owned),
                invite.jam.members.len(),
            )
        });
        let Some((host, count)) = asked else {
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

        let title = match host.as_deref() {
            Some(host) => t!("devices-jam-invite-title", host = host),
            None => t!("devices-jam-invite-title-anonymous"),
        };

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
                this.join(window, cx);
            }))
            .child(
                Modal::new("jam-invite", title)
                    .when(count > 0, |modal| {
                        modal.detail(t!("devices-jam-invite-detail", count = count))
                    })
                    .action(
                        Button::new("decline-jam")
                            .ghost()
                            .label(t!("common-cancel"))
                            .on_click(cx.listener(|this, _, window, cx| this.decline(window, cx))),
                    )
                    .action(
                        Button::new("join-jam")
                            .primary()
                            .label(t!("devices-jam-invite-join"))
                            .on_click(cx.listener(|this, _, window, cx| this.join(window, cx))),
                    )
                    .on_dismiss(cx.listener(|this, _, window, cx| this.decline(window, cx))),
            )
            .into_any_element()
    }
}

/// The list of people in the Jam the listener is in, opened as a popup over the player bar. The
/// owner gets a button to remove anyone else. It closes by itself once the Jam is over.
pub(crate) struct JamPeople {
    devices: Entity<Devices>,
    /// Where the list ends while it is open.
    at: Option<Point<Pixels>>,
}

impl JamPeople {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let devices = Sonora::global(cx).devices.clone();
        cx.observe(&devices, |this, devices, cx| {
            if devices.read(cx).jam().is_none() {
                this.at = None;
            }
            cx.notify();
        })
        .detach();
        Self { devices, at: None }
    }

    /// Opens the list upward from `at`, a point on the strip or menu that asked for it.
    pub fn show(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.at = Some(at);
        cx.notify();
    }
}

impl Render for JamPeople {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(at) = self.at else {
            return div().absolute().into_any_element();
        };
        let Some(jam) = self.devices.read(cx).jam() else {
            return div().absolute().into_any_element();
        };

        let theme = *cx.theme();
        let size = theme.metrics.control_small;
        let devices = self.devices.clone();
        let rows = jam.members.iter().enumerate().map(|(index, member)| {
            let removable = jam.owner && !member.host && !member.you;
            let id = member.id.clone();
            let devices = devices.clone();

            MenuItem::new(("jam-member", index), "").content(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .child(face(member, size))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(name(member).to_owned())),
                    )
                    .when(member.host, |this| {
                        this.child(badge(t!("devices-jam-host"), theme))
                    })
                    .when(member.you, |this| {
                        this.child(badge(t!("devices-jam-you"), theme))
                    })
                    .when(removable, |this| {
                        this.child(
                            Button::new(("jam-remove", index))
                                .small()
                                .label(t!("devices-jam-remove"))
                                .on_click(move |_, _, cx| devices.read(cx).kick(&id)),
                        )
                    }),
            )
        });

        Popup::new(at, Menu::new("jam-people").w(Picker::WIDE).items(rows))
            .above()
            .on_close(cx.listener(|this, _, _, cx| {
                this.at = None;
                cx.notify();
            }))
            .into_any_element()
    }
}

/// The name of the person who owns `jam`, when the provider gave one.
pub(crate) fn host_name(jam: &Jam) -> Option<&str> {
    jam.host()
        .map(|host| host.name.as_str())
        .filter(|name| !name.is_empty())
}

/// What to call `member`: their name, or the id the provider knows them by when it gave none.
fn name(member: &JamMember) -> &str {
    match member.name.is_empty() {
        true => &member.id,
        false => &member.name,
    }
}

/// A member's round picture at `size`, or their initials while they have none.
pub(crate) fn face(member: &JamMember, size: Pixels) -> AnyElement {
    match &member.image {
        Some(image) => Avatar::new(Some(image.clone()))
            .size(size)
            .into_any_element(),
        None => Initials::new(name(member).to_owned(), size).into_any_element(),
    }
}

/// The first `limit` members' faces side by side at `size`, with how many more there are after
/// them.
pub(crate) fn faces(members: &[JamMember], limit: usize, size: Pixels, theme: Theme) -> Div {
    let more = members.len().saturating_sub(limit);

    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_1()
        .children(members.iter().take(limit).map(|member| face(member, size)))
        .when(more > 0, |this| {
            this.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .h(size)
                    .min_w(size)
                    .px_1()
                    .rounded_full()
                    .bg(theme.secondary)
                    .text_size(theme.text(Text::Tiny))
                    .text_color(theme.muted_foreground)
                    .child(t!("devices-jam-more", count = more)),
            )
        })
}

/// A small tag beside a member's name.
fn badge(label: SharedString, theme: Theme) -> impl IntoElement {
    div()
        .flex_none()
        .px_1()
        .rounded(theme.radius)
        .bg(theme.muted)
        .text_size(theme.text(Text::Tiny))
        .text_color(theme.muted_foreground)
        .child(label)
}
