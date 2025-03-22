//! Config parsing diagnostics.
//!
//! The types in this module are based on `Report` and `Diagnostic` from [miette], which is what
//! niri's config parsing uses.
//!
//! [miette]: https://crates.io/crates/miette

use serde::Serialize;

// Doc-comments in this file are losely based on ones from miette, licensed under Apache-2.0.
//
// https://github.com/zkat/miette

/// Diagnostic from parsing a file.
#[derive(Serialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Diagnostic {
    /// Printable message.
    pub message: String,
    /// Diagnostic severity.
    pub severity: Severity,
    /// URL to visit for a more detailed explanation.
    pub url: Option<String>,
    /// Additional help about this diagnostic.
    pub help: Option<String>,
    /// Name of the file where the diagnostic occurred.
    pub filename: Option<String>,
    /// Labels to apply to this diagnostic's file.
    pub labels: Vec<Label>,
    /// Additional related diagnostics.
    pub related: Vec<Diagnostic>,
}

/// Diagnostic severity.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Severity {
    /// Just some help.
    Advice,
    /// Warning to take note of.
    Warning,
    /// Critical failure.
    Error,
}

/// Labeled [`Span`].
#[derive(Serialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Label {
    /// Label string.
    pub label: String,
    /// Span the label applies to.
    pub span: Span,
}

/// Span within a source file.
#[derive(Serialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Span {
    /// The 0-based starting byte offset.
    pub offset: usize,
    /// Number of bytes this span spans.
    pub length: usize,
    pub start: Option<LineSpan>,
    pub end: Option<LineSpan>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct LineSpan {
    /// The 0-indexed line into the file.
    pub line: usize,
    /// The 0-indexed column into the file, relative to `line`.
    pub col: usize,
}
