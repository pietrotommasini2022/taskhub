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

fn notion_req(method: &str, path: &str, token: &str, body: Option<&Value>) -> Option<Value> {
    let url = format!("https://api.notion.com/v1{}", path);
    let mut h = Map::new();
    h.insert("Authorization".into(), Value::String(format!("Bearer {}", token)));
    h.insert("Notion-Version".into(), Value::String("2022-06-28".into()));
    sdk::http_request_json(method, &url, Some(&h), body)
}

fn dispatch(action: &str, input: Value) -> Value {
    let token = input["token"].as_str().unwrap_or("");
    if token.is_empty() { return sdk::respond_error("missing token"); }

    match action {
        "database.query" => {
            let db_id = input["database_id"].as_str().unwrap_or("");
            if db_id.is_empty() { return sdk::respond_error("missing database_id"); }
            let path = format!("/databases/{}/query", db_id);
            let mut body = json!({});
            if let Some(f) = input.get("filter") { body["filter"] = f.clone(); }
            if let Some(s) = input.get("sorts") { body["sorts"] = s.clone(); }
            if let Some(ps) = input["page_size"].as_u64() { body["page_size"] = json!(ps); }
            match notion_req("POST", &path, token, Some(&body)) {
                Some(v) => {
                    let results = v["results"].as_array().unwrap_or(&vec![]).iter().map(|p| {
                        json!({"id": p["id"], "properties": p["properties"], "url": p["url"]})
                    }).collect::<Vec<_>>();
                    json!(results)
                }
                None => sdk::respond_error("database.query failed"),
            }
        }
        "page.create" => {
            let parent_id = input["parent_id"].as_str().unwrap_or("");
            let title = input["title"].as_str().unwrap_or("");
            if parent_id.is_empty() { return sdk::respond_error("missing parent_id"); }
            let mut props = input["properties"].clone();
            if props.is_null() { props = json!({}); }
            props["title"] = json!([{"text": {"content": title}}]);
            let mut body = json!({
                "parent": {"database_id": parent_id},
                "properties": props,
            });
            if let Some(c) = input.get("children") { body["children"] = c.clone(); }
            match notion_req("POST", "/pages", token, Some(&body)) {
                Some(v) => json!({"id": v["id"], "url": v["url"]}),
                None => sdk::respond_error("page.create failed"),
            }
        }
        "page.update" => {
            let page_id = input["page_id"].as_str().unwrap_or("");
            if page_id.is_empty() { return sdk::respond_error("missing page_id"); }
            let props = input.get("properties").cloned().unwrap_or_else(|| json!({}));
            let body = json!({"properties": props});
            let path = format!("/pages/{}", page_id);
            match notion_req("PATCH", &path, token, Some(&body)) {
                Some(v) => json!({"id": v["id"], "url": v["url"]}),
                None => sdk::respond_error("page.update failed"),
            }
        }
        "block.append" => {
            let block_id = input["block_id"].as_str().unwrap_or("");
            if block_id.is_empty() { return sdk::respond_error("missing block_id"); }
            let children = input.get("children").cloned().unwrap_or_else(|| json!([]));
            let body = json!({"children": children});
            let path = format!("/blocks/{}/children", block_id);
            match notion_req("PATCH", &path, token, Some(&body)) {
                Some(_) => json!({"ok": true}),
                None => sdk::respond_error("block.append failed"),
            }
        }
        _ => sdk::respond_error(&format!("unknown action: {}", action)),
    }
}
