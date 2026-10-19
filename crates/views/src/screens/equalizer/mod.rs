use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, Entity, FocusHandle, Focusable as _, Render, ScrollHandle,
    SharedString, StyleRefinement, Task, Window, div,
};
use i18n::t;
use music::equalizer::{Curve, MAX_FILTERS, PREAMP_RANGE, Preamp, Preset};
use music::output::{DeviceOption, OutputId, Port};
use router::Destination;
use state::{
    AppSettings, Equalizer, Outputs, RuleDevice, Signal, Sonora, Sound, equalizer_profile_label,
};
use ui::{
    ActiveTheme as _, Button, Dismiss, FORM_CONTEXT, InfoCard, Input, Lazy, MenuItem, Modal,
    Picker, Popovers, Room, Scrollbar, Submit, Switch, TabBar, Text,
};

use crate::chrome::Chrome;
use crate::screens::settings;
use crate::shared::confirm::{self, Confirm};
use crate::shared::equalizer::{preset_key, sound_picker};

use graph::EqualizerGraph;
use meter::OutputMeter;
use rows::{Row, figure, measured};

mod graph;
mod meter;
mod rows;
mod rules;
mod transfer;

const SOUND: &str = "equalizer-sound";
const PREAMP: &str = "equalizer-preamp";
/// How wide a number field is, in cover sizes.
const FIELD: f32 = 0.6;
/// How wide the name dialog is, in cover sizes.
const DIALOG: f32 = 2.8;
/// How wide the sound picker and its menu are in a wide room, in cover sizes. A narrower room
/// lets the picker fill its row instead.
const SOUND_WIDTH: f32 = 2.2;
/// How narrow a card's description gets before the action beside it wraps underneath, in
/// cover sizes.
const BLURB: f32 = 1.6;

/// The equalizer's own screen. Simple mode shows the sound picker, undo and the response graph,
/// and advanced mode adds a card each for the preamp, the filters, the device rules and
/// Equalizer APO text in and out.
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
    /// What the name dialog is open for, while it is.
    naming: Option<Naming>,
    /// The device and the sound chosen in the row that adds a device rule, until it is added.
    rule_device: Option<RuleDevice>,
    rule_sound: Option<Sound>,
    /// The name dialog's own focus, and what had focus before it opened, given back on close.
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    /// The curve the fields were last filled from, so they are only rewritten when it changes.
    shown: Option<Curve>,
    graph: Entity<EqualizerGraph>,
    /// The level meter and clip warning under the graph, which redraw on their own.
    meter: Entity<OutputMeter>,
    /// What the screen last drew from the equalizer and the output, so a notify
    /// from them that changed none of it draws nothing.
    drawn: Option<Drawn>,
    /// The spectrum's, the output meter's and the signal path's reads, held while the screen is
    /// the one shown and dropped with it.
    watch: Option<Task<()>>,
    /// The file being read in or written out.
    file: Option<Task<()>>,
}

/// What the name dialog is open for.
#[derive(Clone)]
enum Naming {
    /// Saving the playing sound as a new profile.
    Save,
    /// Renaming the profile at this index, by its stored name then, so a profile list that
    /// changed meanwhile renames nothing.
    Rename(usize, String),
}

/// What the screen shows from the equalizer and the output. Position ticks and
/// meter reads that leave all of it as it was do not redraw the screen, and the level meter and
/// clip warning redraw on their own.
#[derive(PartialEq)]
struct Drawn {
    curve: Curve,
    on: bool,
    history: (bool, bool),
    preamp: f64,
    output: Option<OutputId>,
    port: Option<Port>,
    /// What the device rules' menu offers.
    devices: Vec<DeviceOption>,
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
        let meter = cx.new(|cx| OutputMeter::new(equalizer.clone(), signal.clone(), cx));
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
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        cx.observe(&outputs, |this, _, cx| this.redraw(cx)).detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

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
            naming: None,
            rule_device: None,
            rule_sound: None,
            focus: cx.focus_handle(),
            restore: None,
            shown: None,
            graph,
            meter,
            drawn: None,
            watch: None,
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

    /// What the screen shows from the equalizer and the output right now.
    fn drawing(&self, cx: &App) -> Drawn {
        let equalizer = self.equalizer.read(cx);
        let outputs = self.outputs.read(cx);
        Drawn {
            curve: equalizer.curve().clone(),
            on: equalizer.enabled(),
            history: equalizer.history(),
            preamp: equalizer.preamp_db(),
            output: outputs.binding_key(),
            port: outputs
                .playing_sink()
                .and_then(|sink| sink.shown_port().cloned()),
            devices: outputs.devices().to_vec(),
        }
    }

    /// Redraws the screen when anything it shows from the equalizer or the output changed since
    /// it last drew.
    fn redraw(&mut self, cx: &mut Context<Self>) {
        let drawn = self.drawing(cx);
        if self.drawn.as_ref() != Some(&drawn) {
            self.drawn = Some(drawn);
            cx.notify();
        }
    }

    /// Reads the output meter, the signal path and the spectrum while the screen is the one
    /// shown, and the outputs the device rules offer once it comes into view.
    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if visible != self.watch.is_some() {
            self.watch = visible.then(|| self.signal.update(cx, |signal, cx| signal.watch(cx)));
            if visible {
                self.outputs.update(cx, |outputs, cx| outputs.refresh(cx));
            }
        }
    }

    /// What stands in the preview column beside the page, or `None` for no column.
    pub(crate) fn preview(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        None
    }

    /// Closes an open menu on escape. Returns false when none was open, so the escape can close
    /// settings instead.
    pub(crate) fn escape(&mut self) -> bool {
        let open = self.popovers.any();
        self.popovers.close();
        open
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

    /// A name for the playing sound: the preset it matches while it is the unnamed profile, else
    /// the first numbered profile name no profile shows yet.
    fn suggested_name(&self, cx: &App) -> SharedString {
        let settings = self.settings.read(cx);
        let preset = settings
            .equalizer_scratch()
            .filter(|scratch| *scratch == settings.equalizer_profile())
            .and_then(|_| Preset::matching(self.equalizer.read(cx).curve()));
        if let Some(preset) = preset {
            return i18n::lookup(preset_key(preset), None);
        }
        let taken: Vec<SharedString> = settings
            .equalizer_profiles()
            .map(equalizer_profile_label)
            .collect();
        (1..)
            .map(|number| t!("equalizer-profile-new", number = number))
            .find(|name| !taken.contains(name))
            .unwrap_or_default()
    }

    /// Opens the name dialog for `naming`, filled with `text` and focused.
    fn open_naming(
        &mut self,
        naming: Naming,
        text: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restore = window.focused(cx);
        self.name.update(cx, |input, cx| {
            input.set_text(text, cx);
            input.focus(window, cx);
        });
        self.naming = Some(naming);
        cx.notify();
    }

    /// Opens the name dialog to save the playing sound as a new profile.
    fn open_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.suggested_name(cx);
        self.open_naming(Naming::Save, name, window, cx);
    }

    /// Opens the name dialog on the playing profile, filled with the name it shows.
    fn open_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.settings.read(cx);
        let (index, stored) = (
            settings.equalizer_profile(),
            settings.equalizer_profile_name().to_owned(),
        );
        let shown = equalizer_profile_label(&stored);
        self.open_naming(Naming::Rename(index, stored), shown, window, cx);
    }

    fn close_naming(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.naming = None;
        if let Some(focus) = self.restore.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Saves or renames with the name the dialog holds. An empty name is not taken and the
    /// dialog stays open.
    fn submit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(naming) = self.naming.clone() else {
            return;
        };
        let name = self.name.read(cx).text().to_owned();
        let done = match naming {
            Naming::Rename(index, from) => self.settings.update(cx, |settings, cx| {
                settings.rename_equalizer_profile(index, &from, &name, cx)
            }),
            Naming::Save => {
                let named = !name.trim().is_empty();
                if named {
                    self.equalizer
                        .update(cx, |equalizer, cx| equalizer.save_as(&name, cx));
                }
                named
            }
        };
        if done {
            self.close_naming(window, cx);
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

    /// The Simple and Advanced switch the page opens with. The trail in the header names the page,
    /// so it carries no title of its own. The choice is remembered across launches.
    fn header(&self, advanced: bool, cx: &mut Context<Self>) -> AnyElement {
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

        TabBar::new("equalizer-mode")
            .flex_none()
            .self_start()
            .items(modes)
            .into_any_element()
    }

    /// The main card: the one picker for every preset and profile with the on switch, undo and
    /// redo beside it, then the response graph with the output meter under it. Advanced mode adds
    /// what can be done to the playing profile.
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
        let on = self.equalizer.read(cx).enabled();
        let settings = self.settings.read(cx);
        let only = settings.equalizer_profiles().count() < 2;
        // the unnamed profile is where presets load, so it keeps no name and is never removed
        let built_in = settings.equalizer_scratch() == Some(settings.equalizer_profile());
        let (undo, redo) = self.equalizer.read(cx).history();
        let width = theme.metrics.cover * SOUND_WIDTH;

        let picker = sound_picker(SOUND, &self.popovers, &self.equalizer, &self.settings, cx)
            .large()
            .left()
            .width(width)
            .map(|picker| match room {
                Room::Wide | Room::Vast => picker.w(width),
                Room::Roomy => picker.flex_1().min_w_0(),
                Room::Tight | Room::Snug => match advanced {
                    true => picker.w_full(),
                    false => picker.flex_1().min_w_0(),
                },
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
                    action("equalizer-save-as", "icons/plus.svg", "equalizer-save-as")
                        .on_click(cx.listener(|this, _, window, cx| this.open_save(window, cx))),
                )
                .child(
                    action("equalizer-rename", "icons/pencil.svg", "equalizer-rename")
                        .disabled(built_in)
                        .on_click(cx.listener(|this, _, window, cx| this.open_rename(window, cx))),
                )
                .child(
                    action("equalizer-delete", "icons/trash-2.svg", "equalizer-delete")
                        .disabled(only || built_in)
                        .on_click(cx.listener(|this, _, _, cx| this.delete(cx))),
                )
        });

        section(t!("equalizer-sound"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x_2()
                    .gap_y_3()
                    .child(picker)
                    .child(Switch::new("equalizer-enabled", on).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.equalizer
                                .update(cx, |equalizer, cx| equalizer.set_enabled(!on, cx))
                        },
                    )))
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
                .flex_none()
                .whitespace_nowrap()
                .text_size(theme.text(Text::Small))
                .text_color(theme.muted_foreground)
                .child(t!("equalizer-decibels-value", db = figure(resolved, 1)))
                .into_any_element(),
            false => measured(self.preamp.clone(), t!("equalizer-decibels"), cx),
        };
        let controls = div()
            .flex()
            .items_center()
            .gap_3()
            .child(mode)
            .child(value)
            .into_any_element();

        section(t!("equalizer-preamp"))
            .child(blurb(t!("equalizer-preamp-detail"), Some(controls), cx))
            .into_any_element()
    }

    /// The dialog that names a profile, titled for saving or renaming.
    fn name_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let title = match self.naming.as_ref()? {
            Naming::Save => t!("equalizer-save-as"),
            Naming::Rename(..) => t!("equalizer-rename"),
        };
        let theme = *cx.theme();
        let modal = Modal::new("equalizer-name-dialog", title)
            .w(theme.metrics.cover * DIALOG)
            .child(self.name.clone())
            .action(
                Button::new("equalizer-name-cancel")
                    .ghost()
                    .label(t!("common-cancel"))
                    .on_click(cx.listener(|this, _, window, cx| this.close_naming(window, cx))),
            )
            .action(
                Button::new("equalizer-name-save")
                    .primary()
                    .label(t!("common-save"))
                    .on_click(cx.listener(|this, _, window, cx| this.submit_name(window, cx))),
            )
            .on_dismiss(cx.listener(|this, _, window, cx| this.close_naming(window, cx)));

        Some(
            div()
                .absolute()
                .inset_0()
                .key_context(FORM_CONTEXT)
                .track_focus(&self.focus)
                .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                    cx.stop_propagation();
                    this.close_naming(window, cx);
                }))
                .on_action(cx.listener(|this, _: &Submit, window, cx| {
                    cx.stop_propagation();
                    this.submit_name(window, cx);
                }))
                .child(modal)
                .into_any_element(),
        )
    }
}

impl Render for EqualizerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let room = Chrome::room(window, cx);
        let advanced = self.settings.read(cx).equalizer_advanced();
        let sound = self.sound(room, advanced, cx);

        let view = cx.entity();
        let preview = self.preview(window, cx);
        let body = div()
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
                        .child(card(&view, "equalizer-preamp", |this, _, cx| {
                            this.preamp_card(cx)
                        }))
                        .child(card(&view, "equalizer-filters", |this, window, cx| {
                            this.filters(Chrome::room(window, cx).fits(Room::Wide), cx)
                        })),
                )
                .child(card(&view, "equalizer-rules", |this, _, cx| this.rules(cx)))
                .child(card(&view, "equalizer-transfer", |this, _, cx| {
                    this.transfer(cx)
                }))
            });

        settings::frame(
            &Destination::Equalizer,
            "equalizer-page",
            &self.scrollbar,
            body,
            preview,
            window,
            cx,
        )
        .children(self.name_dialog(cx))
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

/// An advanced card that is built and laid out only while it is on screen, so scrolling the
/// long page costs what shows rather than every card on it.
fn card(
    view: &Entity<EqualizerView>,
    id: &'static str,
    build: impl FnOnce(&mut EqualizerView, &mut Window, &mut Context<EqualizerView>) -> AnyElement
    + 'static,
) -> Lazy {
    let view = view.clone();
    Lazy::new(id, move |window, cx| {
        view.update(cx, |this, cx| build(this, window, cx))
    })
}

/// A part of the screen under its title, laid out like a card without the card around it.
fn section(title: SharedString) -> InfoCard {
    InfoCard::new(title).plain()
}
