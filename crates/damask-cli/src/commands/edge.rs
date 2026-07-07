use anyhow::Context;
use damask_core::Fact;
use damask_store::{DamaskProject, FactWriter};
use std::env;

use super::helpers;
use crate::error::Result;
use crate::output::Format;

#[allow(clippy::too_many_arguments)]
pub fn run(
    from: &str,
    to: &str,
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
    ns_override: Option<&str>,
    format: Format,
) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let ns = helpers::resolve_ns(&project, ns_override)?;

    let from_id = helpers::parse_endpoint(from).context("invalid 'from' ID")?;
    let to_id = helpers::parse_endpoint(to).context("invalid 'to' ID")?;

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
    let config = project.read_config().map_err(|e| anyhow::anyhow!("{}", e))?;
    config
        .validate_ns_payload(&ns, &payload_value)
        .map_err(|e| anyhow::anyhow!(e))?;

    let edge = helpers::build_edge(from_id, to_id, rel, payload_value, &ns);

    let fact = Fact::Edge(edge.clone());
    let edges_file = project.edges_file(&ns);
    FactWriter::append(&edges_file, &fact).map_err(|e| anyhow::anyhow!("{}", e))?;

    match format {
        Format::Human => {
            println!("{}", crate::output::human::format_edge_created(&edge));
        }
        Format::Json => {
            crate::output::json::print_edge(&edge);
        }
    }

    Ok(())
}
