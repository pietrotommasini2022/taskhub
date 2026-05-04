use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use taskhub_wasm_host::PluginRegistry;

use crate::commands::init::taskhub_dir;

fn registry() -> Result<PluginRegistry> {
    let dir = taskhub_dir()?.join("plugins");
    Ok(PluginRegistry::new(dir))
}

pub fn install(source: &str) -> Result<()> {
    let src = PathBuf::from(source);
    anyhow::ensure!(src.exists(), "path does not exist: {source}");
    let reg = registry()?;
    std::fs::create_dir_all(taskhub_dir()?.join("plugins"))?;
    let manifest = reg.install_from_dir(&src)?;
    println!("Installed plugin '{}' v{}", manifest.id, manifest.version);
    Ok(())
}

pub fn list() -> Result<()> {
    let reg = registry()?;
    let plugins = reg.list()?;
    if plugins.is_empty() {
        println!("No plugins installed. Use `taskhub plugin install <path>`.");
        return Ok(());
    }
    println!("{:<20} {:<10} {}", "ID", "VERSION", "DESCRIPTION");
    for p in &plugins {
        println!("{:<20} {:<10} {}", p.id, p.version, p.description);
    }
    Ok(())
}

pub fn info(id: &str) -> Result<()> {
    let reg = registry()?;
    let plugins = reg.list()?;
    let p = plugins.iter().find(|p| p.id == id)
        .ok_or_else(|| anyhow::anyhow!("plugin '{}' not installed", id))?;
    println!("ID:          {}", p.id);
    println!("Version:     {}", p.version);
    println!("Description: {}", p.description);
    println!("Author:      {}", p.author);
    println!("License:     {}", p.license);
    if !p.actions.is_empty() {
        println!("Actions:");
        for a in &p.actions {
            println!("  {:<20} {}", a.id, a.description);
        }
    }
    println!("Permissions:");
    if !p.permissions.network.is_empty() {
        println!("  network:    {:?}", p.permissions.network);
    }
    if !p.permissions.filesystem.is_empty() {
        println!("  filesystem: {:?}", p.permissions.filesystem);
    }
    if !p.permissions.secrets.is_empty() {
        println!("  secrets:    {:?}", p.permissions.secrets);
    }
    if p.permissions.shell {
        println!("  shell:      true");
    }
    Ok(())
}

pub fn remove(id: &str) -> Result<()> {
    let reg = registry()?;
    reg.remove(id)?;
    println!("Removed plugin '{id}'");
    Ok(())
}

pub fn new_plugin(name: &str) -> Result<()> {
    let dest = PathBuf::from(name);
    anyhow::ensure!(!dest.exists(), "directory '{}' already exists", name);

    // Find template directory relative to this binary's location or repo.
    let template = find_template()?;

    copy_dir_replace(&template, &dest, name)?;

    println!("Plugin scaffolded at ./{name}");
    println!("Next steps:");
    println!("  1. Edit {name}/plugin.toml — set id, description, permissions");
    println!("  2. Edit {name}/src/lib.rs — implement your actions");
    println!("  3. rustup target add wasm32-wasip1");
    println!("  4. cargo build --target wasm32-wasip1 --release --manifest-path {name}/Cargo.toml");
    println!("  5. cp {name}/target/wasm32-wasip1/release/*.wasm {name}/plugin.wasm");
    println!("  6. taskhub plugin install ./{name}");
    Ok(())
}

fn find_template() -> Result<PathBuf> {
    // Check next to binary first, then repo-relative path.
    let candidates = [
        PathBuf::from("plugins/_template"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("plugins/_template")))
            .unwrap_or_default(),
    ];
    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }
    anyhow::bail!(
        "plugin template not found. Expected at plugins/_template relative to current directory."
    )
}

fn copy_dir_replace(src: &Path, dst: &Path, plugin_name: &str) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_replace(&src_path, &dst_path, plugin_name)?;
        } else {
            let content = std::fs::read_to_string(&src_path)
                .with_context(|| format!("read {}", src_path.display()))?;
            let content = content.replace("{{plugin_name}}", plugin_name);
            std::fs::write(&dst_path, content)?;
        }
    }
    Ok(())
}
