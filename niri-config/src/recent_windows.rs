use super::ModKey;
use crate::utils::{Flag, MergeWith};

/// Delay before the window focus is considered to be locked-in for Window
/// MRU ordering. For now the delay is not configurable.
pub const DEFAULT_MRU_COMMIT_MS: u64 = 750;

#[derive(Debug, PartialEq)]
pub struct RecentWindows {
    pub on: bool,
    pub mod_key: ModKey,
    pub enable_selection_animation: bool,
}

impl Default for RecentWindows {
    fn default() -> Self {
        RecentWindows {
            on: true,
            enable_selection_animation: false,
            mod_key: ModKey::Alt,
        }
    }
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct RecentWindowsPart {
    #[knuffel(child)]
    pub on: bool,
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument, str))]
    pub mod_key: Option<ModKey>,
    #[knuffel(child)]
    pub enable_selection_animation: Option<Flag>,
}

impl MergeWith<RecentWindowsPart> for RecentWindows {
    fn merge_with(&mut self, part: &RecentWindowsPart) {
        self.on |= part.on;
        if part.off {
            self.on = false;
        }
        merge!((self, part), enable_selection_animation);
        merge_clone!((self, part), mod_key);
    }
}
