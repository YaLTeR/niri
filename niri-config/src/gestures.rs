use niri_macros::Mergeable;

use crate::FloatOrInt;

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Mergeable)]
pub struct Gestures {
    #[knuffel(child, default)]
    pub dnd_edge_view_scroll: DndEdgeViewScroll,
    #[knuffel(child, default)]
    pub dnd_edge_workspace_switch: DndEdgeWorkspaceSwitch,
    #[knuffel(child, default)]
    pub hot_corners: HotCorners,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq, Mergeable)]
pub struct DndEdgeViewScroll {
    #[knuffel(child, unwrap(argument), default = Self::default().trigger_width)]
    pub trigger_width: FloatOrInt<0, 65535>,
    #[knuffel(child, unwrap(argument), default = Self::default().delay_ms)]
    pub delay_ms: u16,
    #[knuffel(child, unwrap(argument), default = Self::default().max_speed)]
    pub max_speed: FloatOrInt<0, 1_000_000>,
}

impl Default for DndEdgeViewScroll {
    fn default() -> Self {
        Self {
            trigger_width: FloatOrInt(30.), // Taken from GTK 4.
            delay_ms: 100,
            max_speed: FloatOrInt(1500.),
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq, Mergeable)]
pub struct DndEdgeWorkspaceSwitch {
    #[knuffel(child, unwrap(argument), default = Self::default().trigger_height)]
    pub trigger_height: FloatOrInt<0, 65535>,
    #[knuffel(child, unwrap(argument), default = Self::default().delay_ms)]
    pub delay_ms: u16,
    #[knuffel(child, unwrap(argument), default = Self::default().max_speed)]
    pub max_speed: FloatOrInt<0, 1_000_000>,
}

impl Default for DndEdgeWorkspaceSwitch {
    fn default() -> Self {
        Self {
            trigger_height: FloatOrInt(50.),
            delay_ms: 100,
            max_speed: FloatOrInt(1500.),
        }
    }
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Mergeable)]
pub struct HotCorners {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub top_left: bool,
    #[knuffel(child)]
    pub top_right: bool,
    #[knuffel(child)]
    pub bottom_left: bool,
    #[knuffel(child)]
    pub bottom_right: bool,
}
