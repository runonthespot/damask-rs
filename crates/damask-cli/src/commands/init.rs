use anyhow::Context;
use damask_store::DamaskProject;
use std::env;
use std::path::Path;

use crate::error::Result;

const SKILL_MD: &str = include_str!("claude_skill.md");
const CODEX_SKILL_MD: &str = include_str!("codex_skill.md");

const SETTINGS_JSON: &str = r#"{"permissions":{"allow":["Bash(damask *)"]}}"#;

pub fn run(claude: bool, codex: bool) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;

    // Try to init; if already initialized and a scaffolding flag was requested, discover instead.
    let project = match DamaskProject::init(&cwd) {
        Ok(p) => {
            println!("Initialized .damask/ in {}", p.root.display());
            println!("  edges/       — namespace JSONL files");
            println!("  config.json  — project configuration");

            // Create .gitignore
            let gitignore_path = p.damask_dir.join(".gitignore");
            std::fs::write(
                &gitignore_path,
                "index.db\nindex.db-wal\nindex.db-shm\nedges/.private/\nedges/.views/\nedges/.local/\n",
            )
            .context("failed to write .gitignore")?;

            p
        }
        Err(e) if claude || codex => {
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

    if claude {
        scaffold_claude(&project.root)?;
    }
    if codex {
        scaffold_codex(&project.root)?;
    }

    Ok(())
}

const DAMASK_PERMISSION: &str = "Bash(damask *)";

/// Read an existing settings.json, add "Bash(damask *)" to permissions.allow
/// if not already present, and write it back. Preserves all existing entries.
fn ensure_damask_allowlisted(settings_path: &Path) -> Result<()> {
    let contents =
        std::fs::read_to_string(settings_path).context("failed to read .claude/settings.json")?;

    let mut doc: serde_json::Value =
        serde_json::from_str(&contents).context("failed to parse .claude/settings.json")?;

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

    let already = allow
        .iter()
        .any(|v| v.as_str() == Some(DAMASK_PERMISSION));

    if already {
        println!("  .claude/settings.json already allows \"{}\"", DAMASK_PERMISSION);
    } else {
        allow.push(serde_json::Value::String(DAMASK_PERMISSION.to_string()));
        let updated = serde_json::to_string_pretty(&doc)
            .context("failed to serialize updated settings.json")?;
        std::fs::write(settings_path, updated.as_bytes())
            .context("failed to write .claude/settings.json")?;
        println!(
            "  Added \"{}\" to .claude/settings.json",
            DAMASK_PERMISSION
        );
    }

    Ok(())
}

fn scaffold_codex(root: &Path) -> Result<()> {
    let skill_dir = root.join(".agents/skills/damask");
    std::fs::create_dir_all(&skill_dir).context("failed to create .agents/skills/damask/")?;

    let skill_path = skill_dir.join("SKILL.md");
    if skill_path.exists() {
        let content = std::fs::read_to_string(&skill_path)
            .context("failed to read .agents/skills/damask/SKILL.md")?;
        if content.contains("# Damask") {
            println!("  .agents/skills/damask/SKILL.md already exists");
            return Ok(());
        }
    }

    std::fs::write(&skill_path, CODEX_SKILL_MD)
        .context("failed to write .agents/skills/damask/SKILL.md")?;
    println!("  Created .agents/skills/damask/SKILL.md");

    println!();
    println!("Codex CLI skill created. Damask skill available in Codex.");
    Ok(())
}

fn scaffold_claude(root: &Path) -> Result<()> {
    // Create .claude/skills/damask/ directory
    let skill_dir = root.join(".claude/skills/damask");
    std::fs::create_dir_all(&skill_dir).context("failed to create .claude/skills/damask/")?;

    // Write SKILL.md
    let skill_path = skill_dir.join("SKILL.md");
    std::fs::write(&skill_path, SKILL_MD).context("failed to write SKILL.md")?;

    // Write or update settings.json to ensure damask is allowlisted
    let settings_path = root.join(".claude/settings.json");
    if settings_path.exists() {
        ensure_damask_allowlisted(&settings_path)?;
    } else {
        std::fs::write(&settings_path, SETTINGS_JSON)
            .context("failed to write .claude/settings.json")?;
    }

    println!();
    println!("Claude Code skill created. Use /damask in Claude Code to get started.");

    Ok(())
}
