use anyhow::Context;
use damask_store::DamaskProject;
use std::env;

use crate::error::Result;

pub fn run() -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::init(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("failed to initialize damask project")?;

    println!("Initialized .damask/ in {}", project.root.display());
    println!("  edges/       — namespace JSONL files");
    println!("  config.json  — project configuration");

    // Create .gitignore
    let gitignore_path = project.damask_dir.join(".gitignore");
    std::fs::write(
        &gitignore_path,
        "index.db\nindex.db-wal\nindex.db-shm\nedges/.private/\nedges/.views/\nedges/.local/\n",
    )
    .context("failed to write .gitignore")?;

    Ok(())
}
