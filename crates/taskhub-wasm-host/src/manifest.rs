use serde::{Deserialize, Serialize};
use std::path::Path;
use anyhow::{Context, Result};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginPermissions {
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub filesystem: Vec<String>,
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(default)]
    pub shell: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDef {
    pub id: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub actions: Vec<ActionDef>,
    #[serde(default)]
    pub permissions: PluginPermissions,
}

impl PluginManifest {
    pub fn from_toml(s: &str) -> Result<Self> {
        toml::from_str(s).context("parse plugin.toml")
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("read {}", path.display()))?;
        Self::from_toml(&s)
    }

    pub fn has_action(&self, id: &str) -> bool {
        self.actions.iter().any(|a| a.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest() {
        let toml = r#"
id = "weather"
version = "0.1.0"
description = "Get weather information"
author = "test"
license = "MIT"

[[actions]]
id = "get"
description = "Get current weather"

[permissions]
network = ["wttr.in"]
"#;
        let m = PluginManifest::from_toml(toml).unwrap();
        assert_eq!(m.id, "weather");
        assert_eq!(m.actions.len(), 1);
        assert!(m.has_action("get"));
        assert_eq!(m.permissions.network, vec!["wttr.in"]);
    }
}
