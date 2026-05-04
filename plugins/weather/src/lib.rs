mod sdk;
use sdk::{execute_action, http_get, respond, respond_error};
use serde_json::{json, Value};

#[no_mangle]
pub extern "C" fn taskhub_execute(
    action_ptr: u32, action_len: u32,
    input_ptr: u32,  input_len:  u32,
) -> u64 {
    execute_action(action_ptr, action_len, input_ptr, input_len, dispatch)
}

fn dispatch(action: &str, input: Value) -> Value {
    match action {
        "get" => action_get(input),
        _ => respond_error(&format!("unknown action: {action}")),
    }
}

fn action_get(input: Value) -> Value {
    let city = input["city"].as_str().unwrap_or("auto");
    let url = format!("https://wttr.in/{}?format=j1", urlencode(city));

    let body = match http_get(&url) {
        Some(b) => b,
        None => return respond_error("HTTP request to wttr.in failed"),
    };

    let data: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return respond_error(&format!("failed to parse wttr.in response: {e}")),
    };

    let current = &data["current_condition"][0];

    respond(json!({
        "city": city,
        "temp_c": current["temp_C"].as_str().unwrap_or("?"),
        "condition": current["weatherDesc"][0]["value"].as_str().unwrap_or("?"),
        "wind_kmh": current["windspeedKmph"].as_str().unwrap_or("?"),
        "humidity_pct": current["humidity"].as_str().unwrap_or("?"),
    }))
}

fn urlencode(s: &str) -> String {
    s.chars().map(|c| match c {
        ' ' => '+'.to_string(),
        c if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') => c.to_string(),
        _ => format!("%{:02X}", c as u32),
    }).collect()
}

#[no_mangle]
pub extern "C" fn taskhub_alloc(size: u32) -> u32 { sdk::alloc(size) }

#[no_mangle]
pub extern "C" fn taskhub_dealloc(ptr: u32, size: u32) { sdk::dealloc(ptr, size) }
