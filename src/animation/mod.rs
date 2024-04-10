use std::time::Duration;

use keyframe::functions::{EaseOutCubic, EaseOutQuad};
use keyframe::EasingFunction;
use portable_atomic::{AtomicF64, Ordering};

use crate::utils::get_monotonic_time;

mod spring;
pub use spring::{Spring, SpringParams};

pub static ANIMATION_SLOWDOWN: AtomicF64 = AtomicF64::new(1.);

#[derive(Debug)]
pub struct Animation {
    from: f64,
    to: f64,
    duration: Duration,
    /// Time until the animation first reaches `to`.
    ///
    /// Best effort; not always exactly precise.
    clamped_duration: Duration,
    start_time: Duration,
    current_time: Duration,
    kind: Kind,
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Easing {
        curve: Curve,
    },
    Spring(Spring),
    Deceleration {
        initial_velocity: f64,
        deceleration_rate: f64,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum Curve {
    EaseOutQuad,
    EaseOutCubic,
    EaseOutExpo,
}

impl Animation {
    pub fn new(
        from: f64,
        to: f64,
        initial_velocity: f64,
        config: niri_config::Animation,
        default: niri_config::Animation,
    ) -> Self {
        if config.off {
            return Self::ease(from, to, 0, Curve::EaseOutCubic);
        }

        // Resolve defaults.
        let (kind, easing_defaults) = match (config.kind, default.kind) {
            // Configured spring.
            (configured @ niri_config::AnimationKind::Spring(_), _) => (configured, None),
            // Configured nothing, defaults spring.
            (
                niri_config::AnimationKind::Easing(easing),
                defaults @ niri_config::AnimationKind::Spring(_),
            ) if easing == niri_config::EasingParams::unfilled() => (defaults, None),
            // Configured easing or nothing, defaults easing.
            (
                configured @ niri_config::AnimationKind::Easing(_),
                niri_config::AnimationKind::Easing(defaults),
            ) => (configured, Some(defaults)),
            // Configured easing, defaults spring.
            (
                configured @ niri_config::AnimationKind::Easing(_),
                niri_config::AnimationKind::Spring(_),
            ) => (configured, None),
        };

        match kind {
            niri_config::AnimationKind::Spring(p) => {
                let params = SpringParams::new(p.damping_ratio, f64::from(p.stiffness), p.epsilon);

                let spring = Spring {
                    from,
                    to,
                    initial_velocity,
                    params,
                };
                Self::spring(spring)
            }
            niri_config::AnimationKind::Easing(p) => {
                let defaults = easing_defaults.unwrap_or(niri_config::EasingParams::default());
                let duration_ms = p.duration_ms.or(defaults.duration_ms).unwrap();
                let curve = Curve::from(p.curve.or(defaults.curve).unwrap());
                Self::ease(from, to, u64::from(duration_ms), curve)
            }
        }
    }

    pub fn ease(from: f64, to: f64, duration_ms: u64, curve: Curve) -> Self {
        // FIXME: ideally we shouldn't use current time here because animations started within the
        // same frame cycle should have the same start time to be synchronized.
        let now = get_monotonic_time();

        let duration = Duration::from_millis(duration_ms);
        let kind = Kind::Easing { curve };

        Self {
            from,
            to,
            duration,
            // Our current curves never overshoot.
            clamped_duration: duration,
            start_time: now,
            current_time: now,
            kind,
        }
    }

    pub fn spring(spring: Spring) -> Self {
        let _span = tracy_client::span!("Animation::spring");

        // FIXME: ideally we shouldn't use current time here because animations started within the
        // same frame cycle should have the same start time to be synchronized.
        let now = get_monotonic_time();

        let duration = spring.duration();
        let clamped_duration = spring.clamped_duration().unwrap_or(duration);
        let kind = Kind::Spring(spring);

        Self {
            from: spring.from,
            to: spring.to,
            duration,
            clamped_duration,
            start_time: now,
            current_time: now,
            kind,
        }
    }

    pub fn decelerate(
        from: f64,
        initial_velocity: f64,
        deceleration_rate: f64,
        threshold: f64,
    ) -> Self {
        // FIXME: ideally we shouldn't use current time here because animations started within the
        // same frame cycle should have the same start time to be synchronized.
        let now = get_monotonic_time();

        let duration_s = if initial_velocity == 0. {
            0.
        } else {
            let coeff = 1000. * deceleration_rate.ln();
            (-coeff * threshold / initial_velocity.abs()).ln() / coeff
        };
        let duration = Duration::from_secs_f64(duration_s);

        let to = from - initial_velocity / (1000. * deceleration_rate.ln());

        let kind = Kind::Deceleration {
            initial_velocity,
            deceleration_rate,
        };

        Self {
            from,
            to,
            duration,
            clamped_duration: duration,
            start_time: now,
            current_time: now,
            kind,
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

    pub fn is_clamped_done(&self) -> bool {
        self.current_time >= self.start_time + self.clamped_duration
    }

    pub fn value(&self) -> f64 {
        if self.is_done() {
            return self.to;
        }

        let passed = self.current_time - self.start_time;

        match self.kind {
            Kind::Easing { curve } => {
                let passed = passed.as_secs_f64();
                let total = self.duration.as_secs_f64();
                let x = (passed / total).clamp(0., 1.);
                curve.y(x) * (self.to - self.from) + self.from
            }
            Kind::Spring(spring) => spring.value_at(passed),
            Kind::Deceleration {
                initial_velocity,
                deceleration_rate,
            } => {
                let passed = passed.as_secs_f64();
                let coeff = 1000. * deceleration_rate.ln();
                self.from + (deceleration_rate.powf(1000. * passed) - 1.) / coeff * initial_velocity
            }
        }
    }

    /// Returns a value that stops at the target value after first reaching it.
    ///
    /// Best effort; not always exactly precise.
    pub fn clamped_value(&self) -> f64 {
        if self.is_clamped_done() {
            return self.to;
        }

        self.value()
    }

    pub fn to(&self) -> f64 {
        self.to
    }

    #[cfg(test)]
    pub fn from(&self) -> f64 {
        self.from
    }

    pub fn offset(&mut self, offset: f64) {
        self.from += offset;
        self.to += offset;

        if let Kind::Spring(spring) = &mut self.kind {
            spring.from += offset;
            spring.to += offset;
        }
    }
}

impl Curve {
    pub fn y(self, x: f64) -> f64 {
        match self {
            Curve::EaseOutQuad => EaseOutQuad.y(x),
            Curve::EaseOutCubic => EaseOutCubic.y(x),
            Curve::EaseOutExpo => 1. - 2f64.powf(-10. * x),
        }
    }
}

impl From<niri_config::AnimationCurve> for Curve {
    fn from(value: niri_config::AnimationCurve) -> Self {
        match value {
            niri_config::AnimationCurve::EaseOutQuad => Curve::EaseOutQuad,
            niri_config::AnimationCurve::EaseOutCubic => Curve::EaseOutCubic,
            niri_config::AnimationCurve::EaseOutExpo => Curve::EaseOutExpo,
        }
    }
}
