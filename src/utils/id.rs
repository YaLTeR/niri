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
            value: AtomicU32::new(0),
        }
    }

    pub fn next(&self) -> u32 {
        self.value.fetch_add(1, Ordering::SeqCst)
    }
}

impl Default for IdCounter {
    fn default() -> Self {
        Self::new()
    }
}
