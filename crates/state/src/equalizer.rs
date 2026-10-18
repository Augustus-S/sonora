//! The equalizer the app plays through: the curve every engine reads, the profiles the settings
//! store it in, undo and redo, the profile each output asks for, and the clip warning the output
//! meter raises against it.

use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{Context, Entity, Subscription, Task};
use music::Meter;
use music::equalizer::{Curve, Edit, EqualizerWriter, Preamp, Preset, apo};

use crate::overload::{OutputLevel, Overload};
use crate::{AppSettings, BoundOutput, Outputs, Playback, Reloaded};

/// How long after the last glided edit the curve is stored, so a drag or a run of wheel steps
/// reaches the settings once, when it pauses.
const GLIDE_STORE: Duration = Duration::from_millis(300);
/// How many equalizer edits can be undone.
const UNDO_STEPS: usize = 50;

/// The equalizer as it plays. Created once in `Sonora`, it hands every engine the same reader
/// and meter, so a change reaches the output without a restart. It follows the output the sound
/// plays on and switches to the profile bound to it.
pub struct Equalizer {
    settings: Entity<AppSettings>,
    writer: EqualizerWriter,
    /// Shared with every engine like the equalizer's reader, and read only while watched.
    meter: Meter,
    overload: Overload,
    /// The output's level as the meter last read it, notified on its own.
    output_level: Entity<OutputLevel>,
    /// Stores a glided curve in the settings once the edits pause.
    glide_store: Option<Task<()>>,
    history: History,
    /// The id of the output played on as `Outputs` last settled it, so a binding edit can play
    /// what that output asks for and the same output again is not a move.
    output: Option<String>,
    /// The volume as playback last had it, so a change to it takes the clip warning down.
    volume: f32,
    _subscriptions: Vec<Subscription>,
}

/// The playing profile's curves from before each edit and from before each undo, newest last.
/// It is kept in memory only and emptied when another profile plays.
#[derive(Default)]
struct History {
    undo: Vec<Curve>,
    redo: Vec<Curve>,
}

impl History {
    /// Remembers `curve`, the one an edit is about to replace, and forgets what was undone.
    fn record(&mut self, curve: &Curve) {
        if self.undo.len() == UNDO_STEPS {
            self.undo.remove(0);
        }
        self.undo.push(curve.clone());
        self.redo.clear();
    }

    /// Steps back from `current` and returns the curve to play instead.
    fn undo(&mut self, current: &Curve) -> Option<Curve> {
        let curve = self.undo.pop()?;
        self.redo.push(current.clone());
        Some(curve)
    }

    /// Steps forward from `current` again and returns the curve to play instead.
    fn redo(&mut self, current: &Curve) -> Option<Curve> {
        let curve = self.redo.pop()?;
        self.undo.push(current.clone());
        Some(curve)
    }
}

impl Equalizer {
    /// Plays what the settings hold. It follows no output until `follow` is called, since
    /// `Outputs` is built after the playback that reads this equalizer.
    pub fn new(settings: Entity<AppSettings>, cx: &mut Context<Self>) -> Self {
        let reloaded = cx.subscribe(&settings, |this, _, _: &Reloaded, cx| {
            // The file's curve wins over a glide that was still to be stored.
            this.glide_store = None;
            this.follow_settings(cx)
        });
        let writer = EqualizerWriter::new(
            settings.read(cx).equalizer(),
            settings.read(cx).equalizer_curve(),
        );
        Self {
            settings,
            writer,
            meter: Meter::default(),
            overload: Overload::default(),
            output_level: cx.new(|_| OutputLevel::default()),
            glide_store: None,
            history: History::default(),
            output: None,
            volume: 0.,
            _subscriptions: vec![reloaded],
        }
    }

    /// Starts following the output `outputs` settles on, and the volume of `playback`, which
    /// takes the clip warning down when it changes.
    pub(crate) fn follow(
        &mut self,
        playback: &Entity<Playback>,
        outputs: &Entity<Outputs>,
        cx: &mut Context<Self>,
    ) {
        self.volume = playback.read(cx).volume();
        self._subscriptions
            .push(cx.observe(playback, |this, playback, cx| {
                let volume = playback.read(cx).volume();
                if volume != this.volume {
                    this.volume = volume;
                    this.overload.settle(Instant::now());
                }
            }));
        self._subscriptions
            .push(cx.observe(outputs, |this, outputs, cx| {
                let settled = outputs
                    .read(cx)
                    .settled_output()
                    .map(|output| output.id.clone());
                if let Some(id) = settled {
                    this.follow_output(id, cx);
                }
            }));
    }

    /// The reader every engine plays the curve through.
    pub(crate) fn reader(&self) -> music::Equalizer {
        self.writer.reader()
    }

    /// The meter every engine's output reports its loudest sample to.
    pub(crate) fn meter(&self) -> Meter {
        self.meter.clone()
    }

    pub fn enabled(&self) -> bool {
        self.writer.enabled()
    }

    pub fn curve(&self) -> &Curve {
        self.writer.curve()
    }

    /// Turns the equalizer on or off. The engines pick the change up on their next frame and
    /// fade to it, so no restart and no click.
    pub fn set_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        self.writer.set_enabled(on);
        self.overload.settle(Instant::now());
        self.settings
            .update(cx, |settings, cx| settings.set_equalizer(on, cx));
        cx.notify();
    }

    /// Puts a whole curve in place of the playing profile's, the way a preset pick or a typed
    /// value does: the engines cross-fade to it.
    pub fn set_curve(&mut self, curve: Curve, cx: &mut Context<Self>) {
        if curve == *self.writer.curve() {
            return;
        }
        self.history.record(self.writer.curve());
        self.replace_curve(curve, cx);
    }

    /// Whether there is an edit to undo, and whether there is an undo to redo.
    pub fn history(&self) -> (bool, bool) {
        (!self.history.undo.is_empty(), !self.history.redo.is_empty())
    }

    /// Plays the curve from before the last edit, cross-fading to it.
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        if let Some(curve) = self.history.undo(self.writer.curve()) {
            self.replace_curve(curve, cx);
        }
    }

    /// Plays the curve the last undo took away again.
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        if let Some(curve) = self.history.redo(self.writer.curve()) {
            self.replace_curve(curve, cx);
        }
    }

    /// Puts a curve in place that moves one filter a little from the playing one, the way a
    /// drag does: the engines glide it there. The settings store it once the edits pause.
    pub fn glide_curve(&mut self, curve: Curve, cx: &mut Context<Self>) {
        if curve == *self.writer.curve() {
            return;
        }
        // A glide waiting to be stored belongs to the gesture that started it.
        if self.glide_store.is_none() {
            self.history.record(self.writer.curve());
        }
        self.writer.set_curve(curve, Edit::Glide);
        self.overload.settle(Instant::now());
        self.glide_store = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(GLIDE_STORE).await;
            this.update(cx, |this, cx| this.store_glided(cx)).ok();
        }));
        cx.notify();
    }

    /// The preamp the playing curve resolves to, in decibels.
    pub fn preamp_db(&self) -> f64 {
        self.writer.preamp_db()
    }

    /// The spectrum of the music on its way into the equalizer, on every output.
    pub fn heard(&self) -> music::Analyzer {
        self.writer.heard().clone()
    }

    /// How many decibels over full scale the clip warning says the output went, to a tenth,
    /// while something watches. It changes at most about once a second, and the equalizer
    /// notifies only when it does.
    pub fn overload(&self) -> Option<f64> {
        self.overload.db()
    }

    /// The preamp "Lower preamp" would set, or `None` without an overload or when the preamp
    /// is as low as it goes.
    pub fn lowered_preamp(&self) -> Option<f64> {
        self.overload.lowered(self.writer.preamp_db())
    }

    /// The output's recent level. It notifies on its own about ten times a second while the
    /// meter is watched, so a level meter observes it rather than the equalizer.
    pub fn output_level(&self) -> Entity<OutputLevel> {
        self.output_level.clone()
    }

    /// Starts the level meter and the clip warning afresh, from no overload, for a screen that
    /// starts watching. `Signal::watch` then reads the meter on its own tick.
    pub(crate) fn reset_meter(&mut self, cx: &mut Context<Self>) {
        self.overload = Overload::default();
        self.meter.take();
        self.output_level.update(cx, |level, cx| level.reset(cx));
        cx.notify();
    }

    /// Reads the meter once, the loudest sample since the read before, feeding the level meter
    /// and the clip warning. It notifies only when the warning changed.
    pub(crate) fn read_meter(&mut self, cx: &mut Context<Self>) {
        let (peak, now) = (self.meter.take(), Instant::now());
        self.output_level
            .update(cx, |level, cx| level.observe(peak, now, cx));
        if self.overload.observe(peak, now) {
            cx.notify();
        }
    }

    /// Lowers the preamp by the overload and a margin, made manual, so what overloaded fits.
    pub fn lower_preamp(&mut self, cx: &mut Context<Self>) {
        let Some(preamp) = self.lowered_preamp() else {
            return;
        };
        let filters = self.writer.curve().filters().to_vec();
        if let Ok(curve) = Curve::new(Preamp::Manual(preamp), filters) {
            self.set_curve(curve, cx);
        }
    }

    /// Binds `output`, the one played on now, to profile `index`, or unbinds it with `None`,
    /// and plays the profile the output then asks for.
    pub fn bind_output(
        &mut self,
        output: &BoundOutput,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.settings.update(cx, |settings, cx| {
            settings.bind_equalizer_output(output.clone(), index, cx)
        });
        self.play_output_profile(cx);
    }

    /// Makes profile `index` the one for outputs no profile is bound to, or none with `None`,
    /// and plays it when the output played on is one of those.
    pub fn set_other_outputs(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.settings.update(cx, |settings, cx| {
            settings.set_equalizer_other_outputs(index, cx)
        });
        let settings = self.settings.read(cx);
        let unbound = self
            .output
            .as_deref()
            .is_some_and(|id| settings.equalizer_profile_bound_to(id).is_none());
        if unbound {
            self.play_output_profile(cx);
        }
    }

    /// Plays profile `index`, cross-fading to its curve.
    pub fn select_profile(&mut self, index: usize, cx: &mut Context<Self>) {
        self.store_glided(cx);
        self.settings.update(cx, |settings, cx| {
            settings.select_equalizer_profile(index, cx)
        });
        self.follow_settings(cx);
    }

    /// Plays profile `index` and turns the equalizer on, the way picking a sound does.
    pub fn play_profile(&mut self, index: usize, cx: &mut Context<Self>) {
        self.select_profile(index, cx);
        if !self.enabled() {
            self.set_enabled(true, cx);
        }
    }

    /// Plays `preset` in the profile with no name, switching to it first when another plays, and
    /// turns the equalizer on. A named profile is never overwritten.
    pub fn play_preset(&mut self, preset: Preset, cx: &mut Context<Self>) {
        let curve = preset.curve();
        let settings = self.settings.read(cx);
        if settings.equalizer_scratch() != Some(settings.equalizer_profile()) {
            self.store_glided(cx);
            self.settings.update(cx, |settings, cx| {
                settings.select_equalizer_scratch(&curve, cx)
            });
            self.follow_settings(cx);
        }
        self.set_curve(curve, cx);
        if !self.enabled() {
            self.set_enabled(true, cx);
        }
    }

    /// Adds a profile named after `name` holding `curve` and plays it. `AppSettings` makes the
    /// name unique.
    pub fn add_profile(&mut self, name: &str, curve: Curve, cx: &mut Context<Self>) {
        self.store_glided(cx);
        self.settings.update(cx, |settings, cx| {
            settings.add_equalizer_profile(name, &curve, cx)
        });
        self.follow_settings(cx);
    }

    /// Removes profile `index` while it is still the one named `name`, unless it is the only
    /// one. When it was playing, the output's own profile plays, else the one `AppSettings` picks.
    pub fn remove_profile(&mut self, index: usize, name: &str, cx: &mut Context<Self>) {
        self.store_glided(cx);
        let playing = self.settings.read(cx).equalizer_profile() == index;
        self.settings.update(cx, |settings, cx| {
            settings.remove_equalizer_profile(index, name, cx)
        });
        self.follow_settings(cx);
        if playing {
            self.play_output_profile(cx);
        }
    }

    /// Reads an Equalizer APO or AutoEQ text into a new profile named after `name` and plays
    /// it. Returns the lines it left out, or why nothing was imported.
    pub fn import(
        &mut self,
        name: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Result<Vec<apo::Skipped>, apo::Refused> {
        let parsed = apo::parse(text)?;
        self.add_profile(name, parsed.curve, cx);
        Ok(parsed.skipped)
    }

    /// The playing curve as an Equalizer APO text.
    pub fn export(&self) -> String {
        apo::export(self.writer.curve())
    }

    fn replace_curve(&mut self, curve: Curve, cx: &mut Context<Self>) {
        self.writer.set_curve(curve, Edit::Replace);
        self.overload.settle(Instant::now());
        self.save_curve(cx);
    }

    /// Stores a glided curve that waits for the edits to pause now, so a profile change that
    /// follows keeps it in the profile it was made in.
    fn store_glided(&mut self, cx: &mut Context<Self>) {
        if self.glide_store.take().is_some() {
            self.save_curve(cx);
        }
    }

    /// Takes in output `id` as the one played on, and plays the profile it asks for when it is
    /// another output than the last one.
    fn follow_output(&mut self, id: String, cx: &mut Context<Self>) {
        if self.output.as_ref() == Some(&id) {
            return;
        }
        self.output = Some(id);
        self.play_output_profile(cx);
    }

    /// Plays the profile the output played on asks for, when one is bound to it or set for
    /// other outputs and it is not playing already.
    fn play_output_profile(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings.read(cx);
        let Some(index) = self
            .output
            .as_deref()
            .and_then(|id| settings.equalizer_profile_for_output(id))
        else {
            return;
        };
        if index != settings.equalizer_profile() {
            log::info!("equalizer: playing profile {index}, the one for the output played on");
            self.select_profile(index, cx);
        }
    }

    /// Plays what the settings say, the equalizer on or off and the selected profile's curve,
    /// as one replace, so the sound, the picker and the screen agree after a profile change.
    fn follow_settings(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings.read(cx);
        let (on, curve) = (settings.equalizer(), settings.equalizer_curve());
        self.history = History::default();
        self.overload.settle(Instant::now());
        // Whichever change is heard goes last, so only one replace reaches the ear.
        match on {
            true => {
                self.writer.set_curve(curve, Edit::Replace);
                self.writer.set_enabled(true);
            }
            false => {
                self.writer.set_enabled(false);
                self.writer.set_curve(curve, Edit::Replace);
            }
        }
        cx.notify();
    }

    fn save_curve(&mut self, cx: &mut Context<Self>) {
        self.glide_store = None;
        let curve = self.writer.curve().clone();
        self.settings
            .update(cx, |settings, cx| settings.set_equalizer_curve(&curve, cx));
        cx.notify();
    }
}
