use std::sync::atomic::{AtomicU32, Ordering};

/// Counter that returns unique IDs.
///
/// Under the hood it uses a `u32` that will eventually wrap around. When incrementing it once a
/// second, it will wrap around after about 136 years.
pub struct IdCounter {
    value: AtomicU32,
}

impl IdCounter {
    pub const fn new() -> Self {
        Self {
            // Start from 1 to reduce the possibility that some other code that uses these IDs will
            // get confused.
            value: AtomicU32::new(1),
        }
    }

    pub fn next(&self) -> u32 {
        self.value.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for IdCounter {
    fn default() -> Self {
        Self::new()
    }
}
