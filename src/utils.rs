use std::time::Duration;

use smithay::reexports::nix::time::{clock_gettime, ClockId};

pub fn get_monotonic_time() -> Duration {
    Duration::from(clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap())
}
