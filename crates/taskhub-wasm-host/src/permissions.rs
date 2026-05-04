use crate::manifest::PluginPermissions;
use anyhow::{bail, Result};
use url::Url;

pub struct PermissionChecker<'a> {
    perms: &'a PluginPermissions,
    plugin_id: &'a str,
}

impl<'a> PermissionChecker<'a> {
    pub fn new(plugin_id: &'a str, perms: &'a PluginPermissions) -> Self {
        Self { perms, plugin_id }
    }

    pub fn check_network(&self, url: &str) -> Result<()> {
        let host = extract_host(url)?;
        if self.perms.network.iter().any(|allowed| {
            allowed == "*" || allowed == &host || host.ends_with(&format!(".{allowed}"))
        }) {
            return Ok(());
        }
        bail!(
            "plugin '{}' network permission denied: host '{}' not in allowlist {:?}",
            self.plugin_id,
            host,
            self.perms.network
        )
    }

    pub fn check_shell(&self) -> Result<()> {
        if self.perms.shell {
            return Ok(());
        }
        bail!(
            "plugin '{}' shell permission denied: 'shell: true' not declared in manifest",
            self.plugin_id
        )
    }

    pub fn check_secret(&self, key: &str) -> Result<()> {
        if self.perms.secrets.iter().any(|s| s == "*" || s == key) {
            return Ok(());
        }
        bail!(
            "plugin '{}' secret permission denied: key '{}' not in allowlist {:?}",
            self.plugin_id,
            key,
            self.perms.secrets
        )
    }

    pub fn check_filesystem(&self, path: &str) -> Result<()> {
        if self.perms.filesystem.iter().any(|allowed| {
            allowed == "*" || path.starts_with(allowed.trim_end_matches('*'))
        }) {
            return Ok(());
        }
        bail!(
            "plugin '{}' filesystem permission denied: path '{}' not in allowlist {:?}",
            self.plugin_id,
            path,
            self.perms.filesystem
        )
    }
}

fn extract_host(url: &str) -> Result<String> {
    let parsed = Url::parse(url)?;
    parsed
        .host_str()
        .map(|h| h.to_string())
        .ok_or_else(|| anyhow::anyhow!("URL has no host: {url}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginPermissions;

    fn perms(network: &[&str]) -> PluginPermissions {
        PluginPermissions {
            network: network.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn network_exact_match() {
        let p = perms(&["wttr.in"]);
        let c = PermissionChecker::new("weather", &p);
        assert!(c.check_network("https://wttr.in/London").is_ok());
        assert!(c.check_network("https://evil.com/steal").is_err());
    }

    #[test]
    fn network_wildcard() {
        let p = perms(&["*"]);
        let c = PermissionChecker::new("any", &p);
        assert!(c.check_network("https://anything.example.com").is_ok());
    }

    #[test]
    fn network_subdomain() {
        let p = perms(&["example.com"]);
        let c = PermissionChecker::new("p", &p);
        assert!(c.check_network("https://api.example.com/v1").is_ok());
        assert!(c.check_network("https://notexample.com").is_err());
    }

    #[test]
    fn shell_denied_by_default() {
        let p = PluginPermissions::default();
        let c = PermissionChecker::new("p", &p);
        assert!(c.check_shell().is_err());
    }
}
