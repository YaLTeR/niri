use niri_config::layer_rule::{LayerRule, Match};
use niri_config::BlockOutFrom;
use smithay::desktop::LayerSurface;

pub mod mapped;
pub use mapped::MappedLayer;

/// Rules fully resolved for a layer-shell surface.
#[derive(Debug, PartialEq)]
pub struct ResolvedLayerRules {
    /// Extra opacity to draw this window with.
    pub opacity: Option<f32>,
    /// Whether to block out this window from certain render targets.
    pub block_out_from: Option<BlockOutFrom>,
}

impl ResolvedLayerRules {
    pub const fn empty() -> Self {
        Self {
            opacity: None,
            block_out_from: None,
        }
    }

    pub fn compute(rules: &[LayerRule], surface: &LayerSurface, is_at_startup: bool) -> Self {
        let _span = tracy_client::span!("ResolvedLayerRules::compute");

        let mut resolved = ResolvedLayerRules::empty();

        for rule in rules {
            let matches = |m: &Match| {
                if let Some(at_startup) = m.at_startup {
                    if at_startup != is_at_startup {
                        return false;
                    }
                }

                surface_matches(surface, m)
            };

            if !(rule.matches.is_empty() || rule.matches.iter().any(matches)) {
                continue;
            }

            if rule.excludes.iter().any(matches) {
                continue;
            }

            if let Some(x) = rule.opacity {
                resolved.opacity = Some(x);
            }
            if let Some(x) = rule.block_out_from {
                resolved.block_out_from = Some(x);
            }
        }

        resolved
    }
}

fn surface_matches(surface: &LayerSurface, m: &Match) -> bool {
    if let Some(namespace_re) = &m.namespace {
        if !namespace_re.0.is_match(surface.namespace()) {
            return false;
        }
    }

    true
}
