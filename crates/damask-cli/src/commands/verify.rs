//! Verifiable edges: claims that defend themselves.
//!
//! An edge payload may carry a `check` field — a shell command whose exit
//! code revalidates the claim (a grep, a test invocation, a config probe).
//! `damask verify` runs every check among active open edges and reports
//! pass/fail; with `--auto` it appends endorsement meta-edges for passes and
//! dispute meta-edges for failures, so mechanically-checkable knowledge
//! stays calibrated without an agent in the loop.
//!
//! Checks run with `sh -c` from the project root — the same trust level as
//! a Makefile or package.json script checked into the repo. Verification is
//! always an explicit invocation, never run from a hook.
//!
//! To bound meta-edge growth, `--auto` records each outcome at most once:
//! an edge with an existing auto-endorsement is not re-endorsed on pass,
//! and one with an existing auto-dispute is not re-disputed on failure.

use anyhow::Context;
use damask_core::{DamaskId, Edge, EdgeId, Fact, PayloadEnvelope};
use damask_store::index::query::EdgeRow;
use damask_store::{update_index_with_mode, DamaskProject, FactWriter, IndexMode, IndexQuery};
use std::env;
use std::time::{Duration, Instant};

use crate::error::Result;
use crate::output::Format;

use super::helpers::{ambient_agent, ambient_session};

/// Marker field on auto-generated meta-edges.
const AUTO_MARKER: &str = "check_auto";

#[derive(Debug, PartialEq)]
enum Outcome {
    Passed,
    Failed(Option<i32>),
    TimedOut,
}

struct CheckResult {
    edge_id: String,
    rel: String,
    summary: String,
    check: String,
    outcome: Outcome,
    /// What --auto did: "endorsed", "disputed", or "" (no action / already recorded).
    action: String,
}

pub fn run(auto: bool, timeout_secs: u64, ns_override: Option<&str>, format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let q = IndexQuery::new(&conn);

    let edges = q
        .all_active_open_edges_ns(ns_override)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut results = Vec::new();
    let timeout = Duration::from_secs(timeout_secs);

    for edge in edges {
        let payload: serde_json::Value =
            serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
        let Some(check) = payload.get("check").and_then(|v| v.as_str()) else {
            continue;
        };
        let env_payload = PayloadEnvelope::new(&payload);
        let summary = env_payload
            .summary()
            .map(|s| s.to_string())
            .unwrap_or_default();

        let outcome = run_check(check, &project.root, timeout);

        let action = if auto {
            apply_auto(&q, &project, &edge, check, &outcome)?
        } else {
            String::new()
        };

        results.push(CheckResult {
            edge_id: edge.id.clone(),
            rel: edge.rel.clone(),
            summary,
            check: check.to_string(),
            outcome,
            action,
        });
    }

    match format {
        Format::Human => print_human(&results, auto),
        Format::Json => print_json(&results),
    }

    let any_failed = results
        .iter()
        .any(|r| !matches!(r.outcome, Outcome::Passed));
    if any_failed {
        std::process::exit(1);
    }
    Ok(())
}

/// Run a check command with a timeout, polling rather than blocking so a
/// hung check can't wedge the CLI.
fn run_check(check: &str, root: &std::path::Path, timeout: Duration) -> Outcome {
    let child = std::process::Command::new("sh")
        .arg("-c")
        .arg(check)
        .current_dir(root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return Outcome::Failed(None);
    };

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Outcome::Passed
                } else {
                    Outcome::Failed(status.code())
                };
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Outcome::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return Outcome::Failed(None),
        }
    }
}

/// Record the outcome as a meta-edge, once per outcome kind per edge.
fn apply_auto(
    q: &IndexQuery,
    project: &DamaskProject,
    edge: &EdgeRow,
    check: &str,
    outcome: &Outcome,
) -> Result<String> {
    let (meta_rel, summary) = match outcome {
        Outcome::Passed => (
            "endorsed",
            format!("check passed: `{}`", damask_core::truncate_str(check, 80)),
        ),
        Outcome::Failed(code) => (
            "disputed",
            format!(
                "check failed (exit {}): `{}`",
                code.map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
                damask_core::truncate_str(check, 80)
            ),
        ),
        Outcome::TimedOut => (
            "disputed",
            format!("check timed out: `{}`", damask_core::truncate_str(check, 80)),
        ),
    };

    // Already recorded this outcome kind automatically? Don't pile on.
    // Meta-edges are inactive in the index, so this must use
    // edges_targeting (which includes them), not edges_from.
    let existing = q
        .edges_targeting(&edge.id)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let already = existing.iter().any(|m| {
        m.rel == meta_rel
            && serde_json::from_str::<serde_json::Value>(&m.payload)
                .ok()
                .and_then(|p| p.get(AUTO_MARKER).and_then(|v| v.as_bool()))
                .unwrap_or(false)
    });
    if already {
        return Ok(String::new());
    }

    let target = DamaskId::parse(&edge.id).map_err(|e| anyhow::anyhow!("{}", e))?;
    let meta = Edge {
        id: EdgeId::new(),
        from: Some(target),
        to: None,
        rel: meta_rel.to_string(),
        payload: serde_json::json!({ "summary": summary, AUTO_MARKER: true }),
        ns: edge.ns.clone(),
        ts: chrono::Utc::now(),
        agent: ambient_agent().or_else(|| Some("damask-verify".to_string())),
        session: ambient_session(),
    };
    let edges_file = project.edges_file(&edge.ns);
    FactWriter::append(&edges_file, &Fact::Edge(meta)).map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(meta_rel.to_string())
}

fn print_human(results: &[CheckResult], auto: bool) {
    if results.is_empty() {
        println!("No checkable edges (no active edge has a `check` payload field).");
        return;
    }
    println!();
    for r in results {
        let (glyph, label) = match r.outcome {
            Outcome::Passed => ("\u{2713}", "pass".to_string()),
            Outcome::Failed(code) => (
                "\u{2717}",
                format!("fail (exit {})", code.map(|c| c.to_string()).unwrap_or_else(|| "?".into())),
            ),
            Outcome::TimedOut => ("\u{2717}", "timeout".to_string()),
        };
        let action = if r.action.is_empty() {
            String::new()
        } else {
            format!(" \u{2192} {}", r.action)
        };
        println!("  {glyph} {label}{action}  [{}] {}", r.rel, r.summary);
        println!("      {}  check: {}", r.edge_id, r.check);
    }
    let passed = results
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Passed))
        .count();
    println!();
    println!("  {passed}/{} checks passed{}", results.len(), if auto { " (--auto applied)" } else { "" });
}

fn print_json(results: &[CheckResult]) {
    let items: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let (status, exit_code) = match r.outcome {
                Outcome::Passed => ("passed", None),
                Outcome::Failed(code) => ("failed", code),
                Outcome::TimedOut => ("timeout", None),
            };
            serde_json::json!({
                "edge_id": r.edge_id,
                "rel": r.rel,
                "summary": r.summary,
                "check": r.check,
                "status": status,
                "exit_code": exit_code,
                "auto_action": if r.action.is_empty() { None } else { Some(&r.action) },
            })
        })
        .collect();
    let passed = results
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Passed))
        .count();
    let output = serde_json::json!({
        "total": results.len(),
        "passed": passed,
        "failed": results.len() - passed,
        "results": items,
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
