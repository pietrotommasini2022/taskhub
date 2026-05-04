use crate::manifest::PluginManifest;
use crate::permissions::PermissionChecker;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use taskhub_core::{engine::Action, TaskHubError};
use tracing::{debug, warn};
use wasmtime::{Engine, Linker, Module, Store};

struct PluginState {
    /// Pending HTTP response (written by host import, read by plugin via response_read import).
    http_response: Option<Vec<u8>>,
}

pub struct WasmPlugin {
    manifest: PluginManifest,
    engine: Engine,
    module: Module,
    http_client: reqwest::blocking::Client,
}

impl WasmPlugin {
    pub fn load(wasm_bytes: &[u8], manifest: PluginManifest) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm_bytes).context("compile WASM module")?;
        Ok(Self {
            manifest,
            engine,
            module,
            http_client: reqwest::blocking::Client::new(),
        })
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Execute a plugin action (blocking — must be called from a non-async context or spawn_blocking).
    pub fn execute_action(&self, action_id: &str, input: Value) -> Result<Value> {
        if !self.manifest.has_action(action_id) {
            bail!("plugin '{}' has no action '{}'", self.manifest.id, action_id);
        }

        let input_json = serde_json::to_string(&input)?;
        let action_id_owned = action_id.to_string();
        let manifest_id = self.manifest.id.clone();
        let permissions = self.manifest.permissions.clone();
        let http_client = self.http_client.clone();

        let mut linker: Linker<PluginState> = Linker::new(&self.engine);

        // Import: taskhub::log(level, ptr, len)
        linker.func_wrap(
            "taskhub",
            "log",
            |mut caller: wasmtime::Caller<'_, PluginState>, level: i32, ptr: i32, len: i32| {
                if let Some(msg) = read_wasm_str(&mut caller, ptr as u32, len as u32) {
                    match level {
                        0 => tracing::error!(target: "plugin", "{}", msg),
                        1 => warn!(target: "plugin", "{}", msg),
                        2 => tracing::info!(target: "plugin", "{}", msg),
                        _ => debug!(target: "plugin", "{}", msg),
                    }
                }
            },
        )?;

        // Import: taskhub::http_request(method_ptr, method_len, url_ptr, url_len,
        //                               headers_json_ptr, headers_json_len,
        //                               body_ptr, body_len) -> i32
        // Generic HTTP request. headers_json = JSON object {"Key": "Value"}.
        // Returns response length, -1 on error.
        let permissions_clone = permissions.clone();
        let manifest_id_clone = manifest_id.clone();
        let http_client_req = http_client.clone();
        linker.func_wrap(
            "taskhub",
            "http_request",
            move |mut caller: wasmtime::Caller<'_, PluginState>,
                  method_ptr: i32, method_len: i32,
                  url_ptr: i32, url_len: i32,
                  headers_ptr: i32, headers_len: i32,
                  body_ptr: i32, body_len: i32| -> i32 {
                let method = read_wasm_str(&mut caller, method_ptr as u32, method_len as u32)
                    .unwrap_or_else(|| "GET".into());
                let Some(url) = read_wasm_str(&mut caller, url_ptr as u32, url_len as u32) else {
                    return -1;
                };
                let checker = PermissionChecker::new(&manifest_id_clone, &permissions_clone);
                if let Err(e) = checker.check_network(&url) {
                    warn!("{}", e);
                    return -1;
                }
                let headers_json = if headers_len > 0 {
                    read_wasm_str(&mut caller, headers_ptr as u32, headers_len as u32)
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                } else {
                    None
                };
                let body_bytes = if body_len > 0 {
                    read_wasm_bytes(&mut caller, body_ptr as u32, body_len as u32)
                } else {
                    vec![]
                };

                let m = reqwest::Method::from_bytes(method.as_bytes())
                    .unwrap_or(reqwest::Method::GET);
                let mut req = http_client_req.request(m, &url);
                if let Some(serde_json::Value::Object(map)) = headers_json {
                    for (k, v) in &map {
                        if let Some(val) = v.as_str() {
                            req = req.header(k.as_str(), val);
                        }
                    }
                }
                if !body_bytes.is_empty() {
                    req = req.body(body_bytes);
                }
                match req.send().and_then(|r| r.bytes()) {
                    Ok(bytes) => {
                        let len = bytes.len() as i32;
                        caller.data_mut().http_response = Some(bytes.to_vec());
                        len
                    }
                    Err(e) => { warn!("plugin http_request '{}' failed: {}", url, e); -1 }
                }
            },
        )?;

        // Keep backward-compat http_get import (wraps http_request).
        let permissions_clone = permissions.clone();
        let manifest_id_clone = manifest_id.clone();
        let http_client2 = http_client.clone();
        linker.func_wrap(
            "taskhub",
            "http_get",
            move |mut caller: wasmtime::Caller<'_, PluginState>, url_ptr: i32, url_len: i32| -> i32 {
                let Some(url) = read_wasm_str(&mut caller, url_ptr as u32, url_len as u32) else { return -1; };
                let checker = PermissionChecker::new(&manifest_id_clone, &permissions_clone);
                if let Err(e) = checker.check_network(&url) { warn!("{}", e); return -1; }
                match http_client2.get(&url).send().and_then(|r| r.bytes()) {
                    Ok(bytes) => { let l = bytes.len() as i32; caller.data_mut().http_response = Some(bytes.to_vec()); l }
                    Err(e) => { warn!("plugin http_get '{}' failed: {}", url, e); -1 }
                }
            },
        )?;

        // Import: taskhub::http_response_read(dst_ptr, dst_len) -> i32
        // Copies buffered response into plugin memory. Returns bytes written, -1 if no response.
        linker.func_wrap(
            "taskhub",
            "http_response_read",
            |mut caller: wasmtime::Caller<'_, PluginState>, dst_ptr: i32, dst_len: i32| -> i32 {
                let Some(resp) = caller.data_mut().http_response.take() else {
                    return -1;
                };
                let to_write = resp.len().min(dst_len as usize);
                let mem = caller.get_export("memory").and_then(|e| e.into_memory());
                if let Some(mem) = mem {
                    let data = unsafe { mem.data_mut(&mut caller) };
                    let start = dst_ptr as usize;
                    if start + to_write <= data.len() {
                        data[start..start + to_write].copy_from_slice(&resp[..to_write]);
                        return to_write as i32;
                    }
                }
                -1
            },
        )?;

        let mut store = Store::new(&self.engine, PluginState { http_response: None });
        let instance = linker
            .instantiate(&mut store, &self.module)
            .context("instantiate WASM module")?;

        let alloc = instance
            .get_typed_func::<u32, u32>(&mut store, "taskhub_alloc")
            .context("plugin missing export 'taskhub_alloc'")?;
        let dealloc = instance
            .get_typed_func::<(u32, u32), ()>(&mut store, "taskhub_dealloc")
            .context("plugin missing export 'taskhub_dealloc'")?;
        let execute = instance
            .get_typed_func::<(u32, u32, u32, u32), u64>(&mut store, "taskhub_execute")
            .context("plugin missing export 'taskhub_execute'")?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .context("plugin missing export 'memory'")?;

        let action_bytes = action_id_owned.as_bytes();
        let input_bytes = input_json.as_bytes();

        let action_ptr = alloc.call(&mut store, action_bytes.len() as u32)?;
        let input_ptr = alloc.call(&mut store, input_bytes.len() as u32)?;

        memory
            .write(&mut store, action_ptr as usize, action_bytes)
            .context("write action to WASM memory")?;
        memory
            .write(&mut store, input_ptr as usize, input_bytes)
            .context("write input to WASM memory")?;

        let packed = execute.call(
            &mut store,
            (action_ptr, action_bytes.len() as u32, input_ptr, input_bytes.len() as u32),
        )?;

        dealloc.call(&mut store, (action_ptr, action_bytes.len() as u32))?;
        dealloc.call(&mut store, (input_ptr, input_bytes.len() as u32))?;

        let result_ptr = (packed >> 32) as u32;
        let result_len = (packed & 0xFFFF_FFFF) as u32;

        if result_len == 0 {
            return Ok(Value::Null);
        }

        let mut out_bytes = vec![0u8; result_len as usize];
        memory
            .read(&store, result_ptr as usize, &mut out_bytes)
            .context("read result from WASM memory")?;
        dealloc.call(&mut store, (result_ptr, result_len))?;

        let result: Value =
            serde_json::from_slice(&out_bytes).context("parse plugin output JSON")?;

        if let Some(err) = result.get("__taskhub_error").and_then(|e| e.as_str()) {
            bail!("plugin '{}' action '{}' error: {}", manifest_id, action_id_owned, err);
        }

        Ok(result)
    }
}

pub struct WasmAction {
    plugin: Arc<WasmPlugin>,
    plugin_id: String,
    action_id: String,
}

impl WasmAction {
    pub fn new(plugin: Arc<WasmPlugin>, action_id: String) -> Self {
        let plugin_id = plugin.manifest().id.clone();
        Self { plugin, plugin_id, action_id }
    }
}

#[async_trait]
impl Action for WasmAction {
    fn plugin_id(&self) -> &str { &self.plugin_id }
    fn action_id(&self) -> &str { &self.action_id }

    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        let plugin = self.plugin.clone();
        let action_id = self.action_id.clone();
        tokio::task::spawn_blocking(move || plugin.execute_action(&action_id, input))
            .await
            .map_err(|e| TaskHubError::Plugin(e.to_string()))?
            .map_err(|e| TaskHubError::Plugin(e.to_string()))
    }
}

fn read_wasm_str(caller: &mut wasmtime::Caller<'_, PluginState>, ptr: u32, len: u32) -> Option<String> {
    let mem = caller.get_export("memory")?.into_memory()?;
    let mut buf = vec![0u8; len as usize];
    mem.read(caller, ptr as usize, &mut buf).ok()?;
    String::from_utf8(buf).ok()
}

fn read_wasm_bytes(caller: &mut wasmtime::Caller<'_, PluginState>, ptr: u32, len: u32) -> Vec<u8> {
    let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
        return vec![];
    };
    let mut buf = vec![0u8; len as usize];
    mem.read(caller, ptr as usize, &mut buf).ok();
    buf
}
