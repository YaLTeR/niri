use smithay::desktop::Window;
use smithay::output::Output;

use super::ResolvedWindowRules;
use crate::layout::workspace::ColumnWidth;

#[derive(Debug)]
pub struct Unmapped {
    pub window: Window,
    pub state: InitialConfigureState,
}

#[derive(Debug)]
pub enum InitialConfigureState {
    /// The window has not been initially configured yet.
    NotConfigured {
        /// Whether the window requested to be fullscreened, and the requested output, if any.
        wants_fullscreen: Option<Option<Output>>,
    },
    /// The window has been configured.
    Configured {
        /// Up-to-date rules.
        ///
        /// We start tracking window rules when sending the initial configure, since they don't
        /// affect anything before that.
        rules: ResolvedWindowRules,

        /// Resolved default width for this window.
        ///
        /// `None` means that the window will pick its own width.
        width: Option<ColumnWidth>,

        /// Whether the window should open full-width.
        is_full_width: bool,

        /// Output to open this window on.
        ///
        /// This can be `None` in cases like:
        ///
        /// - There are no outputs connected.
        /// - This is a dialog with a parent, and there was no explicit output set, so this dialog
        ///   should fetch the parent's current output again upon mapping.
        output: Option<Output>,
    },
}

impl Unmapped {
    /// Wraps a newly created window that hasn't been initially configured yet.
    pub fn new(window: Window) -> Self {
        Self {
            window,
            state: InitialConfigureState::NotConfigured {
                wants_fullscreen: None,
            },
        }
    }

    pub fn needs_initial_configure(&self) -> bool {
        matches!(self.state, InitialConfigureState::NotConfigured { .. })
    }
}
