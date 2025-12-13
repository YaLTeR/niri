use std::sync::atomic::{AtomicU64, Ordering};

/// Counter that returns unique IDs.
pub struct IdCounter {
    value: AtomicU64,
    hidden_value: AtomicU64, // Start from max u64, work backwards
}

impl IdCounter {
    pub const fn new() -> Self {
        Self {
            // Start from 1 to reduce the possibility that some other code that uses these IDs will
            // get confused.
            value: AtomicU64::new(1),
            hidden_value: AtomicU64::new(u64::MAX),
        }
    }

    pub fn next(&self) -> u64 {
        self.value.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for IdCounter {
    fn default() -> Self {
        Self::new()
    }
}
