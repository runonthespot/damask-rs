use damask_core::{Edge, Span};

/// Format a span for human-readable output.
pub fn format_span(span: &Span) -> String {
    let lines = match span.lines {
        Some([s, e]) => format!(":{}-{}", s, e),
        None => String::new(),
    };
    let snippet = span
        .snippet
        .as_deref()
        .map(|s| format!(" — \"{}\"", damask_core::truncate_str(s, 60)))
        .unwrap_or_default();
    format!("{}{} ({}){}", span.path, lines, span.id, snippet)
}

/// Format an edge creation confirmation.
pub fn format_edge_created(edge: &Edge) -> String {
    let from = edge
        .from
        .as_ref()
        .map(|id| id.to_string())
        .unwrap_or_else(|| "_".to_string());
    let to = edge
        .to
        .as_ref()
        .map(|id| id.to_string())
        .unwrap_or_else(|| "_".to_string());
    format!("{} ({} → {} [{}])", edge.id, from, to, edge.rel)
}
