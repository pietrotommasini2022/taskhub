use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::commands::init::taskhub_dir;

pub fn new(name: &str) -> Result<()> {
    let clean = name.trim_end_matches(".yaml").trim_end_matches(".yml");
    anyhow::ensure!(
        !clean.contains('/') && !clean.contains('\\'),
        "workflow name must not contain path separators"
    );

    let workflows_dir = taskhub_dir()?.join("workflows");
    std::fs::create_dir_all(&workflows_dir)?;

    let dest = workflows_dir.join(format!("{clean}.yaml"));
    anyhow::ensure!(
        !dest.exists(),
        "workflow '{}' already exists at {}",
        clean,
        dest.display()
    );

    std::fs::write(&dest, workflow_template(clean))
        .with_context(|| format!("write {}", dest.display()))?;

    println!("Created: {}", dest.display());
    open_in_editor(&dest)
}

pub fn open(name: Option<&str>) -> Result<()> {
    let target: PathBuf = match name {
        None => taskhub_dir()?.join("workflows"),
        Some(n) => crate::resolve::resolve_workflow_path(n)?,
    };
    open_in_editor(&target)
}

pub fn list() -> Result<()> {
    let workflows_dir = taskhub_dir()?.join("workflows");
    let mut rows: Vec<(String, String)> = std::fs::read_dir(&workflows_dir)
        .with_context(|| format!("read {}", workflows_dir.display()))?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("yaml" | "yml")
            )
            .then(|| {
                let stem = p.file_stem()?.to_str()?.to_string();
                let trigger = std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|yaml| taskhub_core::Workflow::parse(&yaml).ok())
                    .map(|wf| format!("{:?}", wf.on.trigger).to_lowercase())
                    .unwrap_or_else(|| "parse error".into());
                Some((stem, trigger))
            })?
        })
        .collect();

    if rows.is_empty() {
        println!("No workflows found. Create one with `taskhub workflow new <name>`.");
        return Ok(());
    }

    rows.sort_by(|a, b| a.0.cmp(&b.0));
    println!("{:<30} TRIGGER", "NAME");
    for (name, trigger) in &rows {
        println!("{:<30} {}", name, trigger);
    }
    Ok(())
}

fn workflow_template(name: &str) -> String {
    format!(
        r#"name: {name}

on:
  trigger: manual
  # To run on a schedule instead:
  # trigger: schedule
  # every: 5m

steps:
  - id: run
    uses: core/shell
    with:
      command: echo "Hello from {name}"
"#
    )
}

fn open_in_editor(path: &std::path::Path) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .ok()
        .or_else(find_editor_fallback);

    match editor {
        Some(ed) => {
            std::process::Command::new(&ed)
                .arg(path)
                .status()
                .with_context(|| format!("launch editor '{ed}'"))?;
        }
        None => {
            println!("Open manually:");
            println!("  {}", path.display());
        }
    }
    Ok(())
}

fn find_editor_fallback() -> Option<String> {
    let check_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    for editor in ["code", "notepad"] {
        if std::process::Command::new(check_cmd)
            .arg(editor)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(editor.to_string());
        }
    }
    None
}
