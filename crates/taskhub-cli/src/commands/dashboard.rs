use anyhow::{Context, Result};
use std::sync::Arc;
use taskhub_core::{daemon::run_dashboard, Storage};

use crate::commands::init::db_path;

pub async fn run() -> Result<()> {
    let db = db_path()?;
    if !db.exists() {
        anyhow::bail!("taskhub not initialized — run `taskhub init` first");
    }
    let storage = Arc::new(Storage::open(&db).context("open storage")?);
    let addr = "127.0.0.1:8766".parse().unwrap();
    println!("Dashboard: http://{addr}");
    run_dashboard(storage, addr).await
}
