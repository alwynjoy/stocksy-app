//! OpenAI-compatible chat (OpenAI, DeepSeek, Kimi, Custom) with tools + streaming

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

fn tool_defs_to_openai(tools: &[ToolDef]) -> Value {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema
                }
            })
        })
        .collect::<Vec<_>>()
        .into()
}

fn api_url(base: &str, path: &str) -> String {
    let b = base.trim_end_matches('/');
    let v1 = if b.ends_with("/v1") { b.to_string() } else { format!("{b}/v1") };
    format!("{v1}{path}")
}

fn is_openai_reasoning_model(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("o1") || m.contains("o3") || m.contains("o4")
        || m.starts_with("gpt-5") || m.starts_with("gpt-6") || m.contains("luna") || m.contains("reason")
}

fn is_kimi_thinking_model(m: &str) -> bool {
    m.to_lowercase().starts_with("kimi-")
}
fn should_omit_temperature(provider: &str, model: &str) -> bool {
    if provider == "kimi" || is_kimi_thinking_model(model) { return true; }
    if is_openai_reasoning_model(model) { return true; }
    if model.to_lowercase().contains("deepseek-reasoner") { return true; }
    false
}

fn extract_tokens(u: &Value) -> (Option<u32>, Option<u32>) {
    // Handles prompt_tokens/completion_tokens, input_tokens/output_tokens, and total_tokens fallback
    let prompt = u.get("prompt_tokens").or_else(|| u.get("input_tokens")).and_then(|v| v.as_u64());
    let completion = u.get("completion_tokens").or_else(|| u.get("output_tokens")).and_then(|v| v.as_u64());
    let total = u.get("total_tokens").and_then(|v| v.as_u64());
    // Derive per user's instruction: if only total available, derive input = total - output
    if prompt.is_none() && total.is_some() {
        let derived = total.unwrap().saturating_sub(completion.unwrap_or(0));
        return (Some(derived as u32), completion.map(|v| v as u32).or(total.map(|v| v as u32)));
    }
    (prompt.map(|v| v as u32), completion.map(|v| v as u32).or(total.map(|v| v as u32)))
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
    let url = api_url(&opts.base_url, "/chat/completions");

    let messages = {
        let mut out = vec![serde_json::json!({"role":"system","content": opts.system})];
        for m in &opts.messages {
            if m.role == "tool" {
                out.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": m.tool_call_id,
                    "content": m.content
                }));
            } else if m.role == "assistant" {
                if let Some(tcs) = &m.tool_calls {
                    // OpenAI requires assistant tool_calls array with stringified arguments
                    let openai_tcs: Vec<Value> = tcs.iter().map(|tc| {
                        let args_str = match &tc.arguments {
                            Value::String(s) => s.clone(),
                            v => serde_json::to_string(v).unwrap_or_else(|_| "{}".into()),
                        };
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": { "name": tc.name, "arguments": args_str }
                        })
                    }).collect();
                    if m.content.is_empty() {
                        out.push(serde_json::json!({"role":"assistant","tool_calls": openai_tcs}));
                    } else {
                        out.push(serde_json::json!({"role":"assistant","content": m.content, "tool_calls": openai_tcs}));
                    }
                } else {
                    out.push(serde_json::json!({"role": m.role, "content": m.content}));
                }
            } else {
                out.push(serde_json::json!({"role": m.role, "content": m.content}));
            }
        }
        out
    };

    // Build body with provider-scoped token limits & temperature:
    // - openai: only max_completion_tokens (gpt-6-luna rejects max_tokens)
    // - deepseek/kimi/custom: keep dual for backward compat
    // - temperature omitted for reasoning/thinking models (per doc: only 1 allowed)
    let mut body = if opts.provider == "openai" {
        if should_omit_temperature(&opts.provider, &opts.model) {
            serde_json::json!({
                "model": opts.model,
                "messages": messages,
                "tools": tool_defs_to_openai(&opts.tools),
                "tool_choice": "auto",
                "max_completion_tokens": opts.max_tokens,
                "stream": true
            })
        } else {
            serde_json::json!({
                "model": opts.model,
                "messages": messages,
                "tools": tool_defs_to_openai(&opts.tools),
                "tool_choice": "auto",
                "max_completion_tokens": opts.max_tokens,
                "temperature": opts.temperature,
                "stream": true
            })
        }
    } else {
        if should_omit_temperature(&opts.provider, &opts.model) {
            serde_json::json!({
                "model": opts.model,
                "messages": messages,
                "tools": tool_defs_to_openai(&opts.tools),
                "tool_choice": "auto",
                "max_tokens": opts.max_tokens,
                "max_completion_tokens": opts.max_tokens,
                "stream": true
            })
        } else {
            serde_json::json!({
                "model": opts.model,
                "messages": messages,
                "tools": tool_defs_to_openai(&opts.tools),
                "tool_choice": "auto",
                "max_tokens": opts.max_tokens,
                "max_completion_tokens": opts.max_tokens,
                "temperature": opts.temperature,
                "stream": true
            })
        }
    };
    // Reasoning/thinking handling (doc-correct):
    // - OpenAI reasoning: reasoning_effort none/medium/low (already handles tools)
    // - Kimi k3: reasoning_effort low/high/max mapped from thinking_budget
    // - Kimi k2.6/k2.7: thinking: {type:"enabled"} (yes thinking enabled per your pick)
    // - DeepSeek/Qwen: reasoning {budget} (legacy)
    if opts.provider == "openai" {
        if is_openai_reasoning_model(&opts.model) {
            let has_tools = !opts.tools.is_empty();
            let effort = if has_tools { "none" } else { "medium" };
            body["reasoning_effort"] = serde_json::json!(effort);
            if let Some(logger) = &debug_logger {
                let reason = if has_tools { "tools_present_chat_completions" } else { "reasoning_model" };
                logger.log("openai_reasoning_effort", serde_json::json!({"model": opts.model, "effort": effort, "provider": opts.provider, "reason": reason}));
            }
        } else if let Some(logger) = &debug_logger {
            logger.log("openai_reasoning_effort_skipped", serde_json::json!({"model": opts.model, "provider": opts.provider, "reason": "non_reasoning_model"}));
        }
    } else if opts.thinking_budget > 0 {
        let lower = opts.model.to_lowercase();
        if lower.starts_with("kimi-k3") {
            let effort = if opts.thinking_budget < 8000 { "low" } else if opts.thinking_budget < 16000 { "medium" } else if opts.thinking_budget < 24000 { "high" } else { "max" };
            body["reasoning_effort"] = serde_json::json!(effort);
        } else if lower.starts_with("kimi-") {
            // k2.6/k2.7 per doc: thinking enabled, temperature already omitted
            body["thinking"] = serde_json::json!({"type": "enabled"});
        } else if lower.starts_with("moonshot-v1") {
            // legacy moonshot — no reasoning param, temperature kept
        } else if lower.contains("deepseek") || lower.contains("qwen") || lower.contains("reason") {
            body["reasoning"] = serde_json::json!({"budget": opts.thinking_budget});
        } else {
            body["thinking_budget"] = serde_json::json!(opts.thinking_budget);
        }
    }
    // Ensure usage is returned in streaming for OpenAI (required for prompt_tokens) and for consistency for deepseek/kimi/custom
    body["stream_options"] = serde_json::json!({"include_usage": true});

    let mut resp = client()
        .post(&url)
        .header("Authorization", format!("Bearer {}", opts.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("openai request failed: {e}"))?;

    let mut status = resp.status();
    if !status.is_success() {
        let txt = resp.text().await.unwrap_or_default();
        let lower = txt.to_lowercase();
        let is_param_err = lower.contains("unknown_parameter")
            || lower.contains("unsupported_parameter")
            || lower.contains("unrecognized")
            || lower.contains("invalid_request")
            || lower.contains("extra inputs");
        // Retry for gpt-6-luna: Function tools + reasoning_effort not supported on chat/completions - set to "none" and retry
        if status.as_u16() == 400 && lower.contains("reasoning_effort") && (lower.contains("are not supported") || lower.contains("not supported")) && lower.contains("function tools") {
            if let Some(logger) = &debug_logger {
                logger.log("openai_retry_strip_reasoning_for_tools", serde_json::json!({"model": opts.model, "provider": opts.provider, "error": &txt[..txt.len().min(250)]}));
            }
            body.as_object_mut().unwrap().remove("reasoning");
            body.as_object_mut().unwrap().remove("thinking_budget");
            body["reasoning_effort"] = serde_json::json!("none");
            resp = client()
                .post(&url)
                .header("Authorization", format!("Bearer {}", opts.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("openai retry failed stripping reasoning for tools: {e}"))?;
            status = resp.status();
            if !status.is_success() {
                let txt2 = resp.text().await.unwrap_or_default();
                return Err(format!("OpenAI HTTP {}: {} (retry after stripping reasoning for tools)", status, &txt2[..txt2.len().min(800)]));
            }
        } else if status.as_u16() == 400 && (lower.contains("thinking_budget") || lower.contains("reasoning") || lower.contains("reasoning_effort") || lower.contains("stream_options") || lower.contains("\"thinking\"") || lower.contains("thinking")) && is_param_err {
            if let Some(logger) = &debug_logger {
                logger.log("openai_retry_strip_thinking", serde_json::json!({"model": opts.model, "provider": opts.provider, "error": &txt[..txt.len().min(250)], "is_param_err": is_param_err}));
            }
            let had_effort = body.as_object().map(|o| o.contains_key("reasoning_effort")).unwrap_or(false);
            body.as_object_mut().unwrap().remove("thinking_budget");
            body.as_object_mut().unwrap().remove("reasoning");
            body.as_object_mut().unwrap().remove("reasoning_effort");
            body.as_object_mut().unwrap().remove("thinking");
            body.as_object_mut().unwrap().remove("stream_options");
            resp = client()
                .post(&url)
                .header("Authorization", format!("Bearer {}", opts.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("openai retry failed: {e}"))?;
            status = resp.status();
            if !status.is_success() {
                let txt2 = resp.text().await.unwrap_or_default();
                return Err(format!("OpenAI HTTP {}: {} (retry after stripping thinking, had_effort={})", status, &txt2[..txt2.len().min(800)], had_effort));
            }
        } else if status.as_u16() == 400 && lower.contains("temperature") && (lower.contains("only 1") || lower.contains("unsupported_value") || lower.contains("not supported") || lower.contains("invalid temperature") || is_param_err) {
            if let Some(logger) = &debug_logger {
                logger.log("openai_retry_strip_temperature", serde_json::json!({"model": opts.model, "provider": opts.provider, "error": &txt[..txt.len().min(250)]}));
            }
            body.as_object_mut().unwrap().remove("temperature");
            resp = client()
                .post(&url)
                .header("Authorization", format!("Bearer {}", opts.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("openai retry failed stripping temperature: {e}"))?;
            status = resp.status();
            if !status.is_success() {
                let txt2 = resp.text().await.unwrap_or_default();
                return Err(format!("OpenAI HTTP {}: {} (retry after stripping temperature)", status, &txt2[..txt2.len().min(800)]));
            }
        } else if opts.provider == "openai" && status.as_u16() == 400 && lower.contains("max_tokens") && is_param_err {
            // gpt-6-luna rejects max_tokens, keep only max_completion_tokens
            if let Some(logger) = &debug_logger {
                logger.log("openai_retry_strip_max_tokens", serde_json::json!({"model": opts.model, "provider": opts.provider, "error": &txt[..txt.len().min(250)]}));
            }
            body.as_object_mut().unwrap().remove("max_tokens");
            resp = client()
                .post(&url)
                .header("Authorization", format!("Bearer {}", opts.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("openai retry failed stripping max_tokens: {e}"))?;
            status = resp.status();
            if !status.is_success() {
                let txt2 = resp.text().await.unwrap_or_default();
                return Err(format!("OpenAI HTTP {}: {} (retry after stripping max_tokens)", status, &txt2[..txt2.len().min(800)]));
            }
        } else {
            return Err(format!("OpenAI HTTP {}: {}", status, &txt[..txt.len().min(800)]));
        }
    }

    // Streaming SSE: data: {"choices":[{"delta":{"content":"...","tool_calls":[...]}}]}
    let mut stream = resp.bytes_stream();
    let mut content = String::new();
    let mut tool_calls_map: std::collections::HashMap<usize, ToolCall> = std::collections::HashMap::new();
    let mut usage = ChatUsage { input_tokens: None, output_tokens: None };
    let mut buf = String::new();
    let mut stream_err: Option<String> = None;
    let mut reasoning_chars: usize = 0;
    let thinking_limit = opts.thinking_budget as usize * 4; // approx chars

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
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf = buf[pos+1..].to_string();
            if line.is_empty() || line.starts_with(':') { continue; }
            if !line.starts_with("data:") { continue; }
            let data = line.trim_start_matches("data:").trim();
            if data == "[DONE]" { break; }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(u) = v.get("usage") {
                    let (inp, out) = extract_tokens(u);
                    if inp.is_some() { usage.input_tokens = inp; }
                    if out.is_some() { usage.output_tokens = out; }
                }
                if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
                    for ch in choices {
                        if let Some(delta) = ch.get("delta") {
                            let content_opt = delta.get("content").and_then(|c| c.as_str()).filter(|s| !s.is_empty());
                            let reasoning_opt = delta.get("reasoning_content").and_then(|c| c.as_str()).or_else(|| delta.get("reasoning").and_then(|c| c.as_str())).filter(|s| !s.is_empty());
                            // Avoid duplicate emit when server mirrors same text in both fields (some Qwen MLX builds)
                            if let (Some(txt), Some(reasoning)) = (content_opt, reasoning_opt) {
                                if txt == reasoning {
                                    content.push_str(txt);
                                    if let Some(logger) = &debug_logger {
                                        logger.log_thinking(txt, &opts.provider, &opts.model, "openai.content", &job_id);
                                    }
                                    if let Some(handle) = &on_token {
                                        emit_llm_token(handle, &job_id, txt, &opts.provider, &opts.model);
                                    }
                                } else {
                                    content.push_str(txt);
                                    if let Some(logger) = &debug_logger {
                                        logger.log_thinking(txt, &opts.provider, &opts.model, "openai.content", &job_id);
                                    }
                                    if let Some(handle) = &on_token {
                                        emit_llm_token(handle, &job_id, txt, &opts.provider, &opts.model);
                                    }
                                    let next_len = reasoning_chars + reasoning.len();
                                    let over_budget = opts.thinking_budget > 0 && thinking_limit > 0 && next_len > thinking_limit;
                                    if !over_budget {
                                        reasoning_chars = next_len;
                                        if let Some(logger) = &debug_logger {
                                            logger.log_thinking(reasoning, &opts.provider, &opts.model, "openai.reasoning", &job_id);
                                        }
                                        if let Some(handle) = &on_token {
                                            emit_llm_token(handle, &job_id, reasoning, &opts.provider, &opts.model);
                                        }
                                    } else {
                                        reasoning_chars = next_len;
                                    }
                                }
                            } else if let Some(txt) = content_opt {
                                content.push_str(txt);
                                if let Some(logger) = &debug_logger {
                                    logger.log_thinking(txt, &opts.provider, &opts.model, "openai.content", &job_id);
                                }
                                if let Some(handle) = &on_token {
                                    emit_llm_token(handle, &job_id, txt, &opts.provider, &opts.model);
                                }
                            } else if let Some(reasoning) = reasoning_opt {
                                // DeepSeek/Kimi/Qwen reasoning/thinking field
                                let next_len = reasoning_chars + reasoning.len();
                                let over_budget = opts.thinking_budget > 0 && thinking_limit > 0 && next_len > thinking_limit;
                                if !over_budget {
                                    reasoning_chars = next_len;
                                    if let Some(logger) = &debug_logger {
                                        logger.log_thinking(reasoning, &opts.provider, &opts.model, "openai.reasoning", &job_id);
                                    }
                                    if let Some(handle) = &on_token {
                                        emit_llm_token(handle, &job_id, reasoning, &opts.provider, &opts.model);
                                    }
                                } else {
                                    reasoning_chars = next_len;
                                }
                            }
                            if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                                for tc in tcs {
                                    let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                    let entry = tool_calls_map.entry(idx).or_insert_with(|| ToolCall { id: "".into(), name: "".into(), arguments: Value::Null });
                                    if let Some(id) = tc.get("id").and_then(|i| i.as_str()) { entry.id = id.to_string(); }
                                    if let Some(func) = tc.get("function") {
                                        if let Some(name) = func.get("name").and_then(|n| n.as_str()) { entry.name = name.to_string(); }
                                        if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                            // arguments come as JSON string fragments, accumulate
                                            let cur = entry.arguments.as_str().unwrap_or("");
                                            let mut new = cur.to_string();
                                            new.push_str(args);
                                            entry.arguments = Value::String(new);
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(u) = ch.get("usage") {
                            let (inp, out) = extract_tokens(u);
                            if usage.input_tokens.is_none() { usage.input_tokens = inp; }
                            if usage.output_tokens.is_none() { usage.output_tokens = out; }
                        }
                    }
                }
            }
        }
    }
    // Flush any buffered thinking even on stream error (save partial, per user choice 5)
    if let Some(logger) = &debug_logger { logger.flush_thinking(); }

    // Fallback to non-stream on stream decode error / timeout (local Qwen MLX can stall on large context)
    if let Some(err) = stream_err {
        if let Some(logger) = &debug_logger {
            logger.log("stream_error_fallback", serde_json::json!({"error": err, "model": opts.model, "provider": opts.provider, "fallback": "non-stream"}));
        }
        // If we already have partial content/tool_calls, try to salvage; otherwise do full non-stream retry
        if content.is_empty() && tool_calls_map.is_empty() {
            let mut fallback_body = if opts.provider == "openai" {
                if should_omit_temperature(&opts.provider, &opts.model) {
                    serde_json::json!({
                        "model": opts.model,
                        "messages": messages,
                        "tools": tool_defs_to_openai(&opts.tools),
                        "tool_choice": "auto",
                        "max_completion_tokens": opts.max_tokens,
                        "stream": false
                    })
                } else {
                    serde_json::json!({
                        "model": opts.model,
                        "messages": messages,
                        "tools": tool_defs_to_openai(&opts.tools),
                        "tool_choice": "auto",
                        "max_completion_tokens": opts.max_tokens,
                        "temperature": opts.temperature,
                        "stream": false
                    })
                }
            } else {
                if should_omit_temperature(&opts.provider, &opts.model) {
                    serde_json::json!({
                        "model": opts.model,
                        "messages": messages,
                        "tools": tool_defs_to_openai(&opts.tools),
                        "tool_choice": "auto",
                        "max_tokens": opts.max_tokens,
                        "max_completion_tokens": opts.max_tokens,
                        "stream": false
                    })
                } else {
                    serde_json::json!({
                        "model": opts.model,
                        "messages": messages,
                        "tools": tool_defs_to_openai(&opts.tools),
                        "tool_choice": "auto",
                        "max_tokens": opts.max_tokens,
                        "max_completion_tokens": opts.max_tokens,
                        "temperature": opts.temperature,
                        "stream": false
                    })
                }
            };
            if opts.provider == "openai" {
                if is_openai_reasoning_model(&opts.model) {
                    let has_tools = !opts.tools.is_empty();
                    let effort = if has_tools { "none" } else { "medium" };
                    fallback_body["reasoning_effort"] = serde_json::json!(effort);
                    if let Some(logger) = &debug_logger {
                        let reason = if has_tools { "tools_present_chat_completions" } else { "reasoning_model_fallback" };
                        logger.log("openai_reasoning_effort", serde_json::json!({"model": opts.model, "effort": effort, "provider": opts.provider, "fallback": true, "reason": reason}));
                    }
                } else if let Some(logger) = &debug_logger {
                    logger.log("openai_reasoning_effort_skipped", serde_json::json!({"model": opts.model, "provider": opts.provider, "fallback": true, "reason": "non_reasoning_model"}));
                }
            } else if opts.thinking_budget > 0 {
                let lower = opts.model.to_lowercase();
                if lower.starts_with("kimi-k3") {
                    let effort = if opts.thinking_budget < 8000 { "low" } else if opts.thinking_budget < 16000 { "medium" } else if opts.thinking_budget < 24000 { "high" } else { "max" };
                    fallback_body["reasoning_effort"] = serde_json::json!(effort);
                } else if lower.starts_with("kimi-") {
                    fallback_body["thinking"] = serde_json::json!({"type": "enabled"});
                } else if lower.starts_with("moonshot-v1") {
                    // legacy — no reasoning param
                } else if lower.contains("deepseek") || lower.contains("qwen") || lower.contains("reason") {
                    fallback_body["reasoning"] = serde_json::json!({"budget": opts.thinking_budget});
                } else {
                    fallback_body["thinking_budget"] = serde_json::json!(opts.thinking_budget);
                }
            }
            let mut fb_resp = client()
                .post(&url)
                .header("Authorization", format!("Bearer {}", opts.api_key))
                .header("Content-Type", "application/json")
                .json(&fallback_body)
                .send()
                .await
                .map_err(|e| format!("openai fallback request failed after stream error {err}: {e}"))?;
            let mut status = fb_resp.status();
            if !status.is_success() {
                let txt = fb_resp.text().await.unwrap_or_default();
                let lower = txt.to_lowercase();
                let is_param_err = lower.contains("unknown_parameter")
                    || lower.contains("unsupported_parameter")
                    || lower.contains("unrecognized")
                    || lower.contains("invalid_request")
                    || lower.contains("extra inputs");
                if status.as_u16() == 400 && lower.contains("reasoning_effort") && (lower.contains("are not supported") || lower.contains("not supported")) && lower.contains("function tools") {
                    if let Some(logger) = &debug_logger {
                        logger.log("openai_retry_strip_reasoning_for_tools", serde_json::json!({"model": opts.model, "provider": opts.provider, "error": &txt[..txt.len().min(250)], "fallback": true}));
                    }
                    fallback_body.as_object_mut().unwrap().remove("reasoning");
                    fallback_body.as_object_mut().unwrap().remove("thinking_budget");
                    fallback_body["reasoning_effort"] = serde_json::json!("none");
                    fb_resp = client()
                        .post(&url)
                        .header("Authorization", format!("Bearer {}", opts.api_key))
                        .header("Content-Type", "application/json")
                        .json(&fallback_body)
                        .send()
                        .await
                        .map_err(|e| format!("openai fallback retry failed stripping reasoning for tools after stream error {err}: {e}"))?;
                    status = fb_resp.status();
                    if !status.is_success() {
                        let txt2 = fb_resp.text().await.unwrap_or_default();
                        return Err(format!("OpenAI fallback HTTP {}: {} (original stream error: {})", status, &txt2[..txt2.len().min(800)], err));
                    }
                } else if status.as_u16() == 400 && (lower.contains("thinking_budget") || lower.contains("reasoning") || lower.contains("reasoning_effort") || lower.contains("stream_options") || lower.contains("\"thinking\"") || lower.contains("thinking")) && is_param_err {
                    if let Some(logger) = &debug_logger {
                        logger.log("openai_retry_strip_thinking", serde_json::json!({"model": opts.model, "provider": opts.provider, "error": &txt[..txt.len().min(250)], "fallback": true, "is_param_err": is_param_err}));
                    }
                    fallback_body.as_object_mut().unwrap().remove("thinking_budget");
                    fallback_body.as_object_mut().unwrap().remove("reasoning");
                    fallback_body.as_object_mut().unwrap().remove("reasoning_effort");
                    fallback_body.as_object_mut().unwrap().remove("thinking");
                    fallback_body.as_object_mut().unwrap().remove("stream_options");
                    fb_resp = client()
                        .post(&url)
                        .header("Authorization", format!("Bearer {}", opts.api_key))
                        .header("Content-Type", "application/json")
                        .json(&fallback_body)
                        .send()
                        .await
                        .map_err(|e| format!("openai fallback retry failed after stream error {err}: {e}"))?;
                    status = fb_resp.status();
                    if !status.is_success() {
                        let txt2 = fb_resp.text().await.unwrap_or_default();
                        return Err(format!("OpenAI fallback HTTP {}: {} (original stream error: {})", status, &txt2[..txt2.len().min(800)], err));
                    }
                } else if status.as_u16() == 400 && lower.contains("temperature") && (lower.contains("only 1") || lower.contains("unsupported_value") || lower.contains("not supported") || lower.contains("invalid temperature") || is_param_err) {
                    if let Some(logger) = &debug_logger {
                        logger.log("openai_retry_strip_temperature", serde_json::json!({"model": opts.model, "provider": opts.provider, "error": &txt[..txt.len().min(250)], "fallback": true}));
                    }
                    fallback_body.as_object_mut().unwrap().remove("temperature");
                    fb_resp = client()
                        .post(&url)
                        .header("Authorization", format!("Bearer {}", opts.api_key))
                        .header("Content-Type", "application/json")
                        .json(&fallback_body)
                        .send()
                        .await
                        .map_err(|e| format!("openai fallback retry failed stripping temperature after stream error {err}: {e}"))?;
                    status = fb_resp.status();
                    if !status.is_success() {
                        let txt2 = fb_resp.text().await.unwrap_or_default();
                        return Err(format!("OpenAI fallback HTTP {}: {} (original stream error: {})", status, &txt2[..txt2.len().min(800)], err));
                    }
                } else if opts.provider == "openai" && status.as_u16() == 400 && lower.contains("max_tokens") && is_param_err {
                    if let Some(logger) = &debug_logger {
                        logger.log("openai_retry_strip_max_tokens", serde_json::json!({"model": opts.model, "provider": opts.provider, "error": &txt[..txt.len().min(250)], "fallback": true}));
                    }
                    fallback_body.as_object_mut().unwrap().remove("max_tokens");
                    fb_resp = client()
                        .post(&url)
                        .header("Authorization", format!("Bearer {}", opts.api_key))
                        .header("Content-Type", "application/json")
                        .json(&fallback_body)
                        .send()
                        .await
                        .map_err(|e| format!("openai fallback retry failed stripping max_tokens after stream error {err}: {e}"))?;
                    status = fb_resp.status();
                    if !status.is_success() {
                        let txt2 = fb_resp.text().await.unwrap_or_default();
                        return Err(format!("OpenAI fallback HTTP {}: {} (original stream error: {})", status, &txt2[..txt2.len().min(800)], err));
                    }
                } else {
                    return Err(format!("OpenAI fallback HTTP {}: {} (original stream error: {})", status, &txt[..txt.len().min(800)], err));
                }
            }
            let v: Value = fb_resp.json().await.map_err(|e| format!("openai fallback decode failed: {e} (original stream error: {})", err))?;
            // Extract from non-stream shape: choices[0].message
            if let Some(choice) = v.get("choices").and_then(|c| c.as_array()).and_then(|a| a.get(0)) {
                if let Some(msg) = choice.get("message") {
                    if let Some(txt) = msg.get("content").and_then(|c| c.as_str()) {
                        content = txt.to_string();
                        if let Some(handle) = &on_token {
                            emit_llm_token(handle, &job_id, txt, &opts.provider, &opts.model);
                        }
                        if let Some(logger) = &debug_logger {
                            logger.log_thinking(txt, &opts.provider, &opts.model, "openai.fallback", &job_id);
                        }
                    }
                    if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                        for (idx, tc) in tcs.iter().enumerate() {
                            let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or(&format!("call_{idx}")).to_string();
                            let name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("").to_string();
                            let args = tc.get("function").and_then(|f| f.get("arguments")).cloned().unwrap_or(Value::Null);
                            // args may be stringified JSON
                            let args_val = if let Value::String(s) = &args {
                                serde_json::from_str::<Value>(s).unwrap_or(args.clone())
                            } else { args.clone() };
                            tool_calls_map.insert(idx, ToolCall { id, name, arguments: args_val });
                        }
                    }
                }
                if let Some(u) = v.get("usage") {
                    let (inp, out) = extract_tokens(u);
                    usage.input_tokens = inp;
                    usage.output_tokens = out;
                }
            }
            if content.is_empty() && tool_calls_map.is_empty() {
                return Err(format!("stream error: {err} and fallback produced no content"));
            }
        }
    }

    // finalize tool calls: parse arguments string -> JSON
    let mut tool_calls: Vec<ToolCall> = tool_calls_map.into_values().collect();
    for tc in &mut tool_calls {
        if let Value::String(s) = &tc.arguments {
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                tc.arguments = parsed;
            } else if s.is_empty() {
                tc.arguments = serde_json::json!({});
            }
        }
    }
    // sort by id for determinism
    tool_calls.sort_by(|a,b| a.id.cmp(&b.id));
    if let Some(logger) = &debug_logger { logger.flush_thinking(); }

    Ok(ChatResponse { content, tool_calls, usage, stop_reason: None })
}
