use anyhow::{bail, Context};
use damask_core::{DamaskId, Fact};
use damask_store::{DamaskProject, FactWriter};
use std::env;

use super::helpers;
use crate::error::Result;
use crate::output::Format;

#[allow(clippy::too_many_arguments)]
pub fn run(
    file: &str,
    start: u32,
    end: u32,
    rel: &str,
    payload: Option<&str>,
    payload_file: Option<&str>,
    stdin: bool,
    summary: Option<&str>,
    confidence: Option<f64>,
    action: Option<&str>,
    severity: Option<&str>,
    fields: &[String],
    tags: &[String],
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
    // A record without any payload source has nothing to say — teach the
    // shortest correct form at the moment of need.
    if payload.is_none() && payload_file.is_none() && !stdin && summary.is_none() {
        bail!(
            "record needs a payload — the simplest form:\n  \
             damask record {file} {start} {end} {rel} -m \"what you found\" -c 0.9"
        );
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

    // Compose payload from JSON sources + flags before building anything
    let payload_value = helpers::compose_payload(
        payload,
        payload_file,
        stdin,
        summary,
        confidence,
        action,
        severity,
        fields,
        tags,
    )?;

    // Namespace schemas assert domain vocabulary — validate before writing.
    let config = project
        .read_config()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    config
        .validate_ns_payload(&ns, &payload_value)
        .map_err(|e| anyhow::anyhow!(e))?;

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
