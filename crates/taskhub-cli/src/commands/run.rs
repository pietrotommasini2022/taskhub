use anyhow::{Context, Result};
use std::sync::Arc;
use taskhub_core::{Engine, Storage, Workflow};
use taskhub_core::types::{RunStatus, TriggerKind};
use taskhub_wasm_host::PluginRegistry;

use crate::commands::init::{db_path, taskhub_dir};

pub async fn run(path: &str, dry_run: bool) -> Result<()> {
    let yaml = std::fs::read_to_string(path)
        .with_context(|| format!("read {path}"))?;
    let wf = Workflow::parse(&yaml)
        .with_context(|| format!("parse {path}"))?;
    wf.validate()
        .with_context(|| format!("validate {path}"))?;

    let storage = Arc::new(if dry_run {
        Storage::open_in_memory().context("open in-memory storage")?
    } else {
        let db = db_path()?;
        if !db.exists() {
            anyhow::bail!("taskhub not initialized — run `taskhub init` first");
        }
        Storage::open(&db).context("open storage")?
    });

    let mut engine = Engine::new(storage);
    if dry_run {
        engine = engine.with_dry_run();
    }
    taskhub_plugins_core::register_all(&mut engine);

    // Load installed WASM plugins.
    if !dry_run {
        if let Ok(dir) = taskhub_dir() {
            let registry = PluginRegistry::new(dir.join("plugins"));
            if let Err(e) = registry.register_all(&mut engine) {
                tracing::warn!("failed to load some plugins: {e}");
            }
        }
    }

    let workflow_id = path.to_string();
    let run = engine
        .run(&wf, &workflow_id, TriggerKind::Manual, None)
        .await
        .context("run workflow")?;

    match run.status {
        RunStatus::Success => {
            println!("✓ {} — success ({} ms)",
                wf.name,
                run.finished_at
                    .zip(Some(run.started_at))
                    .map(|(f, s)| (f - s).num_milliseconds())
                    .unwrap_or(0)
            );
        }
        RunStatus::Failed => {
            eprintln!("✗ {} — failed: {}", wf.name, run.error.as_deref().unwrap_or("unknown"));
            std::process::exit(1);
        }
        _ => {}
    }
    Ok(())
}
