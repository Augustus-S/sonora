use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, Entity, FocusHandle, Focusable as _, FontWeight, Render,
    ScrollHandle, SharedString, Task, Window, div, svg,
};
use i18n::t;
use music::equalizer::{Curve, MAX_FILTERS, PREAMP_RANGE, Preamp};
use state::{AppSettings, Outputs, Playback, Signal, Sonora, equalizer_profile_label};
use ui::{
    ActiveTheme as _, Button, Dismiss, FORM_CONTEXT, InfoCard, Input, MenuItem, Modal, Picker,
    Popovers, Room, Scrollbar, Scroller, Submit, TabBar, Text, heading,
};

use crate::chrome::Chrome;
use crate::shared::confirm::{self, Confirm};
use crate::shared::equalizer::sound_picker;

use graph::Graph;
use rows::{Row, figure};

mod graph;
mod outputs;
mod path;
mod rows;
mod transfer;

const SOUND: &str = "equalizer-sound";
const PREAMP: &str = "equalizer-preamp";
/// Below this an overload rounds to no tenth of a decibel at all.
const SLIGHT_OVERLOAD: f64 = 0.05;
/// How wide a number field is, in cover sizes.
const FIELD: f32 = 0.6;
/// How wide the rename dialog is, in cover sizes.
const DIALOG: f32 = 2.8;
/// How wide the sound picker and its menu are in a wide room, in cover sizes. A narrower room
/// lets the picker fill its row instead.
const SOUND_WIDTH: f32 = 2.2;
/// How narrow a card's description gets before the action beside it wraps underneath, in
/// cover sizes.
const BLURB: f32 = 1.6;
/// How strongly the overload notice's border and fill take the danger color.
const OVERLOAD_EDGE: f32 = 0.4;
const OVERLOAD_FILL: f32 = 0.08;

/// The equalizer's own screen. Simple mode shows the sound picker, undo and the response graph,
/// and advanced mode adds a card each for the preamp, the filters, the per-output profiles,
/// Equalizer APO text in and out, and the signal path.
pub(crate) struct EqualizerView {
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    signal: Entity<Signal>,
    outputs: Entity<Outputs>,
    popovers: Popovers,
    scrollbar: Entity<Scrollbar>,
    preamp: Entity<Input>,
    rows: Vec<Row>,
    name: Entity<Input>,
    /// The profile the rename dialog is open for, by its index and stored name then, so a
    /// profile list that changed meanwhile renames nothing.
    renaming: Option<(usize, String)>,
    /// The rename dialog's own focus, and what had focus before it opened, given back on close.
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    /// The curve the fields were last filled from, so they are only rewritten when it changes.
    shown: Option<Curve>,
    graph: Graph,
    /// The output meter's reads and the spectrum's, held while the screen is the one shown and
    /// dropped with it.
    metering: Option<[Task<()>; 2]>,
    /// The signal path's reads, held while it is open in advanced mode.
    path: Option<Task<()>>,
    /// The file being read in or written out.
    file: Option<Task<()>>,
}

impl EqualizerView {
    pub(crate) fn new(
        playback: Entity<Playback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = Sonora::global(cx).settings.clone();
        let signal = Sonora::global(cx).signal.clone();
        let outputs = Sonora::global(cx).outputs.clone();
        let id = cx.entity_id();
        let preamp = cx.new(|cx| Input::new("", cx).compact());
        let rows = (0..MAX_FILTERS).map(|_| Row::new(cx)).collect();

        cx.observe(&playback, |this, _, cx| {
            this.show_curve(cx);
            cx.notify();
        })
        .detach();
        cx.observe(&settings, |this, settings, cx| {
            if !settings.read(cx).equalizer_advanced() {
                this.path = None;
            }
            cx.notify();
        })
        .detach();
        cx.observe(&signal, |_, _, cx| cx.notify()).detach();
        cx.observe(&outputs, |_, _, cx| cx.notify()).detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

        let mut view = Self {
            playback,
            settings,
            signal,
            outputs,
            popovers: Popovers::default(),
            scrollbar: cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(id)),
            preamp,
            rows,
            name: cx.new(|cx| Input::new("equalizer-profile-name-hint", cx)),
            renaming: None,
            focus: cx.focus_handle(),
            restore: None,
            shown: None,
            graph: Graph::new(),
            metering: None,
            path: None,
            file: None,
        };
        let fields: Vec<_> = view
            .fields()
            .map(|(field, input)| (field, input.read(cx).focus_handle(cx)))
            .collect();
        for (field, focus) in fields {
            cx.on_blur(&focus, window, move |this, _, cx| this.commit(field, cx))
                .detach();
        }
        view.show_curve(cx);
        view
    }

    /// Reads the output meter, the playing rate and the spectrum while the screen is the one
    /// shown.
    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if visible != self.metering.is_some() {
            self.metering = visible.then(|| {
                [
                    self.playback
                        .update(cx, |playback, cx| playback.watch_overload(cx)),
                    self.signal.update(cx, |signal, cx| signal.watch(cx)),
                ]
            });
        }
        if !visible {
            self.path = None;
        }
    }

    /// Plays `curve`, an edit made on this screen.
    fn apply(&mut self, curve: Curve, cx: &mut Context<Self>) {
        self.graph.edited(&curve);
        self.playback
            .update(cx, |playback, cx| playback.set_equalizer_curve(curve, cx));
    }

    fn set_preamp(&mut self, auto: bool, cx: &mut Context<Self>) {
        let Some(curve) = self.shown.clone() else {
            return;
        };
        let preamp = self.playback.read(cx).equalizer_preamp_db();
        if auto == (curve.preamp() == Preamp::Auto) {
            return;
        }
        // A manual preamp starts where the automatic one was, rounded down so switching never
        // raises the level.
        let preamp = match auto {
            true => Preamp::Auto,
            false => Preamp::Manual(
                ((preamp * 10.).floor() / 10.).clamp(*PREAMP_RANGE.start(), *PREAMP_RANGE.end()),
            ),
        };
        if let Ok(curve) = Curve::new(preamp, curve.filters().to_vec()) {
            self.apply(curve, cx);
        }
    }

    fn new_profile(&mut self, cx: &mut Context<Self>) {
        let count = self.settings.read(cx).equalizer_profiles().count();
        let name = t!("equalizer-profile-new", number = count + 1);
        let curve = Curve::new(Preamp::Auto, Vec::new()).expect("an empty curve is valid");
        self.playback.update(cx, |playback, cx| {
            playback.add_equalizer_profile(&name, curve, cx)
        });
    }

    fn duplicate_profile(&mut self, cx: &mut Context<Self>) {
        let current = equalizer_profile_label(self.settings.read(cx).equalizer_profile_name());
        let name = t!("equalizer-profile-copy", name = &current);
        let curve = self.playback.read(cx).equalizer_curve().clone();
        self.playback.update(cx, |playback, cx| {
            playback.add_equalizer_profile(&name, curve, cx)
        });
    }

    /// Opens the rename dialog on the playing profile, filled with the name it shows.
    fn open_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.settings.read(cx);
        let (index, stored) = (
            settings.equalizer_profile(),
            settings.equalizer_profile_name().to_owned(),
        );
        let shown = equalizer_profile_label(&stored);
        self.restore = window.focused(cx);
        self.name.update(cx, |input, cx| {
            input.set_text(shown, cx);
            input.focus(window, cx);
        });
        self.renaming = Some((index, stored));
        cx.notify();
    }

    fn close_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.renaming = None;
        if let Some(focus) = self.restore.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Renames the profile the dialog was opened for to what it holds. An empty name is not
    /// saved and the dialog stays open.
    fn rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((index, from)) = self.renaming.clone() else {
            return;
        };
        let name = self.name.read(cx).text().to_owned();
        let renamed = self.settings.update(cx, |settings, cx| {
            settings.rename_equalizer_profile(index, &from, &name, cx)
        });
        if renamed {
            self.close_rename(window, cx);
        }
    }

    /// Asks before deleting the playing profile, and deletes the one it asked about.
    fn delete(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings.read(cx);
        let index = settings.equalizer_profile();
        let name = settings.equalizer_profile_name().to_owned();
        let playback = self.playback.clone();
        Confirm::ask(
            confirm::Kind::EqualizerProfile(name.clone().into()),
            move |cx| {
                playback.update(cx, |playback, cx| {
                    playback.remove_equalizer_profile(index, &name, cx)
                })
            },
            cx,
        );
    }

    /// The page title and what the screen is for, with the Simple and Advanced switch on the
    /// right. The choice is remembered across launches.
    fn header(&self, advanced: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let mode = |id: &'static str, label: SharedString, choice: bool| {
            Button::new(id)
                .small()
                .ghost()
                .label(label)
                .selected(advanced == choice)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_equalizer_advanced(choice, cx)
                    })
                }))
        };
        let modes = [
            mode("equalizer-simple", t!("equalizer-simple"), false),
            mode("equalizer-advanced", t!("equalizer-advanced"), true),
        ];

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .min_w_0()
                    .child(heading(t!("settings-equalizer"), cx))
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(t!("equalizer-subtitle")),
                    ),
            )
            .child(TabBar::new("equalizer-mode").flex_none().items(modes))
            .into_any_element()
    }

    /// A slim notice of how far the output went over full scale lately, and the fix, while it
    /// overloads. The fix is left out when the preamp is as low as it goes.
    fn overload(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let playback = self.playback.read(cx);
        let over = playback.overload()?;
        let fixable = playback.equalizer() && playback.lowered_preamp().is_some();
        let theme = *cx.theme();
        let size = theme.text(Text::Small);
        Some(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_x_3()
                .gap_y_2()
                .px(theme.metrics.pad * 1.5)
                .py(theme.metrics.pad)
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.danger.opacity(OVERLOAD_EDGE))
                .bg(theme.danger.opacity(OVERLOAD_FILL))
                .text_size(size)
                .child(
                    svg()
                        .path(icons::path("icons/triangle-alert.svg"))
                        .size(size)
                        .flex_none()
                        .text_color(theme.danger),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.danger)
                        .child(match over < SLIGHT_OVERLOAD {
                            true => t!("equalizer-overload-slight"),
                            false => t!("equalizer-overload", db = figure(over, 1)),
                        }),
                )
                .when(fixable, |this| {
                    this.child(
                        Button::new("equalizer-lower-preamp")
                            .small()
                            .outline()
                            .label(t!("equalizer-lower-preamp"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.playback
                                    .update(cx, |playback, cx| playback.lower_preamp(cx))
                            })),
                    )
                })
                .into_any_element(),
        )
    }

    /// The main card: the one picker for every preset and profile with undo and redo beside it,
    /// then the response graph. Advanced mode adds what can be done to the playing profile.
    fn sound(&mut self, room: Room, advanced: bool, cx: &mut Context<Self>) -> AnyElement {
        let graph = self.graph(room, cx);
        let theme = *cx.theme();
        let wide = room.fits(Room::Wide);
        let on = self.playback.read(cx).equalizer();
        let only = self.settings.read(cx).equalizer_profiles().count() < 2;
        let (undo, redo) = self.playback.read(cx).equalizer_history();
        let width = theme.metrics.cover * SOUND_WIDTH;

        let picker = sound_picker(SOUND, &self.popovers, &self.playback, &self.settings, cx)
            .large()
            .left()
            .width(width)
            .map(|picker| match wide {
                true => picker.w(width),
                false => picker.flex_1().min_w_0(),
            });
        let action = |id: &'static str, icon: &'static str, tooltip: &'static str| {
            Button::new(id).ghost().icon(icon).tooltip(tooltip)
        };
        let history = div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                action("equalizer-undo", "icons/undo-2.svg", "equalizer-undo")
                    .disabled(!undo)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.playback
                            .update(cx, |playback, cx| playback.undo_equalizer(cx))
                    })),
            )
            .child(
                action("equalizer-redo", "icons/redo-2.svg", "equalizer-redo")
                    .disabled(!redo)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.playback
                            .update(cx, |playback, cx| playback.redo_equalizer(cx))
                    })),
            );
        let manage = advanced.then(|| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .ml_auto()
                .child(
                    action(
                        "equalizer-new",
                        "icons/plus.svg",
                        "equalizer-profile-new-tooltip",
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.new_profile(cx))),
                )
                .child(
                    action(
                        "equalizer-duplicate",
                        "icons/copy.svg",
                        "equalizer-duplicate",
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.duplicate_profile(cx))),
                )
                .child(
                    action("equalizer-rename", "icons/pencil.svg", "equalizer-rename")
                        .on_click(cx.listener(|this, _, window, cx| this.open_rename(window, cx))),
                )
                .child(
                    action("equalizer-delete", "icons/trash-2.svg", "equalizer-delete")
                        .disabled(only)
                        .on_click(cx.listener(|this, _, _, cx| this.delete(cx))),
                )
        });

        InfoCard::new(t!("equalizer-sound"))
            .child(blurb(
                match on {
                    true => t!("equalizer-sound-detail"),
                    false => t!("equalizer-sound-off"),
                },
                None,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x_2()
                    .gap_y_3()
                    .child(picker)
                    .child(history)
                    .children(manage),
            )
            .child(graph)
            .into_any_element()
    }

    /// The preamp's card: whether it follows the filters on its own or holds a level, and that
    /// level.
    fn preamp_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let curve = self.playback.read(cx).equalizer_curve();
        let auto = curve.preamp() == Preamp::Auto;
        let resolved = self.playback.read(cx).equalizer_preamp_db();
        let mode = Picker::new(
            PREAMP,
            &self.popovers,
            match auto {
                true => t!("equalizer-preamp-auto"),
                false => t!("equalizer-preamp-manual"),
            },
        )
        .width(Picker::NARROW)
        .items(
            [
                (true, t!("equalizer-preamp-auto")),
                (false, t!("equalizer-preamp-manual")),
            ]
            .map(|(choice, label)| {
                MenuItem::new(("equalizer-preamp-mode", usize::from(choice)), label)
                    .selected(choice == auto)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_preamp(choice, cx)))
            }),
        );
        let value = match auto {
            true => div()
                .text_size(theme.text(Text::Small))
                .text_color(theme.muted_foreground)
                .child(t!("equalizer-decibels-value", db = figure(resolved, 1)))
                .into_any_element(),
            false => self.number(&self.preamp, t!("equalizer-decibels"), cx),
        };
        let controls = div()
            .flex()
            .items_center()
            .gap_3()
            .child(mode)
            .child(value)
            .into_any_element();

        InfoCard::new(t!("equalizer-preamp"))
            .child(blurb(t!("equalizer-preamp-detail"), Some(controls), cx))
            .into_any_element()
    }

    fn rename_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.renaming.as_ref()?;
        let theme = *cx.theme();
        let modal = Modal::new("equalizer-rename-dialog", t!("equalizer-rename"))
            .w(theme.metrics.cover * DIALOG)
            .child(self.name.clone())
            .action(
                Button::new("equalizer-rename-cancel")
                    .ghost()
                    .label(t!("common-cancel"))
                    .on_click(cx.listener(|this, _, window, cx| this.close_rename(window, cx))),
            )
            .action(
                Button::new("equalizer-rename-save")
                    .primary()
                    .label(t!("common-save"))
                    .on_click(cx.listener(|this, _, window, cx| this.rename(window, cx))),
            )
            .on_dismiss(cx.listener(|this, _, window, cx| this.close_rename(window, cx)));

        Some(
            div()
                .absolute()
                .inset_0()
                .key_context(FORM_CONTEXT)
                .track_focus(&self.focus)
                .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                    cx.stop_propagation();
                    this.close_rename(window, cx);
                }))
                .on_action(cx.listener(|this, _: &Submit, window, cx| {
                    cx.stop_propagation();
                    this.rename(window, cx);
                }))
                .child(modal)
                .into_any_element(),
        )
    }
}

impl Render for EqualizerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inset = cx.theme().metrics.inset;
        let room = Chrome::room(window, cx);
        let wide = room.fits(Room::Wide);
        let advanced = self.settings.read(cx).equalizer_advanced();
        let sound = self.sound(room, advanced, cx);

        let page = Scroller::new("equalizer-page", &self.scrollbar)
            .p(inset)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(self.header(advanced, cx))
                            .children(self.overload(cx)),
                    )
                    .child(sound)
                    .when(advanced, |this| {
                        this.child(
                            div()
                                .key_context(FORM_CONTEXT)
                                .on_action(cx.listener(|this, _: &Submit, window, cx| {
                                    cx.stop_propagation();
                                    this.submit(window, cx);
                                }))
                                .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                                    cx.stop_propagation();
                                    this.revert(window, cx);
                                }))
                                .flex()
                                .flex_col()
                                .gap_6()
                                .child(self.preamp_card(cx))
                                .child(self.filters(wide, cx)),
                        )
                        .children(self.outputs(cx))
                        .child(self.transfer(cx))
                        .child(self.signal_path(cx))
                    }),
            );

        div()
            .size_full()
            .child(page)
            .children(self.rename_dialog(cx))
    }
}

/// The muted line that says what a card is for, with the card's action on its right. The
/// action wraps under the line when the room is too narrow for both.
fn blurb(description: SharedString, action: Option<AnyElement>, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x_4()
        .gap_y_3()
        .child(
            div()
                .flex_1()
                .min_w(theme.metrics.cover * BLURB)
                .text_size(theme.text(Text::Small))
                .text_color(theme.muted_foreground)
                .child(description),
        )
        .when_some(action, |this, action| {
            this.child(div().flex_none().child(action))
        })
}

/// One setting inside a card, laid out as on the settings page: a title over a muted detail on
/// the left and its control on the right.
fn entry(title: SharedString, detail: SharedString, action: impl IntoElement, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_x_4()
        .gap_y_2()
        .py_3()
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(theme.metrics.cover * BLURB)
                .gap_1()
                .child(title)
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(theme.text(Text::Small))
                        .text_color(theme.muted_foreground)
                        .child(detail),
                ),
        )
        .child(div().flex_none().child(action))
}
