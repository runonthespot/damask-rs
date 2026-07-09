use anyhow::{bail, Context};
use damask_core::{DamaskId, Fact};
use damask_store::{DamaskProject, FactWriter};
use serde::Deserialize;
use std::env;

use super::helpers;
use crate::error::Result;
use crate::output::Format;

#[derive(Deserialize)]
#[serde(untagged)]
enum BatchItem {
    SpanItem { span: SpanInstruction },
    EdgeItem { edge: EdgeInstruction },
}

#[derive(Deserialize)]
struct SpanInstruction {
    path: String,
    start: u32,
    end: u32,
    #[serde(default)]
    symbol: Option<String>,
}

#[derive(Deserialize)]
struct EdgeInstruction {
    from: String,
    to: String,
    rel: String,
    #[serde(default = "default_payload")]
    payload: serde_json::Value,
}

fn default_payload() -> serde_json::Value {
    serde_json::json!({})
}

pub fn run(
    stdin: bool,
    file: Option<&str>,
    ns_override: Option<&str>,
    format: Format,
) -> Result<()> {
    let input = read_input(stdin, file)?;

    let items: Vec<BatchItem> = serde_json::from_str(&input)
        .context("batch input is not a valid JSON array of span/edge instructions")?;

    if items.is_empty() {
        bail!("batch is empty — nothing to create");
    }

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let ns = helpers::resolve_ns(&project, ns_override)?;

    // Phase 1: Validate all items before writing anything
    for (i, item) in items.iter().enumerate() {
        match item {
            BatchItem::SpanItem { span } => {
                if span.start > span.end {
                    bail!(
                        "batch[{i}]: start line ({}) must be <= end line ({})",
                        span.start,
                        span.end
                    );
                }
                if span.start == 0 {
                    bail!("batch[{i}]: lines are 1-indexed; start must be >= 1");
                }
                let file_path = project.root.join(&span.path);
                if !file_path.exists() {
                    bail!("batch[{i}]: file not found: {}", span.path);
                }
            }
            BatchItem::EdgeItem { edge } => {
                validate_ref(&edge.from, i, "from")?;
                validate_ref(&edge.to, i, "to")?;
            }
        }
    }

    // Phase 2: Build all facts sequentially
    let config = project
        .read_config()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut facts: Vec<Fact> = Vec::with_capacity(items.len());
    let mut created_ids: Vec<DamaskId> = Vec::with_capacity(items.len());

    for (i, item) in items.iter().enumerate() {
        match item {
            BatchItem::SpanItem { span: inst } => {
                let span = helpers::build_span(
                    &project,
                    &inst.path,
                    inst.start,
                    inst.end,
                    inst.symbol.as_deref(),
                    &ns,
                )?;
                let id = DamaskId::Span(span.id.clone());
                facts.push(Fact::Span(span));
                created_ids.push(id);
            }
            BatchItem::EdgeItem { edge: inst } => {
                let from_id = resolve_ref(&inst.from, &created_ids, i)?;
                let to_id = resolve_ref(&inst.to, &created_ids, i)?;
                helpers::validate_payload(&inst.payload)
                    .map_err(|e| anyhow::anyhow!("item {i}: {e}"))?;
                config
                    .validate_ns_payload(&ns, &inst.payload)
                    .map_err(|e| anyhow::anyhow!("item {i}: {e}"))?;
                let edge =
                    helpers::build_edge(from_id, to_id, &inst.rel, inst.payload.clone(), &ns);
                let id = DamaskId::Edge(edge.id.clone());
                facts.push(Fact::Edge(edge));
                created_ids.push(id);
            }
        }
    }

    // Phase 3: Atomic write
    let edges_file = project.edges_file(&ns);
    FactWriter::append_all(&edges_file, &facts).map_err(|e| anyhow::anyhow!("{}", e))?;

    // Phase 4: Output
    match format {
        Format::Human => {
            for fact in &facts {
                match fact {
                    Fact::Span(s) => println!("{}", crate::output::human::format_span(s)),
                    Fact::Edge(e) => println!("{}", crate::output::human::format_edge_created(e)),
                }
            }
            println!("\n{} facts created.", facts.len());
        }
        Format::Json => {
            crate::output::json::print_facts(&facts);
        }
    }

    Ok(())
}

fn read_input(stdin: bool, file: Option<&str>) -> anyhow::Result<String> {
    if let Some(path) = file {
        return std::fs::read_to_string(path).context(format!("failed to read batch file: {path}"));
    }
    if stdin {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .context("failed to read from stdin")?;
        return Ok(buf);
    }
    bail!("specify --stdin or --file <path> to provide batch input");
}

/// Validate a back-reference or literal endpoint during Phase 1.
fn validate_ref(s: &str, current_index: usize, field: &str) -> anyhow::Result<()> {
    if s == "_" {
        return Ok(());
    }
    if let Some(idx_str) = s.strip_prefix('$') {
        let idx: usize = idx_str.parse().context(format!(
            "batch[{current_index}].{field}: invalid back-reference: {s}"
        ))?;
        if idx >= current_index {
            bail!(
                "batch[{current_index}].{field}: back-reference ${idx} must refer to an earlier item (0..{current_index})"
            );
        }
        return Ok(());
    }
    // Literal ID — validate it parses. Spell out the accepted forms:
    // agents working from garbled examples (e.g. "from":"record") must be
    // able to self-correct from this message alone.
    DamaskId::parse(s).map_err(|e| {
        anyhow::anyhow!(
            "batch[{current_index}].{field}: \"{s}\" is not a valid endpoint ({e}). \
             Use \"$N\" to reference the fact at index N in this batch, \"_\" for null, \
             or a literal span/edge ID (s_…/e_…). Example: {{\"edge\": {{\"from\":\"$0\", \
             \"to\":\"_\", \"rel\":\"risk\", \"payload\":{{…}}}}}}. See `damask help batch`."
        )
    })?;
    Ok(())
}

/// Resolve a back-reference, literal ID, or "_" to an Option<DamaskId>.
fn resolve_ref(
    s: &str,
    created_ids: &[DamaskId],
    _current_index: usize,
) -> anyhow::Result<Option<DamaskId>> {
    if s == "_" {
        return Ok(None);
    }
    if let Some(idx_str) = s.strip_prefix('$') {
        let idx: usize = idx_str.parse()?;
        return Ok(Some(created_ids[idx].clone()));
    }
    let id = DamaskId::parse(s).map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(Some(id))
}
