use chrono::Utc;
use crate::engine::Engine;
use crate::types::TriggerKind;
use crate::workflow::{parse_every, Workflow, TriggerKind as WorkflowTriggerKind};
use anyhow::Result;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

pub struct FilesystemWatcher {
    workflows: Vec<(Workflow, String)>,
}

impl FilesystemWatcher {
    pub fn new() -> Self {
        Self { workflows: vec![] }
    }

    pub fn register(&mut self, workflow: Workflow, workflow_id: String) {
        self.workflows.push((workflow, workflow_id));
    }

    pub async fn run(self, engine: Arc<Engine>) -> Result<()> {
        for (workflow, workflow_id) in self.workflows {
            let engine = engine.clone();
            tokio::spawn(watch_workflow(workflow, workflow_id, engine));
        }
        Ok(())
    }
}

impl Default for FilesystemWatcher {
    fn default() -> Self {
        Self::new()
    }
}

async fn watch_workflow(workflow: Workflow, workflow_id: String, engine: Arc<Engine>) {
    let watch_path_raw = match &workflow.on.watch_path {
        Some(p) => p.clone(),
        None => return,
    };
    let watch_path_str = shellexpand::tilde(&watch_path_raw).into_owned();
    let watch_path = PathBuf::from(&watch_path_str);

    let debounce_secs = workflow
        .on
        .debounce
        .as_deref()
        .and_then(|d| parse_every(d).ok())
        .unwrap_or(2);

    let patterns = workflow.on.patterns.clone().unwrap_or_default();
    let events = workflow.on.events.clone().unwrap_or_default();
    let recursive = workflow.on.recursive;

    let (tx, mut rx) = mpsc::channel::<Event>(64);

    let mut watcher = match RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        },
        Config::default(),
    ) {
        Ok(w) => w,
        Err(e) => {
            error!(error = %e, path = %watch_path.display(), "failed to create fs watcher");
            return;
        }
    };

    let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
    if let Err(e) = watcher.watch(&watch_path, mode) {
        error!(error = %e, path = %watch_path.display(), "failed to watch path");
        return;
    }

    info!(path = %watch_path.display(), workflow = %workflow.name, "filesystem watcher started");

    let debounce = Duration::from_secs(debounce_secs);

    loop {
        let event = match tokio::time::timeout(Duration::from_secs(60), rx.recv()).await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(_) => continue, // timeout — keep watching
        };

        // Drain debounce window.
        tokio::time::sleep(debounce).await;
        while let Ok(extra) = rx.try_recv() {
            let _ = extra; // discard
        }

        // Check event kind filter.
        if !events.is_empty() {
            let kind_str = event_kind_str(&event.kind);
            if !events.iter().any(|e| e == kind_str) {
                continue;
            }
        }

        // Check pattern filter.
        let paths = &event.paths;
        if !patterns.is_empty() {
            let any_match = paths.iter().any(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                patterns.iter().any(|pat| glob_match(pat, name))
            });
            if !any_match {
                continue;
            }
        }

        let payload = json!({
            "path": paths.first().and_then(|p| p.to_str()).unwrap_or(""),
            "event_kind": event_kind_str(&event.kind),
            "timestamp": chrono::Utc::now().timestamp(),
        });

        info!(path = %watch_path.display(), workflow = %workflow.name, "filesystem trigger fired");
        let wf = workflow.clone();
        let wf_id = workflow_id.clone();
        let eng = engine.clone();
        tokio::spawn(async move {
            if let Err(e) = eng.run(&wf, &wf_id, TriggerKind::Filesystem, Some(payload)).await {
                warn!(error = %e, "filesystem triggered run failed");
            }
        });
    }
}

fn event_kind_str(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Create(_) => "create",
        EventKind::Modify(_) => "modify",
        EventKind::Remove(_) => "delete",
        EventKind::Access(_) => "access",
        _ => "other",
    }
}

/// Simple glob: supports `*` wildcard.
fn glob_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(suffix);
    }
    pattern == name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_patterns() {
        assert!(glob_match("*.pdf", "report.pdf"));
        assert!(glob_match("*.pdf", "doc.pdf"));
        assert!(!glob_match("*.pdf", "doc.zip"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("report.*", "report.pdf"));
        assert!(glob_match("exact.txt", "exact.txt"));
    }
}
