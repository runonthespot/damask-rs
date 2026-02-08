use damask_core::{Edge, Span};

/// Print a span as JSON to stdout.
pub fn print_span(span: &Span) {
    let json = serde_json::to_string_pretty(span).expect("span serialization failed");
    println!("{json}");
}

/// Print an edge as JSON to stdout.
pub fn print_edge(edge: &Edge) {
    let json = serde_json::to_string_pretty(edge).expect("edge serialization failed");
    println!("{json}");
}
