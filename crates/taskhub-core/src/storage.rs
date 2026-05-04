use crate::error::TaskHubError;
use crate::schema::{SCHEMA_SQL, SCHEMA_VERSION};
use crate::types::{RunStatus, StepRun, StepStatus, TriggerKind, WorkflowRun};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

pub struct Storage {
    conn: Mutex<Connection>,
}

// Connection is Send but not Sync. Mutex<Connection> is both Send + Sync.
unsafe impl Sync for Storage {}

impl Storage {
    pub fn open(path: &Path) -> Result<Self, TaskHubError> {
        let conn = Connection::open(path)?;
        let s = Self { conn: Mutex::new(conn) };
        s.migrate()?;
        Ok(s)
    }

    pub fn open_in_memory() -> Result<Self, TaskHubError> {
        let conn = Connection::open_in_memory()?;
        let s = Self { conn: Mutex::new(conn) };
        s.migrate()?;
        Ok(s)
    }

    fn migrate(&self) -> Result<(), TaskHubError> {
        let conn = self.conn.lock().unwrap();
        let user_version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if user_version < SCHEMA_VERSION {
            conn.execute_batch(SCHEMA_SQL)?;
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        }
        Ok(())
    }

    // ── runs ──────────────────────────────────────────────────────────────

    pub fn insert_run(&self, run: &WorkflowRun) -> Result<(), TaskHubError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO runs (id, workflow_id, workflow_name, status, trigger_kind,
             trigger_payload, started_at, finished_at, error)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                run.id, run.workflow_id, run.workflow_name,
                status_str(&run.status), trigger_str(&run.trigger_kind),
                run.trigger_payload.as_ref().map(|v| v.to_string()),
                run.started_at.timestamp(),
                run.finished_at.map(|t| t.timestamp()),
                run.error,
            ],
        )?;
        Ok(())
    }

    pub fn update_run_status(
        &self, id: &str, status: &RunStatus,
        finished_at: Option<DateTime<Utc>>, error: Option<&str>,
    ) -> Result<(), TaskHubError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE runs SET status=?1, finished_at=?2, error=?3 WHERE id=?4",
            params![status_str(status), finished_at.map(|t| t.timestamp()), error, id],
        )?;
        Ok(())
    }

    pub fn list_runs(&self, limit: usize) -> Result<Vec<WorkflowRun>, TaskHubError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, workflow_id, workflow_name, status, trigger_kind,
                    trigger_payload, started_at, finished_at, error
             FROM runs ORDER BY started_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_run)?;
        rows.map(|r| r.map_err(TaskHubError::Storage)).collect()
    }

    // ── step_runs ─────────────────────────────────────────────────────────

    pub fn insert_step_run(&self, sr: &StepRun) -> Result<(), TaskHubError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO step_runs (id, run_id, step_id, status, output, error, attempt, started_at, finished_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                sr.id, sr.run_id, sr.step_id,
                step_status_str(&sr.status),
                sr.output.as_ref().map(|v| v.to_string()),
                sr.error, sr.attempt,
                sr.started_at.timestamp(),
                sr.finished_at.map(|t| t.timestamp()),
            ],
        )?;
        Ok(())
    }

    pub fn update_step_run(
        &self, id: &str, status: &StepStatus,
        output: Option<&serde_json::Value>, error: Option<&str>,
        finished_at: DateTime<Utc>,
    ) -> Result<(), TaskHubError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE step_runs SET status=?1, output=?2, error=?3, finished_at=?4 WHERE id=?5",
            params![
                step_status_str(status), output.map(|v| v.to_string()),
                error, finished_at.timestamp(), id,
            ],
        )?;
        Ok(())
    }

    pub fn get_step_runs_for_run(&self, run_id: &str) -> Result<Vec<StepRun>, TaskHubError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, run_id, step_id, status, output, error, attempt, started_at, finished_at
             FROM step_runs WHERE run_id=?1 ORDER BY started_at",
        )?;
        let rows = stmt.query_map(params![run_id], row_to_step_run)?;
        rows.map(|r| r.map_err(TaskHubError::Storage)).collect()
    }

    // ── credentials ──────────────────────────────────────────────────────

    pub fn upsert_credential(&self, key: &str, ciphertext: &[u8]) -> Result<(), TaskHubError> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO credentials (key, ciphertext, created_at, updated_at) VALUES (?1,?2,?3,?3)
             ON CONFLICT(key) DO UPDATE SET ciphertext=excluded.ciphertext, updated_at=excluded.updated_at",
            params![key, ciphertext, now],
        )?;
        Ok(())
    }

    pub fn get_credential(&self, key: &str) -> Result<Option<Vec<u8>>, TaskHubError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT ciphertext FROM credentials WHERE key=?1")?;
        let mut rows = stmt.query(params![key])?;
        Ok(if let Some(row) = rows.next()? { Some(row.get(0)?) } else { None })
    }

    pub fn list_credential_keys(&self) -> Result<Vec<String>, TaskHubError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key FROM credentials ORDER BY key")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.map(|r| r.map_err(TaskHubError::Storage)).collect()
    }

    pub fn delete_credential(&self, key: &str) -> Result<(), TaskHubError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM credentials WHERE key=?1", params![key])?;
        Ok(())
    }

    // ── helpers ───────────────────────────────────────────────────────────

    pub fn new_run_id() -> String { Uuid::now_v7().to_string() }
    pub fn new_step_run_id() -> String { Uuid::now_v7().to_string() }
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRun> {
    let status_s: String = row.get(3)?;
    let trigger_s: String = row.get(4)?;
    let payload_s: Option<String> = row.get(5)?;
    let started: i64 = row.get(6)?;
    let finished: Option<i64> = row.get(7)?;
    Ok(WorkflowRun {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        workflow_name: row.get(2)?,
        status: parse_run_status(&status_s),
        trigger_kind: parse_trigger_kind(&trigger_s),
        trigger_payload: payload_s.and_then(|s| serde_json::from_str(&s).ok()),
        started_at: DateTime::from_timestamp(started, 0).unwrap_or_default(),
        finished_at: finished.and_then(|t| DateTime::from_timestamp(t, 0)),
        error: row.get(8)?,
    })
}

fn row_to_step_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<StepRun> {
    let status_s: String = row.get(3)?;
    let output_s: Option<String> = row.get(4)?;
    let started: i64 = row.get(7)?;
    let finished: Option<i64> = row.get(8)?;
    Ok(StepRun {
        id: row.get(0)?,
        run_id: row.get(1)?,
        step_id: row.get(2)?,
        status: parse_step_status(&status_s),
        output: output_s.and_then(|s| serde_json::from_str(&s).ok()),
        error: row.get(5)?,
        attempt: row.get::<_, i64>(6)? as u32,
        started_at: DateTime::from_timestamp(started, 0).unwrap_or_default(),
        finished_at: finished.and_then(|t| DateTime::from_timestamp(t, 0)),
    })
}

fn status_str(s: &RunStatus) -> &'static str {
    match s {
        RunStatus::Pending => "pending", RunStatus::Running => "running",
        RunStatus::Success => "success", RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
    }
}
fn trigger_str(t: &TriggerKind) -> &'static str {
    match t {
        TriggerKind::Schedule => "schedule", TriggerKind::Webhook => "webhook",
        TriggerKind::Filesystem => "filesystem", TriggerKind::Manual => "manual",
    }
}
fn step_status_str(s: &StepStatus) -> &'static str {
    match s {
        StepStatus::Pending => "pending", StepStatus::Running => "running",
        StepStatus::Success => "success", StepStatus::Failed => "failed",
        StepStatus::Skipped => "skipped",
    }
}
fn parse_run_status(s: &str) -> RunStatus {
    match s {
        "pending" => RunStatus::Pending, "running" => RunStatus::Running,
        "success" => RunStatus::Success, "failed" => RunStatus::Failed,
        _ => RunStatus::Cancelled,
    }
}
fn parse_trigger_kind(s: &str) -> TriggerKind {
    match s {
        "schedule" => TriggerKind::Schedule, "webhook" => TriggerKind::Webhook,
        "filesystem" => TriggerKind::Filesystem, _ => TriggerKind::Manual,
    }
}
fn parse_step_status(s: &str) -> StepStatus {
    match s {
        "pending" => StepStatus::Pending, "running" => StepStatus::Running,
        "success" => StepStatus::Success, "failed" => StepStatus::Failed,
        _ => StepStatus::Skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RunStatus, StepStatus, TriggerKind};

    fn make_run(id: &str) -> WorkflowRun {
        WorkflowRun {
            id: id.to_string(), workflow_id: "wf1".to_string(),
            workflow_name: "My Workflow".to_string(), status: RunStatus::Running,
            trigger_kind: TriggerKind::Manual, trigger_payload: None,
            started_at: Utc::now(), finished_at: None, error: None,
        }
    }

    #[test]
    fn insert_and_list_run() {
        let s = Storage::open_in_memory().unwrap();
        s.insert_run(&make_run("run-1")).unwrap();
        let runs = s.list_runs(10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, "run-1");
    }

    #[test]
    fn update_run_status() {
        let s = Storage::open_in_memory().unwrap();
        s.insert_run(&make_run("run-2")).unwrap();
        s.update_run_status("run-2", &RunStatus::Success, Some(Utc::now()), None).unwrap();
        let runs = s.list_runs(10).unwrap();
        assert_eq!(runs[0].status, RunStatus::Success);
    }

    #[test]
    fn step_run_roundtrip() {
        let s = Storage::open_in_memory().unwrap();
        s.insert_run(&make_run("run-3")).unwrap();
        let sr = StepRun {
            id: "sr-1".to_string(), run_id: "run-3".to_string(),
            step_id: "fetch".to_string(), status: StepStatus::Success,
            output: Some(serde_json::json!({"ok": true})),
            error: None, attempt: 1,
            started_at: Utc::now(), finished_at: Some(Utc::now()),
        };
        s.insert_step_run(&sr).unwrap();
        let steps = s.get_step_runs_for_run("run-3").unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].step_id, "fetch");
    }
}
