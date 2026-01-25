use niri_ipc::ZoomMovement;

use crate::utils::MergeWith;
use crate::FloatOrInt;

/// Global zoom configuration.
#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq)]
pub struct ZoomConfig {
    /// Default zoom factor (1.0 to 100.0).
    #[knuffel(child, unwrap(argument))]
    pub default_factor: Option<FloatOrInt<1, 100>>,

    /// Zoom movement mode (cursor-follow or edge-pushed).
    #[knuffel(child, unwrap(argument, str))]
    pub movement: Option<ZoomMovement>,

    /// Threshold for edge-pushed movement (0.0 to 1.0).
    #[knuffel(child, unwrap(argument))]
    pub threshold: Option<f64>,
}

impl MergeWith<ZoomConfig> for ZoomConfig {
    fn merge_with(&mut self, part: &ZoomConfig) {
        if part.default_factor.is_some() {
            self.default_factor = part.default_factor;
        }
        if part.movement.is_some() {
            self.movement = part.movement;
        }
        if part.threshold.is_some() {
            self.threshold = part.threshold;
        }
    }
}
