use crate::engine::Engine;
use crate::scheduler::{run_scheduled, ScheduledWorkflow};
use crate::storage::Storage;
use crate::triggers::filesystem::FilesystemWatcher;
use crate::triggers::webhook::WebhookServer;
use crate::workflow::{Workflow, TriggerKind as WorkflowTriggerKind};
use anyhow::{Context, Result};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

pub struct DaemonConfig {
    pub workflows_dir: PathBuf,
    pub webhook_addr: SocketAddr,
    pub dashboard_addr: SocketAddr,
}

impl DaemonConfig {
    pub fn default_for_home(home: &Path) -> Self {
        Self {
            workflows_dir: home.join(".taskhub").join("workflows"),
            webhook_addr: "127.0.0.1:8765".parse().unwrap(),
            dashboard_addr: "127.0.0.1:8766".parse().unwrap(),
        }
    }
}

pub async fn run_daemon(
    config: DaemonConfig,
    engine: Arc<Engine>,
    storage: Arc<Storage>,
) -> Result<()> {
    std::fs::create_dir_all(&config.workflows_dir)?;

    let workflows = load_workflows(&config.workflows_dir);
    info!(count = workflows.len(), dir = %config.workflows_dir.display(), "workflows loaded");

    let mut webhook_server = WebhookServer::new();
    let mut fs_watcher = FilesystemWatcher::new();

    for (wf, wf_id) in &workflows {
        match wf.on.trigger {
            WorkflowTriggerKind::Schedule => {
                match ScheduledWorkflow::from_workflow(wf.clone(), wf_id.clone()) {
                    Ok(entry) => {
                        let eng = engine.clone();
                        tokio::spawn(run_scheduled(entry, eng));
                    }
                    Err(e) => warn!(workflow = %wf.name, error = %e, "skip schedule"),
                }
            }
            WorkflowTriggerKind::Webhook => {
                webhook_server.register(wf.clone(), wf_id.clone());
            }
            WorkflowTriggerKind::Filesystem => {
                fs_watcher.register(wf.clone(), wf_id.clone());
            }
            WorkflowTriggerKind::Manual => {}
        }
    }

    // Start webhook server.
    let webhook_engine = engine.clone();
    let webhook_addr = config.webhook_addr;
    tokio::spawn(async move {
        if let Err(e) = webhook_server.run(webhook_engine, webhook_addr).await {
            warn!(error = %e, "webhook server error");
        }
    });

    // Start filesystem watchers.
    fs_watcher.run(engine.clone()).await?;

    // Start dashboard.
    let dash_storage = storage.clone();
    let dash_addr = config.dashboard_addr;
    tokio::spawn(async move {
        if let Err(e) = run_dashboard(dash_storage, dash_addr).await {
            warn!(error = %e, "dashboard error");
        }
    });

    // Hot-reload: watch workflows directory.
    let (reload_tx, mut reload_rx) = mpsc::channel::<PathBuf>(16);
    let workflows_dir = config.workflows_dir.clone();
    let _watcher = spawn_dir_watcher(workflows_dir, reload_tx)?;

    info!("daemon running. Ctrl+C to stop.");
    info!("  Webhooks : http://{}", config.webhook_addr);
    info!("  Dashboard: http://{}", config.dashboard_addr);

    let mut reload_buf: Vec<PathBuf> = vec![];
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown signal received");
                break;
            }
            Some(path) = reload_rx.recv() => {
                reload_buf.push(path);
                // Drain more within 500ms
                tokio::time::sleep(Duration::from_millis(500)).await;
                while let Ok(p) = reload_rx.try_recv() { reload_buf.push(p); }
                for p in reload_buf.drain(..) {
                    info!(path = %p.display(), "workflow hot-reload (restart daemon to apply)");
                }
            }
        }
    }
    Ok(())
}

fn load_workflows(dir: &Path) -> Vec<(Workflow, String)> {
    let mut result = vec![];
    let Ok(entries) = std::fs::read_dir(dir) else { return result; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") &&
           path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        let Ok(yaml) = std::fs::read_to_string(&path) else { continue; };
        match Workflow::parse(&yaml).and_then(|wf| wf.validate().map(|_| wf)) {
            Ok(wf) => {
                let id = path.to_string_lossy().to_string();
                result.push((wf, id));
            }
            Err(e) => warn!(path = %path.display(), error = %e, "skip invalid workflow"),
        }
    }
    result
}

fn spawn_dir_watcher(
    dir: PathBuf,
    tx: mpsc::Sender<PathBuf>,
) -> Result<RecommendedWatcher> {
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                for path in event.paths {
                    if path.extension().and_then(|e| e.to_str()) == Some("yaml") ||
                       path.extension().and_then(|e| e.to_str()) == Some("yml") {
                        let _ = tx.blocking_send(path);
                    }
                }
            }
        },
        Config::default(),
    )?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct DashState(Arc<Storage>);

pub async fn run_dashboard(storage: Arc<Storage>, addr: SocketAddr) -> Result<()> {
    use axum::{routing::get, Router};
    use tokio::net::TcpListener;

    let app = Router::new()
        .route("/", get(dashboard_html))
        .route("/api/runs", get(api_runs))
        .route("/api/runs/:id/steps", get(api_steps))
        .with_state(DashState(storage));

    info!(%addr, "dashboard listening");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn dashboard_html() -> axum::response::Html<&'static str> {
    axum::response::Html(DASHBOARD_HTML)
}

async fn api_runs(
    axum::extract::State(DashState(storage)): axum::extract::State<DashState>,
) -> axum::Json<serde_json::Value> {
    let runs = storage.list_runs(100).unwrap_or_default();
    axum::Json(serde_json::to_value(&runs).unwrap_or(serde_json::Value::Array(vec![])))
}

async fn api_steps(
    axum::extract::Path(run_id): axum::extract::Path<String>,
    axum::extract::State(DashState(storage)): axum::extract::State<DashState>,
) -> axum::Json<serde_json::Value> {
    let steps = storage.get_step_runs_for_run(&run_id).unwrap_or_default();
    axum::Json(serde_json::to_value(&steps).unwrap_or(serde_json::Value::Array(vec![])))
}

static DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>TaskHub Dashboard</title>
<style>
  body { font-family: monospace; background: #0d1117; color: #c9d1d9; margin: 2rem; }
  h1 { color: #58a6ff; }
  table { border-collapse: collapse; width: 100%; }
  th, td { border: 1px solid #30363d; padding: .5rem 1rem; text-align: left; }
  th { background: #161b22; }
  .success { color: #3fb950; }
  .failed  { color: #f85149; }
  .running { color: #d29922; }
</style>
</head>
<body>
<h1>TaskHub</h1>
<p>Local dashboard — read only.</p>
<div id="runs"><em>Loading...</em></div>
<script>
async function load() {
  const resp = await fetch('/api/runs');
  const runs = await resp.json().catch(() => []);
  const rows = runs.map(r =>
    `<tr><td>${r.id.slice(0,8)}</td><td>${r.workflow_name}</td>
     <td class="${r.status}">${r.status}</td><td>${r.started_at}</td></tr>`
  ).join('');
  document.getElementById('runs').innerHTML =
    `<table><tr><th>Run</th><th>Workflow</th><th>Status</th><th>Started</th></tr>${rows}</table>`;
}
load();
setInterval(load, 5000);
</script>
</body>
</html>"#;
