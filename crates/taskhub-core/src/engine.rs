use crate::error::TaskHubError;
use crate::storage::Storage;
use crate::template::{resolve, resolve_value, TemplateContext};
use crate::types::{RunStatus, StepRun, StepStatus, TriggerKind, WorkflowRun};
use crate::workflow::{OnError, Workflow};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{error, info, instrument, warn};

/// Trait every built-in action must implement.
#[async_trait]
pub trait Action: Send + Sync {
    fn plugin_id(&self) -> &str;
    fn action_id(&self) -> &str;
    fn full_id(&self) -> String {
        format!("{}/{}", self.plugin_id(), self.action_id())
    }
    async fn execute(&self, input: Value) -> Result<Value, TaskHubError>;
}

pub struct Engine {
    storage: Arc<Storage>,
    actions: HashMap<String, Arc<dyn Action>>,
    dry_run: bool,
}

impl Engine {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            actions: HashMap::new(),
            dry_run: false,
        }
    }

    pub fn with_dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    pub fn register(&mut self, action: Arc<dyn Action>) {
        self.actions.insert(action.full_id(), action);
    }

    #[instrument(skip(self, workflow), fields(workflow = %workflow.name))]
    pub async fn run(
        &self,
        workflow: &Workflow,
        workflow_id: &str,
        trigger_kind: TriggerKind,
        trigger_payload: Option<Value>,
    ) -> Result<WorkflowRun, TaskHubError> {
        let run_id = Storage::new_run_id();
        let mut run = WorkflowRun {
            id: run_id.clone(),
            workflow_id: workflow_id.to_string(),
            workflow_name: workflow.name.clone(),
            status: RunStatus::Running,
            trigger_kind,
            trigger_payload: trigger_payload.clone(),
            started_at: Utc::now(),
            finished_at: None,
            error: None,
        };

        if !self.dry_run {
            self.storage.insert_run(&run)?;
        }
        info!(run_id = %run_id, "workflow run started");

        let mut ctx = TemplateContext::default();
        if let Some(ref payload) = trigger_payload {
            if let Value::Object(map) = payload {
                for (k, v) in map {
                    ctx.trigger.insert(k.clone(), v.clone());
                }
            }
        }

        let result = self.execute_steps(workflow, &run_id, &mut ctx).await;

        run.finished_at = Some(Utc::now());
        match result {
            Ok(()) => {
                run.status = RunStatus::Success;
                info!(run_id = %run_id, "workflow run succeeded");
            }
            Err(ref e) => {
                run.status = RunStatus::Failed;
                run.error = Some(e.to_string());
                error!(run_id = %run_id, error = %e, "workflow run failed");
            }
        }

        if !self.dry_run {
            self.storage.update_run_status(
                &run_id,
                &run.status,
                run.finished_at,
                run.error.as_deref(),
            )?;
        }

        Ok(run)
    }

    async fn execute_steps(
        &self,
        workflow: &Workflow,
        run_id: &str,
        ctx: &mut TemplateContext,
    ) -> Result<(), TaskHubError> {
        for step in &workflow.steps {
            // Evaluate `if` condition.
            if let Some(ref cond) = step.condition {
                let resolved = resolve(cond, ctx);
                if !is_truthy(&resolved) {
                    info!(step_id = %step.id, "step skipped (condition false)");
                    if !self.dry_run {
                        let sr = StepRun {
                            id: Storage::new_step_run_id(),
                            run_id: run_id.to_string(),
                            step_id: step.id.clone(),
                            status: StepStatus::Skipped,
                            output: None,
                            error: None,
                            attempt: 1,
                            started_at: Utc::now(),
                            finished_at: Some(Utc::now()),
                        };
                        self.storage.insert_step_run(&sr)?;
                    }
                    continue;
                }
            }

            // Resolve `with` inputs against context.
            let input_base = Value::Object(
                step.with
                    .iter()
                    .map(|(k, v)| (k.clone(), resolve_value(v, ctx)))
                    .collect(),
            );

            // Handle `for_each`.
            if let Some(ref each_expr) = step.for_each {
                let resolved = resolve(each_expr, ctx);
                let items: Vec<Value> = if let Ok(v) = serde_json::from_str::<Value>(&resolved) {
                    match v {
                        Value::Array(arr) => arr,
                        other => vec![other],
                    }
                } else {
                    vec![Value::String(resolved)]
                };

                for item in items {
                    ctx.item = Some(item.clone());
                    let input = resolve_value(&input_base, ctx);
                    let output = self
                        .execute_step_with_retry(run_id, step, input)
                        .await;
                    ctx.item = None;
                    match output {
                        Ok(out) => ctx.set_step_output(&step.id, out),
                        Err(e) => match &step.on_error {
                            Some(OnError::Continue) => {
                                warn!(step_id = %step.id, error = %e, "step error, continuing");
                            }
                            _ => return Err(e),
                        },
                    }
                }
            } else {
                let input = resolve_value(&input_base, ctx);
                let output = self
                    .execute_step_with_retry(run_id, step, input)
                    .await;
                match output {
                    Ok(out) => ctx.set_step_output(&step.id, out),
                    Err(e) => match &step.on_error {
                        Some(OnError::Continue) => {
                            warn!(step_id = %step.id, error = %e, "step error, continuing");
                        }
                        _ => return Err(e),
                    },
                }
            }
        }
        Ok(())
    }

    async fn execute_step_with_retry(
        &self,
        run_id: &str,
        step: &crate::workflow::Step,
        input: Value,
    ) -> Result<Value, TaskHubError> {
        let max_attempts = step.retry.as_ref().map(|r| r.max_attempts).unwrap_or(1);
        let backoff_secs = step
            .retry
            .as_ref()
            .and_then(|r| r.backoff.as_deref())
            .and_then(|b| crate::workflow::parse_every(b).ok())
            .unwrap_or(1);
        let step_timeout = step
            .timeout
            .as_deref()
            .and_then(|t| crate::workflow::parse_every(t).ok())
            .map(Duration::from_secs);

        let mut last_err = None;
        for attempt in 1..=max_attempts {
            let sr_id = Storage::new_step_run_id();
            let started_at = Utc::now();

            if !self.dry_run {
                let sr = StepRun {
                    id: sr_id.clone(),
                    run_id: run_id.to_string(),
                    step_id: step.id.clone(),
                    status: StepStatus::Running,
                    output: None,
                    error: None,
                    attempt,
                    started_at,
                    finished_at: None,
                };
                self.storage.insert_step_run(&sr)?;
            }

            let result = self.dispatch_action(&step.uses, input.clone(), step_timeout).await;
            let finished_at = Utc::now();

            match result {
                Ok(output) => {
                    if !self.dry_run {
                        self.storage.update_step_run(
                            &sr_id,
                            &StepStatus::Success,
                            Some(&output),
                            None,
                            finished_at,
                        )?;
                    }
                    info!(step_id = %step.id, attempt, "step succeeded");
                    return Ok(output);
                }
                Err(e) => {
                    if !self.dry_run {
                        self.storage.update_step_run(
                            &sr_id,
                            &StepStatus::Failed,
                            None,
                            Some(&e.to_string()),
                            finished_at,
                        )?;
                    }
                    warn!(step_id = %step.id, attempt, error = %e, "step attempt failed");
                    last_err = Some(e);
                    if attempt < max_attempts {
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| {
            TaskHubError::Plugin(format!("step '{}' failed with no error", step.id))
        }))
    }

    async fn dispatch_action(
        &self,
        uses: &str,
        input: Value,
        step_timeout: Option<Duration>,
    ) -> Result<Value, TaskHubError> {
        if self.dry_run {
            info!(uses, "dry-run: would execute action");
            return Ok(Value::Null);
        }

        let action = self
            .actions
            .get(uses)
            .ok_or_else(|| TaskHubError::Plugin(format!("unknown action: {uses}")))?;

        let fut = action.execute(input);
        if let Some(dur) = step_timeout {
            timeout(dur, fut)
                .await
                .map_err(|_| TaskHubError::Plugin(format!("action '{uses}' timed out")))?
        } else {
            fut.await
        }
    }
}

fn is_truthy(s: &str) -> bool {
    !matches!(s.trim(), "" | "false" | "0" | "null")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;
    use crate::workflow::Workflow;
    use std::sync::Arc;

    struct EchoAction;
    #[async_trait]
    impl Action for EchoAction {
        fn plugin_id(&self) -> &str { "test" }
        fn action_id(&self) -> &str { "echo" }
        async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
            Ok(input)
        }
    }

    struct FailAction;
    #[async_trait]
    impl Action for FailAction {
        fn plugin_id(&self) -> &str { "test" }
        fn action_id(&self) -> &str { "fail" }
        async fn execute(&self, _: Value) -> Result<Value, TaskHubError> {
            Err(TaskHubError::Plugin("intentional failure".into()))
        }
    }

    fn engine_with_echo() -> Engine {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let mut engine = Engine::new(storage);
        engine.register(Arc::new(EchoAction));
        engine
    }

    #[tokio::test]
    async fn runs_single_step() {
        let engine = engine_with_echo();
        let yaml = r#"
name: test
steps:
  - id: s1
    uses: test/echo
    with:
      msg: hello
"#;
        let wf = Workflow::parse(yaml).unwrap();
        let run = engine.run(&wf, "wf-1", TriggerKind::Manual, None).await.unwrap();
        assert_eq!(run.status, RunStatus::Success);
    }

    #[tokio::test]
    async fn step_if_false_skips() {
        let engine = engine_with_echo();
        let yaml = r#"
name: test
steps:
  - id: s1
    uses: test/echo
    if: "false"
"#;
        let wf = Workflow::parse(yaml).unwrap();
        let run = engine.run(&wf, "wf-1", TriggerKind::Manual, None).await.unwrap();
        assert_eq!(run.status, RunStatus::Success);
    }

    #[tokio::test]
    async fn on_error_continue_doesnt_fail_run() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let mut engine = Engine::new(storage);
        engine.register(Arc::new(FailAction));
        let yaml = r#"
name: test
steps:
  - id: s1
    uses: test/fail
    on_error: continue
"#;
        let wf = Workflow::parse(yaml).unwrap();
        let run = engine.run(&wf, "wf-1", TriggerKind::Manual, None).await.unwrap();
        assert_eq!(run.status, RunStatus::Success);
    }

    #[tokio::test]
    async fn dry_run_succeeds_without_storage_writes() {
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let engine = Engine::new(storage.clone()).with_dry_run();
        let yaml = r#"
name: test
steps:
  - id: s1
    uses: test/echo
"#;
        let wf = Workflow::parse(yaml).unwrap();
        let run = engine.run(&wf, "wf-1", TriggerKind::Manual, None).await.unwrap();
        assert_eq!(run.status, RunStatus::Success);
        // No rows written in dry-run
        assert_eq!(storage.list_runs(10).unwrap().len(), 0);
    }

    #[test]
    fn is_truthy_cases() {
        assert!(is_truthy("true"));
        assert!(is_truthy("1"));
        assert!(!is_truthy("false"));
        assert!(!is_truthy("0"));
        assert!(!is_truthy("null"));
        assert!(!is_truthy(""));
    }
}
