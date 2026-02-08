use anyhow::{bail, Context};
use damask_core::{Fact, Span, SpanId};
use damask_store::{DamaskProject, FactWriter};
use std::env;
use std::path::Path;

use crate::error::Result;
use crate::output::Format;

pub fn run(
    file: &str,
    start: u32,
    end: u32,
    symbol: Option<&str>,
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

    let ns = resolve_ns(&project, ns_override)?;

    // Read the file to extract snippet and compute content hash.
    let file_path = project.root.join(file);
    let (snippet, content_hash) = if file_path.exists() {
        extract_span_content(&file_path, start, end)?
    } else {
        (None, None)
    };

    let commit = git_head_commit(&project.root);

    let span = Span {
        id: SpanId::new(),
        path: file.to_string(),
        lines: Some([start, end]),
        snippet,
        symbol: symbol.map(|s| s.to_string()),
        content_hash,
        commit,
        ns: ns.clone(),
        ts: chrono::Utc::now(),
        agent: None,
        session: None,
    };

    let fact = Fact::Span(span.clone());
    let edges_file = project.edges_file(&ns);
    FactWriter::append(&edges_file, &fact).map_err(|e| anyhow::anyhow!("{}", e))?;

    match format {
        Format::Human => {
            println!("{}", crate::output::human::format_span(&span));
        }
        Format::Json => {
            crate::output::json::print_span(&span);
        }
    }

    Ok(())
}

fn resolve_ns(project: &DamaskProject, ns_override: Option<&str>) -> Result<String> {
    if let Some(ns) = ns_override {
        return Ok(ns.to_string());
    }
    project
        .active_ns()
        .ok_or_else(|| anyhow::anyhow!("no active namespace — use `damask ns set <name>` or --ns"))
}

/// Extract the first line as snippet and compute content hash.
fn extract_span_content(
    path: &Path,
    start: u32,
    end: u32,
) -> Result<(Option<String>, Option<String>)> {
    let content = std::fs::read_to_string(path).context("failed to read file")?;
    let lines: Vec<&str> = content.lines().collect();

    let start_idx = (start as usize).saturating_sub(1);
    let end_idx = (end as usize).min(lines.len());

    if start_idx >= lines.len() {
        return Ok((None, None));
    }

    // Snippet: first line, truncated to 80 chars.
    let first_line = lines[start_idx];
    let snippet = if first_line.len() > 80 {
        let mut end = 77;
        while end > 0 && !first_line.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &first_line[..end])
    } else {
        first_line.to_string()
    };

    // Content hash: truncated SHA-256 of span text.
    let span_text: String = lines[start_idx..end_idx].join("\n");
    let hash = damask_resolve::content_hash(&span_text);

    Ok((Some(snippet), Some(hash)))
}

/// Get the current git HEAD commit hash, or None if not in a git repo.
fn git_head_commit(root: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let hash = String::from_utf8(o.stdout).ok()?;
            let trimmed = hash.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
}
