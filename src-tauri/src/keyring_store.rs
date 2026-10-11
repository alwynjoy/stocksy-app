//! Secret storage for API keys.
//!
//! All secrets are stored in the `secrets` table in SQLite, obfuscated with
//! a machine-derived XOR key (HOME path + USER). This prevents plaintext
//! API keys sitting in the DB file while remaining compatible with unsigned
//! development builds on all platforms (macOS/Windows/Linux).

use rusqlite::Connection;

/// All secret keys the application manages. Never written to `app_settings`.
pub const SECRET_KEYS: [&str; 7] = [
    "llm.anthropic.api_key",
    "llm.deepseek.api_key",
    "llm.openai.api_key",
    "llm.kimi.api_key",
    "llm.custom.api_key",
    "search.tavily_key",
    "search.scrapingdog_key",
];

/// Derive a simple machine-specific obfuscation key.
/// Uses the HOME path + USERNAME env var — pure std, no extra crates.
fn machine_key() -> Vec<u8> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/stocksy".to_string());
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "stocksy-user".to_string());
    let raw = format!("stocksy-desktop:{}:{}", home, user);
    // Repeat to fill at least 256 bytes for XOR key
    let bytes = raw.as_bytes();
    let mut key = Vec::with_capacity(256);
    while key.len() < 256 {
        key.extend_from_slice(bytes);
    }
    key
}

/// XOR-obfuscate bytes with the machine key, then base64-encode.
fn obfuscate(plaintext: &str) -> String {
    let key = machine_key();
    let bytes = plaintext.as_bytes();
    let obfuscated: Vec<u8> = bytes
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % key.len()])
        .collect();
    base64_encode(&obfuscated)
}

/// base64-decode then XOR-deobfuscate.
fn deobfuscate(encoded: &str) -> Option<String> {
    let bytes = base64_decode(encoded)?;
    let key = machine_key();
    let plain: Vec<u8> = bytes
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % key.len()])
        .collect();
    String::from_utf8(plain).ok()
}

// Minimal base64 encode/decode (no extra crate needed — std only)
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(CHARS[(b0 >> 2) as usize] as char);
        out.push(CHARS[((b0 & 3) << 4 | b1 >> 4) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((b1 & 0xf) << 2 | b2 >> 6) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 { return None; }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let v: Vec<u8> = chunk.iter().map(|&c| val(c)).collect::<Option<Vec<_>>>()?;
        out.push(v[0] << 2 | v[1] >> 4);
        if chunk[2] != b'=' { out.push(v[1] << 4 | v[2] >> 2); }
        if chunk[3] != b'=' { out.push(v[2] << 6 | v[3]); }
    }
    Some(out)
}

// ── Public API (takes a conn reference) ────────────────────────────────────

/// Ensure the `secrets` table exists. Called from db::init.
pub fn ensure_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS secrets (
            key   TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );",
    )
}

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    let result = conn.query_row(
        "SELECT value FROM secrets WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(encoded) => Ok(deobfuscate(&encoded)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("secret read failed for {key}: {e}")),
    }
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return delete(conn, key);
    }
    let encoded = obfuscate(value);
    conn.execute(
        "INSERT INTO secrets (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, encoded],
    )
    .map(|_| ())
    .map_err(|e| format!("secret write failed for {key}: {e}"))
}

pub fn delete(conn: &Connection, key: &str) -> Result<(), String> {
    conn.execute("DELETE FROM secrets WHERE key = ?1", rusqlite::params![key])
        .map(|_| ())
        .map_err(|e| format!("secret delete failed for {key}: {e}"))
}

pub fn status(conn: &Connection, key: &str) -> Result<bool, String> {
    Ok(get(conn, key)?.is_some())
}
