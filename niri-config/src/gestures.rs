use crate::layout::{DndEdgeViewScroll, DndEdgeWorkspaceSwitch, HotCorners};

#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq)]
pub struct Gestures {
    #[knuffel(child, default)]
    pub dnd_edge_view_scroll: DndEdgeViewScroll,
    #[knuffel(child, default)]
    pub dnd_edge_workspace_switch: DndEdgeWorkspaceSwitch,
    #[knuffel(child, default)]
    pub hot_corners: HotCorners,
}

impl Gestures {
    pub fn merge_with(&mut self, other: &Self) {
        self.dnd_edge_view_scroll
            .merge_with(&other.dnd_edge_view_scroll);
        self.dnd_edge_workspace_switch
            .merge_with(&other.dnd_edge_workspace_switch);
        self.hot_corners.merge_with(&other.hot_corners);
    }
}
