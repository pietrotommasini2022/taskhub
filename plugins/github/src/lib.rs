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

fn auth_headers(token: &str) -> Map<String, Value> {
    let mut h = Map::new();
    h.insert("Authorization".into(), Value::String(format!("Bearer {}", token)));
    h.insert("Accept".into(), Value::String("application/vnd.github+json".into()));
    h.insert("X-GitHub-Api-Version".into(), Value::String("2022-11-28".into()));
    h.insert("User-Agent".into(), Value::String("TaskHub/0.1".into()));
    h
}

fn api(method: &str, path: &str, token: &str, body: Option<&Value>) -> Option<Value> {
    let url = format!("https://api.github.com{}", path);
    let h = auth_headers(token);
    sdk::http_request_json(method, &url, Some(&h), body)
}

fn dispatch(action: &str, input: Value) -> Value {
    let token = input["token"].as_str().unwrap_or("");
    if token.is_empty() {
        return sdk::respond_error("missing token");
    }
    match action {
        "notifications.list" => {
            let all = input["all"].as_bool().unwrap_or(false);
            let path = format!("/notifications?all={}", all);
            match api("GET", &path, token, None) {
                Some(v) => {
                    let items = v.as_array().unwrap_or(&vec![]).iter().map(|n| {
                        json!({
                            "id": n["id"],
                            "title": n["subject"]["title"],
                            "type": n["subject"]["type"],
                            "repo": n["repository"]["full_name"],
                            "unread": n["unread"],
                            "updated_at": n["updated_at"],
                        })
                    }).collect::<Vec<_>>();
                    json!(items)
                }
                None => sdk::respond_error("notifications.list request failed"),
            }
        }
        "notifications.mark_read" => {
            match api("PUT", "/notifications", token, None) {
                Some(_) | None => json!({"ok": true}),
            }
        }
        "issues.list" => {
            let owner = input["owner"].as_str().unwrap_or("");
            let repo = input["repo"].as_str().unwrap_or("");
            if owner.is_empty() || repo.is_empty() {
                return sdk::respond_error("missing owner or repo");
            }
            let state = input["state"].as_str().unwrap_or("open");
            let labels = input["labels"].as_str().unwrap_or("");
            let mut path = format!("/repos/{}/{}/issues?state={}", owner, repo, state);
            if !labels.is_empty() {
                path.push_str(&format!("&labels={}", labels));
            }
            match api("GET", &path, token, None) {
                Some(v) => {
                    let items = v.as_array().unwrap_or(&vec![]).iter().map(|i| {
                        json!({
                            "number": i["number"],
                            "title": i["title"],
                            "state": i["state"],
                            "url": i["html_url"],
                            "labels": i["labels"].as_array().map(|l| l.iter().map(|x| &x["name"]).collect::<Vec<_>>()),
                            "created_at": i["created_at"],
                        })
                    }).collect::<Vec<_>>();
                    json!(items)
                }
                None => sdk::respond_error("issues.list request failed"),
            }
        }
        "issues.create" => {
            let owner = input["owner"].as_str().unwrap_or("");
            let repo = input["repo"].as_str().unwrap_or("");
            let title = input["title"].as_str().unwrap_or("");
            if owner.is_empty() || repo.is_empty() || title.is_empty() {
                return sdk::respond_error("missing owner, repo, or title");
            }
            let mut body = json!({"title": title});
            if let Some(b) = input["body"].as_str() { body["body"] = json!(b); }
            if let Some(l) = input.get("labels") { body["labels"] = l.clone(); }
            let path = format!("/repos/{}/{}/issues", owner, repo);
            match api("POST", &path, token, Some(&body)) {
                Some(v) => json!({"number": v["number"], "url": v["html_url"]}),
                None => sdk::respond_error("issues.create failed"),
            }
        }
        "issues.comment" => {
            let owner = input["owner"].as_str().unwrap_or("");
            let repo = input["repo"].as_str().unwrap_or("");
            let number = input["number"].as_u64().unwrap_or(0);
            let body_text = input["body"].as_str().unwrap_or("");
            if owner.is_empty() || repo.is_empty() || number == 0 || body_text.is_empty() {
                return sdk::respond_error("missing owner, repo, number, or body");
            }
            let path = format!("/repos/{}/{}/issues/{}/comments", owner, repo, number);
            let body = json!({"body": body_text});
            match api("POST", &path, token, Some(&body)) {
                Some(v) => json!({"id": v["id"], "url": v["html_url"]}),
                None => sdk::respond_error("issues.comment failed"),
            }
        }
        "pulls.list" => {
            let owner = input["owner"].as_str().unwrap_or("");
            let repo = input["repo"].as_str().unwrap_or("");
            if owner.is_empty() || repo.is_empty() {
                return sdk::respond_error("missing owner or repo");
            }
            let state = input["state"].as_str().unwrap_or("open");
            let path = format!("/repos/{}/{}/pulls?state={}", owner, repo, state);
            match api("GET", &path, token, None) {
                Some(v) => {
                    let items = v.as_array().unwrap_or(&vec![]).iter().map(|p| {
                        json!({
                            "number": p["number"],
                            "title": p["title"],
                            "state": p["state"],
                            "url": p["html_url"],
                            "draft": p["draft"],
                        })
                    }).collect::<Vec<_>>();
                    json!(items)
                }
                None => sdk::respond_error("pulls.list failed"),
            }
        }
        "pulls.review" => {
            let owner = input["owner"].as_str().unwrap_or("");
            let repo = input["repo"].as_str().unwrap_or("");
            let pull_number = input["pull_number"].as_u64().unwrap_or(0);
            let event = input["event"].as_str().unwrap_or("COMMENT");
            if owner.is_empty() || repo.is_empty() || pull_number == 0 {
                return sdk::respond_error("missing owner, repo, or pull_number");
            }
            let mut body = json!({"event": event});
            if let Some(b) = input["body"].as_str() { body["body"] = json!(b); }
            let path = format!("/repos/{}/{}/pulls/{}/reviews", owner, repo, pull_number);
            match api("POST", &path, token, Some(&body)) {
                Some(v) => json!({"id": v["id"], "state": v["state"]}),
                None => sdk::respond_error("pulls.review failed"),
            }
        }
        "repo.dispatch" => {
            let owner = input["owner"].as_str().unwrap_or("");
            let repo = input["repo"].as_str().unwrap_or("");
            let event_type = input["event_type"].as_str().unwrap_or("");
            if owner.is_empty() || repo.is_empty() || event_type.is_empty() {
                return sdk::respond_error("missing owner, repo, or event_type");
            }
            let mut body = json!({"event_type": event_type});
            if let Some(p) = input.get("client_payload") { body["client_payload"] = p.clone(); }
            let path = format!("/repos/{}/{}/dispatches", owner, repo);
            match api("POST", &path, token, Some(&body)) {
                Some(_) | None => json!({"ok": true}),
            }
        }
        _ => sdk::respond_error(&format!("unknown action: {}", action)),
    }
}
