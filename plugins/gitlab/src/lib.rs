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

fn api(method: &str, host: &str, path: &str, token: &str, body: Option<&Value>) -> Option<Value> {
    let url = format!("https://{}/api/v4{}", host, path);
    let mut h = Map::new();
    h.insert("PRIVATE-TOKEN".into(), Value::String(token.to_string()));
    h.insert("Content-Type".into(), Value::String("application/json".into()));
    sdk::http_request_json(method, &url, Some(&h), body)
}

fn dispatch(action: &str, input: Value) -> Value {
    let token = input["token"].as_str().unwrap_or("");
    if token.is_empty() { return sdk::respond_error("missing token"); }
    let host = input["host"].as_str().unwrap_or("gitlab.com");

    match action {
        "todos.list" => {
            match api("GET", host, "/todos", token, None) {
                Some(v) => {
                    let items = v.as_array().unwrap_or(&vec![]).iter().map(|t| {
                        json!({
                            "id": t["id"],
                            "title": t["body"],
                            "type": t["target_type"],
                            "project": t["project"]["path_with_namespace"],
                            "state": t["state"],
                        })
                    }).collect::<Vec<_>>();
                    json!(items)
                }
                None => sdk::respond_error("todos.list failed"),
            }
        }
        "todos.mark_done" => {
            let id = input["id"].as_u64().unwrap_or(0);
            if id == 0 { return sdk::respond_error("missing id"); }
            let path = format!("/todos/{}/mark_as_done", id);
            api("POST", host, &path, token, None);
            json!({"ok": true})
        }
        "merge_requests.list" => {
            let project_id = urlenc(input["project_id"].as_str().unwrap_or(""));
            if project_id.is_empty() { return sdk::respond_error("missing project_id"); }
            let state = input["state"].as_str().unwrap_or("opened");
            let path = format!("/projects/{}/merge_requests?state={}", project_id, state);
            match api("GET", host, &path, token, None) {
                Some(v) => {
                    let items = v.as_array().unwrap_or(&vec![]).iter().map(|m| {
                        json!({
                            "iid": m["iid"],
                            "title": m["title"],
                            "state": m["state"],
                            "url": m["web_url"],
                            "draft": m["draft"],
                        })
                    }).collect::<Vec<_>>();
                    json!(items)
                }
                None => sdk::respond_error("merge_requests.list failed"),
            }
        }
        "issues.list" => {
            let project_id = urlenc(input["project_id"].as_str().unwrap_or(""));
            if project_id.is_empty() { return sdk::respond_error("missing project_id"); }
            let state = input["state"].as_str().unwrap_or("opened");
            let path = format!("/projects/{}/issues?state={}", project_id, state);
            match api("GET", host, &path, token, None) {
                Some(v) => {
                    let items = v.as_array().unwrap_or(&vec![]).iter().map(|i| {
                        json!({
                            "iid": i["iid"],
                            "title": i["title"],
                            "state": i["state"],
                            "url": i["web_url"],
                        })
                    }).collect::<Vec<_>>();
                    json!(items)
                }
                None => sdk::respond_error("issues.list failed"),
            }
        }
        _ => sdk::respond_error(&format!("unknown action: {}", action)),
    }
}

fn urlenc(s: &str) -> String {
    s.replace('/', "%2F")
}
