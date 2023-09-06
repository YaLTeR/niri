use std::time::Duration;

use keyframe::functions::EaseOutCubic;
use keyframe::EasingFunction;
use portable_atomic::{AtomicF64, Ordering};

use crate::utils::get_monotonic_time;

pub static ANIMATION_SLOWDOWN: AtomicF64 = AtomicF64::new(1.);

#[derive(Debug)]
pub struct Animation {
    from: f64,
    to: f64,
    duration: Duration,
    start_time: Duration,
    current_time: Duration,
}

impl Animation {
    pub fn new(from: f64, to: f64, over: Duration) -> Self {
        // FIXME: ideally we shouldn't use current time here because animations started within the
        // same frame cycle should have the same start time to be synchronized.
        let now = get_monotonic_time();

        Self {
            from,
            to,
            duration: over.mul_f64(ANIMATION_SLOWDOWN.load(Ordering::Relaxed)),
            start_time: now,
            current_time: now,
        }
    }

    pub fn set_current_time(&mut self, time: Duration) {
        self.current_time = time;
    }

    pub fn is_done(&self) -> bool {
        self.current_time >= self.start_time + self.duration
    }

    pub fn value(&self) -> f64 {
        let passed = (self.current_time - self.start_time).as_secs_f64();
        let total = self.duration.as_secs_f64();
        let x = (passed / total).clamp(0., 1.);
        EaseOutCubic.y(x) * (self.to - self.from) + self.from
    }
}
