use niri_config::{BlockOutFrom, BorderRule, Match, WindowRule};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{
    ToplevelSurface, XdgToplevelSurfaceData, XdgToplevelSurfaceRoleAttributes,
};

use crate::layout::workspace::ColumnWidth;

pub mod mapped;
pub use mapped::Mapped;

pub mod unmapped;
pub use unmapped::{InitialConfigureState, Unmapped};

/// Reference to a mapped or unmapped window.
#[derive(Debug, Clone, Copy)]
pub enum WindowRef<'a> {
    Unmapped(&'a Unmapped),
    Mapped(&'a Mapped),
}

/// Rules fully resolved for a window.
#[derive(Debug, PartialEq)]
pub struct ResolvedWindowRules {
    /// Default width for this window.
    ///
    /// - `None`: unset (global default should be used).
    /// - `Some(None)`: set to empty (window picks its own width).
    /// - `Some(Some(width))`: set to a particular width.
    pub default_width: Option<Option<ColumnWidth>>,

    /// Output to open this window on.
    pub open_on_output: Option<String>,

    /// Whether the window should open full-width.
    pub open_maximized: Option<bool>,

    /// Whether the window should open fullscreen.
    pub open_fullscreen: Option<bool>,

    /// Extra bound on the minimum window width.
    pub min_width: Option<u16>,
    /// Extra bound on the minimum window height.
    pub min_height: Option<u16>,
    /// Extra bound on the maximum window width.
    pub max_width: Option<u16>,
    /// Extra bound on the maximum window height.
    pub max_height: Option<u16>,

    /// Focus ring overrides.
    pub focus_ring: BorderRule,
    /// Window border overrides.
    pub border: BorderRule,

    /// Whether or not to draw the border with a solid background.
    ///
    /// `None` means using the SSD heuristic.
    pub draw_border_with_background: Option<bool>,

    /// Extra opacity to draw this window with.
    pub opacity: Option<f32>,

    /// Whether to block out this window from certain render targets.
    pub block_out_from: Option<BlockOutFrom>,
}

impl<'a> WindowRef<'a> {
    pub fn toplevel(self) -> &'a ToplevelSurface {
        match self {
            WindowRef::Unmapped(unmapped) => unmapped.toplevel(),
            WindowRef::Mapped(mapped) => mapped.toplevel(),
        }
    }

    pub fn is_focused(self) -> bool {
        match self {
            WindowRef::Unmapped(_) => false,
            WindowRef::Mapped(mapped) => mapped.is_focused(),
        }
    }
}

impl ResolvedWindowRules {
    pub const fn empty() -> Self {
        Self {
            default_width: None,
            open_on_output: None,
            open_maximized: None,
            open_fullscreen: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            focus_ring: BorderRule {
                off: false,
                on: false,
                width: None,
                active_color: None,
                inactive_color: None,
                active_gradient: None,
                inactive_gradient: None,
            },
            border: BorderRule {
                off: false,
                on: false,
                width: None,
                active_color: None,
                inactive_color: None,
                active_gradient: None,
                inactive_gradient: None,
            },
            draw_border_with_background: None,
            opacity: None,
            block_out_from: None,
        }
    }

    pub fn compute(rules: &[WindowRule], window: WindowRef) -> Self {
        let _span = tracy_client::span!("ResolvedWindowRules::compute");

        let mut resolved = ResolvedWindowRules::empty();

        let toplevel = window.toplevel();
        with_states(toplevel.wl_surface(), |states| {
            let mut role = states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap();

            // Ensure server_pending like in Smithay's with_pending_state().
            if role.server_pending.is_none() {
                role.server_pending = Some(role.current_server_state().clone());
            }

            let mut open_on_output = None;

            for rule in rules {
                let matches = |m| window_matches(window, &role, m);

                if !(rule.matches.is_empty() || rule.matches.iter().any(matches)) {
                    continue;
                }

                if rule.excludes.iter().any(matches) {
                    continue;
                }

                if let Some(x) = rule
                    .default_column_width
                    .as_ref()
                    .map(|d| d.0.map(ColumnWidth::from))
                {
                    resolved.default_width = Some(x);
                }

                if let Some(x) = rule.open_on_output.as_deref() {
                    open_on_output = Some(x);
                }

                if let Some(x) = rule.open_maximized {
                    resolved.open_maximized = Some(x);
                }

                if let Some(x) = rule.open_fullscreen {
                    resolved.open_fullscreen = Some(x);
                }

                if let Some(x) = rule.min_width {
                    resolved.min_width = Some(x);
                }
                if let Some(x) = rule.min_height {
                    resolved.min_height = Some(x);
                }
                if let Some(x) = rule.max_width {
                    resolved.max_width = Some(x);
                }
                if let Some(x) = rule.max_height {
                    resolved.max_height = Some(x);
                }

                resolved.focus_ring.merge_with(&rule.focus_ring);
                resolved.border.merge_with(&rule.border);

                if let Some(x) = rule.draw_border_with_background {
                    resolved.draw_border_with_background = Some(x);
                }
                if let Some(x) = rule.opacity {
                    resolved.opacity = Some(x);
                }
                if let Some(x) = rule.block_out_from {
                    resolved.block_out_from = Some(x);
                }
            }

            resolved.open_on_output = open_on_output.map(|x| x.to_owned());
        });

        resolved
    }
}

fn window_matches(window: WindowRef, role: &XdgToplevelSurfaceRoleAttributes, m: &Match) -> bool {
    // Must be ensured by the caller.
    let server_pending = role.server_pending.as_ref().unwrap();

    if let Some(is_focused) = m.is_focused {
        if window.is_focused() != is_focused {
            return false;
        }
    }

    if let Some(is_active) = m.is_active {
        // Our "is-active" definition corresponds to the window having a pending Activated state.
        let pending_activated = server_pending
            .states
            .contains(xdg_toplevel::State::Activated);
        if is_active != pending_activated {
            return false;
        }
    }

    if let Some(app_id_re) = &m.app_id {
        let Some(app_id) = &role.app_id else {
            return false;
        };
        if !app_id_re.is_match(app_id) {
            return false;
        }
    }

    if let Some(title_re) = &m.title {
        let Some(title) = &role.title else {
            return false;
        };
        if !title_re.is_match(title) {
            return false;
        }
    }

    true
}
