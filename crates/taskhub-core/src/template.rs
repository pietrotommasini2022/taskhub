use serde_json::Value;
use std::collections::HashMap;

/// Resolve `${{ expr }}` placeholders in a string.
/// Context keys: `steps.<id>.output`, `trigger.body`, `trigger.headers`,
/// `trigger.query`, `secrets.<KEY>`, `item` (for_each current element).
pub fn resolve(input: &str, ctx: &TemplateContext) -> String {
    let mut result = input.to_string();
    // Repeatedly replace until stable (handles nested, though we don't support deep nesting).
    for _ in 0..10 {
        let replaced = replace_once(&result, ctx);
        if replaced == result {
            break;
        }
        result = replaced;
    }
    result
}

fn replace_once(input: &str, ctx: &TemplateContext) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("${{") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 3..];
        if let Some(end) = rest.find("}}") {
            let expr = rest[..end].trim();
            out.push_str(&ctx.resolve_expr(expr));
            rest = &rest[end + 2..];
        } else {
            // Unclosed — emit as-is.
            out.push_str("${{");
        }
    }
    out.push_str(rest);
    out
}

/// Resolve a JSON `Value`, recursively handling string leaves.
pub fn resolve_value(v: &Value, ctx: &TemplateContext) -> Value {
    match v {
        Value::String(s) => {
            let resolved = resolve(s, ctx);
            // If the whole string was a single expression, try to preserve the type.
            if s.starts_with("${{") && s.ends_with("}}") && s.matches("${{").count() == 1 {
                let expr = s[3..s.len() - 2].trim();
                if let Some(val) = ctx.resolve_expr_typed(expr) {
                    return val;
                }
            }
            Value::String(resolved)
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), resolve_value(v, ctx)))
                .collect(),
        ),
        Value::Array(arr) => Value::Array(arr.iter().map(|v| resolve_value(v, ctx)).collect()),
        other => other.clone(),
    }
}

#[derive(Default)]
pub struct TemplateContext {
    /// step id → output value
    pub steps: HashMap<String, Value>,
    /// trigger payload fields
    pub trigger: HashMap<String, Value>,
    /// secret values (resolved before execution, not stored in context for logging)
    pub secrets: HashMap<String, String>,
    /// current item in for_each loop
    pub item: Option<Value>,
}

impl TemplateContext {
    pub fn set_step_output(&mut self, step_id: &str, output: Value) {
        self.steps.insert(step_id.to_string(), serde_json::json!({"output": output}));
    }

    fn resolve_expr(&self, expr: &str) -> String {
        self.resolve_expr_typed(expr)
            .map(|v| value_to_string(&v))
            .unwrap_or_else(|| format!("${{{{ {expr} }}}}"))
    }

    fn resolve_expr_typed(&self, expr: &str) -> Option<Value> {
        let parts: Vec<&str> = expr.splitn(3, '.').collect();
        match parts.as_slice() {
            ["steps", step_id, rest] => {
                let output = self.steps.get(*step_id)?;
                json_path(output, rest)
            }
            ["trigger", key] => self.trigger.get(*key).cloned(),
            ["trigger", key, rest] => {
                let v = self.trigger.get(*key)?;
                json_path(v, rest)
            }
            ["secrets", key] => self.secrets.get(*key).map(|s| Value::String(s.clone())),
            ["item"] => self.item.clone(),
            ["item", rest] => self.item.as_ref().and_then(|v| json_path(v, rest)),
            _ => None,
        }
    }
}

fn json_path(v: &Value, path: &str) -> Option<Value> {
    let mut cur = v;
    for key in path.split('.') {
        cur = cur.get(key)?;
    }
    Some(cur.clone())
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_step(step_id: &str, output: Value) -> TemplateContext {
        let mut ctx = TemplateContext::default();
        ctx.set_step_output(step_id, output);
        ctx
    }

    #[test]
    fn resolves_step_output_string() {
        let ctx = ctx_with_step("fetch", serde_json::json!({"title": "hello"}));
        assert_eq!(
            resolve("${{ steps.fetch.output.title }}", &ctx),
            "hello"
        );
    }

    #[test]
    fn unresolved_passthrough() {
        let ctx = TemplateContext::default();
        let s = "${{ steps.missing.output }}";
        assert_eq!(resolve(s, &ctx), s);
    }

    #[test]
    fn no_template_unchanged() {
        let ctx = TemplateContext::default();
        assert_eq!(resolve("plain text", &ctx), "plain text");
    }

    #[test]
    fn secret_resolved() {
        let mut ctx = TemplateContext::default();
        ctx.secrets.insert("MY_TOKEN".to_string(), "abc123".to_string());
        assert_eq!(resolve("Bearer ${{ secrets.MY_TOKEN }}", &ctx), "Bearer abc123");
    }

    #[test]
    fn item_in_foreach() {
        let mut ctx = TemplateContext::default();
        ctx.item = Some(Value::String("repo-name".to_string()));
        assert_eq!(resolve("${{ item }}", &ctx), "repo-name");
    }
}
