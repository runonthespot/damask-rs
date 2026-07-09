//! Deterministic session-1 seeding.
//!
//! An empty graph returns nothing, so the loop's first session pays full
//! exploration cost with zero payoff — the chicken-and-egg adoption
//! killer. `bootstrap` crosses zero→nonzero mechanically (no LLM): project
//! manifests become `describes` edges, TODO/FIXME comments become `gotcha`
//! hypotheses, and frequently co-committed file pairs become `co_change`
//! edges. Everything is stamped `agent: damask-bootstrap` with hypothesis
//! status and modest confidence, so agents treat it as scaffolding to
//! confirm or dispute, not verified knowledge.

use anyhow::Context;
use damask_core::Fact;
use damask_store::{DamaskProject, FactWriter};
use std::collections::HashMap;
use std::env;
use std::path::Path;

use super::helpers;
use crate::error::Result;
use crate::output::Format;

const BOOTSTRAP_NS: &str = "bootstrap";
const BOOTSTRAP_AGENT: &str = "damask-bootstrap";
/// Cap on TODO/FIXME gotchas so a legacy codebase doesn't flood the graph.
const MAX_TODOS: usize = 50;
/// Commits of history scanned for co-change pairs.
const COCHANGE_COMMITS: usize = 200;
/// Minimum co-occurrences for a pair to be recorded.
const COCHANGE_MIN: usize = 3;
/// Maximum co-change pairs recorded.
const COCHANGE_MAX: usize = 10;
/// Files changed above this count in one commit are skipped as bulk
/// operations (renames, formatting sweeps) that say nothing about coupling.
const COCHANGE_COMMIT_CAP: usize = 20;

/// Root manifests worth describing, with a human label. README is included
/// deliberately: in a sparse repo it is often the only seedable anchor,
/// and "start here" is exactly what a cold session needs.
const MANIFESTS: &[(&str, &str)] = &[
    ("Cargo.toml", "Rust workspace/crate manifest"),
    ("package.json", "Node package manifest"),
    ("pyproject.toml", "Python project manifest"),
    ("setup.py", "Python package setup"),
    ("setup.cfg", "Python package configuration"),
    ("requirements.txt", "Python dependency list"),
    ("go.mod", "Go module manifest"),
    ("Gemfile", "Ruby dependency manifest"),
    ("pom.xml", "Maven build manifest"),
    ("build.gradle", "Gradle build manifest"),
    ("Makefile", "Build entry point"),
    ("Dockerfile", "Container build definition"),
    ("docker-compose.yml", "Service topology definition"),
    ("README.md", "Project overview — start here"),
];

pub fn run(force: bool, format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let edges_file = project.edges_file(BOOTSTRAP_NS);
    let existing = std::fs::read_to_string(&edges_file)
        .map(|c| c.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0);
    if existing > 0 && !force {
        println!(
            "Already bootstrapped ({existing} facts in the '{BOOTSTRAP_NS}' namespace). \
             Use --force to regenerate."
        );
        return Ok(());
    }

    // One span per file within this run, so co-change and TODO edges on
    // the same file share an anchor.
    let mut span_ids: HashMap<String, damask_core::SpanId> = HashMap::new();
    let mut facts: Vec<Fact> = Vec::new();

    let manifests = seed_manifests(&project, &mut facts, &mut span_ids);
    let todos = seed_todos(&project, &mut facts, &mut span_ids);
    let cochanges = seed_cochange(&project, &mut facts, &mut span_ids);

    if facts.is_empty() {
        println!("Nothing to seed: no manifests, TODO/FIXME comments, or co-change history found.");
        return Ok(());
    }

    // Fresh (or --force regenerated) machine-owned namespace: replace
    // wholesale rather than append duplicates.
    FactWriter::write_all(&edges_file, &facts).map_err(|e| anyhow::anyhow!("{}", e))?;

    match format {
        Format::Human => {
            println!("Bootstrapped '{BOOTSTRAP_NS}' namespace:");
            println!("  {manifests} manifest describes");
            println!("  {todos} TODO/FIXME gotchas");
            println!("  {cochanges} co-change pairs");
            println!();
            println!("These are hypotheses (agent: {BOOTSTRAP_AGENT}) — confirm with");
            println!("`damask endorse <id>` or contradict with `damask dispute <id>` as you work.");
            println!("See them: `damask orient` or `damask at <file>`");
        }
        Format::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "ns": BOOTSTRAP_NS,
                    "manifests": manifests,
                    "todos": todos,
                    "cochange_pairs": cochanges,
                    "facts": facts.len(),
                })
            );
        }
    }

    Ok(())
}

/// Span anchored at line 1 of `file`, cached per path, stamped as
/// bootstrap-owned. Returns None when the file can't be spanned (empty,
/// unreadable, vanished).
fn span_for(
    project: &DamaskProject,
    file: &str,
    span_ids: &mut HashMap<String, damask_core::SpanId>,
    facts: &mut Vec<Fact>,
) -> Option<damask_core::SpanId> {
    if let Some(id) = span_ids.get(file) {
        return Some(id.clone());
    }
    let mut span = helpers::build_span(project, file, 1, 1, None, BOOTSTRAP_NS).ok()?;
    span.agent = Some(BOOTSTRAP_AGENT.to_string());
    span.session = None;
    let id = span.id.clone();
    span_ids.insert(file.to_string(), id.clone());
    facts.push(Fact::Span(span));
    Some(id)
}

fn bootstrap_edge(
    from: damask_core::SpanId,
    to: Option<damask_core::SpanId>,
    rel: &str,
    payload: serde_json::Value,
) -> Fact {
    let mut edge = helpers::build_edge(
        Some(damask_core::DamaskId::Span(from)),
        to.map(damask_core::DamaskId::Span),
        rel,
        payload,
        BOOTSTRAP_NS,
    );
    edge.agent = Some(BOOTSTRAP_AGENT.to_string());
    edge.session = None;
    Fact::Edge(edge)
}

/// Name/description extraction from a manifest, cheap and line-based.
fn manifest_detail(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    if path.file_name()?.to_str()? == "package.json" {
        let v: serde_json::Value = serde_json::from_str(&content).ok()?;
        let name = v.get("name").and_then(|n| n.as_str());
        let desc = v.get("description").and_then(|d| d.as_str());
        return match (name, desc) {
            (Some(n), Some(d)) => Some(format!("{n}: {d}")),
            (Some(n), None) => Some(n.to_string()),
            _ => None,
        };
    }
    // TOML-ish line scan (Cargo.toml, pyproject.toml): first name/description keys.
    let mut name = None;
    let mut desc = None;
    for line in content.lines().take(100) {
        let l = line.trim();
        if let Some(v) = l
            .strip_prefix("name = \"")
            .and_then(|r| r.strip_suffix('"'))
        {
            name.get_or_insert(v.to_string());
        }
        if let Some(v) = l
            .strip_prefix("description = \"")
            .and_then(|r| r.strip_suffix('"'))
        {
            desc.get_or_insert(v.to_string());
        }
    }
    match (name, desc) {
        (Some(n), Some(d)) => Some(format!("{n}: {d}")),
        (Some(n), None) => Some(n),
        _ => None,
    }
}

fn seed_manifests(
    project: &DamaskProject,
    facts: &mut Vec<Fact>,
    span_ids: &mut HashMap<String, damask_core::SpanId>,
) -> usize {
    let mut count = 0;
    for (file, label) in MANIFESTS {
        let path = project.root.join(file);
        if !path.is_file() {
            continue;
        }
        let Some(span_id) = span_for(project, file, span_ids, facts) else {
            continue;
        };
        let detail = manifest_detail(&path)
            .map(|d| format!(" — {d}"))
            .unwrap_or_default();
        facts.push(bootstrap_edge(
            span_id,
            None,
            "describes",
            serde_json::json!({
                "summary": format!("{label}{detail}"),
                "confidence": 0.6,
                "status": "hypothesis",
                "tags": ["bootstrap", "manifest"],
            }),
        ));
        count += 1;
    }
    count
}

/// Tracked text files via `git ls-files`; None outside a git repo.
fn tracked_files(root: &Path) -> Option<Vec<String>> {
    let out = std::process::Command::new("git")
        .args(["ls-files"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(String::from)
            .collect(),
    )
}

fn seed_todos(
    project: &DamaskProject,
    facts: &mut Vec<Fact>,
    span_ids: &mut HashMap<String, damask_core::SpanId>,
) -> usize {
    let Some(files) = tracked_files(&project.root) else {
        return 0;
    };
    let mut count = 0;
    'files: for file in files {
        if file.starts_with(".damask/") || file.starts_with(".claude/") {
            continue;
        }
        let path = project.root.join(&file);
        let Ok(meta) = path.metadata() else { continue };
        if meta.len() > 1_000_000 {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue; // binary or unreadable
        };
        for (idx, line) in content.lines().enumerate() {
            let Some(pos) = ["TODO", "FIXME", "HACK"]
                .iter()
                .filter_map(|m| line.find(m))
                .min()
            else {
                continue;
            };
            let text: String = line[pos..].chars().take(100).collect();
            let line_no = (idx + 1) as u32;

            // A dedicated one-line span at the comment itself.
            let Ok(mut span) =
                helpers::build_span(project, &file, line_no, line_no, None, BOOTSTRAP_NS)
            else {
                continue;
            };
            span.agent = Some(BOOTSTRAP_AGENT.to_string());
            span.session = None;
            let span_id = span.id.clone();
            facts.push(Fact::Span(span));
            facts.push(bootstrap_edge(
                span_id,
                None,
                "gotcha",
                serde_json::json!({
                    "summary": text,
                    "confidence": 0.5,
                    "status": "hypothesis",
                    "tags": ["bootstrap", "todo"],
                }),
            ));
            count += 1;
            if count >= MAX_TODOS {
                let _ = span_ids; // spans cached only for manifest/co-change reuse
                break 'files;
            }
        }
    }
    count
}

fn seed_cochange(
    project: &DamaskProject,
    facts: &mut Vec<Fact>,
    span_ids: &mut HashMap<String, damask_core::SpanId>,
) -> usize {
    let out = std::process::Command::new("git")
        .args([
            "log",
            "--name-only",
            "--pretty=format:%H",
            "-n",
            &COCHANGE_COMMITS.to_string(),
        ])
        .current_dir(&project.root)
        .output()
        .ok()
        .filter(|o| o.status.success());
    let Some(out) = out else {
        return 0;
    };
    let text = String::from_utf8_lossy(&out.stdout);

    // Parse commit blocks: hash line, then file lines until blank.
    let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();
    let mut commits_scanned = 0usize;
    let mut current: Vec<String> = Vec::new();
    let flush = |files: &mut Vec<String>, counts: &mut HashMap<(String, String), usize>| {
        if files.len() >= 2 && files.len() <= COCHANGE_COMMIT_CAP {
            files.sort();
            for i in 0..files.len() {
                for j in (i + 1)..files.len() {
                    *counts
                        .entry((files[i].clone(), files[j].clone()))
                        .or_insert(0) += 1;
                }
            }
        }
        files.clear();
    };
    for line in text.lines() {
        if line.len() == 40 && line.chars().all(|c| c.is_ascii_hexdigit()) {
            flush(&mut current, &mut pair_counts);
            commits_scanned += 1;
        } else if !line.trim().is_empty() {
            current.push(line.to_string());
        }
    }
    flush(&mut current, &mut pair_counts);

    let mut pairs: Vec<((String, String), usize)> = pair_counts
        .into_iter()
        .filter(|(_, n)| *n >= COCHANGE_MIN)
        .collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs.truncate(COCHANGE_MAX);

    let mut count = 0;
    for ((a, b), n) in pairs {
        // Both files must still exist to be worth anchoring.
        if !project.root.join(&a).is_file() || !project.root.join(&b).is_file() {
            continue;
        }
        let Some(from) = span_for(project, &a, span_ids, facts) else {
            continue;
        };
        let Some(to) = span_for(project, &b, span_ids, facts) else {
            continue;
        };
        facts.push(bootstrap_edge(
            from,
            Some(to),
            "co_change",
            serde_json::json!({
                "summary": format!(
                    "{a} and {b} changed together in {n} of the last {commits_scanned} commits — likely coupled"
                ),
                "confidence": 0.5,
                "status": "hypothesis",
                "tags": ["bootstrap", "co-change"],
            }),
        ));
        count += 1;
    }
    count
}
