use miette::{Diagnostic, LabeledSpan, SourceCode, SourceSpan, SpanContents};
use serde::Serialize;

#[derive(Serialize)]
pub struct JsonReport {
    pub message: String,
    pub severity: Severity,
    pub url: Option<String>,
    pub help: Option<String>,
    pub filename: String,
    pub labels: Vec<Label>,
    pub related: Vec<JsonReport>,
}

impl From<&miette::Report> for JsonReport {
    fn from(report: &miette::Report) -> Self {
        JsonReport::from_diagnostic(report.as_ref(), None)
    }
}

impl JsonReport {
    /// Implementation based on [miette::JSONReportHandler::render_report()].
    fn from_diagnostic(diagnostic: &dyn Diagnostic, parent_src: Option<&dyn SourceCode>) -> Self {
        let src = diagnostic.source_code().or(parent_src);
        Self {
            message: diagnostic.to_string(),
            severity: diagnostic
                .severity()
                .map(Into::into)
                .unwrap_or(Severity::Error),
            url: diagnostic.url().as_ref().map(ToString::to_string),
            help: diagnostic.help().as_ref().map(ToString::to_string),
            filename: {
                // If there are no labels available, fall back to a meaningless default span as we
                // **really** just want the file name.
                // (Though if that fails because (0,0) is out of bounds that isn't too bad)
                let span = diagnostic
                    .labels()
                    .as_mut()
                    .and_then(Iterator::next)
                    .unwrap_or(LabeledSpan::new_with_span(None, SourceSpan::from((0, 0))));

                src.and_then(|src| {
                    src.read_span(span.inner(), 0, 0)
                        .ok()
                        .as_deref()
                        .and_then(SpanContents::name)
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_default()
            },
            labels: diagnostic
                .labels()
                .map(|iter| {
                    iter.map(|label| Label {
                        label: label.label().map(ToOwned::to_owned).unwrap_or_default(),
                        span: Span {
                            offset: label.offset(),
                            length: label.len(),
                        },
                    })
                    .collect()
                })
                .unwrap_or_default(),
            related: diagnostic
                .related()
                .map(|iter| {
                    iter.map(|diagnostic| JsonReport::from_diagnostic(diagnostic, src))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Advice,
    Warning,
    Error,
}

impl From<miette::Severity> for Severity {
    fn from(value: miette::Severity) -> Self {
        match value {
            miette::Severity::Advice => Self::Advice,
            miette::Severity::Warning => Self::Warning,
            miette::Severity::Error => Self::Error,
        }
    }
}

#[derive(Serialize)]
pub struct Label {
    pub label: String,
    pub span: Span,
}

#[derive(Serialize)]
pub struct Span {
    pub offset: usize,
    pub length: usize,
}
