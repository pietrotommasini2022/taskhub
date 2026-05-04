use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use taskhub_core::Storage;
use tracing::info;

pub fn run() -> Result<()> {
    let dir = taskhub_dir()?;

    if dir.exists() {
        println!("taskhub already initialized at {}", dir.display());
        return Ok(());
    }

    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;

    let db_path = dir.join("state.db");
    Storage::open(&db_path).context("initialize database")?;

    let workflows_dir = dir.join("workflows");
    fs::create_dir_all(&workflows_dir)?;

    println!("Initialized taskhub at {}", dir.display());
    info!("taskhub initialized");
    Ok(())
}

pub fn taskhub_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".taskhub"))
}

pub fn db_path() -> Result<PathBuf> {
    Ok(taskhub_dir()?.join("state.db"))
}
