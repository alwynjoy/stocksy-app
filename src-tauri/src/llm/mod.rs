//! Live LLM provider introspection: "Fetch Models" and "Test Connection"
//! against the configured `base_url`, using the stored API key from the
//! SQLite `secrets` table (XOR-obfuscated). Anthropic uses its native `/v1/models` + `/v1/messages` API;
//! OpenAI/DeepSeek/Kimi/Custom use the OpenAI-compatible shape.

pub mod anthropic;
pub mod openai_compat;
pub mod provider;

use serde::{Deserialize, Serialize};

pub const PROVIDERS: [&str; 5] = ["anthropic", "deepseek", "openai", "kimi", "custom"];

pub const DEFAULT_MODELS: [&str; 5] = [
    "claude-sonnet-4-5",
    "deepseek-reasoner",
    "gpt-4o",
    "moonshot-v1-auto",
    "",
];

pub fn is_provider(p: &str) -> bool {
    PROVIDERS.contains(&p)
}

#[derive(Serialize)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: u64,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelMeta>,
}

#[derive(Deserialize)]
struct ModelMeta {
    id: String,
}

#[derive(Serialize, Deserialize)]
struct ChatRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

fn api_url(base: &str, path: &str) -> String {
    let b = base.trim_end_matches('/');
    let v1 = if b.ends_with("/v1") {
        b.to_string()
    } else {
        format!("{b}/v1")
    };
    format!("{v1}{path}")
}

fn is_dated_snapshot(id: &str) -> bool {
    // Anthropic dated snapshots contain an 8-digit date like 20250219 or 20241022
    // We hide them because user wants only alias models (e.g. claude-sonnet-4-5)
    let bytes = id.as_bytes();
    if bytes.len() < 8 {
        return false;
    }
    for window in bytes.windows(8) {
        if window.iter().all(|b| b.is_ascii_digit()) {
            return true;
        }
    }
    false
}

fn filter_models(provider: &str, ids: Vec<String>) -> Vec<String> {
    ids.into_iter()
        .filter(|id| match provider {
            "anthropic" => {
                id.starts_with("claude-")
                    && !id.contains("realtime")
                    && !id.ends_with("-audio")
                    && !is_dated_snapshot(id)
            }
            "openai" => {
                id.starts_with("gpt-")
                    || id.starts_with("o1")
                    || id.starts_with("o3")
                    || id.starts_with("o4")
            }
            "kimi" => id.starts_with("moonshot-") || id.starts_with("kimi-"),
            "deepseek" => id.starts_with("deepseek-"),
            "custom" => true,
            _ => !id.contains("embed") && !id.contains("speech") && !id.contains("image"),
        })
        .collect()
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .build()
        .map_err(|e| format!("http client init failed: {e}"))
}

fn apply_auth(
    req: reqwest::RequestBuilder,
    provider: &str,
    api_key: &Option<String>,
) -> reqwest::RequestBuilder {
    if provider == "anthropic" {
        let req = req.header("anthropic-version", "2023-06-01");
        match api_key {
            Some(k) if !k.is_empty() => req.header("x-api-key", k),
            _ => req,
        }
    } else {
        match api_key {
            Some(k) if !k.is_empty() => req.header("Authorization", format!("Bearer {k}")),
            _ => req,
        }
    }
}

pub async fn fetch_models(
    provider: &str,
    base_url: &str,
    api_key: Option<String>,
) -> Result<Vec<String>, String> {
    if !is_provider(provider) {
        return Err(format!("unknown provider: {provider}"));
    }

    let url = api_url(base_url, "/models");
    let req = apply_auth(client()?.get(&url), provider, &api_key);
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let snippet = &body[..body.len().min(300)];
        return Err(format!("HTTP {status}: {snippet}"));
    }
    let parsed: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("unexpected response shape: {e}"))?;
    let mut ids = filter_models(provider, parsed.data.into_iter().map(|m| m.id).collect());
    if ids.is_empty() {
        return Err("provider returned no usable model ids".to_string());
    }
    ids.sort();
    Ok(ids)
}

pub async fn test_connection(
    provider: &str,
    base_url: &str,
    model: &str,
    api_key: Option<String>,
) -> Result<TestResult, String> {
    if !is_provider(provider) {
        return Err(format!("unknown provider: {provider}"));
    }
    let url = if provider == "anthropic" {
        format!("{}/v1/messages", base_url.trim_end_matches('/'))
    } else {
        api_url(base_url, "/chat/completions")
    };

    let body = serde_json::to_string(&ChatRequest {
        model: model.to_string(),
        max_tokens: 1,
        messages: vec![ChatMessage {
            role: "user".into(),
            content: "ping".into(),
        }],
    })
    .map_err(|e| e.to_string())?;

    let start = std::time::Instant::now();
    let resp = apply_auth(client()?.post(&url), provider, &api_key)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let latency_ms = start.elapsed().as_millis() as u64;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let snippet = text.chars().take(300).collect::<String>();

    if status.is_success() {
        return Ok(TestResult {
            ok: true,
            message: format!("Connected ({latency_ms} ms)."),
            latency_ms,
        });
    }

    // Any non-2xx is a failure — show the actual API error message to the user
    // so they can act on it (auth failure, workspace scoping issue, wrong model, etc.)
    Ok(TestResult {
        ok: false,
        message: format!("HTTP {status}: {snippet}"),
        latency_ms,
    })
}

/// Dispatch chat to correct provider with streaming token emits + optional debug logger + optional jobId for queue demux.
pub async fn chat_with_tools(
    provider: &str,
    model: &str,
    base_url: &str,
    api_key: &str,
    system: &str,
    messages: Vec<provider::ChatMessage>,
    tools: Vec<provider::ToolDef>,
    temperature: f32,
    max_tokens: u32,
    thinking_budget: u32,
    app_handle: Option<tauri::AppHandle>,
    debug_logger: Option<std::sync::Arc<crate::debug_log::DebugLogger>>,
) -> Result<provider::ChatResponse, String> {
    chat_with_tools_job(provider, model, base_url, api_key, system, messages, tools, temperature, max_tokens, thinking_budget, app_handle, debug_logger, None).await
}

pub async fn chat_with_tools_job(
    provider: &str,
    model: &str,
    base_url: &str,
    api_key: &str,
    system: &str,
    messages: Vec<provider::ChatMessage>,
    tools: Vec<provider::ToolDef>,
    temperature: f32,
    max_tokens: u32,
    thinking_budget: u32,
    app_handle: Option<tauri::AppHandle>,
    debug_logger: Option<std::sync::Arc<crate::debug_log::DebugLogger>>,
    job_id: Option<String>,
) -> Result<provider::ChatResponse, String> {
    let opts = provider::ChatOptions {
        provider: provider.to_string(),
        model: model.to_string(),
        base_url: base_url.to_string(),
        api_key: api_key.to_string(),
        system: system.to_string(),
        messages,
        tools,
        temperature,
        max_tokens,
        thinking_budget,
    };
    if provider == "anthropic" {
        anthropic::chat(&opts, app_handle, debug_logger, job_id).await
    } else {
        openai_compat::chat(&opts, app_handle, debug_logger, job_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_are_4plus1() {
        assert_eq!(PROVIDERS.len(), 5);
        assert!(is_provider("anthropic"));
        assert!(is_provider("custom"));
        assert!(!is_provider("google"));
    }

    #[test]
    fn models_filter_groups_correctly() {
        let ids = vec![
            "claude-sonnet-4-5".into(),
            "claude-3-7-sonnet-20250219".into(),
            "claude-realtime".into(),
            "gpt-4o".into(),
            "gpt-o3".into(),
            "text-embedding-3-small".into(),
            "moonshot-v1-auto".into(),
            "deepseek-reasoner".into(),
        ];
        assert_eq!(
            filter_models("anthropic", ids.clone()),
            vec!["claude-sonnet-4-5"]
        );
        assert_eq!(
            filter_models("openai", ids.clone()),
            vec!["gpt-4o", "gpt-o3"]
        );
        assert_eq!(filter_models("kimi", ids.clone()), vec!["moonshot-v1-auto"]);
        assert_eq!(
            filter_models("deepseek", ids.clone()),
            vec!["deepseek-reasoner"]
        );
        assert!(filter_models("custom", ids).contains(&"text-embedding-3-small".to_string()));
    }

    #[test]
    fn api_urls_normalize_v1() {
        assert_eq!(
            api_url("https://api.openai.com", "/models"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            api_url("https://api.moonshot.ai/v1", "/models"),
            "https://api.moonshot.ai/v1/models"
        );
        assert_eq!(
            api_url("http://localhost:11434/v1/", "/models"),
            "http://localhost:11434/v1/models"
        );
    }
}
