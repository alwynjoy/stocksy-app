//! Debug logger for AI agentic loops — JSONL per run under <app_data>/logs/
//! Captures inputs, system prompt, LLM requests/responses, per-token thinking, tool calls/results, engine I/O.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

const MAX_LOG_FILES: usize = 100;
const MAX_PAYLOAD_STR: usize = 20_000;
const MAX_TOOL_RESULT_STR: usize = 12_000;
const THINKING_BUFFER_CHARS: usize = 512;
const THINKING_FLUSH_MS: u64 = 500;

fn log_dir_for(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("logs")
}

fn redact_value(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                let kl = k.to_lowercase();
                if kl.contains("api_key")
                    || kl.contains("tavily_key")
                    || kl.contains("brave_key")
                    || kl.contains("custom_provider_key")
                    || kl == "authorization"
                {
                    out.insert(k, Value::String("***REDACTED***".into()));
                } else if kl.contains("key") && val.is_string() {
                    // heuristic: any key-like string containing 'key' with long value
                    let s = val.as_str().unwrap_or("");
                    if s.len() > 8 {
                        out.insert(k, Value::String("***REDACTED***".into()));
                    } else {
                        out.insert(k, redact_value(val));
                    }
                } else {
                    out.insert(k, redact_value(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(redact_value).collect()),
        Value::String(s) => {
            // also scrub Authorization-like header values that slipped in as strings
            if s.starts_with("Bearer ") || s.starts_with("tvly-") || s.starts_with("BSA") {
                Value::String("***REDACTED***".into())
            } else {
                Value::String(s)
            }
        }
        other => other,
    }
}

fn truncate_value(v: Value, limit: usize) -> Value {
    match v {
        Value::String(s) => {
            if s.len() > limit {
                Value::String(format!("{}…[truncated {} chars]", &s[..limit], s.len() - limit))
            } else {
                Value::String(s)
            }
        }
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                out.insert(k, truncate_value(val, limit));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(|x| truncate_value(x, limit)).collect()),
        other => other,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LogFileMeta {
    pub run_id: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified: String,
}

struct ThinkingBufferInner {
    buf: String,
    count: usize,
    last_flush: Instant,
}

pub struct DebugLogger {
    pub run_id: String,
    pub path: PathBuf,
    start: Instant,
    seq: AtomicUsize,
    enabled: bool,
    thinking_buffer: Mutex<ThinkingBufferInner>,
}

impl DebugLogger {
    /// Create a new logger for a run. Writes a header line and prunes old logs.
    pub fn new(db_path: &Path, ticker: &str, model: &str) -> Self {
        let dir = log_dir_for(db_path);
        let _ = std::fs::create_dir_all(&dir);
        let ts = chrono::Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
        let safe_model = model
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        let safe_ticker = ticker.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else {'-' }).collect::<String>();
        let run_id = format!("{}-{}-{}", safe_ticker, safe_model, ts);
        let path = dir.join(format!("{}.jsonl", run_id));
        let logger = Self {
            run_id: run_id.clone(),
            path: path.clone(),
            start: Instant::now(),
            seq: AtomicUsize::new(0),
            enabled: true,
            thinking_buffer: Mutex::new(ThinkingBufferInner { buf: String::new(), count: 0, last_flush: Instant::now() }),
        };
        // header
        logger.log("run_start", json!({"ticker": ticker, "model": model, "log_path": path.display().to_string()}));
        // prune
        let _ = prune_old_logs(&dir);
        logger
    }

    /// No-op logger for tests.
    pub fn noop() -> Self {
        Self {
            run_id: "noop".into(),
            path: PathBuf::from("/tmp/stocksy-noop.jsonl"),
            start: Instant::now(),
            seq: AtomicUsize::new(0),
            enabled: false,
            thinking_buffer: Mutex::new(ThinkingBufferInner { buf: String::new(), count: 0, last_flush: Instant::now() }),
        }
    }

    pub fn is_enabled(&self) -> bool { self.enabled }

    pub fn log(&self, kind: &str, payload: Value) {
        if !self.enabled { return; }
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let elapsed = self.start.elapsed().as_millis() as i64;
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        // redact + apply per-kind truncation
        let limit = match kind {
            "tool_result" | "tool_search_result" | "tool_scrape_result" => MAX_TOOL_RESULT_STR,
            "llm_response" | "engine_output" => MAX_PAYLOAD_STR,
            _ => MAX_PAYLOAD_STR,
        };
        let safe = truncate_value(redact_value(payload), limit);
        let entry = json!({
            "ts": ts,
            "elapsed_ms": elapsed,
            "seq": seq,
            "run_id": self.run_id,
            "kind": kind,
            "payload": safe
        });
        let line = serde_json::to_string(&entry).unwrap_or_else(|_| "{}".into()) + "\n";
        // best-effort append
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) {
            use std::io::Write as _;
            let _ = f.write_all(line.as_bytes());
        }
        // also mirror to stdout for `tauri dev` console
        println!("[debug_log:{}] {} {}", self.run_id, kind, serde_json::to_string(&entry).unwrap_or_default());
    }

    /// Aggregated thinking — buffers per-token deltas and flushes as thinking_agg (512 chars / 500ms / newline). Keeps file small while UI stays per-token.
    pub fn log_thinking(&self, delta: &str, provider: &str, model: &str, stream: &str, job_id: &Option<String>) {
        if !self.enabled || delta.is_empty() {
            return;
        }
        let mut buf = match self.thinking_buffer.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        buf.buf.push_str(delta);
        buf.count += 1;
        let should_flush = buf.buf.len() >= THINKING_BUFFER_CHARS
            || buf.last_flush.elapsed() >= Duration::from_millis(THINKING_FLUSH_MS)
            || delta.contains('\n');
        if should_flush {
            let aggregated = std::mem::take(&mut buf.buf);
            let count = std::mem::replace(&mut buf.count, 0);
            buf.last_flush = Instant::now();
            drop(buf);
            self.log(
                "thinking_agg",
                json!({"aggregated_delta": aggregated, "count": count, "provider": provider, "model": model, "stream": stream, "jobId": job_id}),
            );
        }
    }

    pub fn flush_thinking(&self) {
        if !self.enabled {
            return;
        }
        let mut buf = match self.thinking_buffer.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if !buf.buf.is_empty() {
            let aggregated = std::mem::take(&mut buf.buf);
            let count = std::mem::replace(&mut buf.count, 0);
            buf.last_flush = Instant::now();
            drop(buf);
            self.log("thinking_agg", json!({"aggregated_delta": aggregated, "count": count, "stream": "flush", "jobId": Option::<String>::None}));
        }
    }

    /// Convenience: log with automatic elapsed + seq handled via `log`.
    pub fn log_llm_request(&self, turn: usize, system_len: usize, messages: &Value, tools: &Value, opts: &Value) {
        self.log("llm_request", json!({"turn": turn, "system_len": system_len, "messages": messages, "tools": tools, "opts": opts}));
    }
    pub fn log_llm_response(&self, turn: usize, content: &str, tool_calls: &Value, usage: &Value, latency_ms: u64) {
        self.log("llm_response", json!({"turn": turn, "content": content, "tool_calls": tool_calls, "usage": usage, "latency_ms": latency_ms}));
    }
    pub fn log_tool_call(&self, turn: usize, id: &str, name: &str, args: &Value) {
        self.log("tool_call", json!({"turn": turn, "id": id, "name": name, "args": args}));
    }
    pub fn log_tool_result(&self, turn: usize, id: &str, name: &str, result: &Value, duration_ms: u64) {
        self.log("tool_result", json!({"turn": turn, "id": id, "name": name, "result": result, "duration_ms": duration_ms}));
    }
}

fn prune_old_logs(dir: &Path) -> std::io::Result<()> {
    let mut entries: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("jsonl") { continue; }
        let mt = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        entries.push((p, mt));
    }
    if entries.len() <= MAX_LOG_FILES { return Ok(()); }
    entries.sort_by_key(|(_, t)| *t);
    let to_remove = entries.len() - MAX_LOG_FILES;
    for (p, _) in entries.iter().take(to_remove) {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

// ── Helpers for Tauri commands ───────────────────────────────────────────────

pub fn list_logs(db_path: &Path) -> Vec<LogFileMeta> {
    let dir = log_dir_for(db_path);
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("jsonl") { continue; }
        let meta = e.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                Some(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            })
            .unwrap_or_default();
        let run_id = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        out.push(LogFileMeta { run_id, path: p.display().to_string(), size_bytes: size, modified });
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

pub fn read_log(db_path: &Path, run_id: &str, limit: Option<usize>) -> Result<(String, Vec<Value>), String> {
    let dir = log_dir_for(db_path);
    // sanitize run_id to avoid path traversal
    if run_id.contains('/') || run_id.contains('\\') || run_id.contains("..") {
        return Err("invalid run_id".into());
    }
    let path = dir.join(format!("{}.jsonl", run_id));
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read log failed: {e}"))?;
    let lines: Vec<Value> = data
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect();
    let total = lines.len();
    let sliced = if let Some(n) = limit {
        let n = n.clamp(1, 5000);
        if lines.len() > n { lines[lines.len()-n..].to_vec() } else { lines }
    } else {
        lines
    };
    Ok((format!("{} lines", total), sliced))
}

pub fn clear_logs(db_path: &Path) -> Result<usize, String> {
    let dir = log_dir_for(db_path);
    let mut n = 0usize;
    for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}

pub fn log_dir_path(db_path: &Path) -> String {
    log_dir_for(db_path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redact_secrets() {
        let v = json!({"llm.anthropic.api_key": "sk-abc123", "search.tavily_key": "tvly-xyz", "ok": "hello"});
        let r = redact_value(v);
        assert_eq!(r["llm.anthropic.api_key"], "***REDACTED***");
        assert_eq!(r["search.tavily_key"], "***REDACTED***");
        assert_eq!(r["ok"], "hello");
    }
    #[test]
    fn truncate_string() {
        let v = Value::String("a".repeat(25000));
        let t = truncate_value(v, 100);
        assert!(t.as_str().unwrap().contains("truncated"));
    }
    #[test]
    fn noop_does_not_write() {
        let l = DebugLogger::noop();
        assert!(!l.is_enabled());
        l.log("test", json!({"x": 1}));
    }
    #[test]
    fn log_dir_is_sibling_of_db() {
        let p = Path::new("/tmp/app/stocksy.db");
        assert_eq!(log_dir_for(p), Path::new("/tmp/app/logs"));
    }
}
