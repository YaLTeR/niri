use serde::Serialize;

/// Helper type to return `miette::Report`s in a machine readable format
///
/// See `niri_config::json_report::convert_to_json()` for how to construct this.
#[derive(Serialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct JsonReport {
    pub message: String,
    pub severity: Severity,
    pub url: Option<String>,
    pub help: Option<String>,
    pub filename: String,
    pub labels: Vec<Label>,
    pub related: Vec<JsonReport>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Severity {
    Advice,
    Warning,
    Error,
}

#[derive(Serialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Label {
    pub label: String,
    pub span: Span,
}

#[derive(Serialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Span {
    pub offset: usize,
    pub length: usize,
    pub start: Option<LineSpan>,
    pub end: Option<LineSpan>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct LineSpan {
    /// 0-indexed line into the file
    pub line: usize,
    /// 0-indexed column into the file
    pub col: usize,
}
