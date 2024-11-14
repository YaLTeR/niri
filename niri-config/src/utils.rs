use std::str::FromStr;

use regex::Regex;

/// `Regex` that implements `PartialEq` by its string form.
#[derive(Debug, Clone)]
pub struct RegexEq(pub Regex);

impl PartialEq for RegexEq {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for RegexEq {}

impl FromStr for RegexEq {
    type Err = <Regex as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Regex::from_str(s).map(Self)
    }
}
