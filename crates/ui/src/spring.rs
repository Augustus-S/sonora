//! A value animated by a spring, and the clock that steps it. Anything that moves on a spring
//! keeps one `Spring` per value and one `Beat` for all of them, and steps them once a frame.

use std::time::{Duration, Instant};

use gpui::{SpringConfig, SpringState};

/// How close to its target, and how slow, a spring must be to count as at rest. Fine enough that
/// landing on the exact target moves nothing by a visible fraction of a pixel.
const REST: f32 = 0.001;
/// The longest step one frame may take. A frame that comes late, after a stall or a hidden
/// window, moves the spring this far and no further, so it never leaps or blows up.
const STALL: Duration = Duration::from_millis(50);

/// One animated value driven by a spring towards its target.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub config: SpringConfig,
    pub state: SpringState,
    pub target: f32,
}

impl Spring {
    pub fn new(config: SpringConfig) -> Self {
        Self {
            config,
            state: SpringState {
                position: 0.,
                velocity: 0.,
            },
            target: 0.,
        }
    }

    /// A spring already resting at `value`.
    pub fn resting(config: SpringConfig, value: f32) -> Self {
        let mut spring = Self::new(config);
        spring.aim(value);
        spring.finish();
        spring
    }

    pub fn aim(&mut self, target: f32) {
        self.target = target;
    }

    /// Moves the value on by `elapsed`. Once it has come to rest it lands exactly on the target,
    /// since a value left a hair off would keep a shape on fractional pixels for good.
    pub fn step(&mut self, elapsed: Duration) {
        self.state = self
            .config
            .step(self.state, self.target, elapsed.as_secs_f32());
        if self.settled() {
            self.finish();
        }
    }

    pub fn at(&self) -> f32 {
        self.state.position
    }

    pub fn settled(&self) -> bool {
        self.config.is_settled(self.state, self.target, REST)
    }

    /// Lands on the target at once and drops any velocity, for reduced motion.
    pub fn finish(&mut self) {
        self.state = SpringState {
            position: self.target,
            velocity: 0.,
        };
    }
}

/// The clock a set of springs steps by: the time since the frame before, capped so a late frame
/// never takes one long leap. It stops between runs, so the first frame of a new run steps by
/// nothing rather than by however long everything stood still.
#[derive(Clone, Copy, Debug, Default)]
pub struct Beat {
    last: Option<Instant>,
}

impl Beat {
    /// The time to step by this frame.
    pub fn tick(&mut self) -> Duration {
        let now = Instant::now();
        self.last
            .replace(now)
            .map_or(Duration::ZERO, |last| (now - last).min(STALL))
    }

    /// Forgets the last frame once everything has come to rest.
    pub fn stop(&mut self) {
        self.last = None;
    }
}
