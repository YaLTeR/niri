use niri_config::PresetSize;
use smithay::desktop::Window;
use smithay::output::Output;
use smithay::wayland::shell::xdg::ToplevelSurface;
use smithay::wayland::xdg_activation::XdgActivationTokenData;

use super::ResolvedWindowRules;
use crate::layout::scrolling::ColumnWidth;

#[derive(Debug)]
pub struct Unmapped {
    pub window: Window,
    pub state: InitialConfigureState,
    /// Activation token, if one was used on this unmapped window.
    pub activation_token_data: Option<XdgActivationTokenData>,
}

#[allow(clippy::large_enum_variant)]
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

        /// Resolved scrolling default width for this window.
        ///
        /// `None` means that the window will pick its own width.
        width: Option<ColumnWidth>,

        /// Resolved scrolling default height for this window.
        ///
        /// `None` means that the window will pick its own height.
        height: Option<PresetSize>,

        /// Resolved floating default width for this window.
        ///
        /// `None` means that the window will pick its own width.
        floating_width: Option<ColumnWidth>,

        /// Resolved floating default height for this window.
        ///
        /// `None` means that the window will pick its own height.
        floating_height: Option<PresetSize>,

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

        /// Workspace to open this window on.
        workspace_name: Option<String>,
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
            activation_token_data: None,
        }
    }

    pub fn needs_initial_configure(&self) -> bool {
        matches!(self.state, InitialConfigureState::NotConfigured { .. })
    }

    pub fn toplevel(&self) -> &ToplevelSurface {
        self.window.toplevel().expect("no X11 support")
    }
}
