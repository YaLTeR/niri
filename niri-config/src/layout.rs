use knuffel::errors::DecodeError;
use niri_ipc::{ColumnDisplay, SizeChange};

use crate::appearance::{
    Border, FocusRing, InsertHint, Shadow, TabIndicator, WorkspaceShadow, DEFAULT_BACKDROP_COLOR,
    DEFAULT_BACKGROUND_COLOR,
};
use crate::core::{FloatOrInt, MaybeSet};
use crate::utils::expect_only_children;
use crate::Color;

#[derive(knuffel::DecodeScalar, Debug, Default, PartialEq, Eq, Clone, Copy)]
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

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
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
    #[knuffel(child, unwrap(argument, str), default = ColumnDisplay::Normal)]
    pub default_column_display: ColumnDisplay,
    #[knuffel(child, unwrap(argument), default)]
    pub gaps: MaybeSet<FloatOrInt<0, 65535>>,
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
            preset_window_heights: Default::default(),
            center_focused_column: Default::default(),
            always_center_single_column: false,
            empty_workspace_above_first: false,
            default_column_display: ColumnDisplay::Normal,
            gaps: MaybeSet::unset(FloatOrInt(16.)),
            struts: Default::default(),
            background_color: DEFAULT_BACKGROUND_COLOR,
        }
    }
}

impl Layout {
    pub fn merge_with(&mut self, other: &Self) {
        let default = Self::default();
        if other.focus_ring != default.focus_ring {
            self.focus_ring = other.focus_ring;
        }
        if other.border != default.border {
            self.border = other.border;
        }
        if other.shadow != default.shadow {
            self.shadow = other.shadow;
        }
        if other.tab_indicator != default.tab_indicator {
            self.tab_indicator = other.tab_indicator;
        }
        if other.insert_hint != default.insert_hint {
            self.insert_hint = other.insert_hint;
        }
        self.preset_column_widths
            .extend_from_slice(&other.preset_column_widths);
        if other.default_column_width != default.default_column_width {
            self.default_column_width = other.default_column_width;
        }
        self.preset_window_heights
            .extend_from_slice(&other.preset_window_heights);
        if other.center_focused_column != default.center_focused_column {
            self.center_focused_column = other.center_focused_column;
        }
        if other.always_center_single_column != default.always_center_single_column {
            self.always_center_single_column = other.always_center_single_column;
        }
        if other.empty_workspace_above_first != default.empty_workspace_above_first {
            self.empty_workspace_above_first = other.empty_workspace_above_first;
        }
        if other.default_column_display != default.default_column_display {
            self.default_column_display = other.default_column_display;
        }
        if other.gaps.is_set() {
            self.gaps = other.gaps.clone();
        }
        self.struts.merge_with(&other.struts);
        if other.background_color != default.background_color {
            self.background_color = other.background_color;
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefaultPresetSize(pub Option<PresetSize>);

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

impl Default for DefaultPresetSize {
    fn default() -> Self {
        Self(None)
    }
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq)]
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

impl Struts {
    pub fn merge_with(&mut self, other: &Self) {
        let default = Self::default();
        if other.left != default.left {
            self.left = other.left;
        }
        if other.right != default.right {
            self.right = other.right;
        }
        if other.top != default.top {
            self.top = other.top;
        }
        if other.bottom != default.bottom {
            self.bottom = other.bottom;
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct DndEdgeViewScroll {
    #[knuffel(child, unwrap(argument), default)]
    pub trigger_width: MaybeSet<FloatOrInt<0, 65535>>,
    #[knuffel(child, unwrap(argument), default)]
    pub delay_ms: MaybeSet<u16>,
    #[knuffel(child, unwrap(argument), default)]
    pub max_speed: MaybeSet<FloatOrInt<0, 1_000_000>>,
}

impl Default for DndEdgeViewScroll {
    fn default() -> Self {
        Self {
            trigger_width: MaybeSet::unset(FloatOrInt(30.)),
            delay_ms: MaybeSet::unset(100),
            max_speed: MaybeSet::unset(FloatOrInt(1500.)),
        }
    }
}

impl DndEdgeViewScroll {
    pub fn merge_with(&mut self, other: &Self) {
        if other.trigger_width.is_set() {
            self.trigger_width = other.trigger_width.clone();
        }
        if other.delay_ms.is_set() {
            self.delay_ms = other.delay_ms.clone();
        }
        if other.max_speed.is_set() {
            self.max_speed = other.max_speed.clone();
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct DndEdgeWorkspaceSwitch {
    #[knuffel(child, unwrap(argument), default)]
    pub trigger_height: MaybeSet<FloatOrInt<0, 65535>>,
    #[knuffel(child, unwrap(argument), default)]
    pub delay_ms: MaybeSet<u16>,
    #[knuffel(child, unwrap(argument), default)]
    pub max_speed: MaybeSet<FloatOrInt<0, 1_000_000>>,
}

impl Default for DndEdgeWorkspaceSwitch {
    fn default() -> Self {
        Self {
            trigger_height: MaybeSet::unset(FloatOrInt(50.)),
            delay_ms: MaybeSet::unset(100),
            max_speed: MaybeSet::unset(FloatOrInt(1500.)),
        }
    }
}

impl DndEdgeWorkspaceSwitch {
    pub fn merge_with(&mut self, other: &Self) {
        if other.trigger_height.is_set() {
            self.trigger_height = other.trigger_height.clone();
        }
        if other.delay_ms.is_set() {
            self.delay_ms = other.delay_ms.clone();
        }
        if other.max_speed.is_set() {
            self.max_speed = other.max_speed.clone();
        }
    }
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq)]
pub struct HotCorners {
    #[knuffel(child)]
    pub off: bool,
}

impl HotCorners {
    pub fn merge_with(&mut self, other: &Self) {
        let default = Self::default();
        if other.off != default.off {
            self.off = other.off;
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct Overview {
    #[knuffel(child, unwrap(argument), default = Self::default().zoom)]
    pub zoom: FloatOrInt<0, 1>,
    #[knuffel(child, default = Self::default().backdrop_color)]
    pub backdrop_color: Color,
    #[knuffel(child, default)]
    pub workspace_shadow: WorkspaceShadow,
}

impl Default for Overview {
    fn default() -> Self {
        Self {
            zoom: FloatOrInt(0.5),
            backdrop_color: DEFAULT_BACKDROP_COLOR,
            workspace_shadow: WorkspaceShadow::default(),
        }
    }
}

impl Overview {
    pub fn merge_with(&mut self, other: &Self) {
        let default = Self::default();
        if other.zoom != default.zoom {
            self.zoom = other.zoom;
        }
        if other.backdrop_color != default.backdrop_color {
            self.backdrop_color = other.backdrop_color;
        }
        if other.workspace_shadow != default.workspace_shadow {
            self.workspace_shadow = other.workspace_shadow;
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CornerRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl From<CornerRadius> for [f32; 4] {
    fn from(value: CornerRadius) -> Self {
        [
            value.top_left,
            value.top_right,
            value.bottom_right,
            value.bottom_left,
        ]
    }
}

impl From<f32> for CornerRadius {
    fn from(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }
}

impl CornerRadius {
    pub fn fit_to(self, width: f32, height: f32) -> Self {
        // Like in CSS: https://drafts.csswg.org/css-backgrounds/#corner-overlap
        let reduction = f32::min(
            f32::min(
                width / (self.top_left + self.top_right),
                width / (self.bottom_left + self.bottom_right),
            ),
            f32::min(
                height / (self.top_left + self.bottom_left),
                height / (self.top_right + self.bottom_right),
            ),
        );
        let reduction = f32::min(1., reduction);

        Self {
            top_left: self.top_left * reduction,
            top_right: self.top_right * reduction,
            bottom_right: self.bottom_right * reduction,
            bottom_left: self.bottom_left * reduction,
        }
    }

    pub fn at_least(self, min: f32) -> Self {
        Self {
            top_left: self.top_left.max(min),
            top_right: self.top_right.max(min),
            bottom_right: self.bottom_right.max(min),
            bottom_left: self.bottom_left.max(min),
        }
    }

    pub fn expanded_by(self, amount: f32) -> Self {
        Self {
            top_left: self.top_left + amount,
            top_right: self.top_right + amount,
            bottom_right: self.bottom_right + amount,
            bottom_left: self.bottom_left + amount,
        }
    }

    pub fn is_zero(self) -> bool {
        self.top_left == 0.
            && self.top_right == 0.
            && self.bottom_right == 0.
            && self.bottom_left == 0.
    }

    pub fn merge_with(&mut self, other: &Self) {
        if other.top_left != CornerRadius::default().top_left {
            self.top_left = other.top_left;
        }
        if other.top_right != CornerRadius::default().top_right {
            self.top_right = other.top_right;
        }
        if other.bottom_right != CornerRadius::default().bottom_right {
            self.bottom_right = other.bottom_right;
        }
        if other.bottom_left != CornerRadius::default().bottom_left {
            self.bottom_left = other.bottom_left;
        }
    }
}

impl<S> knuffel::Decode<S> for CornerRadius
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        // Check for unexpected type name.
        if let Some(type_name) = &node.type_name {
            ctx.emit_error(DecodeError::unexpected(
                type_name,
                "type name",
                "no type name expected for this node",
            ));
        }

        let decode_radius = |ctx: &mut knuffel::decode::Context<S>,
                             val: &knuffel::ast::Value<S>| {
            // Check for unexpected type name.
            if let Some(typ) = &val.type_name {
                ctx.emit_error(DecodeError::TypeName {
                    span: typ.span().clone(),
                    found: Some((**typ).clone()),
                    expected: knuffel::errors::ExpectedType::no_type(),
                    rust_type: "str",
                });
            }

            // Decode both integers and floats.
            let radius = match *val.literal {
                knuffel::ast::Literal::Int(ref x) => f32::from(match x.try_into() {
                    Ok(x) => x,
                    Err(err) => {
                        ctx.emit_error(DecodeError::conversion(&val.literal, err));
                        0i16
                    }
                }),
                knuffel::ast::Literal::Decimal(ref x) => match x.try_into() {
                    Ok(x) => x,
                    Err(err) => {
                        ctx.emit_error(DecodeError::conversion(&val.literal, err));
                        0.
                    }
                },
                _ => {
                    ctx.emit_error(DecodeError::scalar_kind(
                        knuffel::decode::Kind::Int,
                        &val.literal,
                    ));
                    0.
                }
            };

            if radius < 0. {
                ctx.emit_error(DecodeError::conversion(&val.literal, "radius must be >= 0"));
            }

            radius
        };

        // Get the first argument.
        let mut iter_args = node.arguments.iter();
        let val = iter_args
            .next()
            .ok_or_else(|| DecodeError::missing(node, "additional argument is required"))?;

        let top_left = decode_radius(ctx, val);

        let mut rv = CornerRadius {
            top_left,
            top_right: top_left,
            bottom_right: top_left,
            bottom_left: top_left,
        };

        if let Some(val) = iter_args.next() {
            rv.top_right = decode_radius(ctx, val);

            let val = iter_args.next().ok_or_else(|| {
                DecodeError::missing(node, "either 1 or 4 arguments are required")
            })?;
            rv.bottom_right = decode_radius(ctx, val);

            let val = iter_args.next().ok_or_else(|| {
                DecodeError::missing(node, "either 1 or 4 arguments are required")
            })?;
            rv.bottom_left = decode_radius(ctx, val);

            // Check for unexpected following arguments.
            if let Some(val) = iter_args.next() {
                ctx.emit_error(DecodeError::unexpected(
                    &val.literal,
                    "argument",
                    "unexpected argument",
                ));
            }
        }

        // Check for unexpected properties and children.
        for name in node.properties.keys() {
            ctx.emit_error(DecodeError::unexpected(
                name,
                "property",
                format!("unexpected property `{}`", name.escape_default()),
            ));
        }
        for child in node.children.as_ref().map(|lst| &lst[..]).unwrap_or(&[]) {
            ctx.emit_error(DecodeError::unexpected(
                child,
                "node",
                format!("unexpected node `{}`", child.node_name.escape_default()),
            ));
        }

        Ok(rv)
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct FloatingPosition {
    #[knuffel(property)]
    pub x: FloatOrInt<-65535, 65535>,
    #[knuffel(property)]
    pub y: FloatOrInt<-65535, 65535>,
    #[knuffel(property, default)]
    pub relative_to: RelativeTo,
}

impl Default for FloatingPosition {
    fn default() -> Self {
        Self {
            x: FloatOrInt(0.),
            y: FloatOrInt(0.),
            relative_to: RelativeTo::default(),
        }
    }
}

impl FloatingPosition {
    pub fn merge_with(&mut self, other: &Self) {
        let default = Self::default();
        if other.x != default.x {
            self.x = other.x;
        }
        if other.y != default.y {
            self.y = other.y;
        }
        if other.relative_to != default.relative_to {
            self.relative_to = other.relative_to;
        }
    }
}

#[derive(knuffel::DecodeScalar, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RelativeTo {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    Top,
    Bottom,
    Left,
    Right,
}
