use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, Empty, Entity, FocusHandle, Focusable as _, Render,
    ScrollHandle, SharedString, StyleRefinement, Task, Window, div,
};
use i18n::t;
use music::equalizer::{Curve, MAX_FILTERS, PREAMP_RANGE, Preamp};
use state::{
    AppSettings, BoundOutput, Equalizer, Outputs, Signal, SignalPath, Sonora,
    equalizer_profile_label,
};
use ui::{
    ActiveTheme as _, Button, Dismiss, FORM_CONTEXT, InfoCard, Input, MenuItem, Modal, Picker,
    Popovers, Room, Scrollbar, Scroller, Submit, TabBar, Text, heading,
};

use crate::chrome::Chrome;
use crate::shared::confirm::{self, Confirm};
use crate::shared::equalizer::sound_picker;
use crate::shared::section::Section;

use graph::{EqualizerGraph, Selected};
use meter::OutputMeter;
use rows::{Row, figure};

mod graph;
mod meter;
mod outputs;
mod path;
mod rows;
mod transfer;

const SOUND: &str = "equalizer-sound";
const PREAMP: &str = "equalizer-preamp";
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

/// The equalizer's own screen. Simple mode shows the sound picker, undo and the response graph,
/// and advanced mode adds a card each for the preamp, the filters, the per-output profiles,
/// Equalizer APO text in and out, and the signal path.
pub(crate) struct EqualizerView {
    equalizer: Entity<Equalizer>,
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
    graph: Entity<EqualizerGraph>,
    /// The level meter and clip warning under the graph, which redraw on their own.
    meter: Entity<OutputMeter>,
    /// What the screen last drew from the equalizer, the output and the signal path, so a notify
    /// from them that changed none of it draws nothing.
    drawn: Option<Drawn>,
    cards: Cards,
    /// The spectrum's, the output meter's and the signal path's reads, held while the screen is
    /// the one shown and dropped with it.
    watch: Option<Task<()>>,
    /// Whether the signal path is open, which only advanced mode allows.
    path: bool,
    /// The file being read in or written out.
    file: Option<Task<()>>,
}

/// What the screen shows from the equalizer, the output and the signal path. Position ticks and
/// meter reads that leave all of it as it was do not redraw the screen, and the level meter and
/// clip warning redraw on their own.
#[derive(PartialEq)]
struct Drawn {
    curve: Curve,
    on: bool,
    history: (bool, bool),
    preamp: f64,
    output: Option<BoundOutput>,
    port: Option<String>,
    path: Option<SignalPath>,
}

/// The cards advanced mode adds below the graph, each a section of its own so the spectrum
/// redrawing the graph leaves them as they were painted.
struct Cards {
    preamp: Entity<Section>,
    filters: Entity<Section>,
    devices: Entity<Section>,
    transfer: Entity<Section>,
    path: Entity<Section>,
}

impl EqualizerView {
    pub(crate) fn new(
        equalizer: Entity<Equalizer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = Sonora::global(cx).settings.clone();
        let signal = Sonora::global(cx).signal.clone();
        let outputs = Sonora::global(cx).outputs.clone();
        let id = cx.entity_id();
        let preamp = cx.new(|cx| Input::new("", cx).compact());
        let rows = (0..MAX_FILTERS).map(|_| Row::new(cx)).collect();
        let meter = cx.new(|cx| OutputMeter::new(equalizer.clone(), cx));
        let graph = cx.new(|cx| {
            EqualizerGraph::new(
                equalizer.clone(),
                settings.clone(),
                outputs.clone(),
                signal.clone(),
                cx,
            )
        });

        cx.observe(&equalizer, |this, _, cx| {
            this.show_curve(cx);
            this.redraw(cx);
        })
        .detach();
        cx.observe(&settings, |this, settings, cx| {
            if !settings.read(cx).equalizer_advanced() {
                this.path = false;
            }
            cx.notify();
        })
        .detach();
        cx.observe(&signal, |this, _, cx| {
            if this.path {
                this.redraw(cx);
            }
        })
        .detach();
        cx.observe(&outputs, |this, _, cx| this.redraw(cx)).detach();
        cx.subscribe(&graph, |_, _, _: &Selected, cx| cx.notify())
            .detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();
        let screen = cx.entity();
        let cards = Cards {
            preamp: cx.new(|cx| Section::new(&screen, |this, _, cx| this.preamp_card(cx), cx)),
            filters: cx.new(|cx| {
                Section::new(
                    &screen,
                    |this, window, cx| this.filters(Chrome::room(window, cx).fits(Room::Wide), cx),
                    cx,
                )
            }),
            devices: cx.new(|cx| {
                Section::new(
                    &screen,
                    |this, _, cx| this.outputs(cx).unwrap_or_else(|| Empty.into_any_element()),
                    cx,
                )
            }),
            transfer: cx.new(|cx| Section::new(&screen, |this, _, cx| this.transfer(cx), cx)),
            path: cx.new(|cx| Section::new(&screen, |this, _, cx| this.signal_path(cx), cx)),
        };

        let mut view = Self {
            equalizer,
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
            graph,
            meter,
            drawn: None,
            cards,
            watch: None,
            path: false,
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
        view.drawn = Some(view.drawing(cx));
        view
    }

    /// What the screen shows from the equalizer, the output and the open signal path right now.
    fn drawing(&self, cx: &App) -> Drawn {
        let equalizer = self.equalizer.read(cx);
        let outputs = self.outputs.read(cx);
        Drawn {
            curve: equalizer.curve().clone(),
            on: equalizer.enabled(),
            history: equalizer.history(),
            preamp: equalizer.preamp_db(),
            output: outputs.output(),
            port: outputs.sink().and_then(|sink| sink.port.clone()),
            path: self
                .path
                .then(|| self.signal.read(cx).path().cloned())
                .flatten(),
        }
    }

    /// Redraws the screen when anything it shows from the equalizer, the output or the signal
    /// path changed since it last drew.
    fn redraw(&mut self, cx: &mut Context<Self>) {
        let drawn = self.drawing(cx);
        if self.drawn.as_ref() != Some(&drawn) {
            self.drawn = Some(drawn);
            cx.notify();
        }
    }

    /// Reads the output meter, the signal path and the spectrum while the screen is the one
    /// shown.
    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if visible != self.watch.is_some() {
            self.watch = visible.then(|| self.signal.update(cx, |signal, cx| signal.watch(cx)));
        }
        if !visible {
            self.path = false;
        }
    }

    /// Plays `curve`, an edit made on this screen.
    fn apply(&mut self, curve: Curve, cx: &mut Context<Self>) {
        self.graph.update(cx, |graph, cx| graph.apply(curve, cx));
    }

    fn set_preamp(&mut self, auto: bool, cx: &mut Context<Self>) {
        let Some(curve) = self.shown.clone() else {
            return;
        };
        let preamp = self.equalizer.read(cx).preamp_db();
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
        self.equalizer
            .update(cx, |equalizer, cx| equalizer.add_profile(&name, curve, cx));
    }

    fn duplicate_profile(&mut self, cx: &mut Context<Self>) {
        let current = equalizer_profile_label(self.settings.read(cx).equalizer_profile_name());
        let name = t!("equalizer-profile-copy", name = &current);
        let curve = self.equalizer.read(cx).curve().clone();
        self.equalizer
            .update(cx, |equalizer, cx| equalizer.add_profile(&name, curve, cx));
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
        let equalizer = self.equalizer.clone();
        Confirm::ask(
            confirm::Kind::EqualizerProfile(name.clone().into()),
            move |cx| {
                equalizer.update(cx, |equalizer, cx| {
                    equalizer.remove_profile(index, &name, cx)
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

    /// The main card: the one picker for every preset and profile with undo and redo beside it,
    /// then the response graph with the output meter under it. Advanced mode adds what can be
    /// done to the playing profile.
    fn sound(&mut self, room: Room, advanced: bool, cx: &mut Context<Self>) -> AnyElement {
        let graph = self.graph.clone().cached(
            StyleRefinement::default()
                .w_full()
                .h(graph::height(room, cx)),
        );
        let theme = *cx.theme();
        let meter = self.meter.clone().cached(
            StyleRefinement::default()
                .w_full()
                .h(theme.metrics.control_small),
        );
        let wide = room.fits(Room::Wide);
        let on = self.equalizer.read(cx).enabled();
        let only = self.settings.read(cx).equalizer_profiles().count() < 2;
        let (undo, redo) = self.equalizer.read(cx).history();
        let width = theme.metrics.cover * SOUND_WIDTH;

        let picker = sound_picker(SOUND, &self.popovers, &self.equalizer, &self.settings, cx)
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
                        this.equalizer
                            .update(cx, |equalizer, cx| equalizer.undo(cx))
                    })),
            )
            .child(
                action("equalizer-redo", "icons/redo-2.svg", "equalizer-redo")
                    .disabled(!redo)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.equalizer
                            .update(cx, |equalizer, cx| equalizer.redo(cx))
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
            .child(meter)
            .into_any_element()
    }

    /// The preamp's card: whether it follows the filters on its own or holds a level, and that
    /// level.
    fn preamp_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let curve = self.equalizer.read(cx).curve();
        let auto = curve.preamp() == Preamp::Auto;
        let resolved = self.equalizer.read(cx).preamp_db();
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
        let advanced = self.settings.read(cx).equalizer_advanced();
        let sound = self.sound(room, advanced, cx);
        let routed = self.outputs.read(cx).output().is_some();

        let page = Scroller::new("equalizer-page", &self.scrollbar)
            .p(inset)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(self.header(advanced, cx))
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
                                .child(Section::place(&self.cards.preamp, cx))
                                .child(Section::place(&self.cards.filters, cx)),
                        )
                        .when(routed, |this| {
                            this.child(Section::place(&self.cards.devices, cx))
                        })
                        .child(Section::place(&self.cards.transfer, cx))
                        .child(Section::place(&self.cards.path, cx))
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
