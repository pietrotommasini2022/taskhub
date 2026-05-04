use anyhow::{Context, Result};
use taskhub_core::Workflow;

pub fn run(path: &str) -> Result<()> {
    let yaml = std::fs::read_to_string(path)
        .with_context(|| format!("read {path}"))?;
    let wf = Workflow::parse(&yaml)
        .with_context(|| format!("parse {path}"))?;
    wf.validate()
        .with_context(|| format!("validate {path}"))?;
    println!("{path}: OK");
    Ok(())
}
