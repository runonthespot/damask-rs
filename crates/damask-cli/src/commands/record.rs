use anyhow::{bail, Context};
use damask_core::{DamaskId, Fact};
use damask_store::{DamaskProject, FactWriter};
use std::env;

use super::helpers;
use crate::error::Result;
use crate::output::Format;

pub fn run(
    file: &str,
    start: u32,
    end: u32,
    rel: &str,
    payload: &str,
    symbol: Option<&str>,
    to: &str,
    ns_override: Option<&str>,
    format: Format,
) -> Result<()> {
    if start > end {
        bail!("start line ({start}) must be <= end line ({end})");
    }
    if start == 0 {
        bail!("lines are 1-indexed; start must be >= 1");
    }

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let ns = helpers::resolve_ns(&project, ns_override)?;

    // Fail early if file doesn't exist
    let file_path = project.root.join(file);
    if !file_path.exists() {
        bail!("file not found: {file}");
    }

    // Parse payload before building anything
    let payload_value: serde_json::Value =
        serde_json::from_str(payload).context("payload is not valid JSON")?;

    // Parse --to endpoint
    let to_id = helpers::parse_endpoint(to).context("invalid '--to' ID")?;

    // Build span
    let span = helpers::build_span(&project, file, start, end, symbol, &ns)?;

    // Build edge from span → to
    let edge = helpers::build_edge(
        Some(DamaskId::Span(span.id.clone())),
        to_id,
        rel,
        payload_value,
        &ns,
    );

    // Atomic write: both facts in a single append_all call
    let facts = vec![Fact::Span(span.clone()), Fact::Edge(edge.clone())];
    let edges_file = project.edges_file(&ns);
    FactWriter::append_all(&edges_file, &facts).map_err(|e| anyhow::anyhow!("{}", e))?;

    // Output
    match format {
        Format::Human => {
            println!("{}", crate::output::human::format_span(&span));
            println!("{}", crate::output::human::format_edge_created(&edge));
        }
        Format::Json => {
            crate::output::json::print_facts(&facts);
        }
    }

    Ok(())
}
