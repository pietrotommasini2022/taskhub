mod sdk;
use serde_json::{json, Map, Value};

#[no_mangle]
pub extern "C" fn taskhub_alloc(size: u32) -> u32 { sdk::alloc(size) }
#[no_mangle]
pub extern "C" fn taskhub_dealloc(ptr: u32, size: u32) { sdk::dealloc(ptr, size) }
#[no_mangle]
pub extern "C" fn taskhub_execute(ap: u32, al: u32, ip: u32, il: u32) -> u64 {
    sdk::execute_action(ap, al, ip, il, dispatch)
}

fn dispatch(action: &str, input: Value) -> Value {
    match action {
        "webhook.send" => {
            let url = input["url"].as_str().unwrap_or("");
            if url.is_empty() { return sdk::respond_error("missing url"); }

            let mut body = json!({});
            if let Some(c) = input["content"].as_str() { body["content"] = json!(c); }
            if let Some(e) = input.get("embeds") { body["embeds"] = e.clone(); }
            if let Some(u) = input["username"].as_str() { body["username"] = json!(u); }

            let mut h = Map::new();
            h.insert("Content-Type".into(), Value::String("application/json".into()));

            let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
            match sdk::http_request("POST", url, Some(&h), Some(&body_bytes)) {
                Some(_) => json!({"ok": true}),
                None => sdk::respond_error("webhook.send failed"),
            }
        }
        _ => sdk::respond_error(&format!("unknown action: {}", action)),
    }
}
