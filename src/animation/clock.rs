use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use crate::utils::get_monotonic_time;

/// Clock that can have its time value overridden.
///
/// Can be cloned to share the same clock.
#[derive(Debug, Default, Clone)]
pub struct Clock {
    time_override: Rc<Cell<Option<Duration>>>,
}

impl Clock {
    /// Creates a new [`Clock`] with time override in place.
    pub fn with_override(time: Duration) -> Self {
        Self {
            time_override: Rc::new(Cell::new(Some(time))),
        }
    }

    /// Sets the current time override.
    pub fn set_time_override(&mut self, time: Option<Duration>) {
        self.time_override.set(time);
    }

    /// Gets the current time.
    #[inline]
    pub fn now(&self) -> Duration {
        self.time_override.get().unwrap_or_else(get_monotonic_time)
    }
}

impl PartialEq for Clock {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.time_override, &other.time_override)
    }
}

impl Eq for Clock {}
