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

fn post_json(token: &str, method: &str, body: &Value) -> Option<Value> {
    let url = format!("https://slack.com/api/{}", method);
    let mut h = Map::new();
    h.insert("Authorization".into(), Value::String(format!("Bearer {}", token)));
    sdk::http_request_json("POST", &url, Some(&h), Some(body))
}

fn dispatch(action: &str, input: Value) -> Value {
    let token = input["token"].as_str().unwrap_or("");
    if token.is_empty() { return sdk::respond_error("missing token"); }

    match action {
        "send" => {
            let channel = input["channel"].as_str().unwrap_or("");
            let text = input["text"].as_str().unwrap_or("");
            if channel.is_empty() { return sdk::respond_error("missing channel"); }
            let mut body = json!({"channel": channel, "text": text});
            if let Some(b) = input.get("blocks") { body["blocks"] = b.clone(); }
            match post_json(token, "chat.postMessage", &body) {
                Some(v) if v["ok"].as_bool().unwrap_or(false) => {
                    json!({"ts": v["ts"], "channel": v["channel"]})
                }
                Some(v) => sdk::respond_error(v["error"].as_str().unwrap_or("slack error")),
                None => sdk::respond_error("send failed"),
            }
        }
        "send_thread" => {
            let channel = input["channel"].as_str().unwrap_or("");
            let thread_ts = input["thread_ts"].as_str().unwrap_or("");
            let text = input["text"].as_str().unwrap_or("");
            if channel.is_empty() || thread_ts.is_empty() { return sdk::respond_error("missing channel or thread_ts"); }
            let body = json!({"channel": channel, "thread_ts": thread_ts, "text": text});
            match post_json(token, "chat.postMessage", &body) {
                Some(v) if v["ok"].as_bool().unwrap_or(false) => {
                    json!({"ts": v["ts"], "channel": v["channel"]})
                }
                Some(v) => sdk::respond_error(v["error"].as_str().unwrap_or("slack error")),
                None => sdk::respond_error("send_thread failed"),
            }
        }
        "react" => {
            let channel = input["channel"].as_str().unwrap_or("");
            let timestamp = input["timestamp"].as_str().unwrap_or("");
            let name = input["name"].as_str().unwrap_or("");
            if channel.is_empty() || timestamp.is_empty() || name.is_empty() {
                return sdk::respond_error("missing channel, timestamp, or name");
            }
            let body = json!({"channel": channel, "timestamp": timestamp, "name": name});
            match post_json(token, "reactions.add", &body) {
                Some(v) if v["ok"].as_bool().unwrap_or(false) => json!({"ok": true}),
                Some(v) => sdk::respond_error(v["error"].as_str().unwrap_or("slack error")),
                None => sdk::respond_error("react failed"),
            }
        }
        "upload_file" => {
            let channel = input["channel"].as_str().unwrap_or("");
            let content = input["content"].as_str().unwrap_or("");
            let filename = input["filename"].as_str().unwrap_or("file.txt");
            let title = input["title"].as_str().unwrap_or(filename);
            if channel.is_empty() || content.is_empty() {
                return sdk::respond_error("missing channel or content");
            }
            let body = json!({
                "channels": channel,
                "content": content,
                "filename": filename,
                "title": title,
            });
            match post_json(token, "files.upload", &body) {
                Some(v) if v["ok"].as_bool().unwrap_or(false) => {
                    json!({"id": v["file"]["id"], "url": v["file"]["permalink"]})
                }
                Some(v) => sdk::respond_error(v["error"].as_str().unwrap_or("slack error")),
                None => sdk::respond_error("upload_file failed"),
            }
        }
        _ => sdk::respond_error(&format!("unknown action: {}", action)),
    }
}
