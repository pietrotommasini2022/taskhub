use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    #[serde(default)]
    pub on: TriggerConfig,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TriggerConfig {
    pub trigger: TriggerKind,
    // schedule
    pub every: Option<String>,
    pub cron: Option<String>,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    // webhook
    pub path: Option<String>,
    pub method: Option<String>,
    pub secret: Option<String>,
    // filesystem
    pub watch_path: Option<String>,
    pub events: Option<Vec<String>>,
    pub patterns: Option<Vec<String>>,
    #[serde(default)]
    pub recursive: bool,
    pub debounce: Option<String>,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    #[default]
    Manual,
    Schedule,
    Webhook,
    Filesystem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    #[serde(rename = "uses")]
    pub uses: String,
    #[serde(default)]
    pub with: HashMap<String, serde_json::Value>,
    #[serde(rename = "if")]
    pub condition: Option<String>,
    pub for_each: Option<String>,
    pub retry: Option<RetryConfig>,
    pub timeout: Option<String>,
    pub on_error: Option<OnError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    pub backoff: Option<String>,
}

fn default_max_attempts() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    Continue,
    Fail,
    Goto(String),
}

impl Workflow {
    pub fn parse(yaml: &str) -> Result<Self, crate::error::TaskHubError> {
        serde_yaml::from_str(yaml)
            .map_err(|e| crate::error::TaskHubError::WorkflowParse(e.to_string()))
    }

    pub fn validate(&self) -> Result<(), crate::error::TaskHubError> {
        if self.name.is_empty() {
            return Err(crate::error::TaskHubError::WorkflowParse(
                "workflow name cannot be empty".into(),
            ));
        }
        if self.steps.is_empty() {
            return Err(crate::error::TaskHubError::WorkflowParse(
                "workflow must have at least one step".into(),
            ));
        }
        let mut ids = std::collections::HashSet::new();
        for step in &self.steps {
            if step.id.is_empty() {
                return Err(crate::error::TaskHubError::WorkflowParse(
                    "step id cannot be empty".into(),
                ));
            }
            if !ids.insert(&step.id) {
                return Err(crate::error::TaskHubError::WorkflowParse(format!(
                    "duplicate step id: {}",
                    step.id
                )));
            }
            if step.uses.is_empty() {
                return Err(crate::error::TaskHubError::WorkflowParse(format!(
                    "step '{}' missing 'uses'",
                    step.id
                )));
            }
            if let Some(ref r) = step.retry {
                if r.max_attempts == 0 {
                    return Err(crate::error::TaskHubError::WorkflowParse(format!(
                        "step '{}' retry.max_attempts must be > 0",
                        step.id
                    )));
                }
            }
        }
        match self.on.trigger {
            TriggerKind::Schedule => {
                if self.on.every.is_none() && self.on.cron.is_none() {
                    return Err(crate::error::TaskHubError::WorkflowParse(
                        "schedule trigger requires 'every' or 'cron'".into(),
                    ));
                }
                if self.on.every.is_some() && self.on.cron.is_some() {
                    return Err(crate::error::TaskHubError::WorkflowParse(
                        "schedule trigger: use 'every' or 'cron', not both".into(),
                    ));
                }
            }
            TriggerKind::Webhook => {
                if self.on.path.is_none() {
                    return Err(crate::error::TaskHubError::WorkflowParse(
                        "webhook trigger requires 'path'".into(),
                    ));
                }
            }
            TriggerKind::Filesystem => {
                if self.on.watch_path.is_none() {
                    return Err(crate::error::TaskHubError::WorkflowParse(
                        "filesystem trigger requires 'watch_path'".into(),
                    ));
                }
            }
            TriggerKind::Manual => {}
        }
        Ok(())
    }
}

/// Parse an `every` duration string like "30s", "5m", "1h", "2h30m", "1d"
/// into seconds.
pub fn parse_every(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let mut total: u64 = 0;
    let mut num_buf = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            num_buf.push(ch);
        } else {
            let n: u64 = num_buf
                .parse()
                .map_err(|_| format!("invalid duration '{s}'"))?;
            num_buf.clear();
            total += match ch {
                's' => n,
                'm' => n * 60,
                'h' => n * 3600,
                'd' => n * 86400,
                _ => return Err(format!("unknown unit '{ch}' in duration '{s}'")),
            };
        }
    }
    if !num_buf.is_empty() {
        return Err(format!("trailing number with no unit in '{s}'"));
    }
    if total == 0 {
        return Err(format!("duration '{s}' must be > 0"));
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manual_workflow() {
        let yaml = r#"
name: test
steps:
  - id: step1
    uses: core/http
    with:
      url: https://example.com
"#;
        let wf = Workflow::parse(yaml).unwrap();
        assert_eq!(wf.name, "test");
        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.on.trigger, TriggerKind::Manual);
    }

    #[test]
    fn validate_rejects_duplicate_step_ids() {
        let yaml = r#"
name: test
steps:
  - id: a
    uses: core/http
  - id: a
    uses: core/shell
"#;
        let wf = Workflow::parse(yaml).unwrap();
        assert!(wf.validate().is_err());
    }

    #[test]
    fn validate_schedule_requires_every_or_cron() {
        let yaml = r#"
name: test
on:
  trigger: schedule
steps:
  - id: s
    uses: core/http
"#;
        let wf = Workflow::parse(yaml).unwrap();
        assert!(wf.validate().is_err());
    }

    #[test]
    fn parse_every_duration() {
        assert_eq!(parse_every("30s").unwrap(), 30);
        assert_eq!(parse_every("5m").unwrap(), 300);
        assert_eq!(parse_every("1h").unwrap(), 3600);
        assert_eq!(parse_every("1d").unwrap(), 86400);
        assert_eq!(parse_every("2h30m").unwrap(), 9000);
        assert!(parse_every("0s").is_err());
        assert!(parse_every("5x").is_err());
    }
}
