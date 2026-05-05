use anyhow::Result;
use std::path::PathBuf;

use crate::commands::init::taskhub_dir;

/// Resolve a user-supplied workflow argument to an absolute path.
///
/// Resolution order:
///   1. Argument is an existing file path → return as-is
///   2. Try ~/.taskhub/workflows/<name>.yaml
///   3. Try ~/.taskhub/workflows/<name>.yml
///   4. Fail with message listing available workflows
pub fn resolve_workflow_path(name: &str) -> Result<PathBuf> {
    let as_path = PathBuf::from(name);
    if as_path.exists() {
        return Ok(as_path);
    }

    let workflows_dir = taskhub_dir()?.join("workflows");
    for ext in ["yaml", "yml"] {
        let candidate = workflows_dir.join(format!("{name}.{ext}"));
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let available = list_workflow_names(&workflows_dir);
    let hint = if available.is_empty() {
        "  (no workflows found — create one with `taskhub workflow new <name>`)".to_string()
    } else {
        format!(
            "  Available workflows:\n{}",
            available
                .iter()
                .map(|n| format!("    {n}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    anyhow::bail!(
        "workflow '{}' not found as a file or in {}\n{}",
        name,
        workflows_dir.display(),
        hint
    )
}

pub fn list_workflow_names(workflows_dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(workflows_dir) else {
        return vec![];
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("yaml" | "yml")
            )
            .then(|| p.file_stem()?.to_str().map(|s| s.to_string()))
            .flatten()
        })
        .collect();
    names.sort();
    names
}
