use anyhow::Context;
use damask_store::DamaskProject;
use std::env;

use crate::error::Result;

pub fn run() -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    damask_tui::run_tui(&project)?;
    Ok(())
}
