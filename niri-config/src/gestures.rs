use niri_macros::Mergeable;

use crate::{FloatOrInt, MaybeSet};

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
    #[knuffel(child, unwrap(argument), default = MaybeSet::unset(FloatOrInt(30.)))]
    pub trigger_width: MaybeSet<FloatOrInt<0, 65535>>,
    #[knuffel(child, unwrap(argument), default = MaybeSet::unset(100))]
    pub delay_ms: MaybeSet<u16>,
    #[knuffel(child, unwrap(argument), default = MaybeSet::unset(FloatOrInt(1500.)))]
    pub max_speed: MaybeSet<FloatOrInt<0, 1_000_000>>,
}

impl Default for DndEdgeViewScroll {
    fn default() -> Self {
        Self {
            trigger_width: FloatOrInt(30.).into(), // Taken from GTK 4.
            delay_ms: 100.into(),
            max_speed: FloatOrInt(1500.).into(),
        }
    }
}

impl DndEdgeViewScroll {
    pub fn resolved_trigger_width(&self) -> FloatOrInt<0, 65535> {
        *self.trigger_width.value()
    }

    pub fn resolved_delay_ms(&self) -> u16 {
        *self.delay_ms.value()
    }

    pub fn resolved_max_speed(&self) -> FloatOrInt<0, 1_000_000> {
        *self.max_speed.value()
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq, Mergeable)]
pub struct DndEdgeWorkspaceSwitch {
    #[knuffel(child, unwrap(argument), default = MaybeSet::unset(FloatOrInt(50.)))]
    pub trigger_height: MaybeSet<FloatOrInt<0, 65535>>,
    #[knuffel(child, unwrap(argument), default = MaybeSet::unset(100))]
    pub delay_ms: MaybeSet<u16>,
    #[knuffel(child, unwrap(argument), default = MaybeSet::unset(FloatOrInt(1500.)))]
    pub max_speed: MaybeSet<FloatOrInt<0, 1_000_000>>,
}

impl Default for DndEdgeWorkspaceSwitch {
    fn default() -> Self {
        Self {
            trigger_height: FloatOrInt(50.).into(),
            delay_ms: 100.into(),
            max_speed: FloatOrInt(1500.).into(),
        }
    }
}

impl DndEdgeWorkspaceSwitch {
    pub fn resolved_trigger_height(&self) -> FloatOrInt<0, 65535> {
        *self.trigger_height.value()
    }

    pub fn resolved_delay_ms(&self) -> u16 {
        *self.delay_ms.value()
    }

    pub fn resolved_max_speed(&self) -> FloatOrInt<0, 1_000_000> {
        *self.max_speed.value()
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
