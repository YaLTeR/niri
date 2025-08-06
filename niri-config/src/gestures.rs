use crate::layout::{DndEdgeViewScroll, DndEdgeWorkspaceSwitch, HotCorners};

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq)]
pub struct Gestures {
    #[knuffel(child, default)]
    pub dnd_edge_view_scroll: DndEdgeViewScroll,
    #[knuffel(child, default)]
    pub dnd_edge_workspace_switch: DndEdgeWorkspaceSwitch,
    #[knuffel(child, default)]
    pub hot_corners: HotCorners,
}
