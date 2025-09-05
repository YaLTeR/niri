use knuffel::errors::DecodeError;
use niri_ipc::{ColumnDisplay, SizeChange};
use niri_macros::Mergeable;

use crate::appearance::{
    Border, FocusRing, InsertHint, Shadow, TabIndicator, DEFAULT_BACKGROUND_COLOR,
};
use crate::utils::expect_only_children;
use crate::{Color, FloatOrInt};

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Mergeable)]
pub struct Layout {
    #[knuffel(child, default)]
    pub focus_ring: FocusRing,
    #[knuffel(child, default)]
    pub border: Border,
    #[knuffel(child, default)]
    pub shadow: Shadow,
    #[knuffel(child, default)]
    pub tab_indicator: TabIndicator,
    #[knuffel(child, default)]
    pub insert_hint: InsertHint,
    #[knuffel(child, unwrap(children), default)]
    pub preset_column_widths: Vec<PresetSize>,
    #[knuffel(child)]
    pub default_column_width: Option<DefaultPresetSize>,
    #[knuffel(child, unwrap(children), default)]
    pub preset_window_heights: Vec<PresetSize>,
    #[knuffel(child, unwrap(argument), default)]
    pub center_focused_column: CenterFocusedColumn,
    #[knuffel(child)]
    pub always_center_single_column: bool,
    #[knuffel(child)]
    pub empty_workspace_above_first: bool,
    #[knuffel(child, unwrap(argument, str), default = Self::default().default_column_display)]
    pub default_column_display: ColumnDisplay,
    #[knuffel(child, unwrap(argument), default = Self::default().gaps)]
    pub gaps: FloatOrInt<0, 65535>,
    #[knuffel(child, default)]
    pub struts: Struts,
    #[knuffel(child, default = DEFAULT_BACKGROUND_COLOR)]
    pub background_color: Color,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            focus_ring: Default::default(),
            border: Default::default(),
            shadow: Default::default(),
            tab_indicator: Default::default(),
            insert_hint: Default::default(),
            preset_column_widths: Default::default(),
            default_column_width: Default::default(),
            center_focused_column: Default::default(),
            always_center_single_column: false,
            empty_workspace_above_first: false,
            default_column_display: ColumnDisplay::Normal,
            gaps: FloatOrInt(16.),
            struts: Default::default(),
            preset_window_heights: Default::default(),
            background_color: DEFAULT_BACKGROUND_COLOR,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq, Mergeable)]
pub enum PresetSize {
    Proportion(#[knuffel(argument)] f64),
    Fixed(#[knuffel(argument)] i32),
}

impl From<PresetSize> for SizeChange {
    fn from(value: PresetSize) -> Self {
        match value {
            PresetSize::Proportion(prop) => SizeChange::SetProportion(prop * 100.),
            PresetSize::Fixed(fixed) => SizeChange::SetFixed(fixed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Mergeable)]
pub struct DefaultPresetSize(pub Option<PresetSize>);

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Mergeable)]
pub struct Struts {
    #[knuffel(child, unwrap(argument), default)]
    pub left: FloatOrInt<-65535, 65535>,
    #[knuffel(child, unwrap(argument), default)]
    pub right: FloatOrInt<-65535, 65535>,
    #[knuffel(child, unwrap(argument), default)]
    pub top: FloatOrInt<-65535, 65535>,
    #[knuffel(child, unwrap(argument), default)]
    pub bottom: FloatOrInt<-65535, 65535>,
}

#[derive(knuffel::DecodeScalar, Debug, Default, PartialEq, Eq, Clone, Copy, Mergeable)]
pub enum CenterFocusedColumn {
    /// Focusing a column will not center the column.
    #[default]
    Never,
    /// The focused column will always be centered.
    Always,
    /// Focusing a column will center it if it doesn't fit on the screen together with the
    /// previously focused column.
    OnOverflow,
}

impl<S> knuffel::Decode<S> for DefaultPresetSize
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        expect_only_children(node, ctx);

        let mut children = node.children();

        if let Some(child) = children.next() {
            if let Some(unwanted_child) = children.next() {
                ctx.emit_error(DecodeError::unexpected(
                    unwanted_child,
                    "node",
                    "expected no more than one child",
                ));
            }
            PresetSize::decode_node(child, ctx).map(Some).map(Self)
        } else {
            Ok(Self(None))
        }
    }
}
