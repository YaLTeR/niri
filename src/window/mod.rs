use niri_config::{Match, WindowRule};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{
    ToplevelSurface, XdgToplevelSurfaceData, XdgToplevelSurfaceRoleAttributes,
};

use crate::layout::workspace::ColumnWidth;

pub mod mapped;
pub use mapped::Mapped;

pub mod unmapped;
pub use unmapped::{InitialConfigureState, Unmapped};

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

    /// Whether or not to draw the border with a solid background.
    ///
    /// `None` means using the SSD heuristic.
    pub draw_border_with_background: Option<bool>,
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
            draw_border_with_background: None,
        }
    }

    pub fn compute(rules: &[WindowRule], toplevel: &ToplevelSurface) -> Self {
        let _span = tracy_client::span!("ResolvedWindowRules::compute");

        let mut resolved = ResolvedWindowRules::empty();

        with_states(toplevel.wl_surface(), |states| {
            let role = states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap();

            let mut open_on_output = None;

            for rule in rules {
                if !(rule.matches.is_empty()
                    || rule.matches.iter().any(|m| window_matches(&role, m)))
                {
                    continue;
                }

                if rule.excludes.iter().any(|m| window_matches(&role, m)) {
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

                if let Some(x) = rule.draw_border_with_background {
                    resolved.draw_border_with_background = Some(x);
                }
            }

            resolved.open_on_output = open_on_output.map(|x| x.to_owned());
        });

        resolved
    }
}

fn window_matches(role: &XdgToplevelSurfaceRoleAttributes, m: &Match) -> bool {
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
