use anyhow::Context;
use damask_store::DamaskProject;
use std::env;
use std::path::Path;

use crate::error::Result;

/// The canonical skill text embedded in this binary. Installed copies are
/// compared against it: `init` rewrites on drift, `briefing` warns on drift.
pub(crate) const SKILL_MD: &str = include_str!("claude_skill.md");
const CODEX_SKILL_MD: &str = include_str!("codex_skill.md");

/// Write `content` to `path` only when it differs; reports what happened.
/// Keeps installs idempotent and catches stale copies from old binaries.
fn sync_file(path: &Path, content: &str, label: &str) -> Result<()> {
    match std::fs::read_to_string(path) {
        Ok(existing) if existing == content => {
            println!("  {label} already current");
        }
        Ok(_) => {
            std::fs::write(path, content).context(format!("failed to write {label}"))?;
            println!("  Updated {label} (was out of date with this binary)");
        }
        Err(_) => {
            std::fs::write(path, content).context(format!("failed to write {label}"))?;
            println!("  Created {label}");
        }
    }
    Ok(())
}

// Hook commands are guarded so a teammate who clones the repo without the
// damask binary gets zero errors instead of exit-127 spam on every event.
// The SessionStart fallback tells the agent the graph exists and how to get
// it; peek/harvest fall back silently. The `! command -v || damask ...` form
// preserves damask's own exit code when it IS installed (Stop/PostToolUse
// hooks use exit codes deliberately).
const BRIEFING_HOOK_COMMAND: &str = "if command -v damask >/dev/null 2>&1; then damask briefing; else echo 'This repo has a damask knowledge graph (.damask/) shared across agent sessions. Install the damask CLI to inherit it: clone the damask repo and run cargo install --path crates/damask-cli'; fi";
const HARVEST_HOOK_COMMAND: &str = "! command -v damask >/dev/null 2>&1 || damask harvest";
const PEEK_HOOK_COMMAND: &str = "! command -v damask >/dev/null 2>&1 || damask peek";
const BRIEFING_HOOK_KEY: &str = "damask briefing";
const HARVEST_HOOK_KEY: &str = "damask harvest";
const PEEK_HOOK_KEY: &str = "damask peek";
const PEEK_TOOL_MATCHER: &str = "Read|Edit|Write|MultiEdit|NotebookEdit";

/// True when running inside a live Claude Code session — the primary
/// adopter of `damask init` is the agent itself, and it should not need
/// to know about --claude.
fn claude_env_present() -> bool {
    std::env::var_os("CLAUDECODE").is_some() || std::env::var_os("CLAUDE_CODE_SESSION_ID").is_some()
}

/// Default namespace from the repo directory name: lowercased, non
/// [a-z0-9_-] squeezed to '-'. Falls back to "main".
fn default_ns_name(root: &Path) -> String {
    let raw = root
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut out = String::new();
    let mut last_dash = true; // suppress leading dashes
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "main".to_string()
    } else {
        trimmed
    }
}

pub fn run(force_claude: bool, force_codex: bool, no_agents: bool) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;

    // Try to init; if already initialized and a scaffolding flag was requested, discover instead.
    let project = match DamaskProject::init(&cwd) {
        Ok(p) => {
            println!("Initialized .damask/ in {}", p.root.display());
            println!("  edges/       — namespace JSONL files");
            println!("  config.json  — project configuration");

            // Create .gitignore. `.active_ns` is a per-checkout selection,
            // not shared state — committing it caused real cross-namespace
            // write pollution between checkouts.
            let gitignore_path = p.damask_dir.join(".gitignore");
            std::fs::write(
                &gitignore_path,
                ".active_ns\nindex.db\nindex.db-wal\nindex.db-shm\n.session/\nedges/.private/\nedges/.views/\nedges/.local/\n",
            )
            .context("failed to write .gitignore")?;

            // Default namespace so the first `damask record` succeeds
            // without a `ns set` ritual.
            let ns = default_ns_name(&p.root);
            let mut config = p.read_config().map_err(|e| anyhow::anyhow!("{}", e))?;
            config.default_ns = Some(ns.clone());
            let config_json =
                serde_json::to_string_pretty(&config).context("failed to serialize config.json")?;
            std::fs::write(p.config_path(), config_json).context("failed to write config.json")?;
            println!("  Default namespace: {ns} (change with `damask ns set <name>`)");

            p
        }
        Err(e) if force_claude || force_codex => {
            // Already initialized — discover existing project so we can add scaffolding.
            let p = DamaskProject::discover(&cwd)
                .map_err(|de| anyhow::anyhow!("{}", de))
                .context(format!("init failed ({e}) and no existing .damask/ found"))?;
            println!("Found existing .damask/ in {}", p.root.display());
            p
        }
        Err(e) => {
            return Err(anyhow::anyhow!("{}", e).context("failed to initialize damask project"));
        }
    };

    ensure_gitattributes(&project)?;

    if no_agents {
        return Ok(());
    }

    // Determine which agents to scaffold: explicit flags always win;
    // otherwise auto-detect from directory presence OR the live session
    // environment — the primary adopter running `damask init` is the
    // agent itself, and bare init must install the loop for it.
    let root = &project.root;
    let do_claude =
        force_claude || (!force_codex && (root.join(".claude").is_dir() || claude_env_present()));
    let do_codex = force_codex
        || (!force_claude && (root.join(".agents").is_dir() || root.join("AGENTS.md").exists()));

    if do_claude {
        scaffold_claude(root)?;
    }
    if do_codex {
        scaffold_codex(root)?;
    }

    if !do_claude && !do_codex {
        println!();
        println!("No AI agent directories detected (.claude/, .agents/).");
        println!("To add agent integration later:");
        println!("  damask init --claude    # Claude Code");
        println!("  damask init --codex     # OpenAI Codex CLI");
    }

    if do_claude {
        // The SessionStart hook only fires from the NEXT session — print
        // the same briefing inline so the session that ran init gets its
        // warm start too, then show how to make the graph shared.
        println!();
        println!("Warm start for this session (hooks take over from the next one):");
        println!();
        let _ = super::briefing::run(crate::output::Format::Human);
        println!();
        println!("Share the graph with your team and their agents:");
        println!("  git add .damask .claude && git commit -m \"Add damask knowledge fabric\"");
    }

    Ok(())
}

/// Namespace logs are append-only with ULID-keyed lines, so concurrent
/// branches appending to the same file merge safely as a union — without
/// this, parallel agent work produces spurious JSONL merge conflicts.
fn ensure_gitattributes(project: &DamaskProject) -> Result<()> {
    let path = project.damask_dir.join(".gitattributes");
    if path.exists() {
        return Ok(());
    }
    std::fs::write(&path, "edges/*.jsonl merge=union\n")
        .context("failed to write .damask/.gitattributes")?;
    println!("  Wrote .damask/.gitattributes (merge=union for edge logs)");
    Ok(())
}

const DAMASK_PERMISSION: &str = "Bash(damask *)";

/// Add "Bash(damask *)" to permissions.allow if not already present.
/// Returns true if the document was modified.
fn ensure_damask_allowlisted(doc: &mut serde_json::Value) -> Result<bool> {
    // Navigate to permissions.allow, creating the path if needed.
    let allow = doc
        .as_object_mut()
        .context("settings.json root is not an object")?
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("permissions is not an object")?
        .entry("allow")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context("permissions.allow is not an array")?;

    let already = allow.iter().any(|v| v.as_str() == Some(DAMASK_PERMISSION));

    if already {
        println!(
            "  .claude/settings.json already allows \"{}\"",
            DAMASK_PERMISSION
        );
        Ok(false)
    } else {
        allow.push(serde_json::Value::String(DAMASK_PERMISSION.to_string()));
        println!("  Added \"{}\" to .claude/settings.json", DAMASK_PERMISSION);
        Ok(true)
    }
}

/// Ensure a hook entry running `command` exists under hooks.<event>.
/// `key` (e.g. "damask briefing") identifies the hook: an existing entry
/// whose command contains `key` but differs from `command` is upgraded in
/// place (older installs wrote unguarded commands); otherwise a new entry
/// is appended. Returns true if the document was modified.
fn ensure_hook(
    doc: &mut serde_json::Value,
    event: &str,
    matcher: Option<&str>,
    command: &str,
    key: &str,
) -> Result<bool> {
    let entries = doc
        .as_object_mut()
        .context("settings.json root is not an object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("hooks is not an object")?
        .entry(event)
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context(format!("hooks.{event} is not an array"))?;

    let mut found = false;
    let mut modified = false;
    for entry in entries.iter_mut() {
        let Some(hooks) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
            continue;
        };
        for h in hooks.iter_mut() {
            let Some(c) = h.get("command").and_then(|c| c.as_str()) else {
                continue;
            };
            if c.contains(key) {
                found = true;
                if c != command {
                    h["command"] = serde_json::json!(command);
                    modified = true;
                    println!("  Updated {event} hook to guarded form: `{key}`");
                }
            }
        }
    }
    if found {
        if !modified {
            println!("  .claude/settings.json already runs `{key}` on {event}");
        }
        return Ok(modified);
    }

    let mut entry = serde_json::Map::new();
    if let Some(m) = matcher {
        entry.insert("matcher".to_string(), serde_json::json!(m));
    }
    entry.insert(
        "hooks".to_string(),
        serde_json::json!([{"type": "command", "command": command}]),
    );
    entries.push(serde_json::Value::Object(entry));
    println!("  Added {event} hook: `{key}` (guarded for teammates without damask)");
    Ok(true)
}

// Codex has no hook loop (no SessionStart briefing / PostToolUse peek), so
// the loop must be spelled out where Codex actually reads it: AGENTS.md.
// Marked block, replaced in place on re-run to stay current.
const AGENTS_BEGIN: &str = "<!-- damask:begin -->";
const AGENTS_END: &str = "<!-- damask:end -->";
const AGENTS_BLOCK: &str = "<!-- damask:begin -->
## Damask knowledge graph

This repo has a damask knowledge graph in `.damask/` — verified findings
(risks, gotchas, decisions) pinned to exact code regions by the agents who
worked here before you. Codex has no automatic hook loop, so **run the loop
yourself**:

- **Session start:** `damask briefing` — inherit what's known before exploring.
- **Before editing a file:** `damask at <file>` — check what's recorded there.
- **As you work:** record durable findings and signal —
  `damask record <file> <start> <end> <rel> -m \"what you found\" -c 0.9`,
  then `damask endorse <id>` (confirm) / `damask dispute <id> --reason <r>`
  (contradict) / `damask close <id> --reason resolved` (done). IDs accept
  unique prefixes; signalling shows you the edge's history.

Record **judgment** — what surprised you, what broke, why a decision went a
certain way — not descriptions of what the code plainly says. Full
reference: `.agents/skills/damask/SKILL.md`.
<!-- damask:end -->";

/// Ensure AGENTS.md carries the damask block — Codex's real load path.
/// Replaces the marked block on re-run (stays current), else appends.
fn ensure_agents_md(root: &Path) -> Result<()> {
    let path = root.join("AGENTS.md");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated =
        if let (Some(b), Some(e)) = (existing.find(AGENTS_BEGIN), existing.find(AGENTS_END)) {
            let end = e + AGENTS_END.len();
            let mut s = existing.clone();
            s.replace_range(b..end, AGENTS_BLOCK);
            if s == existing {
                println!("  AGENTS.md damask section already current");
                return Ok(());
            }
            println!("  Updated AGENTS.md damask section");
            s
        } else if existing.trim().is_empty() {
            println!("  Created AGENTS.md with damask section");
            format!("# Agent Guide\n\n{AGENTS_BLOCK}\n")
        } else {
            println!("  Added damask section to AGENTS.md");
            format!("{}\n\n{AGENTS_BLOCK}\n", existing.trim_end())
        };
    std::fs::write(&path, updated).context("failed to write AGENTS.md")?;
    Ok(())
}

fn scaffold_codex(root: &Path) -> Result<()> {
    let skill_dir = root.join(".agents/skills/damask");
    std::fs::create_dir_all(&skill_dir).context("failed to create .agents/skills/damask/")?;

    sync_file(
        &skill_dir.join("SKILL.md"),
        CODEX_SKILL_MD,
        ".agents/skills/damask/SKILL.md",
    )?;
    ensure_agents_md(root)?;

    println!();
    println!("Codex integration synced: AGENTS.md loads the damask loop; full skill in .agents/skills/damask/.");
    Ok(())
}

fn scaffold_claude(root: &Path) -> Result<()> {
    // Create .claude/skills/damask/ directory
    let skill_dir = root.join(".claude/skills/damask");
    std::fs::create_dir_all(&skill_dir).context("failed to create .claude/skills/damask/")?;

    sync_file(
        &skill_dir.join("SKILL.md"),
        SKILL_MD,
        ".claude/skills/damask/SKILL.md",
    )?;

    // Write or update settings.json: allowlist damask, install the
    // warm-start (SessionStart → briefing) and harvest (Stop) hooks.
    let settings_path = root.join(".claude/settings.json");
    let existed = settings_path.exists();
    let mut doc: serde_json::Value = if existed {
        let contents = std::fs::read_to_string(&settings_path)
            .context("failed to read .claude/settings.json")?;
        serde_json::from_str(&contents).context("failed to parse .claude/settings.json")?
    } else {
        serde_json::json!({})
    };

    let mut changed = !existed;
    changed |= ensure_damask_allowlisted(&mut doc)?;
    changed |= ensure_hook(
        &mut doc,
        "SessionStart",
        Some("startup|resume|clear"),
        BRIEFING_HOOK_COMMAND,
        BRIEFING_HOOK_KEY,
    )?;
    changed |= ensure_hook(
        &mut doc,
        "Stop",
        None,
        HARVEST_HOOK_COMMAND,
        HARVEST_HOOK_KEY,
    )?;
    changed |= ensure_hook(
        &mut doc,
        "PostToolUse",
        Some(PEEK_TOOL_MATCHER),
        PEEK_HOOK_COMMAND,
        PEEK_HOOK_KEY,
    )?;
    changed |= ensure_hook(
        &mut doc,
        "UserPromptSubmit",
        None,
        PEEK_HOOK_COMMAND,
        PEEK_HOOK_KEY,
    )?;

    if changed {
        let updated = serde_json::to_string_pretty(&doc)
            .context("failed to serialize updated settings.json")?;
        std::fs::write(&settings_path, updated.as_bytes())
            .context("failed to write .claude/settings.json")?;
    }

    println!();
    println!("Claude Code skill synced. Use /damask in Claude Code to get started.");
    println!("Hooks installed: briefing on session start, peek context on file touch/prompt, harvest nudge on stop.");
    if crate::ck::ck_available() {
        println!("ck detected — semantic knowledge search enabled (`damask search --sem`, `ck --jsonl | damask enrich`).");
    } else {
        println!("{}", crate::ck::CK_HINT);
    }

    Ok(())
}
