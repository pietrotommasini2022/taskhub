use anyhow::{Context, Result};
use taskhub_core::Storage;

use crate::commands::init::db_path;

pub fn run() -> Result<()> {
    let db = db_path()?;
    if !db.exists() {
        anyhow::bail!("taskhub not initialized — run `taskhub init` first");
    }
    let storage = Storage::open(&db).context("open storage")?;
    let runs = storage.list_runs(50).context("list runs")?;

    if runs.is_empty() {
        println!("No runs yet.");
        return Ok(());
    }

    println!("{:<38} {:<30} {:<10} {}", "RUN ID", "WORKFLOW", "STATUS", "STARTED");
    for r in &runs {
        println!(
            "{:<38} {:<30} {:<10} {}",
            r.id,
            truncate(&r.workflow_name, 28),
            format!("{:?}", r.status).to_lowercase(),
            r.started_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
