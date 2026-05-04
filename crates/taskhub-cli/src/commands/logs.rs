use anyhow::{Context, Result};
use taskhub_core::Storage;

use crate::commands::init::db_path;

pub fn run(workflow: &str, run_id: Option<&str>) -> Result<()> {
    let db = db_path()?;
    if !db.exists() {
        anyhow::bail!("taskhub not initialized — run `taskhub init` first");
    }
    let storage = Storage::open(&db).context("open storage")?;

    // Find the matching run(s).
    let all_runs = storage.list_runs(200)?;
    let matching: Vec<_> = all_runs
        .iter()
        .filter(|r| {
            let name_match = r.workflow_name == workflow || r.workflow_id == workflow;
            match run_id {
                Some(id) => name_match && r.id == id,
                None => name_match,
            }
        })
        .collect();

    if matching.is_empty() {
        println!("No runs found for workflow '{workflow}'.");
        return Ok(());
    }

    // Show most recent if no run_id.
    let run = if run_id.is_some() { matching[0] } else { matching[0] };

    println!("Run {} — {:?}", run.id, run.status);
    println!("  Workflow : {}", run.workflow_name);
    println!("  Trigger  : {:?}", run.trigger_kind);
    println!("  Started  : {}", run.started_at.format("%Y-%m-%d %H:%M:%S UTC"));
    if let Some(f) = run.finished_at {
        println!("  Finished : {}", f.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    if let Some(ref e) = run.error {
        println!("  Error    : {e}");
    }

    let steps = storage.get_step_runs_for_run(&run.id)?;
    if !steps.is_empty() {
        println!("\nSteps:");
        for sr in &steps {
            println!(
                "  [{:?}] {} (attempt {})",
                sr.status, sr.step_id, sr.attempt
            );
            if let Some(ref out) = sr.output {
                println!("    output: {}", out);
            }
            if let Some(ref e) = sr.error {
                println!("    error: {e}");
            }
        }
    }
    Ok(())
}
