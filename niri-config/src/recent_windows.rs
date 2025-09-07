use super::ModKey;

/// Delay before the window focus is considered to be locked-in for Window
/// MRU ordering. For now the delay is not configurable.
pub const DEFAULT_MRU_COMMIT_MS: u64 = 750;

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct RecentWindows {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument, str), default = Self::default().mod_key)]
    pub mod_key: ModKey,
}

impl Default for RecentWindows {
    fn default() -> Self {
        RecentWindows {
            off: false,
            mod_key: ModKey::Alt,
        }
    }
}
