// TaskHub Plugin SDK — inline copy.
// Handles memory management and host imports.

use serde_json::Value;

// ── Host imports ──────────────────────────────────────────────────────────────

#[link(wasm_import_module = "taskhub")]
extern "C" {
    #[link_name = "log"]
    fn taskhub_log(level: u32, ptr: u32, len: u32);
    #[link_name = "http_get"]
    fn taskhub_http_get(url_ptr: u32, url_len: u32) -> i32;
    #[link_name = "http_response_read"]
    fn taskhub_http_response_read(dst_ptr: u32, dst_len: u32) -> i32;
    #[link_name = "http_request"]
    fn taskhub_http_request(
        method_ptr: u32, method_len: u32,
        url_ptr: u32, url_len: u32,
        headers_ptr: u32, headers_len: u32,
        body_ptr: u32, body_len: u32,
    ) -> i32;
}

// ── Memory management ─────────────────────────────────────────────────────────

pub fn alloc(size: u32) -> u32 {
    let mut buf = Vec::<u8>::with_capacity(size as usize);
    let ptr = buf.as_mut_ptr() as u32;
    std::mem::forget(buf);
    ptr
}

pub fn dealloc(ptr: u32, size: u32) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, 0, size as usize);
    }
}

// ── ABI dispatcher ────────────────────────────────────────────────────────────

pub fn execute_action(
    action_ptr: u32, action_len: u32,
    input_ptr: u32,  input_len: u32,
    dispatch: fn(&str, Value) -> Value,
) -> u64 {
    let action = read_str(action_ptr, action_len);
    let input_bytes = read_bytes(input_ptr, input_len);
    let input: Value = serde_json::from_slice(&input_bytes)
        .unwrap_or(Value::Object(Default::default()));

    let output = dispatch(&action, input);
    write_result(output)
}

fn read_str(ptr: u32, len: u32) -> String {
    let bytes = read_bytes(ptr, len);
    String::from_utf8(bytes).unwrap_or_default()
}

fn read_bytes(ptr: u32, len: u32) -> Vec<u8> {
    unsafe {
        std::slice::from_raw_parts(ptr as *const u8, len as usize).to_vec()
    }
}

fn write_result(v: Value) -> u64 {
    let json = serde_json::to_vec(&v).unwrap_or_default();
    let len = json.len() as u32;
    let ptr = alloc(len);
    unsafe {
        std::ptr::copy_nonoverlapping(json.as_ptr(), ptr as *mut u8, len as usize);
    }
    std::mem::forget(json);
    ((ptr as u64) << 32) | (len as u64)
}

// ── SDK helpers ───────────────────────────────────────────────────────────────

pub fn respond(v: Value) -> Value { v }

pub fn respond_error(msg: &str) -> Value {
    serde_json::json!({ "__taskhub_error": msg })
}

pub fn log_info(msg: &str) {
    unsafe { taskhub_log(2, msg.as_ptr() as u32, msg.len() as u32); }
}

pub fn log_warn(msg: &str) {
    unsafe { taskhub_log(1, msg.as_ptr() as u32, msg.len() as u32); }
}

pub fn http_get(url: &str) -> Option<String> {
    let resp_len = unsafe {
        taskhub_http_get(url.as_ptr() as u32, url.len() as u32)
    };
    if resp_len < 0 { return None; }
    read_response(resp_len as usize).and_then(|b| String::from_utf8(b).ok())
}

pub fn http_request(
    method: &str,
    url: &str,
    headers: Option<&serde_json::Map<String, Value>>,
    body: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let headers_json = headers
        .map(|m| serde_json::to_string(&Value::Object(m.clone())).unwrap_or_default())
        .unwrap_or_default();
    let body_bytes = body.unwrap_or(&[]);
    let resp_len = unsafe {
        taskhub_http_request(
            method.as_ptr() as u32, method.len() as u32,
            url.as_ptr() as u32, url.len() as u32,
            headers_json.as_ptr() as u32, headers_json.len() as u32,
            body_bytes.as_ptr() as u32, body_bytes.len() as u32,
        )
    };
    if resp_len < 0 { return None; }
    read_response(resp_len as usize)
}

pub fn http_request_json(
    method: &str,
    url: &str,
    headers: Option<&serde_json::Map<String, Value>>,
    body: Option<&Value>,
) -> Option<Value> {
    let body_bytes = body.map(|v| serde_json::to_vec(v).unwrap_or_default());
    let mut h = headers.cloned().unwrap_or_default();
    if body.is_some() {
        h.insert("Content-Type".into(), Value::String("application/json".into()));
    }
    let resp = http_request(method, url, Some(&h), body_bytes.as_deref())?;
    serde_json::from_slice(&resp).ok()
}

fn read_response(len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let written = unsafe {
        taskhub_http_response_read(buf.as_mut_ptr() as u32, buf.len() as u32)
    };
    if written < 0 { return None; }
    buf.truncate(written as usize);
    Some(buf)
}
