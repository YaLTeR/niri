use crate::{BlockOutFrom, CornerRadius, RegexEq, ShadowRule};

#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq)]
pub struct LayerRule {
    #[knuffel(children(name = "match"))]
    pub matches: Vec<Match>,
    #[knuffel(children(name = "exclude"))]
    pub excludes: Vec<Match>,

    #[knuffel(child, unwrap(argument))]
    pub opacity: Option<f32>,
    #[knuffel(child, unwrap(argument))]
    pub block_out_from: Option<BlockOutFrom>,
    #[knuffel(child, default)]
    pub shadow: ShadowRule,
    #[knuffel(child)]
    pub geometry_corner_radius: Option<CornerRadius>,
    #[knuffel(child, unwrap(argument))]
    pub place_within_backdrop: Option<bool>,
}

#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq)]
pub struct Match {
    #[knuffel(property, str)]
    pub namespace: Option<RegexEq>,
    #[knuffel(property)]
    pub at_startup: Option<bool>,
}
