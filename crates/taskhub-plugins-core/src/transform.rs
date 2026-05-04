use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use taskhub_core::{engine::Action, TaskHubError};

pub struct TransformAction;

#[async_trait]
impl Action for TransformAction {
    fn plugin_id(&self) -> &str { "core" }
    fn action_id(&self) -> &str { "transform" }

    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        let action = input["action"]
            .as_str()
            .ok_or_else(|| TaskHubError::Plugin("core/transform: 'action' required".into()))?;

        match action {
            "json.parse" => {
                let s = input["input"].as_str()
                    .ok_or_else(|| TaskHubError::Plugin("json.parse: 'input' string required".into()))?;
                serde_json::from_str(s).map_err(|e| TaskHubError::Plugin(format!("json.parse: {e}")))
            }
            "json.stringify" => Ok(Value::String(input["input"].to_string())),
            "merge" => {
                match (&input["a"], &input["b"]) {
                    (Value::Object(ma), Value::Object(mb)) => {
                        let mut out = ma.clone();
                        for (k, v) in mb { out.insert(k.clone(), v.clone()); }
                        Ok(Value::Object(out))
                    }
                    _ => Err(TaskHubError::Plugin("merge: 'a' and 'b' must be objects".into())),
                }
            }
            "pick" => {
                let obj = input["input"].as_object()
                    .ok_or_else(|| TaskHubError::Plugin("pick: 'input' must be object".into()))?;
                let empty = vec![];
                let keys: Vec<&str> = input["keys"].as_array().unwrap_or(&empty)
                    .iter().filter_map(|v| v.as_str()).collect();
                Ok(Value::Object(obj.iter()
                    .filter(|(k, _)| keys.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()))
            }
            "omit" => {
                let obj = input["input"].as_object()
                    .ok_or_else(|| TaskHubError::Plugin("omit: 'input' must be object".into()))?;
                let empty = vec![];
                let keys: Vec<&str> = input["keys"].as_array().unwrap_or(&empty)
                    .iter().filter_map(|v| v.as_str()).collect();
                Ok(Value::Object(obj.iter()
                    .filter(|(k, _)| !keys.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()))
            }
            "regex.match" => {
                let pattern = input["pattern"].as_str()
                    .ok_or_else(|| TaskHubError::Plugin("regex.match: 'pattern' required".into()))?;
                let text = input["input"].as_str()
                    .ok_or_else(|| TaskHubError::Plugin("regex.match: 'input' string required".into()))?;
                let re = Regex::new(pattern)
                    .map_err(|e| TaskHubError::Plugin(format!("regex.match: invalid pattern: {e}")))?;
                let captures: Vec<serde_json::Value> = re.captures_iter(text)
                    .map(|c| {
                        let groups: Vec<_> = c.iter()
                            .map(|m| m.map(|x| Value::String(x.as_str().to_string())).unwrap_or(Value::Null))
                            .collect();
                        Value::Array(groups)
                    })
                    .collect();
                Ok(serde_json::json!({
                    "matched": !captures.is_empty(),
                    "captures": captures,
                    "input": text,
                    "pattern": pattern,
                }))
            }
            "jq" => {
                let query = input["query"].as_str()
                    .ok_or_else(|| TaskHubError::Plugin("jq: 'query' required".into()))?;
                let data = &input["input"];
                jq_eval(query, data)
                    .map_err(|e| TaskHubError::Plugin(format!("jq: {e}")))
            }
            "template" => {
                let tmpl = input["template"].as_str()
                    .ok_or_else(|| TaskHubError::Plugin("template: 'template' required".into()))?;
                let vars = input.get("vars").cloned().unwrap_or_else(|| Value::Object(Default::default()));
                let result = render_template(tmpl, &vars);
                Ok(Value::String(result))
            }
            other => Err(TaskHubError::Plugin(format!("core/transform: unknown action '{other}'"))),
        }
    }
}

// Minimal jq subset: `.field`, `.field.nested`, `.[index]`, `.[]`, identity `.`
fn jq_eval(query: &str, data: &Value) -> Result<Value, String> {
    let query = query.trim();
    if query == "." { return Ok(data.clone()); }

    if !query.starts_with('.') {
        return Err(format!("unsupported query: {}", query));
    }
    let path = &query[1..]; // strip leading .
    if path.is_empty() { return Ok(data.clone()); }

    // Handle .[] — iterate array/object values
    if path == "[]" {
        return match data {
            Value::Array(arr) => Ok(Value::Array(arr.clone())),
            Value::Object(obj) => Ok(Value::Array(obj.values().cloned().collect())),
            _ => Err(".[]: not iterable".into()),
        };
    }

    // Walk dot-separated path, handling [index] and []
    let mut current = data.clone();
    for segment in split_path(path) {
        current = apply_segment(&segment, &current)?;
    }
    Ok(current)
}

fn split_path(path: &str) -> Vec<String> {
    let mut segments = vec![];
    let mut seg = String::new();
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                if !seg.is_empty() { segments.push(seg.clone()); seg.clear(); }
            }
            '[' => {
                if !seg.is_empty() { segments.push(seg.clone()); seg.clear(); }
                let mut bracket = String::new();
                for bc in chars.by_ref() {
                    if bc == ']' { break; }
                    bracket.push(bc);
                }
                segments.push(format!("[{}]", bracket));
            }
            _ => seg.push(c),
        }
    }
    if !seg.is_empty() { segments.push(seg); }
    segments
}

fn apply_segment(seg: &str, current: &Value) -> Result<Value, String> {
    if seg.starts_with('[') && seg.ends_with(']') {
        let inner = &seg[1..seg.len()-1];
        if inner.is_empty() {
            // .[]
            return match current {
                Value::Array(arr) => Ok(Value::Array(arr.clone())),
                Value::Object(obj) => Ok(Value::Array(obj.values().cloned().collect())),
                _ => Err(".[]: not iterable".into()),
            };
        }
        if let Ok(idx) = inner.parse::<usize>() {
            return match current {
                Value::Array(arr) => Ok(arr.get(idx).cloned().unwrap_or(Value::Null)),
                _ => Err(format!(".[{}]: not an array", idx)),
            };
        }
        let key = inner.trim_matches('"');
        return match current {
            Value::Object(obj) => Ok(obj.get(key).cloned().unwrap_or(Value::Null)),
            _ => Err(format!(".[\"{}\"]: not an object", key)),
        };
    }
    match current {
        Value::Object(obj) => Ok(obj.get(seg).cloned().unwrap_or(Value::Null)),
        _ => Err(format!(".{}: not an object (got {:?})", seg, current)),
    }
}

// Simple {{var}} template renderer
fn render_template(tmpl: &str, vars: &Value) -> String {
    let mut result = tmpl.to_string();
    if let Some(obj) = vars.as_object() {
        for (k, v) in obj {
            let placeholder = format!("{{{{{}}}}}", k);
            let val = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            result = result.replace(&placeholder, &val);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskhub_core::engine::Action;

    #[tokio::test]
    async fn json_parse_stringify() {
        let a = TransformAction;
        let parsed = a.execute(serde_json::json!({"action": "json.parse", "input": "{\"x\":1}"})).await.unwrap();
        assert_eq!(parsed["x"], 1);
        let s = a.execute(serde_json::json!({"action": "json.stringify", "input": {"y": 2}})).await.unwrap();
        assert!(s.as_str().unwrap().contains("\"y\""));
    }

    #[tokio::test]
    async fn merge_pick_omit() {
        let a = TransformAction;
        let m = a.execute(serde_json::json!({"action":"merge","a":{"x":1},"b":{"y":2}})).await.unwrap();
        assert_eq!(m["x"], 1); assert_eq!(m["y"], 2);
        let p = a.execute(serde_json::json!({"action":"pick","input":{"a":1,"b":2,"c":3},"keys":["a","c"]})).await.unwrap();
        assert!(p["a"]==1 && p["c"]==3 && p["b"].is_null());
        let o = a.execute(serde_json::json!({"action":"omit","input":{"a":1,"b":2},"keys":["b"]})).await.unwrap();
        assert!(o["a"]==1 && o["b"].is_null());
    }

    #[tokio::test]
    async fn regex_match() {
        let a = TransformAction;
        let r = a.execute(serde_json::json!({"action":"regex.match","pattern":"\\d+","input":"abc 42 def"})).await.unwrap();
        assert!(r["matched"].as_bool().unwrap());
        let caps = r["captures"].as_array().unwrap();
        assert_eq!(caps[0][0].as_str().unwrap(), "42");
    }

    #[tokio::test]
    async fn jq_field() {
        let a = TransformAction;
        let r = a.execute(serde_json::json!({"action":"jq","query":".name","input":{"name":"Alice"}})).await.unwrap();
        assert_eq!(r.as_str().unwrap(), "Alice");
    }

    #[tokio::test]
    async fn jq_nested() {
        let a = TransformAction;
        let r = a.execute(serde_json::json!({
            "action":"jq","query":".user.name",
            "input":{"user":{"name":"Bob"}}
        })).await.unwrap();
        assert_eq!(r.as_str().unwrap(), "Bob");
    }

    #[tokio::test]
    async fn template_render() {
        let a = TransformAction;
        let r = a.execute(serde_json::json!({
            "action": "template",
            "template": "Hello {{name}}, you have {{count}} messages.",
            "vars": {"name": "Alice", "count": 5}
        })).await.unwrap();
        assert_eq!(r.as_str().unwrap(), "Hello Alice, you have 5 messages.");
    }
}
