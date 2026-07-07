//! Warm-start briefing for AI agents.
//!
//! Designed to run as a Claude Code `SessionStart` hook: stdout is injected
//! into the agent's context at the start of every session, eliminating the
//! cold-start exploration tax. Output is compact markdown with a hard size
//! budget, and the command fails open (exit 0, no output) on any error —
//! a hook must never break a session.

use std::env;
use std::fmt::Write as _;

use damask_store::DamaskProject;

use crate::error::Result;
use crate::output::Format;

use super::orient::{collect, EdgeSummary, OrientData};

/// Top edges shown per rel section.
const MAX_PER_SECTION: usize = 3;
/// Maximum rel sections (largest first).
const MAX_SECTIONS: usize = 6;
/// Recent-activity entries.
const MAX_RECENT: usize = 5;
/// Summary truncation width.
const SUMMARY_WIDTH: usize = 100;

pub fn run(format: Format) -> Result<()> {
    // Fail open at every step: no project, no index, no problem.
    let Ok(cwd) = env::current_dir() else {
        return Ok(());
    };
    let Ok(project) = DamaskProject::discover(&cwd) else {
        return Ok(());
    };
    let Ok(data) = collect(None, None, false, false) else {
        return Ok(());
    };

    let mut md = render_markdown(&data);

    // Self-healing: warn when the installed skill predates this binary, so
    // the agent (or user) refreshes it instead of working from stale docs.
    let skill_path = project.root.join(".claude/skills/damask/SKILL.md");
    if let Ok(installed) = std::fs::read_to_string(&skill_path) {
        if installed != super::init::SKILL_MD {
            md.push_str(
                "\n⚠ The installed damask skill (.claude/skills/damask/SKILL.md) is out of \
                 date with this damask binary — run `damask init --claude` to refresh it.\n",
            );
        }
    }

    match format {
        Format::Human => print!("{md}"),
        Format::Json => {
            // Claude Code SessionStart hook JSON envelope.
            let output = serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": md,
                }
            });
            println!("{}", serde_json::to_string(&output).unwrap());
        }
    }

    Ok(())
}

fn render_markdown(data: &OrientData) -> String {
    let mut md = String::new();

    if data.active_edge_count == 0 {
        let _ = writeln!(md, "## Damask knowledge graph\n");
        let _ = writeln!(
            md,
            "This repo uses damask but the graph is empty (cold start). Seed it instantly \
             from manifests, TODO/FIXME comments, and git co-change history:\n"
        );
        let _ = writeln!(md, "    damask bootstrap\n");
        let _ = writeln!(
            md,
            "Then record what you discover as you work — a risk, gotcha, decision, or dependency:\n"
        );
        let _ = writeln!(
            md,
            "    damask record <file> <start> <end> <rel> -m \"what you found\" -c 0.8\n"
        );
        let _ = writeln!(md, "Run `damask help cold-start` for the first-pass playbook.");
        return md;
    }

    let _ = writeln!(md, "## Damask knowledge graph\n");
    let _ = writeln!(
        md,
        "{} edges across {} namespaces ({} active, {} closed; {} endorsements, {} disputes).",
        data.edge_count,
        data.namespace_count,
        data.active_edge_count,
        data.graph_stats.closed_edges,
        data.endorsement_count,
        data.dispute_count,
    );
    if !data.active_ns.is_empty() {
        let _ = writeln!(md, "Active namespace: `{}`.", data.active_ns);
    }

    // Trust line: an agent must know upfront when much of what follows
    // anchors to code that no longer exists.
    if data.open_edge_total > 0 && data.stale_anchored > 0 {
        let ratio = data.stale_anchored as f64 / data.open_edge_total as f64;
        if ratio > 0.2 {
            let _ = writeln!(
                md,
                "\n⚠ **Trust warning:** {}/{} open edges anchor to missing or unresolvable \
                 code — treat unmarked findings below with care and prefer ✅-marked ones. \
                 Review candidates with `damask lint`.",
                data.stale_anchored, data.open_edge_total,
            );
        }
    }

    for section in data.sections.iter().take(MAX_SECTIONS) {
        let _ = writeln!(md, "\n### {} ({})", section.rel, section.edges.len());
        for e in section.edges.iter().take(MAX_PER_SECTION) {
            let _ = writeln!(md, "{}", edge_line(e));
        }
        if section.edges.len() > MAX_PER_SECTION {
            let _ = writeln!(
                md,
                "- … {} more — `damask where \"rel={}\"`",
                section.edges.len() - MAX_PER_SECTION,
                section.rel
            );
        }
    }

    if !data.suspect_spans.is_empty() {
        let total: usize = data.suspect_spans.iter().map(|s| s.open_edge_count).sum();
        let _ = writeln!(
            md,
            "\n### Suspect annotations ({} edges on drifted/changed code)",
            total
        );
        for s in data.suspect_spans.iter().take(3) {
            let lines = s
                .lines
                .map(|(a, b)| format!(":{a}-{b}"))
                .unwrap_or_default();
            let why = if s.resolution != "exact" {
                s.resolution.as_str()
            } else {
                "file changed"
            };
            let _ = writeln!(
                md,
                "- {}{lines} ({why}) — {} edges; confirm or dispute via `damask at {}`",
                s.path, s.open_edge_count, s.path
            );
        }
        if data.suspect_spans.len() > 3 {
            let _ = writeln!(
                md,
                "- … {} more locations — `damask status`",
                data.suspect_spans.len() - 3
            );
        }
    }

    if !data.recent.is_empty() {
        let _ = writeln!(md, "\n### Recent activity");
        for e in data.recent.iter().take(MAX_RECENT) {
            let date = e.ts.split('T').next().unwrap_or(&e.ts);
            let trunc = damask_core::truncate_str(&e.summary, SUMMARY_WIDTH);
            let _ = writeln!(md, "- [{date}] [{}] {trunc}", e.rel);
        }
    }

    let _ = writeln!(
        md,
        "\nBefore changing a file, check what is known: `damask at <file>[:line]`. \
         Search with `damask search \"<query>\"`, full picture with `damask orient`. \
         Record new findings with `damask record`; confirm or contradict existing edges \
         with `damask endorse <id>` / `damask dispute <id>`."
    );

    md
}

fn edge_line(e: &EdgeSummary) -> String {
    let conf = e
        .confidence
        .map(|c| format!("({c:.2}) "))
        .unwrap_or_default();
    let glyph = if e.glyph.is_empty() {
        String::new()
    } else {
        format!("{} ", e.glyph)
    };
    let marks = format!(
        "{}{}",
        if e.endorsements > 0 {
            format!(" \u{00D7}{}\u{2713}", e.endorsements)
        } else {
            String::new()
        },
        if e.disputes > 0 {
            format!(" \u{00D7}{}\u{2717}", e.disputes)
        } else {
            String::new()
        },
    );
    let anchor = e
        .anchor
        .as_deref()
        .map(|a| format!(" @ {a}"))
        .unwrap_or_default();
    let trunc = damask_core::truncate_str(&e.summary, SUMMARY_WIDTH);
    format!("- {conf}{glyph}{trunc}{marks}{anchor} [{}]", e.id)
}
