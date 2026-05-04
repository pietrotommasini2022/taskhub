use crate::host::{WasmAction, WasmPlugin};
use crate::manifest::PluginManifest;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use taskhub_core::Engine;
use tracing::info;

pub struct PluginRegistry {
    plugins_dir: PathBuf,
}

impl PluginRegistry {
    pub fn new(plugins_dir: PathBuf) -> Self {
        Self { plugins_dir }
    }

    /// Default location: ~/.taskhub/plugins/
    pub fn default_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".taskhub").join("plugins"))
    }

    /// Install a plugin from a directory containing `plugin.toml` and `plugin.wasm`.
    pub fn install_from_dir(&self, src: &Path) -> Result<PluginManifest> {
        let manifest_path = src.join("plugin.toml");
        let wasm_path = src.join("plugin.wasm");

        let manifest = PluginManifest::from_file(&manifest_path)?;
        anyhow::ensure!(wasm_path.exists(), "plugin.wasm not found in {}", src.display());

        let dest = self.plugins_dir.join(&manifest.id);
        std::fs::create_dir_all(&dest)?;
        std::fs::copy(&manifest_path, dest.join("plugin.toml"))?;
        std::fs::copy(&wasm_path, dest.join("plugin.wasm"))?;

        info!(plugin_id = %manifest.id, version = %manifest.version, "plugin installed");
        Ok(manifest)
    }

    /// List all installed plugins.
    pub fn list(&self) -> Result<Vec<PluginManifest>> {
        if !self.plugins_dir.exists() {
            return Ok(vec![]);
        }
        let mut manifests = vec![];
        for entry in std::fs::read_dir(&self.plugins_dir)? {
            let entry = entry?;
            let manifest_path = entry.path().join("plugin.toml");
            if manifest_path.exists() {
                match PluginManifest::from_file(&manifest_path) {
                    Ok(m) => manifests.push(m),
                    Err(e) => tracing::warn!("skip malformed plugin {}: {}", entry.path().display(), e),
                }
            }
        }
        Ok(manifests)
    }

    /// Remove an installed plugin.
    pub fn remove(&self, plugin_id: &str) -> Result<()> {
        let dir = self.plugins_dir.join(plugin_id);
        anyhow::ensure!(dir.exists(), "plugin '{}' not installed", plugin_id);
        std::fs::remove_dir_all(&dir)?;
        info!(plugin_id, "plugin removed");
        Ok(())
    }

    /// Load all installed plugins and register their actions with the engine.
    pub fn register_all(&self, engine: &mut Engine) -> Result<()> {
        if !self.plugins_dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(&self.plugins_dir)? {
            let entry = entry?;
            let dir = entry.path();
            let manifest_path = dir.join("plugin.toml");
            let wasm_path = dir.join("plugin.wasm");
            if !manifest_path.exists() || !wasm_path.exists() {
                continue;
            }
            match self.load_plugin(&manifest_path, &wasm_path) {
                Ok(plugin) => {
                    let plugin = Arc::new(plugin);
                    for action in &plugin.manifest().actions.clone() {
                        engine.register(Arc::new(WasmAction::new(
                            plugin.clone(),
                            action.id.clone(),
                        )));
                    }
                    info!(plugin_id = %plugin.manifest().id, "plugin loaded");
                }
                Err(e) => tracing::warn!("failed to load plugin {}: {}", dir.display(), e),
            }
        }
        Ok(())
    }

    fn load_plugin(&self, manifest_path: &Path, wasm_path: &Path) -> Result<WasmPlugin> {
        let manifest = PluginManifest::from_file(manifest_path)?;
        let wasm_bytes = std::fs::read(wasm_path)
            .with_context(|| format!("read {}", wasm_path.display()))?;
        WasmPlugin::load(&wasm_bytes, manifest)
    }
}
