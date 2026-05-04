use crate::engine::Engine;
use crate::error::TaskHubError;
use crate::types::TriggerKind;
use crate::workflow::{parse_every, Workflow, TriggerKind as WorkflowTriggerKind};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

/// A single scheduled entry.
pub struct ScheduledWorkflow {
    pub workflow: Workflow,
    pub workflow_id: String,
    /// Interval in seconds.
    pub interval_secs: u64,
}

impl ScheduledWorkflow {
    pub fn from_workflow(workflow: Workflow, workflow_id: String) -> Result<Self, TaskHubError> {
        if workflow.on.trigger != WorkflowTriggerKind::Schedule {
            return Err(TaskHubError::WorkflowParse(
                "workflow trigger is not 'schedule'".into(),
            ));
        }
        let interval_secs = if let Some(ref every) = workflow.on.every {
            parse_every(every)
                .map_err(|e| TaskHubError::WorkflowParse(e))?
        } else if let Some(ref _cron) = workflow.on.cron {
            // Full cron support in M4; for now minimum 60s.
            60
        } else {
            return Err(TaskHubError::WorkflowParse(
                "schedule trigger needs 'every' or 'cron'".into(),
            ));
        };
        Ok(Self { workflow, workflow_id, interval_secs })
    }
}

/// Run a single workflow on a fixed interval until the task is cancelled.
pub async fn run_scheduled(entry: ScheduledWorkflow, engine: Arc<Engine>) {
    let interval = Duration::from_secs(entry.interval_secs);
    info!(
        workflow = %entry.workflow.name,
        interval_secs = entry.interval_secs,
        "scheduler started"
    );
    loop {
        sleep(interval).await;
        info!(workflow = %entry.workflow.name, "scheduler firing");
        match engine
            .run(&entry.workflow, &entry.workflow_id, TriggerKind::Schedule, None)
            .await
        {
            Ok(run) => info!(run_id = %run.id, "scheduled run finished {:?}", run.status),
            Err(e) => error!(error = %e, "scheduled run error"),
        }
    }
}
