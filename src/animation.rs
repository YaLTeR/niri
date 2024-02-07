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
    curve: Curve,
}

#[derive(Debug, Clone, Copy)]
pub enum Curve {
    EaseOutCubic,
    EaseOutExpo,
}

impl Animation {
    pub fn new(
        from: f64,
        to: f64,
        config: niri_config::Animation,
        default: niri_config::Animation,
    ) -> Self {
        // FIXME: ideally we shouldn't use current time here because animations started within the
        // same frame cycle should have the same start time to be synchronized.
        let now = get_monotonic_time();

        let duration_ms = if config.off {
            0
        } else {
            config.duration_ms.unwrap_or(default.duration_ms.unwrap())
        };
        let duration = Duration::from_millis(u64::from(duration_ms))
            .mul_f64(ANIMATION_SLOWDOWN.load(Ordering::Relaxed));

        let curve = Curve::from(config.curve.unwrap_or(default.curve.unwrap()));

        Self {
            from,
            to,
            duration,
            start_time: now,
            current_time: now,
            curve,
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
        self.curve.y(x) * (self.to - self.from) + self.from
    }

    pub fn to(&self) -> f64 {
        self.to
    }

    #[cfg(test)]
    pub fn from(&self) -> f64 {
        self.from
    }
}

impl Curve {
    pub fn y(self, x: f64) -> f64 {
        match self {
            Curve::EaseOutCubic => EaseOutCubic.y(x),
            Curve::EaseOutExpo => 1. - 2f64.powf(-10. * x),
        }
    }
}

impl From<niri_config::AnimationCurve> for Curve {
    fn from(value: niri_config::AnimationCurve) -> Self {
        match value {
            niri_config::AnimationCurve::EaseOutCubic => Curve::EaseOutCubic,
            niri_config::AnimationCurve::EaseOutExpo => Curve::EaseOutExpo,
        }
    }
}
