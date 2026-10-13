use std::time::Duration;

use gpui::{Context, Entity, Task};
use music::output::{self, Device, Failure, Outcome, Status};

use crate::{AppSettings, Playback};

/// How long a newly picked device holds the picker before it is let go, whether or not it has
/// answered. CoreAudio takes about ten seconds to give up on a device that will not start.
const SWITCH_TIMEOUT: Duration = Duration::from_secs(10);

/// The output devices the system offers, the one chosen, and what the stream Sonora plays into
/// runs at. The list is read off the main thread, since that opens devices on some platforms, and
/// both are read again whenever a stream opens or closes. A device picked here is saved only once
/// it has opened, so one that will not open is never what the next run starts on.
pub struct Outputs {
    settings: Entity<AppSettings>,
    playback: Entity<Playback>,
    devices: Vec<Device>,
    /// Whether `devices` has been read at least once, so an empty list means none.
    listed: bool,
    status: Option<Status>,
    failure: Option<Failure>,
    /// The device playing, or being tried, by cpal id, or `None` to follow the system's default.
    selected: Option<String>,
    /// A device picked and not yet answered for, with the round `output::outcome` answers.
    pending: Option<(Option<String>, u64)>,
    /// Runs out `SWITCH_TIMEOUT` after a pick. While it is set the picker is held.
    holding: Option<Task<()>>,
    listing: Option<Task<()>>,
    _watch: Task<()>,
}

impl Outputs {
    /// Hands the device saved in settings to the engines, so call it before the first one
    /// starts.
    pub fn new(
        settings: Entity<AppSettings>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let selected = settings.read(cx).output_device().map(str::to_owned);
        output::restore(selected.clone());

        let mut changes = output::changes();
        let watch = cx.spawn(async move |this, cx| {
            while changes.changed().await.is_ok() {
                let updated = this.update(cx, |this, cx| {
                    this.status = output::status();
                    this.failure = output::failure();
                    this.settle(cx);
                    this.forget_broken(cx);
                    this.refresh(cx);
                    cx.notify();
                });
                if updated.is_err() {
                    return;
                }
            }
        });

        let mut outputs = Self {
            settings,
            playback,
            devices: Vec::new(),
            listed: false,
            status: output::status(),
            failure: output::failure(),
            selected,
            pending: None,
            holding: None,
            listing: None,
            _watch: watch,
        };
        outputs.refresh(cx);
        outputs
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    /// Whether the system's devices have been read yet.
    pub fn listed(&self) -> bool {
        self.listed
    }

    /// The stream Sonora plays into, or `None` while none is open.
    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref()
    }

    /// Why nothing plays, when the device to play on would not open and nothing took its place.
    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref().filter(|_| self.status.is_none())
    }

    /// The device playing or being tried, or `None` when Sonora follows the system's default.
    /// It may not be connected, in which case Sonora plays on the default until it is.
    pub fn chosen(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// Whether a device was just picked and has not answered yet, so no other can be picked.
    pub fn switching(&self) -> bool {
        self.holding.is_some()
    }

    /// Moves playback onto `id`, or the system's default when `None`, now, paused or not, and
    /// saves it once it opens. A device that would not open leaves nothing playing, `failure`
    /// saying why, and the saved device as it was. Once saved, a device that later will not open
    /// gives way to the default and is forgotten.
    pub fn choose(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if self.switching() {
            return;
        }
        // the same device again, with nothing wrong, would only cut the sound for a moment
        if id == self.selected && self.failure.is_none() {
            return;
        }
        self.selected = id.clone();
        let round = output::choose(id.clone());
        let restarted = self
            .playback
            .update(cx, |playback, cx| playback.restart_outputs(cx));
        // with no engine running nothing opens to answer, so the pick is kept as it is
        if !restarted {
            self.save(id, cx);
            cx.notify();
            return;
        }
        self.pending = Some((id, round));
        self.holding = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SWITCH_TIMEOUT).await;
            this.update(cx, |this, cx| {
                this.holding = None;
                cx.notify();
            })
            .ok();
        }));
        self.settle(cx);
        cx.notify();
    }

    /// Saves the pending device once it has opened, or drops it once it would not, letting the
    /// picker go either way. A device that answers after the timeout is still saved.
    fn settle(&mut self, cx: &mut Context<Self>) {
        let Some((id, round)) = &self.pending else {
            return;
        };
        let Some(outcome) = output::outcome(*round) else {
            return;
        };
        let (id, round) = (id.clone(), *round);
        self.pending = None;
        self.holding = None;
        if let Outcome::Opened(_) = outcome {
            output::keep(round);
            self.save(id, cx);
        }
    }

    /// Forgets the saved device once it is there but would not open, so the next run does not
    /// wait on it again. The system's default already plays in its place. A device that is gone
    /// is never tried, so it never fails and stays saved.
    fn forget_broken(&mut self, cx: &mut Context<Self>) {
        let Some(failure) = self.failure.as_ref().filter(|failure| failure.saved) else {
            return;
        };
        if self.settings.read(cx).output_device() != Some(failure.id.as_str()) {
            return;
        }
        log::warn!(
            "playback: forgetting the saved output device, which would not open: {}",
            failure.error
        );
        self.selected = None;
        self.save(None, cx);
        output::restore(None);
    }

    fn save(&self, id: Option<String>, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.set_output_device(id, cx));
    }

    /// Reads the system's devices again, as when a settings page comes into view.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let listed = cx
            .background_executor()
            .spawn(async move { output::devices() });
        self.listing = Some(cx.spawn(async move |this, cx| {
            let devices = listed.await;
            this.update(cx, |this, cx| {
                this.listing = None;
                this.listed = true;
                if this.devices != devices {
                    this.devices = devices;
                    cx.notify();
                }
            })
            .ok();
        }));
    }
}
