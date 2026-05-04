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

struct Provider<'a> {
    url: &'a str,
    auth_header: String,
}

fn provider_config<'a>(p: &str, api_key: &str, base_url: Option<&'a str>) -> Provider<'a> {
    match p {
        "anthropic" => Provider {
            url: "https://api.anthropic.com/v1/messages",
            auth_header: api_key.to_string(),
        },
        "openai" => Provider {
            url: "https://api.openai.com/v1/chat/completions",
            auth_header: format!("Bearer {}", api_key),
        },
        "openrouter" => Provider {
            url: "https://openrouter.ai/api/v1/chat/completions",
            auth_header: format!("Bearer {}", api_key),
        },
        _ => Provider {
            url: base_url.unwrap_or("http://localhost:11434/api/generate"),
            auth_header: String::new(),
        },
    }
}

fn dispatch(action: &str, input: Value) -> Value {
    let provider = input["provider"].as_str().unwrap_or("openai");
    let api_key = input["api_key"].as_str().unwrap_or("");
    let ollama_url = input["url"].as_str();
    let prov = provider_config(provider, api_key, ollama_url);

    let mut headers = Map::new();
    headers.insert("Content-Type".into(), Value::String("application/json".into()));
    if !prov.auth_header.is_empty() {
        let hname = if provider == "anthropic" { "x-api-key" } else { "Authorization" };
        headers.insert(hname.into(), Value::String(prov.auth_header.clone()));
    }
    if provider == "anthropic" {
        headers.insert("anthropic-version".into(), Value::String("2023-06-01".into()));
    }

    match action {
        "complete" => {
            let prompt = input["prompt"].as_str().unwrap_or("");
            if prompt.is_empty() { return sdk::respond_error("missing prompt"); }
            let model = input["model"].as_str().unwrap_or(default_model(provider));
            let max_tokens = input["max_tokens"].as_u64().unwrap_or(1024);
            let temperature = input["temperature"].as_f64().unwrap_or(0.7);

            let body = match provider {
                "anthropic" => json!({
                    "model": model,
                    "max_tokens": max_tokens,
                    "messages": [{"role": "user", "content": prompt}],
                }),
                "ollama" => json!({
                    "model": model,
                    "prompt": prompt,
                    "stream": false,
                    "options": {"temperature": temperature},
                }),
                _ => json!({
                    "model": model,
                    "messages": [{"role": "user", "content": prompt}],
                    "max_tokens": max_tokens,
                    "temperature": temperature,
                }),
            };

            match sdk::http_request_json("POST", prov.url, Some(&headers), Some(&body)) {
                Some(v) => extract_text(provider, &v),
                None => sdk::respond_error("complete request failed"),
            }
        }
        "chat" => {
            let messages = input.get("messages").cloned().unwrap_or_else(|| json!([]));
            let model = input["model"].as_str().unwrap_or(default_model(provider));
            let max_tokens = input["max_tokens"].as_u64().unwrap_or(1024);
            let temperature = input["temperature"].as_f64().unwrap_or(0.7);

            let body = match provider {
                "anthropic" => {
                    let mut b = json!({
                        "model": model,
                        "max_tokens": max_tokens,
                        "messages": messages,
                    });
                    if let Some(sys) = input["system"].as_str() { b["system"] = json!(sys); }
                    b
                }
                "ollama" => {
                    // Ollama chat API
                    let url = input["url"].as_str().unwrap_or("http://localhost:11434/api/chat");
                    return match sdk::http_request_json("POST", url, Some(&headers), Some(&json!({
                        "model": model,
                        "messages": messages,
                        "stream": false,
                    }))) {
                        Some(v) => extract_text("ollama_chat", &v),
                        None => sdk::respond_error("chat request failed"),
                    };
                }
                _ => json!({
                    "model": model,
                    "messages": messages,
                    "max_tokens": max_tokens,
                    "temperature": temperature,
                }),
            };

            match sdk::http_request_json("POST", prov.url, Some(&headers), Some(&body)) {
                Some(v) => extract_text(provider, &v),
                None => sdk::respond_error("chat request failed"),
            }
        }
        "embed" => {
            let text = input["input"].as_str().unwrap_or("");
            if text.is_empty() { return sdk::respond_error("missing input"); }
            let model = input["model"].as_str().unwrap_or(default_embed_model(provider));

            let (url, body) = match provider {
                "openai" => (
                    "https://api.openai.com/v1/embeddings",
                    json!({"model": model, "input": text}),
                ),
                "anthropic" => {
                    return sdk::respond_error("Anthropic does not support embeddings via API");
                }
                _ => (
                    "https://api.openai.com/v1/embeddings",
                    json!({"model": model, "input": text}),
                ),
            };

            match sdk::http_request_json("POST", url, Some(&headers), Some(&body)) {
                Some(v) => {
                    let embedding = &v["data"][0]["embedding"];
                    json!({"embedding": embedding, "model": v["model"]})
                }
                None => sdk::respond_error("embed request failed"),
            }
        }
        _ => sdk::respond_error(&format!("unknown action: {}", action)),
    }
}

fn default_model(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-haiku-4-5-20251001",
        "openai" => "gpt-4o-mini",
        "openrouter" => "openai/gpt-4o-mini",
        _ => "llama3",
    }
}

fn default_embed_model(provider: &str) -> &'static str {
    match provider {
        "openai" => "text-embedding-3-small",
        _ => "text-embedding-3-small",
    }
}

fn extract_text(provider: &str, v: &Value) -> Value {
    let (text, usage) = match provider {
        "anthropic" => {
            let t = v["content"][0]["text"].as_str().unwrap_or("").to_string();
            let u = json!({"input": v["usage"]["input_tokens"], "output": v["usage"]["output_tokens"]});
            (t, u)
        }
        "ollama" => {
            let t = v["response"].as_str().unwrap_or("").to_string();
            (t, json!({}))
        }
        "ollama_chat" => {
            let t = v["message"]["content"].as_str().unwrap_or("").to_string();
            (t, json!({}))
        }
        _ => {
            let t = v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
            let u = json!({"input": v["usage"]["prompt_tokens"], "output": v["usage"]["completion_tokens"]});
            (t, u)
        }
    };
    json!({"text": text, "usage": usage})
}
