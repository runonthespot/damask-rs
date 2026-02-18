use anyhow::{bail, Context};
use damask_core::{DamaskId, Edge, EdgeId, Span, SpanId};
use damask_store::DamaskProject;
use std::path::Path;

use crate::error::Result;

/// Resolve the active namespace from an override flag, env var, or project config.
pub fn resolve_ns(project: &DamaskProject, ns_override: Option<&str>) -> Result<String> {
    if let Some(ns) = ns_override {
        return Ok(ns.to_string());
    }
    project
        .active_ns()
        .ok_or_else(|| anyhow::anyhow!("no active namespace — use `damask ns set <name>` or --ns"))
}

/// Extract snippet (first line, truncated to 80 chars) and content hash from a file region.
pub fn extract_span_content(
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
pub fn git_head_commit(root: &Path) -> Option<String> {
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

/// Parse an endpoint string: "_" → None, otherwise parse as DamaskId.
pub fn parse_endpoint(s: &str) -> Result<Option<DamaskId>> {
    if s == "_" {
        Ok(None)
    } else {
        let id = DamaskId::parse(s)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context(format!("'{s}' is not a valid span or edge ID"))?;
        Ok(Some(id))
    }
}

/// Resolve the payload from inline JSON, a file, stdin, or default to empty object.
pub fn resolve_payload(
    inline: Option<&str>,
    file: Option<&str>,
    stdin: bool,
) -> Result<serde_json::Value> {
    let source_count = inline.is_some() as u8 + file.is_some() as u8 + stdin as u8;
    if source_count > 1 {
        bail!("cannot specify more than one of: inline payload, --file, --stdin");
    }

    if let Some(path) = file {
        let content = std::fs::read_to_string(path)
            .context(format!("failed to read payload file: {path}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&content).context("payload file is not valid JSON")?;
        return Ok(value);
    }

    if stdin {
        let mut content = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut content)
            .context("failed to read from stdin")?;
        let value: serde_json::Value =
            serde_json::from_str(&content).context("stdin is not valid JSON")?;
        return Ok(value);
    }

    if let Some(json_str) = inline {
        let value: serde_json::Value =
            serde_json::from_str(json_str).context("payload is not valid JSON")?;
        return Ok(value);
    }

    Ok(serde_json::json!({}))
}

/// Build a complete Span from parameters, computing content hash and git commit.
pub fn build_span(
    project: &DamaskProject,
    file: &str,
    start: u32,
    end: u32,
    symbol: Option<&str>,
    ns: &str,
) -> Result<Span> {
    let file_path = project.root.join(file);
    let (snippet, content_hash) = if file_path.exists() {
        extract_span_content(&file_path, start, end)?
    } else {
        (None, None)
    };

    let commit = git_head_commit(&project.root);

    Ok(Span {
        id: SpanId::new(),
        path: file.to_string(),
        lines: Some([start, end]),
        snippet,
        symbol: symbol.map(|s| s.to_string()),
        content_hash,
        commit,
        ns: ns.to_string(),
        ts: chrono::Utc::now(),
        agent: None,
        session: None,
    })
}

/// Build a complete Edge from parameters.
pub fn build_edge(
    from: Option<DamaskId>,
    to: Option<DamaskId>,
    rel: &str,
    payload: serde_json::Value,
    ns: &str,
) -> Edge {
    Edge {
        id: EdgeId::new(),
        from,
        to,
        rel: rel.to_string(),
        payload,
        ns: ns.to_string(),
        ts: chrono::Utc::now(),
        agent: None,
        session: None,
    }
}
