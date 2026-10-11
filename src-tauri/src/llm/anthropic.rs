//! Anthropic native chat with tools + streaming SSE

use super::provider::{ChatOptions, ChatResponse, ChatUsage, ToolCall, ToolDef};
use futures::StreamExt;
use serde_json::Value;
use std::time::Duration;
use tauri::Emitter;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .connect_timeout(std::time::Duration::from_secs(15))
        .tcp_keepalive(std::time::Duration::from_secs(30))
        .build()
        .unwrap()
}

fn tool_defs_to_anthropic(tools: &[ToolDef]) -> Value {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema
            })
        })
        .collect::<Vec<_>>()
        .into()
}

fn emit_llm_token(handle: &tauri::AppHandle, job_id: &Option<String>, delta: &str, provider: &str, model: &str) {
    let payload = serde_json::json!({"delta": delta, "provider": provider, "model": model, "jobId": job_id});
    let _ = handle.emit("llm://token", payload.clone());
    if let Some(jid) = job_id {
        let _ = handle.emit(&format!("llm://token::{}", jid), payload);
        crate::queue::append_job_thinking(handle, jid, delta);
    }
}

pub async fn chat(
    opts: &ChatOptions,
    on_token: Option<tauri::AppHandle>,
    debug_logger: Option<std::sync::Arc<crate::debug_log::DebugLogger>>,
    job_id: Option<String>,
) -> Result<ChatResponse, String> {
    let url = format!("{}/v1/messages", opts.base_url.trim_end_matches('/'));

    // Build messages: Anthropic expects alternating user/assistant, tool results as user with tool_result blocks
    // Coalesce consecutive tool messages into a single user message with multiple tool_result blocks,
    // and preserve assistant tool_use blocks so each tool_result has a preceding tool_use (otherwise 400).
    let mut msgs: Vec<Value> = Vec::new();
    let mut pending_tool_results: Vec<Value> = Vec::new();
    for m in &opts.messages {
        if m.role == "tool" {
            pending_tool_results.push(serde_json::json!({
                "type": "tool_result",
                "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
                "content": m.content
            }));
        } else {
            if !pending_tool_results.is_empty() {
                msgs.push(serde_json::json!({"role": "user", "content": pending_tool_results.clone()}));
                pending_tool_results.clear();
            }
            if m.role == "assistant" {
                if let Some(tcs) = &m.tool_calls {
                    // Assistant with tool_use blocks — must be content array: [text?, tool_use...]
                    let mut blocks: Vec<Value> = Vec::new();
                    if !m.content.is_empty() {
                        blocks.push(serde_json::json!({"type": "text", "text": m.content}));
                    }
                    for tc in tcs {
                        let input: Value = match &tc.arguments {
                            Value::String(s) => serde_json::from_str::<Value>(s).unwrap_or_else(|_| serde_json::json!({})),
                            v if v.is_object() => v.clone(),
                            _ => serde_json::json!({}),
                        };
                        blocks.push(serde_json::json!({"type": "tool_use", "id": tc.id, "name": tc.name, "input": input}));
                    }
                    if blocks.is_empty() {
                        blocks.push(serde_json::json!({"type": "text", "text": ""}));
                    }
                    msgs.push(serde_json::json!({"role": "assistant", "content": blocks}));
                } else {
                    msgs.push(serde_json::json!({"role": m.role, "content": m.content}));
                }
            } else {
                msgs.push(serde_json::json!({"role": m.role, "content": m.content}));
            }
        }
    }
    if !pending_tool_results.is_empty() {
        msgs.push(serde_json::json!({"role": "user", "content": pending_tool_results}));
    }

    // Always enable thinking for reasoning task — use provided budget or default 24000
    let thinking_budget = if opts.thinking_budget > 0 { opts.thinking_budget } else { 24000 };
    let is_adaptive = {
        let m = opts.model.to_lowercase();
        m.contains("5-5") || m.contains("5.5") || m.contains("sonnet-5") || m.contains("opus-5") || m.contains("haiku-5")
    };
    let mut body = if is_adaptive {
        let effort = if thinking_budget < 8000 { "low" } else if thinking_budget < 16000 { "medium" } else if thinking_budget < 24000 { "high" } else { "max" };
        serde_json::json!({
            "model": opts.model,
            "system": opts.system,
            "messages": msgs,
            "tools": tool_defs_to_anthropic(&opts.tools),
            "tool_choice": {"type": "auto"},
            "max_tokens": opts.max_tokens,
            "max_completion_tokens": opts.max_tokens,
            "temperature": 1.0,
            "stream": true,
            "thinking": {"type": "adaptive"},
            "output_config": {"effort": effort}
        })
    } else {
        serde_json::json!({
            "model": opts.model,
            "system": opts.system,
            "messages": msgs,
            "tools": tool_defs_to_anthropic(&opts.tools),
            "tool_choice": {"type": "auto"},
            "max_tokens": opts.max_tokens,
            "max_completion_tokens": opts.max_tokens,
            "temperature": 1.0,
            "stream": true,
            "thinking": {"type": "enabled", "budget_tokens": thinking_budget}
        })
    };

    // One-time retry: try dual (max_tokens + max_completion_tokens) first for proxy compat,
    // strip max_completion_tokens on 400 Extra inputs (strict sonnet-5.5)
    // Also handle thinking.type.enabled -> adaptive for sonnet-5.5
    let mut resp = {
        let req = client()
            .post(&url)
            .header("x-api-key", opts.api_key.clone())
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("anthropic-beta", "interleaved-thinking-2025-05-14")
            .json(&body);
        req.send().await.map_err(|e| format!("anthropic request failed: {e}"))?
    };

    let mut status = resp.status();
    // Retry loop for strict sonnet-5.5: handle max_completion_tokens and thinking.type.enabled
    let mut retry_count = 0;
    while !status.is_success() && retry_count < 2 {
        let txt = resp.text().await.unwrap_or_default();
        let mut should_retry = false;
        if status.as_u16() == 400 && txt.contains("max_completion_tokens") && txt.contains("Extra inputs") {
            if body.get("max_completion_tokens").is_some() {
                if let Some(logger) = &debug_logger {
                    logger.log("anthropic_retry_strip", serde_json::json!({"removed": "max_completion_tokens", "model": opts.model, "error": &txt[..txt.len().min(300)]}));
                }
                body.as_object_mut().unwrap().remove("max_completion_tokens");
                should_retry = true;
            }
        } else if status.as_u16() == 400 && txt.contains("thinking.type.enabled") && txt.contains("adaptive") {
            if let Some(logger) = &debug_logger {
                logger.log("anthropic_retry_adaptive", serde_json::json!({"model": opts.model, "error": &txt[..txt.len().min(300)]}));
            }
            // Switch to adaptive thinking for sonnet-5.5: use output_config.effort instead of budget_tokens
            let effort = if thinking_budget < 8000 { "low" } else if thinking_budget < 16000 { "medium" } else if thinking_budget < 24000 { "high" } else { "max" };
            body["thinking"] = serde_json::json!({"type": "adaptive"});
            body["output_config"] = serde_json::json!({"effort": effort});
            should_retry = true;
        }
        if should_retry {
            retry_count += 1;
            let retry_req = client()
                .post(&url)
                .header("x-api-key", opts.api_key.clone())
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .header("anthropic-beta", "interleaved-thinking-2025-05-14")
                .json(&body);
            resp = retry_req.send().await.map_err(|e| format!("anthropic retry failed: {e}"))?;
            status = resp.status();
            continue;
        } else {
            return Err(format!("Anthropic HTTP {}: {}", status, &txt[..txt.len().min(800)]));
        }
    }
    if !status.is_success() {
        let txt = resp.text().await.unwrap_or_default();
        return Err(format!("Anthropic HTTP {}: {}", status, &txt[..txt.len().min(800)]));
    }

    // Streaming SSE with fallback to non-stream on decode/timeout errors (local Qwen can stall)
    let mut stream = resp.bytes_stream();
    let mut content = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut usage = ChatUsage { input_tokens: None, output_tokens: None };
    let mut current_tool: Option<ToolCall> = None;
    let mut current_json = String::new();
    let mut stream_err: Option<String> = None;
    let mut buf = String::new();
    let mut reasoning_chars: usize = 0;
    let thinking_limit = opts.thinking_budget as usize * 4;
    loop {
        let chunk_opt = match tokio::time::timeout(Duration::from_secs(120), stream.next()).await {
            Ok(opt) => opt,
            Err(_) => { stream_err = Some("idle timeout: no tokens for 120s".into()); break; }
        };
        let Some(chunk) = chunk_opt else { break; };
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => { stream_err = Some(format!("stream error: {e}")); break; }
        };
        let text = String::from_utf8_lossy(&bytes);
        buf.push_str(&text);
        // Process lines
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf = buf[pos + 1..].to_string();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if line.starts_with("event:") {
                continue;
            }
            if line.starts_with("data:") {
                let data = line.trim_start_matches("data:").trim();
                if data == "[DONE]" {
                    break;
                }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    let typ = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match typ {
                        "content_block_delta" => {
                            if let Some(delta) = v.get("delta").and_then(|d| d.get("text")).and_then(|t| t.as_str()) {
                                content.push_str(delta);
                                if let Some(logger) = &debug_logger {
                                    logger.log_thinking(delta, &opts.provider, &opts.model, "anthropic.text", &job_id);
                                }
                                if let Some(handle) = &on_token {
                                    emit_llm_token(handle, &job_id, delta, &opts.provider, &opts.model);
                                }
                            } else if let Some(delta) = v.get("delta").and_then(|d| d.get("thinking")) {
                                // Anthropic extended thinking delta (if thinking_budget enabled)
                                if let Some(th) = delta.as_str() {
                                    let next_len = reasoning_chars + th.len();
                                    let over_budget = opts.thinking_budget > 0 && thinking_limit > 0 && next_len > thinking_limit;
                                    if !over_budget {
                                        reasoning_chars = next_len;
                                        if let Some(logger) = &debug_logger {
                                            logger.log_thinking(th, &opts.provider, &opts.model, "anthropic.thinking", &job_id);
                                        }
                                        if let Some(handle) = &on_token {
                                            emit_llm_token(handle, &job_id, th, &opts.provider, &opts.model);
                                        }
                                    } else {
                                        reasoning_chars = next_len;
                                    }
                                }
                            } else if let Some(delta) = v.get("delta").and_then(|d| d.get("partial_json")) {
                                if let Some(s) = delta.as_str() {
                                    current_json.push_str(s);
                                }
                            }
                        }
                        "content_block_start" => {
                            if let Some(cb) = v.get("content_block") {
                                if cb.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                    let id = cb.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                                    let name = cb.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                    current_tool = Some(ToolCall { id, name, arguments: Value::Null });
                                    current_json.clear();
                                }
                            }
                        }
                        "content_block_stop" => {
                            if let Some(mut tc) = current_tool.take() {
                                if !current_json.is_empty() {
                                    tc.arguments = serde_json::from_str::<Value>(&current_json).unwrap_or(Value::String(current_json.clone()));
                                }
                                tool_calls.push(tc);
                                current_json.clear();
                            }
                        }
                        "message_delta" => {
                            if let Some(u) = v.get("usage") {
                                usage.output_tokens = u.get("output_tokens").and_then(|n| n.as_u64()).map(|n| n as u32);
                            }
                        }
                        "message_start" => {
                            if let Some(msg) = v.get("message").and_then(|m| m.get("usage")) {
                                usage.input_tokens = msg.get("input_tokens").and_then(|n| n.as_u64()).map(|n| n as u32);
                            }
                        }
                        _ => {}
                    }
                    // Fallback: direct content extraction for non-streaming shape (if stream disabled)
                    if let Some(t) = v.get("content").and_then(|c| c.as_array()) {
                        for block in t {
                            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                if let Some(txt) = block.get("text").and_then(|t| t.as_str()) {
                                    if content.is_empty() {
                                        content = txt.to_string();
                                    }
                                }
                            }
                        }
                    }
                    if let Some(u) = v.get("usage") {
                        usage.input_tokens = usage.input_tokens.or_else(|| u.get("input_tokens").and_then(|n| n.as_u64()).map(|n| n as u32));
                        usage.output_tokens = usage.output_tokens.or_else(|| u.get("output_tokens").and_then(|n| n.as_u64()).map(|n| n as u32));
                    }
                }
            }
        }
    }
    if let Some(logger) = &debug_logger { logger.flush_thinking(); }

    // Fallback to non-stream on stream decode error / timeout (local Qwen MLX stall)
    if let Some(err) = stream_err {
        if content.is_empty() && tool_calls.is_empty() {
            if let Some(logger) = &debug_logger {
                logger.log("stream_error_fallback", serde_json::json!({"error": err, "model": opts.model, "provider": opts.provider, "fallback": "non-stream"}));
            }
            let fb_thinking = if opts.thinking_budget > 0 { opts.thinking_budget } else { 24000 };
            let fb_is_adaptive = {
                let m = opts.model.to_lowercase();
                m.contains("5-5") || m.contains("5.5") || m.contains("sonnet-5") || m.contains("opus-5") || m.contains("haiku-5")
            };
            let mut fb_body = if fb_is_adaptive {
                let effort = if fb_thinking < 8000 { "low" } else if fb_thinking < 16000 { "medium" } else if fb_thinking < 24000 { "high" } else { "max" };
                serde_json::json!({
                    "model": opts.model,
                    "system": opts.system,
                    "messages": msgs,
                    "tools": tool_defs_to_anthropic(&opts.tools),
                    "tool_choice": {"type": "auto"},
                    "max_tokens": opts.max_tokens,
                    "max_completion_tokens": opts.max_tokens,
                    "temperature": 1.0,
                    "stream": false,
                    "thinking": {"type": "adaptive"},
                    "output_config": {"effort": effort}
                })
            } else {
                serde_json::json!({
                    "model": opts.model,
                    "system": opts.system,
                    "messages": msgs,
                    "tools": tool_defs_to_anthropic(&opts.tools),
                    "tool_choice": {"type": "auto"},
                    "max_tokens": opts.max_tokens,
                    "max_completion_tokens": opts.max_tokens,
                    "temperature": 1.0,
                    "stream": false,
                    "thinking": {"type": "enabled", "budget_tokens": fb_thinking}
                })
            };
            let fb_req = client()
                .post(&url)
                .header("x-api-key", opts.api_key.clone())
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .header("anthropic-beta", "interleaved-thinking-2025-05-14");
            let mut fb_resp = fb_req.json(&fb_body).send()
                .await
                .map_err(|e| format!("anthropic fallback request failed after stream error {err}: {e}"))?;
            let mut status = fb_resp.status();
            let mut fb_retry_count = 0;
            while !status.is_success() && fb_retry_count < 2 {
                let txt = fb_resp.text().await.unwrap_or_default();
                let mut should_retry = false;
                if status.as_u16() == 400 && txt.contains("max_completion_tokens") && txt.contains("Extra inputs") && fb_body.get("max_completion_tokens").is_some() {
                    if let Some(logger) = &debug_logger {
                        logger.log("anthropic_fallback_retry_strip", serde_json::json!({"removed": "max_completion_tokens", "model": opts.model}));
                    }
                    fb_body.as_object_mut().unwrap().remove("max_completion_tokens");
                    should_retry = true;
                } else if status.as_u16() == 400 && txt.contains("thinking.type.enabled") && txt.contains("adaptive") {
                    if let Some(logger) = &debug_logger {
                        logger.log("anthropic_fallback_retry_adaptive", serde_json::json!({"model": opts.model}));
                    }
                    let effort = if fb_thinking < 8000 { "low" } else if fb_thinking < 16000 { "medium" } else if fb_thinking < 24000 { "high" } else { "max" };
                    fb_body["thinking"] = serde_json::json!({"type": "adaptive"});
                    fb_body["output_config"] = serde_json::json!({"effort": effort});
                    should_retry = true;
                }
                if should_retry {
                    fb_retry_count += 1;
                    let retry_req = client()
                        .post(&url)
                        .header("x-api-key", opts.api_key.clone())
                        .header("anthropic-version", "2023-06-01")
                        .header("Content-Type", "application/json")
                        .header("anthropic-beta", "interleaved-thinking-2025-05-14")
                        .json(&fb_body);
                    fb_resp = retry_req.send().await.map_err(|e| format!("anthropic fallback retry failed after stream error {err}: {e}"))?;
                    status = fb_resp.status();
                    continue;
                } else {
                    return Err(format!("Anthropic fallback HTTP {}: {} (original stream error: {})", status, &txt[..txt.len().min(800)], err));
                }
            }
            if !status.is_success() {
                let txt = fb_resp.text().await.unwrap_or_default();
                return Err(format!("Anthropic fallback HTTP {}: {} (original stream error: {})", status, &txt[..txt.len().min(800)], err));
            }
            let v: Value = fb_resp.json().await.map_err(|e| format!("anthropic fallback decode failed: {e} (original stream error: {})", err))?;
            if let Some(c) = v.get("content").and_then(|c| c.as_array()) {
                for block in c {
                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                        if let Some(txt) = block.get("text").and_then(|t| t.as_str()) {
                            content.push_str(txt);
                            if let Some(logger) = &debug_logger {
                                logger.log_thinking(txt, &opts.provider, &opts.model, "anthropic.fallback", &job_id);
                            }
                            if let Some(handle) = &on_token {
                                emit_llm_token(handle, &job_id, txt, &opts.provider, &opts.model);
                            }
                        }
                    } else if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        tool_calls.push(ToolCall {
                            id: block.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
                            name: block.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                            arguments: block.get("input").cloned().unwrap_or(Value::Null),
                        });
                    }
                }
            }
            if let Some(u) = v.get("usage") {
                usage.input_tokens = u.get("input_tokens").and_then(|n| n.as_u64()).map(|n| n as u32);
                usage.output_tokens = u.get("output_tokens").and_then(|n| n.as_u64()).map(|n| n as u32);
            }
            if content.is_empty() && tool_calls.is_empty() {
                return Err(format!("stream error: {err} and fallback produced no content"));
            }
        }
    }

    if let Some(logger) = &debug_logger { logger.flush_thinking(); }

    // If streaming didn't populate content but we have fallback, try non-stream parse from leftover buf
    if content.is_empty() && tool_calls.is_empty() {
        // Attempt to re-parse as non-stream if buffer still has json
        if !buf.is_empty() {
            if let Ok(v) = serde_json::from_str::<Value>(&buf) {
                if let Some(c) = v.get("content").and_then(|c| c.as_array()) {
                    for block in c {
                        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                            if let Some(txt) = block.get("text").and_then(|t| t.as_str()) {
                                content.push_str(txt);
                            }
                        } else if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            tool_calls.push(ToolCall {
                                id: block.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
                                name: block.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                                arguments: block.get("input").cloned().unwrap_or(Value::Null),
                            });
                        }
                    }
                }
                if let Some(u) = v.get("usage") {
                    usage.input_tokens = u.get("input_tokens").and_then(|n| n.as_u64()).map(|n| n as u32);
                    usage.output_tokens = u.get("output_tokens").and_then(|n| n.as_u64()).map(|n| n as u32);
                }
            }
        }
    }

    Ok(ChatResponse { content, tool_calls, usage, stop_reason: None })
}
