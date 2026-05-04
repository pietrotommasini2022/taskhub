use async_trait::async_trait;
use reqwest::{Client, Method};
use serde_json::Value;
use std::str::FromStr;
use std::time::Duration;
use taskhub_core::{engine::Action, TaskHubError};

pub struct HttpAction {
    client: Client,
}

impl HttpAction {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
        }
    }
}

impl Default for HttpAction {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Action for HttpAction {
    fn plugin_id(&self) -> &str { "core" }
    fn action_id(&self) -> &str { "http" }

    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        let method_str = input["method"].as_str().unwrap_or("GET");
        let url = input["url"]
            .as_str()
            .ok_or_else(|| TaskHubError::Plugin("core/http: 'url' required".into()))?;

        let timeout_secs = input["timeout"]
            .as_str()
            .and_then(|t| taskhub_core::workflow::parse_every(t).ok())
            .unwrap_or(30);

        let method = Method::from_str(method_str)
            .map_err(|_| TaskHubError::Plugin(format!("core/http: invalid method '{method_str}'")))?;

        let mut req = self
            .client
            .request(method, url)
            .timeout(Duration::from_secs(timeout_secs));

        if let Some(headers) = input["headers"].as_object() {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    req = req.header(k.as_str(), val);
                }
            }
        }

        if let Some(query) = input["query"].as_object() {
            let pairs: Vec<(&str, String)> = query
                .iter()
                .filter_map(|(k, v)| {
                    let s = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
                    Some((k.as_str(), s))
                })
                .collect();
            req = req.query(&pairs);
        }

        if !input["body"].is_null() {
            req = req.json(&input["body"]);
        }

        let start = std::time::Instant::now();
        let resp = req.send().await.map_err(|e| TaskHubError::Plugin(e.to_string()))?;

        let status = resp.status().as_u16();
        let headers_map: serde_json::Map<String, Value> = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    Value::String(v.to_str().unwrap_or("").to_string()),
                )
            })
            .collect();
        let duration_ms = start.elapsed().as_millis() as u64;

        if let Some(expected) = input["expect_status"].as_array() {
            let codes: Vec<u16> = expected
                .iter()
                .filter_map(|v| v.as_u64().map(|n| n as u16))
                .collect();
            if !codes.is_empty() && !codes.contains(&status) {
                return Err(TaskHubError::Plugin(format!(
                    "core/http: got status {status}, expected one of {codes:?}"
                )));
            }
        }

        let body_text = resp.text().await.unwrap_or_default();
        let body_json: Option<Value> = serde_json::from_str(&body_text).ok();

        Ok(serde_json::json!({
            "status": status,
            "headers": headers_map,
            "body_json": body_json,
            "body_text": body_text,
            "duration_ms": duration_ms,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskhub_core::engine::Action;

    #[test]
    fn action_ids() {
        let a = HttpAction::new();
        assert_eq!(a.full_id(), "core/http");
    }
}
