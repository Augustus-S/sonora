use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{App, EntityId, Window};
use music::Spectrum;
use ui::Levels;

/// How long the drawn levels take to close 63% of the gap to the analyzer's. A time constant
/// rather than a share per frame, so the motion is the same on a 60 Hz and a 144 Hz display
/// and a frame that came late moves further instead of falling behind.
const SETTLE: Duration = Duration::from_millis(40);
/// The longest gap between two frames that still counts as one step: past it the levels adopt
/// the target outright, so coming back from a stalled window snaps rather than swoops.
const STALL: Duration = Duration::from_millis(250);

#[derive(Default)]
struct State {
    shown: Levels,
    armed: bool,
    visible: bool,
    /// Whether showing lasts one frame only, so a drive whose owner stops drawing it stops too.
    leased: bool,
    beat: Option<Instant>,
}

#[derive(Clone, Default)]
pub struct VisualizerDrive {
    state: Rc<RefCell<State>>,
}

impl VisualizerDrive {
    /// A drive for a visualizer that can drop out of the tree without anyone calling `hide`,
    /// a settings preview say. Each `show` keeps it running for one more frame, so it winds
    /// down on the first frame its owner no longer draws it.
    pub fn leased() -> Self {
        let drive = Self::default();
        drive.state.borrow_mut().leased = true;
        drive
    }

    pub fn levels(&self) -> Levels {
        self.state.borrow().shown.clone()
    }

    pub fn show(&self, watch: EntityId, spectrum: Spectrum, window: &mut Window) {
        let mut state = self.state.borrow_mut();
        state.visible = true;
        if state.armed {
            return;
        }
        state.armed = true;
        state.beat = None;
        drop(state);

        let drive = self.clone();
        window.on_next_frame(move |window, cx| drive.step(watch, spectrum, window, cx));
    }

    pub fn hide(&self) {
        self.state.borrow_mut().visible = false;
    }

    fn step(&self, watch: EntityId, spectrum: Spectrum, window: &mut Window, cx: &mut App) {
        {
            let mut state = self.state.borrow_mut();
            if !state.visible {
                state.armed = false;
                return;
            }
            // The notify below draws the owner within this same frame, and drawing it shows
            // the drive again before the next step comes round.
            if state.leased {
                state.visible = false;
            }

            let now = Instant::now();
            let rate = match state.beat.replace(now) {
                Some(beat) if now.duration_since(beat) < STALL => {
                    1. - (-now.duration_since(beat).as_secs_f32() / SETTLE.as_secs_f32()).exp()
                }
                _ => 1.,
            };
            ease(&mut state.shown.left, spectrum.left(), rate);
            ease(&mut state.shown.right, spectrum.right(), rate);
        }

        cx.notify(watch);
        let drive = self.clone();
        window.on_next_frame(move |window, cx| drive.step(watch, spectrum, window, cx));
    }
}

/// Walks `shown` a share of the way towards `target`, adopting it outright when the band
/// count changes.
fn ease(shown: &mut Vec<f32>, target: Vec<f32>, rate: f32) {
    if shown.len() != target.len() {
        *shown = target;
        return;
    }
    for (shown, target) in shown.iter_mut().zip(&target) {
        *shown += (target - *shown) * rate;
    }
}
