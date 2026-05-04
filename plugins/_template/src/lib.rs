// TaskHub plugin template.
// Compile with: cargo build --target wasm32-wasip1 --release

mod sdk;
use sdk::{execute_action, respond, respond_error};
use serde_json::{json, Value};

// ── Register your actions here ────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn taskhub_execute(
    action_ptr: u32, action_len: u32,
    input_ptr: u32,  input_len:  u32,
) -> u64 {
    execute_action(action_ptr, action_len, input_ptr, input_len, dispatch)
}

fn dispatch(action: &str, input: Value) -> Value {
    match action {
        "run" => action_run(input),
        _ => respond_error(&format!("unknown action: {action}")),
    }
}

// ── Your action implementations ───────────────────────────────────────────────

fn action_run(input: Value) -> Value {
    let message = input["message"].as_str().unwrap_or("Hello from TaskHub plugin!");
    respond(json!({ "output": message }))
}

// ── ABI exports (required by host) ───────────────────────────────────────────

#[no_mangle]
pub extern "C" fn taskhub_alloc(size: u32) -> u32 {
    sdk::alloc(size)
}

#[no_mangle]
pub extern "C" fn taskhub_dealloc(ptr: u32, size: u32) {
    sdk::dealloc(ptr, size)
}
