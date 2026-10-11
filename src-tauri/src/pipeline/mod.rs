//! Pipeline orchestrator — fetch → validate → LLM agentic (10) → parse → engine → save
//! Takes PathBuf to avoid holding non-Send Connection across await.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::debug_log::DebugLogger;
use crate::engine;
use crate::llm::provider::{ChatMessage, ToolDef};
use crate::yahoo::transform::{get_country_name, get_market_params, MarketParamsSnapshot};

fn tool_defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "web_search".into(),
            description: "Search for analyst estimates, forward guidance, earnings transcripts, sector growth outlooks, management commentary. Use for forward estimates, industry growth, competitor analysis.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query, e.g. 'Reliance Industries forward revenue growth'"},
                    "max_results": {"type": "integer", "description": "Max results 1-10", "minimum": 1, "maximum": 10}
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "web_scrape".into(),
            description: "Extract content from a URL. Never scrape .pdf/.doc/.docx/.ppt/.xls/.xlsx — skip those. Provide ai_query to get a focused summary (e.g., 'forward revenue guidance').".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to extract"},
                    "ai_query": {"type": "string", "description": "Focused query for AI extraction, e.g., 'forward revenue guidance' — used by ScrapingDog, ignored by Tavily"}
                },
                "required": ["url"]
            }),
        },
    ]
}

const MAX_TOOL_RESULT_CHARS: usize = 12000;
fn truncate_for_llm(s: &str) -> String {
    if s.len() > MAX_TOOL_RESULT_CHARS {
        format!("{}…[truncated {} chars for LLM context — see debug log for full]", &s[..MAX_TOOL_RESULT_CHARS], s.len() - MAX_TOOL_RESULT_CHARS)
    } else {
        s.to_string()
    }
}

/// Very concise UI-facing error: code + type only, no JSON dump (full stays in debug log).
pub fn shorten_error(full: &str) -> String {
    let t = full.trim();
    // Fast paths for common non-HTTP errors — first line, 140 chars
    let http_pos = t.find("HTTP ");
    if let Some(pos) = http_pos {
        let code: String = t[pos+5..].chars().take(3).collect();
        let is_http = code.chars().all(|c| c.is_ascii_digit());
        if is_http {
            // Try to extract API error type/message from trailing JSON
            if let Some(js_start) = t[pos..].find('{') {
                let js = &t[pos+js_start..];
                if let Ok(v) = serde_json::from_str::<Value>(js) {
                    // Anthropic: {type:error, error:{type, message}}, OpenAI: {error:{type,message}}
                    let (api_type, api_msg) = if let Some(err) = v.get("error").and_then(|e| if e.is_object() { Some(e) } else { None }) {
                        (err.get("type").and_then(|x| x.as_str()).unwrap_or(""), err.get("message").and_then(|x| x.as_str()).unwrap_or(""))
                    } else {
                        (v.get("type").and_then(|x| x.as_str()).unwrap_or(""), v.get("error").and_then(|x| x.as_str()).unwrap_or(""))
                    };
                    let prefix = if t[..pos].contains("Anthropic") { "Anthropic" } else if t[..pos].contains("OpenAI") { "OpenAI" } else { "LLM" };
                    if !api_type.is_empty() {
                        let msg_snip = if !api_msg.is_empty() {
                            let end = api_msg.find('\n').unwrap_or(api_msg.len()).min(90);
                            &api_msg[..end]
                        } else { "" };
                        if msg_snip.is_empty() {
                            return format!("{prefix} HTTP {code} {api_type}").chars().take(160).collect();
                        } else {
                            return format!("{prefix} HTTP {code} {api_type} — {}", msg_snip.chars().take(90).collect::<String>()).chars().take(160).collect();
                        }
                    }
                    return format!("{prefix} HTTP {code}").chars().take(80).collect();
                }
            }
            let prefix = if t[..pos].contains("Anthropic") { "Anthropic" } else if t[..pos].contains("OpenAI") { "OpenAI" } else { "HTTP" };
            return format!("{prefix} {code}").chars().take(40).collect();
        }
    }
    // LLM chat failed, timeout, etc. — take first sentence / 140 chars
    let first = t.lines().next().unwrap_or(t);
    let cut = first.find(" — raw:").map(|i| &first[..i]).unwrap_or(first);
    let trunc: String = cut.chars().take(160).collect();
    if cut.chars().count() > 160 { format!("{trunc}…") } else { trunc }
}

fn normalize_historical_for_compare(v: &Value) -> Option<Vec<Value>> {
    let hist = v.get("financials")?.get("historical_data")?.as_array()?.clone();
    if hist.is_empty() {
        return Some(vec![]);
    }
    let mut out: Vec<Value> = Vec::with_capacity(hist.len());
    for (idx, row) in hist.iter().enumerate() {
        let mut obj = row.as_object()?.clone();
        // Normalize TTM label: first row is TTM (was MM/YYYY before, now "TTM")
        if idx == 0 {
            obj.insert("fiscal_year_end".to_string(), Value::String("TTM".to_string()));
        }
        // Keep only financial fields for compare, sorted keys for determinism
        let mut filtered = serde_json::Map::new();
        for k in ["fiscal_year_end", "revenue", "operating_income", "ebitda", "net_income", "total_debt", "total_equity", "cash_equivalents", "free_cash_flow"] {
            if let Some(val) = obj.get(k) {
                filtered.insert(k.to_string(), val.clone());
            }
        }
        out.push(Value::Object(filtered));
    }
    Some(out)
}

fn is_same_historical_data(cached_parsed: Option<String>, fresh_parsed: &Value) -> bool {
    if let Some(s) = cached_parsed {
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            let c_hist = normalize_historical_for_compare(&v);
            let f_hist = normalize_historical_for_compare(fresh_parsed);
            if let (Some(c), Some(f)) = (c_hist, f_hist) {
                return c == f;
            }
        }
        false
    } else {
        false
    }
}

fn extract_narrative_string(narrative: Option<&Value>, key: &str) -> String {
    match narrative.and_then(|v| v.get(key)) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()).or_else(|| Some(v.to_string())))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.as_str().map(|s| s.to_string()).unwrap_or_else(|| serde_json::to_string(other).unwrap_or_default()),
        None => String::new(),
    }
}

fn extract_scenario_narrative(narratives: Option<&Value>, key: &str) -> Option<String> {
    match narratives.and_then(|v| v.get(key)) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
        Some(Value::Array(arr)) => {
            let s = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()).or_else(|| Some(v.to_string())))
                .collect::<Vec<_>>()
                .join("\n");
            if s.trim().is_empty() { None } else { Some(s) }
        }
        Some(other) => {
            let s = other.as_str().map(|s| s.to_string()).unwrap_or_else(|| serde_json::to_string(other).unwrap_or_default());
            if s.trim().is_empty() || s == "null" { None } else { Some(s) }
        }
        _ => None,
    }
}

fn auto_close_json(s: &str) -> String {
    // String-aware close: counts {,},[,] only outside strings, handles \" escapes, closes unterminated string
    let mut in_str = false;
    let mut escaped = false;
    let mut brace: i32 = 0;
    let mut bracket: i32 = 0;
    for c in s.chars() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
        } else if c == '{' {
            brace += 1;
        } else if c == '}' {
            if brace > 0 { brace -= 1; }
        } else if c == '[' {
            bracket += 1;
        } else if c == ']' {
            if bracket > 0 { bracket -= 1; }
        }
    }
    let mut out = s.to_string();
    if in_str {
        out.push('"');
    }
    if bracket > 0 {
        out.push_str(&"]".repeat(bracket as usize));
    }
    if brace > 0 {
        out.push_str(&"}".repeat(brace as usize));
    }
    out
}

/// If top-level fields are null/missing but nested inside `narrative`, extract them.
/// Some models (deepseek) collapsed the schema and put opportunities/risks/etc inside narrative.
fn coalesce_field(top: Option<Value>, narrative: Option<&Value>, key: &str) -> Option<Value> {
    if let Some(v) = top {
        if !v.is_null() {
            // empty array/object counts as present; caller decides null handling
            return Some(v);
        }
    }
    narrative
        .and_then(|n| n.get(key))
        .cloned()
        .filter(|v| !v.is_null())
}

/// Strip incorrectly nested keys from narrative before persisting — narrative must only have 4 keys.
fn clean_narrative(narrative: Option<Value>) -> Option<Value> {
    narrative.map(|mut v| {
        if let Some(obj) = v.as_object_mut() {
            for k in [
                "opportunities",
                "risks",
                "investment_perspective",
                "scenario_narratives",
                "search_sources",
            ] {
                obj.remove(k);
            }
        }
        v
    })
}

fn is_valid_search_sources(v: &Value) -> bool {
    if let Some(arr) = v.as_array() {
        if arr.is_empty() {
            return true;
        }
        // curated shape: array of objects with source_name
        return arr
            .iter()
            .all(|item| item.get("source_name").and_then(|x| x.as_str()).is_some());
    }
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmDcfInputs {
    revenue_growth_rates: Vec<f64>,
    operating_margins: Vec<f64>,
    terminal_margin: f64,
    terminal_growth_rate: f64,
    fcf_conversion_factor: Option<f64>,
    financial_services_ni_factor: Option<f64>,
    cyclicality_adjustment_bps: Option<i32>,
    fcf_conversion_rationale: Option<String>,
    cyclicality_rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmConfidenceDim {
    score: i32,
    max_score: i32,
    reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmConfidence {
    overall_score: Option<i32>,
    overall_level: Option<String>,
    dimensions: std::collections::HashMap<String, LlmConfidenceDim>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmSuccess {
    abort: bool,
    abort_reason: Option<String>,
    status: Option<String>,
    limitations: Option<Value>,
    dcf_inputs: Option<LlmDcfInputs>,
    confidence: Option<LlmConfidence>,
    narrative: Option<Value>,
    scenario_narratives: Option<Value>,
    opportunities: Option<Value>,
    risks: Option<Value>,
    investment_perspective: Option<Value>,
    search_sources: Option<Value>,
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

fn fix_trailing_commas(s: &str) -> String {
    // Remove , before } or ] only outside strings (zero-dep)
    let mut out = String::with_capacity(s.len());
    let mut in_str = false;
    let mut escaped = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            if escaped {
                escaped = false;
                out.push(c);
            } else if c == '\\' {
                escaped = true;
                out.push(c);
            } else if c == '"' {
                in_str = false;
                out.push(c);
            } else {
                out.push(c);
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            // look ahead for ws then } or ]
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                // skip this comma
                i += 1;
                continue;
            }
            out.push(c);
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn escape_literal_newlines_in_strings(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 10);
    let mut in_str = false;
    let mut escaped = false;
    for c in s.chars() {
        if in_str {
            if escaped {
                escaped = false;
                out.push(c);
            } else if c == '\\' {
                escaped = true;
                out.push(c);
            } else if c == '"' {
                in_str = false;
                out.push(c);
            } else if c == '\n' {
                out.push_str("\\n");
            } else if c == '\r' {
                // normalize \r\n or standalone \r to \n inside strings
                if out.ends_with("\\n") {
                    // already handled? keep
                }
                out.push_str("\\n");
            } else {
                out.push(c);
            }
        } else {
            if c == '"' {
                in_str = true;
            }
            out.push(c);
        }
    }
    out
}

fn extract_json(text: &str) -> Option<String> {
    let t = strip_bom(text).trim();
    // 1. ```json ... ```
    if let Some(start) = t.find("```json") {
        let after = &t[start + 7..];
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim().to_string());
        } else {
            return Some(after.trim().trim_end_matches('`').trim().to_string());
        }
    }
    if let Some(start) = t.find("```") {
        let after = &t[start + 3..];
        if let Some(end) = after.find("```") {
            let cand = after[..end].trim();
            if cand.starts_with('{') {
                return Some(cand.to_string());
            }
        }
    }
    // 2. String-aware outer {} matcher (no markdown)
    let chars: Vec<char> = t.chars().collect();
    let mut in_str = false;
    let mut escaped = false;
    let mut depth: i32 = 0;
    let mut start_idx: Option<usize> = None;
    let mut end_idx: Option<usize> = None;
    for (idx, &c) in chars.iter().enumerate() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
        } else if c == '{' {
            if depth == 0 {
                start_idx = Some(idx);
            }
            depth += 1;
        } else if c == '}' {
            if depth > 0 {
                depth -= 1;
                if depth == 0 {
                    end_idx = Some(idx);
                    break;
                }
            }
        }
    }
    if let (Some(s), Some(e)) = (start_idx, end_idx) {
        return Some(chars[s..=e].iter().collect());
    }
    // Fallback to naive rfind if string-aware fails (e.g. unbalanced due to truncation)
    if let Some(s) = t.find('{') {
        if let Some(e) = t.rfind('}') {
            if e > s {
                return Some(t[s..=e].to_string());
            }
        }
    }
    None
}

fn clean_control_chars(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t').collect()
}

fn parse_llm_json(content: &str) -> Result<LlmSuccess, String> {
    let raw = extract_json(content).ok_or_else(|| "No JSON found in LLM output".to_string())?;
    let cleaned = clean_control_chars(&raw);
    // Zero-dep sanitizer + repair pipeline (tries in order, returns first Ok)
    let candidates: Vec<String> = {
        let sanitized = {
            let s = fix_trailing_commas(&cleaned);
            escape_literal_newlines_in_strings(&s)
        };
        let sanitized_closed = auto_close_json(&sanitized);
        vec![
            cleaned.clone(),
            auto_close_json(&cleaned),
            sanitized.clone(),
            sanitized_closed.clone(),
            // also try cleaned with only trailing commas fixed (no newline escape) then closed
            {
                let s = fix_trailing_commas(&cleaned);
                auto_close_json(&s)
            },
        ]
    };
    let mut last_err = String::new();
    let mut v_opt: Option<Value> = None;
    for cand in &candidates {
        match serde_json::from_str::<Value>(cand) {
            Ok(v) => {
                v_opt = Some(v);
                break;
            }
            Err(e) => {
                last_err = format!("JSON parse failed: {e} — snippet: {}", &cand[..cand.len().min(400)]);
            }
        }
    }
    let v = v_opt.ok_or(last_err)?;
    let mut parsed: LlmSuccess = serde_json::from_value::<LlmSuccess>(v.clone())
        .map_err(|e| format!("Schema mismatch: {e} — got: {}", &serde_json::to_string(&v).unwrap_or_default()[..800.min(v.to_string().len())]))?;
    // coalesce incorrectly nested fields (deepseek collapsed schema)
    if parsed.scenario_narratives.is_none()
        || parsed.opportunities.is_none()
        || parsed.risks.is_none()
        || parsed.investment_perspective.is_none()
        || parsed.search_sources.is_none()
    {
        if let Some(narr) = parsed.narrative.clone() {
            if parsed.scenario_narratives.is_none() {
                parsed.scenario_narratives = coalesce_field(parsed.scenario_narratives.clone(), Some(&narr), "scenario_narratives");
            }
            if parsed.opportunities.is_none() {
                parsed.opportunities = coalesce_field(parsed.opportunities.clone(), Some(&narr), "opportunities");
            }
            if parsed.risks.is_none() {
                parsed.risks = coalesce_field(parsed.risks.clone(), Some(&narr), "risks");
            }
            if parsed.investment_perspective.is_none() {
                parsed.investment_perspective = coalesce_field(parsed.investment_perspective.clone(), Some(&narr), "investment_perspective");
            }
            if parsed.search_sources.is_none() {
                parsed.search_sources = coalesce_field(parsed.search_sources.clone(), Some(&narr), "search_sources");
            }
            // clean narrative to 4 canonical keys before persisting
            parsed.narrative = clean_narrative(parsed.narrative);
        }
    }
    // also clean narrative even if no coalesce needed (ensure no stale nested keys persisted)
    if parsed.narrative.is_some() {
        // if narrative still has nested keys after coalesce (e.g. top-level already present but also nested), strip them
        let has_nested = parsed
            .narrative
            .as_ref()
            .and_then(|n| n.as_object())
            .map(|o| {
                ["opportunities", "risks", "investment_perspective", "scenario_narratives", "search_sources"]
                    .iter()
                    .any(|k| o.contains_key(*k))
            })
            .unwrap_or(false);
        if has_nested {
            parsed.narrative = clean_narrative(parsed.narrative);
        }
    }
    Ok(parsed)
}

fn emit_step_job(handle: &Option<AppHandle>, job_id: &Option<String>, idx: usize, status: &str) {
    if let Some(h) = handle {
        let payload = serde_json::json!({"jobId": job_id, "idx": idx, "status": status});
        let _ = h.emit("pipeline://step", payload.clone());
        if let Some(jid) = job_id {
            let _ = h.emit(&format!("pipeline://step::{}", jid), payload);
            crate::queue::set_job_step(h, jid, idx, status, None);
        }
    }
}
fn emit_job_event(handle: &Option<AppHandle>, job_id: &Option<String>, event: &str, payload: Value) {
    if let Some(h) = handle {
        let mut with_id = payload.clone();
        if let Some(obj) = with_id.as_object_mut() {
            obj.insert("jobId".into(), serde_json::json!(job_id));
        }
        let _ = h.emit(event, with_id.clone());
        if let Some(jid) = job_id {
            let _ = h.emit(&format!("{}::{}", event, jid), with_id.clone());
            if event == "debug://log_path" {
                if let Some(run_id) = with_id.get("run_id").and_then(|v| v.as_str()) {
                    crate::queue::set_job_debug_log(h, jid, run_id.to_string());
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RunAnalysisResult {
    pub report_id: String,
    pub ticker: String,
    pub country_code: String,
    pub status: String,
    pub message: String,
    pub weighted_fair_value: Option<f64>,
    pub base_case_fair_value: Option<f64>,
}

/// Cache tool config before async to avoid holding Connection across await
struct ToolConfig {
    active_provider: String,
    tavily_key: Option<String>,
    scrapingdog_key: Option<String>,
    tavily_search_url: String,
    tavily_extract_url: String,
    scrapingdog_search_url: String,
    scrapingdog_scrape_url: String,
    exclude_exts: Vec<String>,
}

fn load_tool_config(path: &Path) -> Result<ToolConfig, String> {
    let conn = crate::db::open(path).map_err(|e| e.to_string())?;
    let get_secret = |k: &str| crate::keyring_store::get(&conn, k).ok().flatten().filter(|v| !v.trim().is_empty());
    let get_setting = |k: &str, fallback: &str| crate::db::get_setting(&conn, k).ok().flatten().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| fallback.to_string());
    let raw = get_setting("search.exclude_extensions", r#"[".pdf",".doc",".xls"]"#);
    let exclude: Vec<String> = serde_json::from_str(&raw).unwrap_or_else(|_| vec![".pdf".into(), ".doc".into(), ".xls".into()]);
    let active = get_setting("search.active_provider", "tavily").to_lowercase();
    let active_provider = if active == "scrapingdog" { "scrapingdog".to_string() } else { "tavily".to_string() };
    Ok(ToolConfig {
        active_provider,
        tavily_key: get_secret("search.tavily_key"),
        scrapingdog_key: get_secret("search.scrapingdog_key"),
        tavily_search_url: get_setting("search.tavily_search_url", "https://api.tavily.com/search"),
        tavily_extract_url: get_setting("scrape.tavily_extract_url", "https://api.tavily.com/extract"),
        scrapingdog_search_url: get_setting("search.scrapingdog_search_url", "https://api.scrapingdog.com/google"),
        scrapingdog_scrape_url: get_setting("scrape.scrapingdog_scrape_url", "https://api.scrapingdog.com/scrape"),
        exclude_exts: exclude,
    })
}

async fn tool_search(cfg: &ToolConfig, query: String, max_results: usize) -> Value {
    let try_tavily = || cfg.tavily_key.as_ref().map(|k| (k.clone(), cfg.tavily_search_url.clone()));
    let try_dog = || cfg.scrapingdog_key.as_ref().map(|k| (k.clone(), cfg.scrapingdog_search_url.clone()));
    if cfg.active_provider == "tavily" {
        if let Some((key, url)) = try_tavily() {
            return crate::tools::tavily::search(&url, &key, &crate::tools::SearchArgs { query, max_results }, &cfg.exclude_exts).await;
        }
        if let Some((key, url)) = try_dog() {
            let mut v = crate::tools::scrapingdog::search(&url, &key, &crate::tools::SearchArgs { query, max_results }).await;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_fallback_warning".into(), serde_json::json!("API key missing for Tavily, attempting secondary ScrapingDog"));
            }
            return v;
        }
    } else {
        if let Some((key, url)) = try_dog() {
            return crate::tools::scrapingdog::search(&url, &key, &crate::tools::SearchArgs { query, max_results }).await;
        }
        if let Some((key, url)) = try_tavily() {
            let mut v = crate::tools::tavily::search(&url, &key, &crate::tools::SearchArgs { query, max_results }, &cfg.exclude_exts).await;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_fallback_warning".into(), serde_json::json!("API key missing for ScrapingDog, attempting secondary Tavily"));
            }
            return v;
        }
    }
    serde_json::json!({"error": format!("no search provider configured — active is {}; add key for Tavily or ScrapingDog in Settings → AI Agent Tools", cfg.active_provider)})
}

async fn tool_scrape(cfg: &ToolConfig, url: String, ai_query: Option<String>) -> Value {
    for ext in &cfg.exclude_exts {
        if url.to_lowercase().contains(&ext.to_lowercase()) {
            return serde_json::json!({"error": format!("skipped document URL per exclude_extensions: {}", url)});
        }
    }
    let try_tavily = || cfg.tavily_key.as_ref().map(|k| (k.clone(), cfg.tavily_extract_url.clone()));
    let try_dog = || cfg.scrapingdog_key.as_ref().map(|k| (k.clone(), cfg.scrapingdog_scrape_url.clone()));
    if cfg.active_provider == "tavily" {
        if let Some((key, url_str)) = try_tavily() {
            return crate::tools::tavily::extract(&url_str, &key, &crate::tools::ScrapeArgs { url, ai_query: ai_query.clone() }).await;
        }
        if let Some((key, url_str)) = try_dog() {
            let mut v = crate::tools::scrapingdog::scrape(&url_str, &key, &crate::tools::ScrapeArgs { url, ai_query: ai_query.clone() }).await;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_fallback_warning".into(), serde_json::json!("API key missing for Tavily, attempting secondary ScrapingDog"));
            }
            return v;
        }
    } else {
        if let Some((key, url_str)) = try_dog() {
            return crate::tools::scrapingdog::scrape(&url_str, &key, &crate::tools::ScrapeArgs { url, ai_query: ai_query.clone() }).await;
        }
        if let Some((key, url_str)) = try_tavily() {
            let mut v = crate::tools::tavily::extract(&url_str, &key, &crate::tools::ScrapeArgs { url, ai_query: ai_query.clone() }).await;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_fallback_warning".into(), serde_json::json!("API key missing for ScrapingDog, attempting secondary Tavily"));
            }
            return v;
        }
    }
    serde_json::json!({"error": format!("no scrape provider configured — active is {}; add key for Tavily or ScrapingDog in Settings → AI Agent Tools", cfg.active_provider)})
}

pub async fn run_analysis(
    db_path: PathBuf,
    app_handle: Option<AppHandle>,
    ticker: String,
    country_code: String,
    provider_override: Option<String>,
) -> Result<RunAnalysisResult, String> {
    run_analysis_with_job(db_path, app_handle, ticker, country_code, provider_override, None).await
}

pub async fn run_analysis_with_job(
    db_path: PathBuf,
    app_handle: Option<AppHandle>,
    ticker: String,
    country_code: String,
    provider_override: Option<String>,
    job_id: Option<String>,
) -> Result<RunAnalysisResult, String> {
    let t = ticker.trim().to_uppercase();
    let cc = country_code.trim().to_uppercase();
    if t.is_empty() { return Err("ticker empty".into()); }
    let start = std::time::Instant::now();

    // Load provider/config synchronously before async
    // provider_override (from New Analysis dropdown) takes precedence over DB active_provider to avoid race
    let (provider, model, base_url, api_key, temperature, max_tokens, thinking_budget, tool_cfg, system, fetch, used_cached) = {
        let conn = crate::db::open(&db_path).map_err(|e| e.to_string())?;
        let active_provider = if let Some(o) = provider_override.as_deref().map(|s| s.trim().to_lowercase()).filter(|s| crate::llm::is_provider(s)) {
            o
        } else {
            crate::db::get_setting(&conn, "llm.active_provider").map_err(|e| e.to_string())?.unwrap_or_else(|| "anthropic".into()).trim().to_lowercase()
        };
        let provider = active_provider.trim().to_lowercase();
        if !crate::llm::is_provider(&provider) { return Err(format!("unknown provider: {}", provider)); }
        let model = crate::db::get_setting(&conn, &format!("llm.{}.model", provider)).map_err(|e| e.to_string())?.unwrap_or_default();
        if model.trim().is_empty() { return Err(format!("model not configured for {} — set in Settings → LLM Provider", provider)); }
        let base_url = crate::db::get_setting(&conn, &format!("llm.{}.base_url", provider)).map_err(|e| e.to_string())?.unwrap_or_default();
        let api_key = crate::keyring_store::get(&conn, &format!("llm.{}.api_key", provider)).map_err(|e| e.to_string())?.ok_or_else(|| format!("API key for {} not configured — Settings → LLM Provider", provider))?;
        if api_key.trim().is_empty() { return Err(format!("API key for {} empty", provider)); }
        let has_tool = ["search.tavily_key", "search.scrapingdog_key"].iter().any(|k| crate::keyring_store::get(&conn, k).ok().flatten().map(|v| !v.trim().is_empty()).unwrap_or(false));
        if !has_tool { return Err("No AI Agent Tools key configured — Settings → AI Agent Tools needs Tavily or ScrapingDog key".into()); }
        let temperature: f32 = crate::db::get_setting(&conn, "llm.temperature").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0.3);
        let max_tokens: u32 = crate::db::get_setting(&conn, "llm.max_tokens").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(32000);
        let mut thinking_budget: u32 = crate::db::get_setting(&conn, "llm.thinking_budget").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(20000);
        // Thinking budget must be < max_tokens; clamp with at least 10000 gap per user
        if thinking_budget > 0 && thinking_budget >= max_tokens {
            let clamped = max_tokens.saturating_sub(10_000).max(10_000);
            thinking_budget = clamped.min(thinking_budget);
        }
        // Fetch financial data: reuse cached if report already exists for ticker and not same-model re-analysis
        emit_step_job(&app_handle, &job_id, 0, "running");
        let mparams = get_market_params(&conn, &cc).map_err(|e| e.to_string())?;
        let country_name = get_country_name(&conn, &cc);
        let cached_opt = crate::db::get_cached_financial(&conn, &t, &cc).ok().flatten();
        let report_exists_for_ticker: bool = conn.query_row("SELECT COUNT(*) FROM analysis_reports WHERE ticker = ?1", rusqlite::params![t], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0;
        let report_exists_same_model: bool = conn.query_row("SELECT COUNT(*) FROM analysis_reports WHERE ticker = ?1 AND llm_model = ?2", rusqlite::params![t, model], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0;
        let (fetch, used_cached) = if report_exists_for_ticker && cached_opt.is_some() && !report_exists_same_model {
            // Different model but report exists: reuse cached financials, no Yahoo call (staleness only checked for same model)
            let cached = cached_opt.as_ref().unwrap();
            let mut parsed: Value = cached.parsed_financials.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or(Value::Null);
            let raw: Value = serde_json::from_str(&cached.raw_response).unwrap_or(Value::Null);
            if parsed.get("company_info").is_some() && parsed.get("financials").is_some() {
                // Use fresh country market parameters (may have been updated via Settings) but keep ticker financials cached
                let fresh_market_params = serde_json::json!({
                    "risk_free_rate": mparams.risk_free_rate,
                    "market_risk_premium": mparams.market_risk_premium,
                    "corporate_tax_rate": mparams.corporate_tax_rate,
                    "nominal_gdp_growth": mparams.nominal_gdp_growth,
                });
                if let Some(obj) = parsed.as_object_mut() {
                    obj.insert("market_parameters".to_string(), fresh_market_params.clone());
                }
                let fr = crate::yahoo::transform::FetchResult {
                    company_info: parsed.get("company_info").cloned().unwrap_or(Value::Null),
                    current_market_data: parsed.get("current_market_data").cloned().unwrap_or(Value::Null),
                    market_parameters: fresh_market_params,
                    financials: parsed.get("financials").cloned().unwrap_or(Value::Null),
                    status: "completed".to_string(),
                    error_message: None,
                    raw_response: raw,
                    parsed_financials: parsed,
                };
                (fr, true)
            } else {
                // Corrupted cache — fall back to fresh fetch
                let fr = crate::yahoo::transform::fetch_and_transform_with_params(&t, &cc, clone_mparams(&mparams), country_name.clone()).map_err(|e| format!("fetch failed: {e}"))?;
                (fr, false)
            }
        } else {
            // No report yet, or same-model re-analysis (staleness check needs fresh fetch)
            let fr = crate::yahoo::transform::fetch_and_transform_with_params(&t, &cc, clone_mparams(&mparams), country_name.clone()).map_err(|e| format!("fetch failed: {e}"))?;
            (fr, false)
        };
        let system = crate::prompts::stitch(&conn).map_err(|e| e.to_string())?;
        let tool_cfg = load_tool_config(&db_path)?;
        // Need mparams for later engine, but fetch already contains market_parameters
        // Keep mparams for engine fallback
        drop(mparams); // not needed
        (provider, model, base_url, api_key, temperature, max_tokens, thinking_budget, tool_cfg, system, fetch, used_cached)
    };

    // Debug logger per run — persists JSONL under <app_data>/logs/<run_id>.jsonl
    let logger = std::sync::Arc::new(DebugLogger::new(&db_path, &t, &model));
    // Emit log path to frontend for "View log" link (with jobId for queue demux)
    emit_job_event(&app_handle, &job_id, "debug://log_path", serde_json::json!({"run_id": logger.run_id, "path": logger.path.display().to_string(), "ticker": t}));
    logger.log("provider_resolved", serde_json::json!({
        "override": provider_override, "resolved_provider": provider, "harness": if provider == "anthropic" { "anthropic" } else { "openai_compat" },
        "model": model, "base_url": base_url
    }));
    logger.log("inputs", serde_json::json!({
        "ticker": t, "country_code": cc, "provider": provider, "model": model, "base_url": base_url,
        "temperature": temperature, "max_tokens": max_tokens, "thinking_budget": thinking_budget,
        "tool_config": {
            "active_provider": tool_cfg.active_provider,
            "tavily_search_url": tool_cfg.tavily_search_url, "tavily_extract_url": tool_cfg.tavily_extract_url,
            "scrapingdog_search_url": tool_cfg.scrapingdog_search_url, "scrapingdog_scrape_url": tool_cfg.scrapingdog_scrape_url,
            "exclude_exts": tool_cfg.exclude_exts,
            "has_tavily": tool_cfg.tavily_key.is_some(), "has_scrapingdog": tool_cfg.scrapingdog_key.is_some()
        }
    }));
    logger.log("system_prompt", serde_json::json!({"len": system.len(), "text": system}));
    logger.log("fetch_result", serde_json::json!({"status": fetch.status, "company_info": fetch.company_info, "current_market_data": fetch.current_market_data, "market_parameters": fetch.market_parameters, "parsed_financials": fetch.parsed_financials, "used_cached": used_cached}));
    if used_cached {
        logger.log("fetch_cached_reuse", serde_json::json!({"reason": "report already exists for ticker, different model — reused financial_data_cache without Yahoo fetch", "ticker": t, "model": model}));
    }
    emit_job_event(&app_handle, &job_id, "debug://log", serde_json::json!({"kind":"run_start","run_id": logger.run_id}));

    // Handle validation failure early — do NOT save to DB per user request (only success added to report)
    if fetch.status != "completed" {
        let msg = fetch.error_message.clone().unwrap_or_else(|| format!("validation failed: {}", fetch.status));
        logger.log("validation_failed", serde_json::json!({"status": fetch.status, "error": msg}));
        emit_step_job(&app_handle, &job_id, 0, "completed");
        emit_step_job(&app_handle, &job_id, 1, "failed");
        return Ok(RunAnalysisResult { report_id: String::new(), ticker: t.clone(), country_code: cc.clone(), status: "failure".into(), message: msg, weighted_fair_value: None, base_case_fair_value: None });
    }
    emit_step_job(&app_handle, &job_id, 0, "completed");
    emit_step_job(&app_handle, &job_id, 1, "running");
    emit_step_job(&app_handle, &job_id, 1, "completed");

    // Check if report already exists for same ticker+model and financial data unchanged → skip LLM
    // Only relevant when we did a fresh Yahoo fetch (same-model re-analysis); cached-reuse (different model) never skips
    let already_exists = if used_cached {
        false
    } else {
        let conn = crate::db::open(&db_path).map_err(|e| e.to_string())?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM analysis_reports WHERE ticker = ?1 AND llm_model = ?2", rusqlite::params![t, model], |r| r.get(0)).unwrap_or(0);
        if count > 0 {
            let cached = crate::db::get_cached_financial(&conn, &t, &cc).ok().flatten();
            let cached_parsed = cached.and_then(|c| c.parsed_financials);
            is_same_historical_data(cached_parsed, &fetch.parsed_financials)
        } else {
            false
        }
    };
    if already_exists {
        logger.log("skip_analysis", serde_json::json!({"reason": "report already exists for same model and financial data unchanged", "ticker": t, "model": model}));
        // Web Search removed — pipeline is now 4 steps: Fetching → Validating → AI Analysis → Saving (AI already does web_search via tools)
        emit_step_job(&app_handle, &job_id, 2, "completed");
        emit_step_job(&app_handle, &job_id, 3, "completed");
        let conn = crate::db::open(&db_path).map_err(|e| e.to_string())?;
        let existing_id: String = conn.query_row("SELECT id FROM analysis_reports WHERE ticker = ?1 AND llm_model = ?2 ORDER BY analysis_date DESC LIMIT 1", rusqlite::params![t, model], |r| r.get(0)).unwrap_or_else(|_| format!("existing-{}-{}", t.to_lowercase(), model.to_lowercase().replace('.', "-")));
        return Ok(RunAnalysisResult { report_id: existing_id, ticker: t.clone(), country_code: cc.clone(), status: "already_exists".into(), message: "Report already generated for same model and no change in financial data.".into(), weighted_fair_value: None, base_case_fair_value: None });
    }

    // Financial data changed or first time — upsert fresh data to cache for future comparisons
    // Skip upsert when we reused cached data (different model, no Yahoo call)
    if !used_cached {
        let conn = crate::db::open(&db_path).map_err(|e| e.to_string())?;
        let raw_str = serde_json::to_string(&fetch.raw_response).unwrap_or_else(|_| "{}".into());
        let parsed_str = serde_json::to_string(&fetch.parsed_financials).unwrap_or_else(|_| "{}".into());
        let _ = crate::db::upsert_financial_cache(&conn, &t, &cc, &raw_str, &parsed_str);
    }

    let user_json = serde_json::to_string(&fetch.parsed_financials).map_err(|e| e.to_string())?;
    logger.log("user_input", serde_json::json!({"json_len": user_json.len(), "json": serde_json::from_str::<Value>(&user_json).unwrap_or(Value::String(user_json.clone()))}));
    let tools = tool_defs();
    logger.log("tool_defs", serde_json::json!(tools));
    emit_step_job(&app_handle, &job_id, 2, "running");
    let mut messages: Vec<ChatMessage> = vec![ChatMessage { role: "user".into(), content: user_json.clone(), tool_call_id: None, tool_name: None, tool_calls: None }];
    let mut final_content = String::new();
    let mut final_usage = crate::llm::provider::ChatUsage { input_tokens: None, output_tokens: None };
    let mut sum_input: u32 = 0;
    let mut sum_output: u32 = 0;
    let mut search_sources_acc: Vec<Value> = Vec::new();

    for turn in 0..10 {
        let turn_start = std::time::Instant::now();
        logger.log("llm_request", serde_json::json!({
            "turn": turn, "provider": provider, "model": model,
            "system_len": system.len(), "messages": messages, "tools": tools,
            "temperature": temperature, "max_tokens": max_tokens, "thinking_budget": thinking_budget
        }));
        // Retry once on stream/timeout errors (local Qwen on MLX can timeout on large context)
        let resp = {
            let mut last_err: Option<String> = None;
            let mut ok: Option<crate::llm::provider::ChatResponse> = None;
            for attempt in 0..2 {
                match crate::llm::chat_with_tools_job(&provider, &model, &base_url, &api_key, &system, messages.clone(), tools.clone(), temperature, max_tokens, thinking_budget, app_handle.clone(), Some(logger.clone()), job_id.clone()).await {
                    Ok(r) => { ok = Some(r); break; }
                    Err(e) => {
                        // 507 OOM and 401/429 are fail-fast — do not retry as stream
                        let is_oom = e.contains("507") || e.to_lowercase().contains("insufficient storage") || e.contains("memory ceiling");
                        let is_stream = !is_oom && (e.contains("stream error") || e.contains("error decoding") || e.contains("timeout") || e.contains("idle timeout") || e.to_lowercase().contains("decoding response body"));
                        logger.log("llm_error", serde_json::json!({"turn": turn, "attempt": attempt, "error": e, "is_stream": is_stream, "is_oom": is_oom}));
                        if is_stream && attempt == 0 {
                            logger.log("llm_retry", serde_json::json!({"turn": turn, "attempt": attempt+1, "reason": "stream/idle timeout — retrying with backoff"}));
                            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                            continue;
                        } else {
                            // Flush any buffered thinking even on final error (save partial, user choice 5)
                            logger.flush_thinking();
                            last_err = Some(e);
                            break;
                        }
                    }
                }
            }
            match ok {
                Some(r) => r,
                None => {
                    let e = last_err.unwrap_or_else(|| "LLM chat failed".into());
                    // Full stays in debug log (already logged as llm_error); UI gets concise code
                    let short = shorten_error(&e);
                    return Err(format!("LLM chat failed (turn {}): {}", turn+1, short));
                }
            }
        };
        let latency = turn_start.elapsed().as_millis() as u64;
        logger.log("llm_response", serde_json::json!({
            "turn": turn, "content": resp.content, "tool_calls": resp.tool_calls, "usage": resp.usage, "latency_ms": latency
        }));
        final_content = resp.content.clone();
        sum_input = sum_input.saturating_add(resp.usage.input_tokens.unwrap_or(0));
        sum_output = sum_output.saturating_add(resp.usage.output_tokens.unwrap_or(0));
        final_usage = crate::llm::provider::ChatUsage {
            input_tokens: if sum_input > 0 { Some(sum_input) } else { None },
            output_tokens: if sum_output > 0 { Some(sum_output) } else { None },
        };
        if resp.tool_calls.is_empty() {
            logger.log("loop_break", serde_json::json!({"reason": "no tool_calls", "turn": turn}));
            break;
        }
        messages.push(ChatMessage { role: "assistant".into(), content: resp.content.clone(), tool_call_id: None, tool_name: None, tool_calls: Some(resp.tool_calls.clone()) });
        for tc in resp.tool_calls {
            logger.log_tool_call(turn, &tc.id, &tc.name, &tc.arguments);
            let tool_start = std::time::Instant::now();
            let result = match tc.name.as_str() {
                "web_search" => {
                    let query = tc.arguments.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let max_results = tc.arguments.get("max_results").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                    if query.trim().is_empty() { serde_json::json!({"error":"query empty"}) } else {
                        let v = tool_search(&tool_cfg, query.clone(), max_results).await;
                        search_sources_acc.push(serde_json::json!({"query": query, "result": v}));
                        v
                    }
                }
                "web_scrape" => {
                    let url = tc.arguments.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let ai_query = tc.arguments.get("ai_query").and_then(|v| v.as_str()).map(|s| s.to_string());
                    if url.trim().is_empty() { serde_json::json!({"error":"url empty"}) } else {
                        let v = tool_scrape(&tool_cfg, url.clone(), ai_query.clone()).await;
                        search_sources_acc.push(serde_json::json!({"url": url, "ai_query": ai_query, "result": v}));
                        v
                    }
                }
                _ => serde_json::json!({"error": format!("unknown tool {}", tc.name)}),
            };
            let dur = tool_start.elapsed().as_millis() as u64;
            logger.log_tool_result(turn, &tc.id, &tc.name, &result, dur);
            let result_str_full = serde_json::to_string(&result).unwrap_or_else(|_| "{}".into());
            let result_str = truncate_for_llm(&result_str_full);
            if result_str.len() != result_str_full.len() {
                logger.log("tool_result_truncated", serde_json::json!({"turn": turn, "id": tc.id.clone(), "original_len": result_str_full.len(), "truncated_len": result_str.len()}));
            }
            messages.push(ChatMessage { role: "tool".into(), content: result_str, tool_call_id: Some(tc.id.clone()), tool_name: Some(tc.name.clone()), tool_calls: None });
            emit_job_event(&app_handle, &job_id, "pipeline://tool", serde_json::json!({"tool": tc.name, "id": tc.id}));
            emit_job_event(&app_handle, &job_id, "debug://log", serde_json::json!({"kind":"tool_result","tool": tc.name, "id": tc.id, "run_id": logger.run_id}));
        }
        if turn == 9 { logger.log("loop_break", serde_json::json!({"reason": "max turns 10", "turn": turn})); break; }
    }

    // Web Search removed — AI already does web_search via tools; pipeline is now 4 steps (Fetching → Validating → AI Analysis → Saving)
    logger.log("llm_final_content", serde_json::json!({"content": final_content, "usage": final_usage}));
    // log autorepair diagnostics if applied
    if let Some(raw) = extract_json(&final_content) {
        let cleaned = clean_control_chars(&raw);
        let repaired = auto_close_json(&cleaned);
        if repaired != cleaned {
            logger.log("json_autorepair", serde_json::json!({"opens": cleaned.matches('{').count(), "closes": cleaned.matches('}').count(), "repaired": true}));
        }
    }
    let parsed = parse_llm_json(&final_content);
    logger.log("parse_attempt", serde_json::json!({"ok": parsed.is_ok(), "err": parsed.as_ref().err().cloned(), "content_len": final_content.len(), "output_tokens": final_usage.output_tokens, "max_tokens": max_tokens}));
    if parsed.is_err() {
        let err_msg = parsed.as_ref().err().cloned().unwrap_or_default();
        // Empty output from provider (e.g. qwen 3.6 flash) → clean abort, no JSON parse noise, ephemeral only
        if final_content.trim().is_empty() {
            logger.log("parse_failed_empty", serde_json::json!({"model": model, "provider": provider, "output_tokens": final_usage.output_tokens, "content_len": final_content.len()}));
            let msg = format!("Empty JSON received from {} (no output). Try again or switch model.", model);
            emit_step_job(&app_handle, &job_id, 2, "failed");
            return Ok(RunAnalysisResult { report_id: String::new(), ticker: t.clone(), country_code: cc.clone(), status: "failure".into(), message: msg, weighted_fair_value: None, base_case_fair_value: None });
        }
        // Repair already attempted inside parse_llm_json via string-aware auto_close_json
        // If still failing, check for truncation (output_tokens == max_tokens) -> ask user to increase max_tokens
        let is_truncated = final_usage.output_tokens.map(|v| v as u32) == Some(max_tokens);
        if is_truncated {
            logger.log("parse_failed_truncated", serde_json::json!({"error": err_msg, "is_truncated": true, "output_tokens": final_usage.output_tokens, "max_tokens": max_tokens}));
            let msg = format!("JSON incomplete (truncated at {} chars, output_tokens {} == max_tokens {}). Increase Max tokens in Settings → Advanced LLM and re-analyse.", final_content.len(), final_usage.output_tokens.unwrap_or(0), max_tokens);
            emit_step_job(&app_handle, &job_id, 2, "failed");
            return Ok(RunAnalysisResult { report_id: String::new(), ticker: t.clone(), country_code: cc.clone(), status: "failure".into(), message: msg, weighted_fair_value: None, base_case_fair_value: None });
        }
        // For non-truncated parse errors, also guide to increase max_tokens without suggesting specific value per user
        logger.log("parse_failed", serde_json::json!({"error": err_msg, "is_truncated": false}));
        let msg = format!("JSON incomplete: {}. Increase Max tokens in Settings → Advanced LLM and re-analyse.", shorten_error(&err_msg));
        emit_step_job(&app_handle, &job_id, 2, "failed");
        return Ok(RunAnalysisResult { report_id: String::new(), ticker: t.clone(), country_code: cc.clone(), status: "failure".into(), message: msg, weighted_fair_value: None, base_case_fair_value: None });
    }
    let parsed = match parsed {
        Ok(v) => v,
        Err(e) => {
            logger.log("parse_failed", serde_json::json!({"error": e, "raw_snippet": &final_content[..final_content.len().min(2000)]}));
            let short = shorten_error(&e);
            emit_step_job(&app_handle, &job_id, 2, "failed");
            return Ok(RunAnalysisResult { report_id: String::new(), ticker: t.clone(), country_code: cc.clone(), status: "failure".into(), message: short, weighted_fair_value: None, base_case_fair_value: None });
        }
    };
    logger.log("parsed_llm", serde_json::json!(parsed));
    if parsed.abort {
        let reason = parsed.abort_reason.clone().unwrap_or_else(|| "forward_estimates_unavailable".into());
        logger.log("abort", serde_json::json!({"reason": reason}));
        emit_step_job(&app_handle, &job_id, 2, "failed");
        return Ok(RunAnalysisResult { report_id: String::new(), ticker: t.clone(), country_code: cc.clone(), status: "failure".into(), message: reason, weighted_fair_value: None, base_case_fair_value: None });
    }
    let dcf_inputs_raw = parsed.dcf_inputs.clone().ok_or_else(|| "LLM missing dcf_inputs".to_string())?;
    if dcf_inputs_raw.revenue_growth_rates.len() != 5 || dcf_inputs_raw.operating_margins.len() != 5 { return Err("dcf_inputs must have 5 growth and 5 margins".into()); }
    let dcf_inputs = engine::DcfInputs {
        revenue_growth_rates: [dcf_inputs_raw.revenue_growth_rates[0], dcf_inputs_raw.revenue_growth_rates[1], dcf_inputs_raw.revenue_growth_rates[2], dcf_inputs_raw.revenue_growth_rates[3], dcf_inputs_raw.revenue_growth_rates[4]],
        operating_margins: [dcf_inputs_raw.operating_margins[0], dcf_inputs_raw.operating_margins[1], dcf_inputs_raw.operating_margins[2], dcf_inputs_raw.operating_margins[3], dcf_inputs_raw.operating_margins[4]],
        terminal_margin: dcf_inputs_raw.terminal_margin,
        terminal_growth_rate: dcf_inputs_raw.terminal_growth_rate,
        fcf_conversion_factor: dcf_inputs_raw.fcf_conversion_factor,
        financial_services_ni_factor: dcf_inputs_raw.financial_services_ni_factor,
        cyclicality_adjustment_bps: dcf_inputs_raw.cyclicality_adjustment_bps,
        fcf_conversion_rationale: dcf_inputs_raw.fcf_conversion_rationale,
        cyclicality_rationale: dcf_inputs_raw.cyclicality_rationale,
        wacc: None,
    };
    let pf = &fetch.parsed_financials;
    let ci_v = &pf["company_info"];
    let cm_v = &pf["current_market_data"];
    let mp_v = &pf["market_parameters"];
    let fin_v = &pf["financials"];
    // Re-load market params fallback from fetch (already has them)
    let mparams_fallback = MarketParamsSnapshot {
        risk_free_rate: mp_v["risk_free_rate"].as_f64().unwrap_or(0.0656),
        market_risk_premium: mp_v["market_risk_premium"].as_f64().unwrap_or(0.0726),
        corporate_tax_rate: mp_v["corporate_tax_rate"].as_f64().unwrap_or(0.30),
        nominal_gdp_growth: mp_v["nominal_gdp_growth"].as_f64().unwrap_or(0.08733),
    };
    let engine_input = engine::EngineInput {
        company_info: engine::CompanyInfo {
            company_name: ci_v["company_name"].as_str().unwrap_or(&t).to_string(),
            ticker: t.clone(),
            exchange: ci_v["exchange"].as_str().unwrap_or("").to_string(),
            country: ci_v["country"].as_str().unwrap_or(&cc).to_string(),
            sector: ci_v["sector"].as_str().unwrap_or("").to_string(),
            industry: ci_v["industry"].as_str().unwrap_or("").to_string(),
            currency_code: ci_v["currency_code"].as_str().unwrap_or("INR").to_string(),
            is_financial_services: ci_v["is_financial_services"].as_bool().unwrap_or(false),
            market_cap_category: ci_v["market_cap_category"].as_str().or(cm_v["market_cap_category"].as_str()).unwrap_or("mid-cap").to_string(),
        },
        current_market_data: engine::CurrentMarketData {
            current_price: cm_v["current_price"].as_f64().unwrap_or(0.0),
            market_cap: cm_v["market_cap"].as_f64().unwrap_or(0.0),
            price_date: cm_v["price_date"].as_str().unwrap_or("01/01/2026").to_string(),
            shares_outstanding: cm_v["shares_outstanding"].as_f64(),
            market_cap_category: cm_v["market_cap_category"].as_str().map(|s| s.to_string()),
        },
        market_params: engine::MarketParams {
            risk_free_rate: mparams_fallback.risk_free_rate,
            market_risk_premium: mparams_fallback.market_risk_premium,
            corporate_tax_rate: mparams_fallback.corporate_tax_rate,
            nominal_gdp_growth: mparams_fallback.nominal_gdp_growth,
        },
        financials: engine::Financials {
            beta: fin_v["beta"].as_f64().unwrap_or(1.0),
            historical_data: fin_v["historical_data"].as_array().map(|arr| arr.iter().map(|r| engine::HistoricalRow {
                fiscal_year_end: r["fiscal_year_end"].as_str().unwrap_or("").to_string(),
                revenue: r["revenue"].as_f64(),
                operating_income: r["operating_income"].as_f64(),
                ebitda: r["ebitda"].as_f64(),
                net_income: r["net_income"].as_f64(),
                total_debt: r["total_debt"].as_f64(),
                total_equity: r["total_equity"].as_f64(),
                cash_equivalents: r["cash_equivalents"].as_f64(),
                free_cash_flow: r["free_cash_flow"].as_f64(),
            }).collect()).unwrap_or_default(),
        },
        dcf_inputs,
    };
    logger.log("engine_input", serde_json::json!(engine_input));
    let mut dcf_out = engine::run(engine_input).map_err(|e| { logger.log("engine_error", serde_json::json!({"error": e})); format!("engine failed: {e}") })?;
    // Merge LLM-provided scenario_narratives (optional, full raw string, null fallback) into Rust-computed scenarios
    if let Some(narratives) = parsed.scenario_narratives.as_ref() {
        for scenario in &mut dcf_out.scenarios {
            if let Some(txt) = extract_scenario_narrative(Some(narratives), &scenario.scenario_name) {
                scenario.narrative = Some(txt);
            }
        }
        logger.log("scenarios_merged", serde_json::json!({"scenario_narratives": narratives, "merged_scenarios": dcf_out.scenarios}));
    }
    logger.log("engine_output", serde_json::json!(dcf_out));
    emit_step_job(&app_handle, &job_id, 2, "completed");
    emit_step_job(&app_handle, &job_id, 3, "running");
    let search_sources = match parsed.search_sources.clone() {
        Some(v) if is_valid_search_sources(&v) => v,
        Some(v) => {
            logger.log("search_sources_invalid_shape", serde_json::json!({"got": v, "fallback": "tool_trace"}));
            Value::Array(search_sources_acc.clone())
        }
        None => Value::Array(search_sources_acc.clone()),
    };
    let confidence_value = if let Some(conf) = parsed.confidence.clone() {
        let mut dims = serde_json::Map::new();
        for (k, v) in conf.dimensions { dims.insert(k.clone(), serde_json::json!({"score": v.score, "max_score": v.max_score, "reasoning": v.reasoning})); }
        let c = engine::confidence::clamp_and_score(
            dims.get("cash_flow_predictability").and_then(|v| v.get("score")).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            dims.get("competitive_position").and_then(|v| v.get("score")).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            dims.get("financial_health").and_then(|v| v.get("score")).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            dims.get("industry_visibility").and_then(|v| v.get("score")).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            dims.get("management_quality").and_then(|v| v.get("score")).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        );
        serde_json::json!({"overall_score": c.overall_score, "overall_level": c.overall_level, "dimensions": dims})
    } else { serde_json::json!(dcf_out.confidence) };
    let conn = crate::db::open(&db_path).map_err(|e| e.to_string())?;
    let report_id = save_success(&conn, &t, &cc, &fetch, &provider, &model, &dcf_out, &parsed, search_sources.clone(), confidence_value.clone(), final_usage.clone(), start.elapsed().as_millis() as i64)?;
    logger.log("save_success", serde_json::json!({"report_id": report_id, "ticker": t, "weighted_fair_value": dcf_out.weighted_fair_value, "search_sources": search_sources, "confidence": confidence_value, "usage": final_usage, "duration_ms": start.elapsed().as_millis()}));
    logger.log("run_completed", serde_json::json!({"report_id": report_id, "status": "success"}));
    emit_step_job(&app_handle, &job_id, 3, "completed");
    Ok(RunAnalysisResult { report_id, ticker: t, country_code: cc, status: "success".into(), message: "Analysis completed".into(), weighted_fair_value: Some(dcf_out.weighted_fair_value), base_case_fair_value: Some(dcf_out.base_case_fair_value) })
}

fn clone_mparams(m: &MarketParamsSnapshot) -> MarketParamsSnapshot {
    MarketParamsSnapshot { risk_free_rate: m.risk_free_rate, market_risk_premium: m.market_risk_premium, corporate_tax_rate: m.corporate_tax_rate, nominal_gdp_growth: m.nominal_gdp_growth }
}

fn save_success(
    conn: &rusqlite::Connection,
    ticker: &str,
    country_code: &str,
    fetch: &crate::yahoo::transform::FetchResult,
    provider: &str,
    model: &str,
    out: &engine::DcfOutput,
    parsed: &LlmSuccess,
    search_sources: Value,
    confidence_value: Value,
    usage: crate::llm::provider::ChatUsage,
    duration_ms: i64,
) -> Result<String, String> {
    use chrono::Utc;
    let id = format!("rep-{}-{}-{}", ticker.to_lowercase(), model.to_lowercase().replace('.', "-").replace('/', "-"), Utc::now().format("%Y%m%d%H%M%S"));
    let price_date = fetch.current_market_data.get("price_date").and_then(|v| v.as_str()).unwrap_or("01/01/2026").to_string();
    let price_date_norm = normalize_price_date(&price_date);
    let analysis_date = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let company_name = fetch.company_info.get("company_name").and_then(|v| v.as_str()).unwrap_or(ticker).to_string();
    let exchange = fetch.company_info.get("exchange").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let sector = fetch.company_info.get("sector").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let industry = fetch.company_info.get("industry").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let currency = fetch.company_info.get("currency_code").and_then(|v| v.as_str()).unwrap_or("INR").to_string();
    let current_price = fetch.current_market_data.get("current_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let market_cap = fetch.current_market_data.get("market_cap").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let scenarios_json = serde_json::to_string(&out.scenarios).unwrap_or_else(|_| "[]".into());
    let sensitivity_json = serde_json::to_string(&out.sensitivity_matrix).unwrap_or_else(|_| "[]".into());
    // Use clamped LLM-derived confidence (confidence_value) — out.confidence is engine empty (0/low)
    let confidence_score = confidence_value.get("overall_score").and_then(|v| v.as_i64()).unwrap_or(out.confidence.overall_score as i64) as f64;
    let confidence_level = confidence_value.get("overall_level").and_then(|v| v.as_str()).unwrap_or(&out.confidence.overall_level).to_string();
    let confidence_details = serde_json::to_string(&confidence_value).unwrap_or_else(|_| "{}".into());
    let bull_case_fair_value = out.scenarios.iter().find(|s| s.scenario_name == "optimistic").map(|s| s.fair_value).unwrap_or(0.0);
    let bear_case_fair_value = out.scenarios.iter().find(|s| s.scenario_name == "pessimistic").map(|s| s.fair_value).unwrap_or(0.0);
    let dcf_assumptions = serde_json::to_string(&serde_json::json!({
        "terminal_margin": out.terminal_margin,
        "terminal_growth_rate": out.terminal_growth_rate,
        "fcf_factor_used": out.fcf_factor_used,
        "ni_factor_used": out.ni_factor_used,
        "cyclicality_bps_used": out.cyclicality_bps_used,
        "wacc": out.wacc,
        "raw": parsed.dcf_inputs
    })).unwrap_or_else(|_| "{}".into());
    let sensitivity_base = serde_json::json!({
        "wacc": out.wacc,
        "terminal_growth": out.terminal_growth_rate,
        "terminal_margin": out.terminal_margin,
        "base_fair_value": out.base_case_fair_value,
        "fcf_year_5": out.fcf_year_5,
        "shares_outstanding": out.shares_outstanding,
        "latest_cash": out.latest_cash,
        "latest_debt": out.latest_debt
    });
    let country_snapshot = serde_json::to_string(&fetch.market_parameters).unwrap_or_else(|_| "{}".into());
    let search_sources_str = serde_json::to_string(&search_sources).unwrap_or_else(|_| "[]".into());
    let sensitivity_base_str = serde_json::to_string(&sensitivity_base).unwrap_or_else(|_| "{}".into());
    let input_thinking = usage.input_tokens.map(|v| v as i64);
    let input_output = usage.output_tokens.map(|v| v as i64);
    let primary_exists: i64 = conn.query_row("SELECT COUNT(*) FROM analysis_reports WHERE ticker = ?1 AND is_primary = 1", rusqlite::params![ticker], |r| r.get(0)).unwrap_or(0);
    let is_primary = if primary_exists == 0 { 1 } else { 0 };
    conn.execute(
        "INSERT INTO analysis_reports (
            id, ticker, company_name, exchange, country_code, sector, industry, market_cap_category,
            price_date, analysis_date, is_primary, status,
            current_price, market_cap, currency_code,
            base_case_fair_value, weighted_fair_value, bull_case_fair_value, bear_case_fair_value, valuation_gap_pct, valuation_verdict,
            wacc, cost_of_equity, cost_of_debt_after_tax, raw_beta, blume_adjusted_beta,
            terminal_growth_rate, terminal_margin,
            upside_pct, downside_pct, risk_reward_ratio, risk_reward_label,
            scenarios, sensitivity_matrix, confidence_score, confidence_level, confidence_details,
            ai_business_context, ai_historical_performance, ai_growth_projections, ai_opportunities_risks, ai_investment_perspective,
            dcf_assumptions, country_params_snapshot, search_sources, sensitivity_base_parameters,
            llm_provider, llm_model, llm_thinking_tokens, llm_output_tokens, llm_duration_ms
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, 'completed',
            ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24, ?25,
            ?26, ?27,
            ?28, ?29, ?30, ?31,
            ?32, ?33, ?34, ?35, ?36,
            ?37, ?38, ?39, ?40, ?41,
            ?42, ?43, ?44, ?45,
            ?46, ?47, ?48, ?49, ?50
        ) ON CONFLICT(ticker, llm_model, price_date) DO UPDATE SET
            company_name=excluded.company_name, exchange=excluded.exchange, sector=excluded.sector, industry=excluded.industry, market_cap_category=excluded.market_cap_category,
            current_price=excluded.current_price, market_cap=excluded.market_cap, currency_code=excluded.currency_code,
            base_case_fair_value=excluded.base_case_fair_value, weighted_fair_value=excluded.weighted_fair_value, bull_case_fair_value=excluded.bull_case_fair_value, bear_case_fair_value=excluded.bear_case_fair_value, valuation_gap_pct=excluded.valuation_gap_pct, valuation_verdict=excluded.valuation_verdict,
            wacc=excluded.wacc, cost_of_equity=excluded.cost_of_equity, cost_of_debt_after_tax=excluded.cost_of_debt_after_tax, raw_beta=excluded.raw_beta, blume_adjusted_beta=excluded.blume_adjusted_beta, terminal_growth_rate=excluded.terminal_growth_rate, terminal_margin=excluded.terminal_margin,
            upside_pct=excluded.upside_pct, downside_pct=excluded.downside_pct, risk_reward_ratio=excluded.risk_reward_ratio, risk_reward_label=excluded.risk_reward_label,
            scenarios=excluded.scenarios, sensitivity_matrix=excluded.sensitivity_matrix, confidence_score=excluded.confidence_score, confidence_level=excluded.confidence_level, confidence_details=excluded.confidence_details,
            ai_business_context=excluded.ai_business_context, ai_historical_performance=excluded.ai_historical_performance, ai_growth_projections=excluded.ai_growth_projections, ai_opportunities_risks=excluded.ai_opportunities_risks, ai_investment_perspective=excluded.ai_investment_perspective,
            dcf_assumptions=excluded.dcf_assumptions, country_params_snapshot=excluded.country_params_snapshot, search_sources=excluded.search_sources, sensitivity_base_parameters=excluded.sensitivity_base_parameters,
            llm_provider=excluded.llm_provider, llm_thinking_tokens=excluded.llm_thinking_tokens, llm_output_tokens=excluded.llm_output_tokens, llm_duration_ms=excluded.llm_duration_ms,
            analysis_date=excluded.analysis_date, status=excluded.status, error_message=NULL, updated_at=datetime('now')",
        rusqlite::params![
            id, ticker, company_name, exchange, country_code, sector, industry, {
                // Correct source is current_market_data.market_cap_category; fallback to recomputed from market_cap if null
                fetch.current_market_data.get("market_cap_category").and_then(|v| v.as_str())
                    .or(fetch.company_info.get("market_cap_category").and_then(|v| v.as_str()))
                    .map(|s| s.to_string())
                    .or_else(|| {
                        let mc = fetch.current_market_data.get("market_cap").and_then(|v| v.as_f64())
                            .or_else(|| fetch.current_market_data.get("market_cap").and_then(|v| v.as_i64().map(|i| i as f64)));
                        // fallback price*shares if market_cap still null
                        let mc = mc.or_else(|| {
                            let p = fetch.current_market_data.get("current_price").and_then(|v| v.as_f64());
                            let s = fetch.current_market_data.get("shares_outstanding").and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)));
                            match (p, s) { (Some(pp), Some(ss)) if pp>0.0 && ss>0.0 => Some(pp*ss), _ => None }
                        });
                        crate::yahoo::quote::market_cap_category(mc, &country_code)
                    })
                    .unwrap_or_else(|| "mid-cap".to_string())
            },
            price_date_norm, analysis_date, is_primary,
            current_price, market_cap, currency,
            out.base_case_fair_value, out.weighted_fair_value, bull_case_fair_value, bear_case_fair_value, out.valuation_gap_pct, out.valuation_verdict,
            out.wacc, out.cost_of_equity, out.cost_of_debt_after_tax, out.raw_beta, out.blume_adjusted_beta,
            out.terminal_growth_rate, out.terminal_margin,
            out.upside_pct, out.downside_pct, out.risk_reward_ratio, out.risk_reward_label,
            scenarios_json, sensitivity_json, confidence_score, confidence_level, confidence_details,
            parsed.narrative.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default()).unwrap_or_default(),
            extract_narrative_string(parsed.narrative.as_ref(), "historical_performance_insights"),
            extract_narrative_string(parsed.narrative.as_ref(), "growth_projection_rationale"),
            serde_json::to_string(&serde_json::json!({"opportunities": parsed.opportunities, "risks": parsed.risks})).unwrap_or_default(),
            serde_json::to_string(&parsed.investment_perspective).unwrap_or_default(),
            dcf_assumptions, country_snapshot, search_sources_str, sensitivity_base_str,
            provider, model, input_thinking, input_output, duration_ms
        ],
    ).map_err(|e| format!("save success failed: {e}"))?;
    // Enforce max 3 per (ticker, llm_model) ordered by analysis_date DESC — evict oldest
    {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM analysis_reports WHERE ticker = ?1 AND llm_model = ?2",
                rusqlite::params![ticker, model],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if count > 3 {
            let ids_to_keep: Vec<String> = {
                let mut stmt = conn
                    .prepare("SELECT id FROM analysis_reports WHERE ticker = ?1 AND llm_model = ?2 ORDER BY analysis_date DESC LIMIT 3")
                    .map_err(|e| format!("eviction prepare failed: {e}"))?;
                let rows = stmt
                    .query_map(rusqlite::params![ticker, model], |r| r.get::<_, String>(0))
                    .map_err(|e| format!("eviction query failed: {e}"))?;
                rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("eviction collect failed: {e}"))?
            };
            if !ids_to_keep.is_empty() {
                // Build NOT IN clause safely via params
                let placeholders = ids_to_keep.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!(
                    "DELETE FROM analysis_reports WHERE ticker = ?1 AND llm_model = ?2 AND id NOT IN ({})",
                    placeholders
                );
                let mut params: Vec<rusqlite::types::Value> = vec![ticker.to_string().into(), model.to_string().into()];
                params.extend(ids_to_keep.into_iter().map(|s| s.into()));
                conn.execute(&sql, rusqlite::params_from_iter(params)).map_err(|e| format!("eviction delete failed: {e}"))?;
                // If no primary remains, promote newest remaining to primary
                let primary_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM analysis_reports WHERE ticker = ?1 AND is_primary = 1",
                        rusqlite::params![ticker],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if primary_count == 0 {
                    conn.execute(
                        "UPDATE analysis_reports SET is_primary = 1 WHERE id = (SELECT id FROM analysis_reports WHERE ticker = ?1 ORDER BY analysis_date DESC LIMIT 1)",
                        rusqlite::params![ticker],
                    )
                    .ok();
                }
            }
        }
    }
    Ok(id)
}

fn normalize_price_date(s: &str) -> String {
    if s.contains('-') && s.len() >= 10 {
        if s.chars().nth(4) == Some('-') { return s[..10].to_string(); }
    }
    if s.contains('/') {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() == 3 { return format!("{}-{}-{}", parts[2], parts[1], parts[0]); }
        else if parts.len() == 2 { return format!("{}-{}-01", parts[1], parts[0]); }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extract_json_fenced() {
        let txt = r#"Here is story ```json
{"abort":false,"dcf_inputs":{"revenue_growth_rates":[0.1,0.1,0.1,0.1,0.1]}}
``` end"#;
        let j = extract_json(txt).unwrap();
        assert!(j.contains("\"abort\""));
    }
    #[test]
    fn normalize_dates() {
        assert_eq!(normalize_price_date("12/07/2026"), "2026-07-12");
        assert_eq!(normalize_price_date("03/2026"), "2026-03-01");
        assert_eq!(normalize_price_date("2026-09-19"), "2026-09-19");
    }
    #[test]
    fn parse_llm_json_includes_scenario_narratives() {
        let json = r#"{"abort":false,"dcf_inputs":{"revenue_growth_rates":[0.1,0.1,0.1,0.1,0.1],"operating_margins":[0.1,0.1,0.1,0.1,0.1],"terminal_margin":0.1,"terminal_growth_rate":0.05},"scenario_narratives":{"optimistic":"bull story","expected":"base story","pessimistic":"bear story"}}"#;
        let parsed = parse_llm_json(json).unwrap();
        assert!(parsed.scenario_narratives.is_some());
        let v = parsed.scenario_narratives.unwrap();
        assert_eq!(extract_scenario_narrative(Some(&v), "optimistic").unwrap(), "bull story");
        assert_eq!(extract_scenario_narrative(Some(&v), "expected").unwrap(), "base story");
        assert_eq!(extract_scenario_narrative(Some(&v), "pessimistic").unwrap(), "bear story");
        assert!(extract_scenario_narrative(Some(&v), "missing").is_none());
    }
    #[test]
    fn parse_llm_json_without_scenario_narratives_is_none() {
        let json = r#"{"abort":false,"dcf_inputs":{"revenue_growth_rates":[0.1,0.1,0.1,0.1,0.1],"operating_margins":[0.1,0.1,0.1,0.1,0.1],"terminal_margin":0.1,"terminal_growth_rate":0.05}}"#;
        let parsed = parse_llm_json(json).unwrap();
        assert!(parsed.scenario_narratives.is_none());
    }
    #[test]
    fn auto_close_repairs_truncated_json() {
        let raw = r#"{"abort":false,"narrative":{"business_description":"x","historical_performance_insights":"y","growth_projection_rationale":"z","terminal_growth_derivation":"w","search_sources":[{"source_name":"S","source_url":"https://example.com","data_obtained":"d"}]}}"#;
        // missing outer }
        let truncated = &raw[..raw.len()-1];
        let repaired = auto_close_json(truncated);
        assert!(repaired.ends_with('}'));
        let v: Value = serde_json::from_str(&repaired).unwrap();
        assert!(v.get("narrative").is_some());
    }
    #[test]
    fn parse_coalesces_nested_fields_inside_narrative() {
        let json = r#"{"abort":false,"status":"success","dcf_inputs":{"revenue_growth_rates":[0.1,0.1,0.1,0.1,0.1],"operating_margins":[0.1,0.1,0.1,0.1,0.1],"terminal_margin":0.1,"terminal_growth_rate":0.05},"narrative":{"business_description":"desc","historical_performance_insights":"• a","growth_projection_rationale":"r","terminal_growth_derivation":"d","opportunities":[{"catalyst_name":"AI buildout","description":"d","potential_impact":"p","timeline":null,"source_url":null}],"risks":[{"risk_category":"Supply","description":"d","potential_impact":"p","likelihood":"moderate","source_url":null}],"scenario_narratives":{"optimistic":"opt","expected":"exp","pessimistic":"pes"},"investment_perspective":{"verdict":"v","monitoring_metrics":[{"metric_name":"m","target_value":"t","red_flag_condition":"r"}]},"search_sources":[{"source_name":"Src","source_url":"https://example.com","data_obtained":"data"}]}}"#;
        let parsed = parse_llm_json(json).unwrap();
        assert!(parsed.opportunities.is_some(), "opportunities should be coalesced from narrative");
        assert!(parsed.risks.is_some());
        assert!(parsed.scenario_narratives.is_some());
        assert!(parsed.investment_perspective.is_some());
        assert!(parsed.search_sources.is_some());
        // narrative should be cleaned to 4 keys
        let narr = parsed.narrative.unwrap();
        let obj = narr.as_object().unwrap();
        assert_eq!(obj.len(), 4);
        assert!(obj.contains_key("business_description"));
        assert!(!obj.contains_key("opportunities"));
    }
    #[test]
    fn parse_handles_truncated_with_nested_fields() {
        let full = r#"{"abort":false,"status":"success","dcf_inputs":{"revenue_growth_rates":[0.1,0.1,0.1,0.1,0.1],"operating_margins":[0.1,0.1,0.1,0.1,0.1],"terminal_margin":0.1,"terminal_growth_rate":0.05},"narrative":{"business_description":"desc","historical_performance_insights":"• a","growth_projection_rationale":"r","terminal_growth_derivation":"d","opportunities":[{"catalyst_name":"C","description":"d","potential_impact":"p","timeline":null,"source_url":null}],"search_sources":[{"source_name":"S","source_url":"https://example.com","data_obtained":"d"}]}}"#;
        let truncated = &full[..full.len()-1]; // missing final }
        let parsed = parse_llm_json(truncated).unwrap();
        assert!(parsed.opportunities.is_some());
        assert!(parsed.search_sources.is_some());
    }
    #[test]
    fn parse_handles_literal_newline_and_trailing_comma() {
        // literal newline inside string value (unescaped) — ICEMAKE-NS case, zero-dep sanitizer must escape
        let json = "{\"abort\":false,\"dcf_inputs\":{\"revenue_growth_rates\":[0.1,0.1,0.1,0.1,0.1],\"operating_margins\":[0.1,0.1,0.1,0.1,0.1],\"terminal_margin\":0.1,\"terminal_growth_rate\":0.05},\"narrative\":{\"business_description\":\"desc\",\"historical_performance_insights\":\"line1\nline2\",\"growth_projection_rationale\":\"r\",\"terminal_growth_derivation\":\"d\"}}";
        let parsed = parse_llm_json(json).expect("should parse literal newline with sanitizer");
        assert!(parsed.narrative.is_some());
        // trailing comma case
        let json2 = r#"{"abort":false,"dcf_inputs":{"revenue_growth_rates":[0.1,0.1,0.1,0.1,0.1],"operating_margins":[0.1,0.1,0.1,0.1,0.1],"terminal_margin":0.1,"terminal_growth_rate":0.05,},"narrative":{"business_description":"x","historical_performance_insights":"a","growth_projection_rationale":"r","terminal_growth_derivation":"d",}}"#;
        let parsed2 = parse_llm_json(json2).expect("should handle trailing comma");
        assert!(parsed2.narrative.is_some());
        // full ICEMAKE-NS-like with limitations and escaped \n (should still parse)
        let json3 = r#"{"abort":false,"abort_reason":null,"status":"success","limitations":[],"dcf_inputs":{"revenue_growth_rates":[0.27,0.20,0.15,0.12,0.10],"operating_margins":[0.040,0.058,0.068,0.076,0.080],"terminal_margin":0.080,"terminal_growth_rate":0.083,"fcf_conversion_factor":0.45,"cyclicality_adjustment_bps":40},"narrative":{"business_description":"Ice Make","historical_performance_insights":"Sentence 1. Sentence 2. Sentence 3.","growth_projection_rationale":"r","terminal_growth_derivation":"d"},"confidence":{"overall_score":46,"overall_level":"moderate","dimensions":{"cash_flow_predictability":{"score":10,"max_score":30,"reasoning":"r"}}}}"#;
        let parsed3 = parse_llm_json(json3).unwrap();
        assert!(parsed3.dcf_inputs.is_some());
    }
}
