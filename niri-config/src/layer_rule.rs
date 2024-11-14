use crate::{BlockOutFrom, RegexEq};

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
}

#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq)]
pub struct Match {
    #[knuffel(property, str)]
    pub namespace: Option<RegexEq>,
    #[knuffel(property)]
    pub at_startup: Option<bool>,
}
