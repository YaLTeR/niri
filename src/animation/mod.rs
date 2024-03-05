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
        let duration = Duration::from_millis(u64::from(duration_ms));

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
        if self.duration.is_zero() {
            self.current_time = time;
            return;
        }

        let end_time = self.start_time + self.duration;
        if end_time <= self.current_time {
            return;
        }

        let slowdown = ANIMATION_SLOWDOWN.load(Ordering::Relaxed);
        if slowdown <= f64::EPSILON {
            // Zero slowdown will cause the animation to end right away.
            self.current_time = end_time;
            return;
        }

        // We can't change current_time (since the incoming time values are always real-time), so
        // apply the slowdown by shifting the start time to compensate.
        if self.current_time <= time {
            let delta = time - self.current_time;

            let max_delta = end_time - self.current_time;
            let min_slowdown = delta.as_secs_f64() / max_delta.as_secs_f64();
            if slowdown <= min_slowdown {
                // Our slowdown value will cause the animation to end right away.
                self.current_time = end_time;
                return;
            }

            let adjusted_delta = delta.div_f64(slowdown);
            if adjusted_delta >= delta {
                self.start_time -= adjusted_delta - delta;
            } else {
                self.start_time += delta - adjusted_delta;
            }
        } else {
            let delta = self.current_time - time;

            let min_slowdown = delta.as_secs_f64() / self.current_time.as_secs_f64();
            if slowdown <= min_slowdown {
                // Current time was about to jump to before the animation had started; let's just
                // cancel the animation in this case.
                self.current_time = end_time;
                return;
            }

            let adjusted_delta = delta.div_f64(slowdown);
            if adjusted_delta >= delta {
                self.start_time += adjusted_delta - delta;
            } else {
                self.start_time -= delta - adjusted_delta;
            }
        }

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
