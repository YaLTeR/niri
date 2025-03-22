use miette::{LabeledSpan, SourceCode, SourceSpan, SpanContents};
use niri_ipc::diagnostic::{Diagnostic, Label, LineSpan, Severity, Span};

pub fn convert_to_ipc(diagnostic: &dyn miette::Diagnostic) -> Diagnostic {
    diagnostic_to_ipc(diagnostic, None)
}

/// Implementation based on [`miette::JSONReportHandler::render_report()`].
fn diagnostic_to_ipc(
    diagnostic: &dyn miette::Diagnostic,
    parent_src: Option<&dyn SourceCode>,
) -> Diagnostic {
    let src = diagnostic.source_code().or(parent_src);
    Diagnostic {
        message: diagnostic.to_string(),
        severity: diagnostic
            .severity()
            .map(convert_severity)
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
        },
        labels: diagnostic
            .labels()
            .map(|iter| {
                iter.map(|label| Label {
                    label: label.label().map(ToOwned::to_owned).unwrap_or_default(),
                    span: Span {
                        offset: label.offset(),
                        length: label.len(),
                        start: line_span_from_label(src, label.inner()),
                        // Because miette doesn't just give us the ending line + column for
                        // some reason.
                        end: line_span_from_label(src, &{
                            let label = label.inner();
                            SourceSpan::from(label.len() + label.offset())
                        }),
                    },
                })
                .collect()
            })
            .unwrap_or_default(),
        related: diagnostic
            .related()
            .map(|iter| {
                iter.map(|diagnostic| diagnostic_to_ipc(diagnostic, src))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn convert_severity(value: miette::Severity) -> Severity {
    match value {
        miette::Severity::Advice => Severity::Advice,
        miette::Severity::Warning => Severity::Warning,
        miette::Severity::Error => Severity::Error,
    }
}

fn line_span_from_label<S: SourceCode + ?Sized>(
    code: Option<&S>,
    span: &SourceSpan,
) -> Option<LineSpan> {
    let code = code?;
    let content = code.read_span(span, 0, 0).ok()?;
    Some(LineSpan {
        line: content.line(),
        col: content.column(),
    })
}
