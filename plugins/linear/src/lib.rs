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

const URL: &str = "https://api.linear.app/graphql";

fn gql(token: &str, query: &str, variables: Value) -> Option<Value> {
    let body = json!({"query": query, "variables": variables});
    let mut h = Map::new();
    h.insert("Authorization".into(), Value::String(token.to_string()));
    h.insert("Content-Type".into(), Value::String("application/json".into()));
    sdk::http_request_json("POST", URL, Some(&h), Some(&body))
}

fn dispatch(action: &str, input: Value) -> Value {
    let token = input["token"].as_str().unwrap_or("");
    if token.is_empty() { return sdk::respond_error("missing token"); }

    match action {
        "issues.list" => {
            let limit = input["limit"].as_u64().unwrap_or(25);
            let mut filter_parts = vec![];
            if let Some(t) = input["team_id"].as_str() {
                filter_parts.push(format!("team: {{ id: {{ eq: \"{}\" }} }}", t));
            }
            if let Some(s) = input["state"].as_str() {
                filter_parts.push(format!("state: {{ name: {{ eq: \"{}\" }} }}", s));
            }
            let filter = if filter_parts.is_empty() { String::new() } else {
                format!("(filter: {{ {} }})", filter_parts.join(", "))
            };
            let q = format!(r#"
                query {{ issues{} {{
                    nodes {{ id title state {{ name }} priority url }}
                    pageInfo {{ hasNextPage }}
                }} }}
            "#, filter);
            // Linear doesn't use variables for simple list queries
            match gql(token, &q, json!({})) {
                Some(v) => {
                    let nodes = &v["data"]["issues"]["nodes"];
                    let items = nodes.as_array().unwrap_or(&vec![]).iter().take(limit as usize).map(|i| {
                        json!({
                            "id": i["id"],
                            "title": i["title"],
                            "state": i["state"]["name"],
                            "priority": i["priority"],
                            "url": i["url"],
                        })
                    }).collect::<Vec<_>>();
                    json!(items)
                }
                None => sdk::respond_error("issues.list failed"),
            }
        }
        "issues.create" => {
            let team_id = input["team_id"].as_str().unwrap_or("");
            let title = input["title"].as_str().unwrap_or("");
            if team_id.is_empty() || title.is_empty() { return sdk::respond_error("missing team_id or title"); }
            let q = r#"mutation CreateIssue($input: IssueCreateInput!) {
                issueCreate(input: $input) { issue { id url } }
            }"#;
            let mut vars_input = json!({"teamId": team_id, "title": title});
            if let Some(d) = input["description"].as_str() { vars_input["description"] = json!(d); }
            if let Some(p) = input["priority"].as_u64() { vars_input["priority"] = json!(p); }
            match gql(token, q, json!({"input": vars_input})) {
                Some(v) => {
                    let issue = &v["data"]["issueCreate"]["issue"];
                    json!({"id": issue["id"], "url": issue["url"]})
                }
                None => sdk::respond_error("issues.create failed"),
            }
        }
        "issues.update" => {
            let issue_id = input["issue_id"].as_str().unwrap_or("");
            if issue_id.is_empty() { return sdk::respond_error("missing issue_id"); }
            let q = r#"mutation UpdateIssue($id: String!, $input: IssueUpdateInput!) {
                issueUpdate(id: $id, input: $input) { issue { id url } }
            }"#;
            let mut upd = json!({});
            if let Some(t) = input["title"].as_str() { upd["title"] = json!(t); }
            if let Some(s) = input["state_id"].as_str() { upd["stateId"] = json!(s); }
            if let Some(p) = input["priority"].as_u64() { upd["priority"] = json!(p); }
            match gql(token, q, json!({"id": issue_id, "input": upd})) {
                Some(v) => {
                    let issue = &v["data"]["issueUpdate"]["issue"];
                    json!({"id": issue["id"], "url": issue["url"]})
                }
                None => sdk::respond_error("issues.update failed"),
            }
        }
        "issues.comment" => {
            let issue_id = input["issue_id"].as_str().unwrap_or("");
            let body = input["body"].as_str().unwrap_or("");
            if issue_id.is_empty() || body.is_empty() { return sdk::respond_error("missing issue_id or body"); }
            let q = r#"mutation CreateComment($input: CommentCreateInput!) {
                commentCreate(input: $input) { comment { id } }
            }"#;
            match gql(token, q, json!({"input": {"issueId": issue_id, "body": body}})) {
                Some(v) => json!({"id": v["data"]["commentCreate"]["comment"]["id"]}),
                None => sdk::respond_error("issues.comment failed"),
            }
        }
        _ => sdk::respond_error(&format!("unknown action: {}", action)),
    }
}
