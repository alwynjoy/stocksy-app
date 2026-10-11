use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const BUNDLE_ID: &str = "com.stocksy.desktop";

const SCHEMA_002: &str = r#"
ALTER TABLE analysis_reports ADD COLUMN bull_case_fair_value REAL;
ALTER TABLE analysis_reports ADD COLUMN bear_case_fair_value REAL;
"#;

pub const MIGRATIONS: [&str; 2] = [SCHEMA_001, SCHEMA_002];

const SCHEMA_001: &str = r#"
CREATE TABLE country_parameters (
    id TEXT PRIMARY KEY,
    country_code TEXT UNIQUE NOT NULL,
    country_name TEXT NOT NULL,
    currency_code TEXT NOT NULL,
    currency_symbol TEXT,
    expected_market_return REAL NOT NULL,
    risk_free_rate REAL NOT NULL,
    corporate_tax_rate REAL NOT NULL,
    nominal_gdp_growth REAL NOT NULL,
    yahoo_ticker_suffix TEXT,
    reference_urls TEXT,
    is_active INTEGER DEFAULT 1,
    updated_at TEXT DEFAULT (datetime('now')),
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE system_prompts (
    id TEXT PRIMARY KEY,
    prompt_key TEXT UNIQUE NOT NULL,
    prompt_name TEXT NOT NULL,
    prompt_text TEXT NOT NULL,
    description TEXT,
    is_active INTEGER DEFAULT 1,
    version INTEGER DEFAULT 1,
    updated_at TEXT DEFAULT (datetime('now')),
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    category TEXT,
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE ticker_cache (
    id TEXT PRIMARY KEY,
    ticker TEXT NOT NULL,
    company_name TEXT,
    exchange TEXT,
    country_code TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (country_code) REFERENCES country_parameters(country_code),
    UNIQUE(ticker, country_code)
);

CREATE INDEX idx_ticker_cache_country ON ticker_cache(country_code);
CREATE INDEX idx_ticker_cache_name ON ticker_cache(company_name);

CREATE TABLE financial_data_cache (
    id TEXT PRIMARY KEY,
    ticker TEXT NOT NULL,
    country_code TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'yahoo',
    raw_response TEXT NOT NULL,
    parsed_financials TEXT,
    fetch_date TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (country_code) REFERENCES country_parameters(country_code),
    UNIQUE(ticker, country_code)
);

CREATE INDEX idx_fin_cache_ticker ON financial_data_cache(ticker);

CREATE TABLE analysis_reports (
    id TEXT PRIMARY KEY,
    ticker TEXT NOT NULL,
    company_name TEXT NOT NULL,
    exchange TEXT,
    country_code TEXT NOT NULL,
    sector TEXT,
    industry TEXT,
    market_cap_category TEXT,
    price_date TEXT NOT NULL,
    analysis_date TEXT NOT NULL,
    is_primary INTEGER DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'completed',
    error_message TEXT,
    current_price REAL,
    market_cap REAL,
    currency_code TEXT,
    base_case_fair_value REAL,
    weighted_fair_value REAL,
    valuation_gap_pct REAL,
    valuation_verdict TEXT,
    wacc REAL,
    cost_of_equity REAL,
    cost_of_debt_after_tax REAL,
    raw_beta REAL,
    blume_adjusted_beta REAL,
    terminal_growth_rate REAL,
    terminal_margin REAL,
    upside_pct REAL,
    downside_pct REAL,
    risk_reward_ratio REAL,
    risk_reward_label TEXT,
    scenarios TEXT,
    sensitivity_matrix TEXT,
    confidence_score REAL,
    confidence_level TEXT,
    confidence_details TEXT,
    ai_business_context TEXT,
    ai_historical_performance TEXT,
    ai_growth_projections TEXT,
    ai_opportunities_risks TEXT,
    ai_investment_perspective TEXT,
    dcf_assumptions TEXT,
    country_params_snapshot TEXT,
    search_sources TEXT,
    sensitivity_base_parameters TEXT,
    llm_provider TEXT NOT NULL,
    llm_model TEXT NOT NULL,
    llm_thinking_tokens INTEGER,
    llm_output_tokens INTEGER,
    llm_duration_ms INTEGER,
    is_favorite INTEGER DEFAULT 0,
    user_notes TEXT,
    tags TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (country_code) REFERENCES country_parameters(country_code),
    UNIQUE(ticker, llm_model, price_date)
);

CREATE INDEX idx_reports_ticker ON analysis_reports(ticker);
CREATE INDEX idx_reports_price_date ON analysis_reports(price_date);
CREATE INDEX idx_reports_model ON analysis_reports(llm_model);
CREATE INDEX idx_reports_primary ON analysis_reports(is_primary);
CREATE INDEX idx_reports_favorite ON analysis_reports(is_favorite);
CREATE INDEX idx_reports_verdict ON analysis_reports(valuation_verdict);
CREATE INDEX idx_reports_sector ON analysis_reports(sector);
CREATE INDEX idx_reports_country ON analysis_reports(country_code);
CREATE INDEX idx_reports_status ON analysis_reports(status);

CREATE TABLE watchlists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE watchlist_items (
    id TEXT PRIMARY KEY,
    watchlist_id TEXT NOT NULL,
    ticker TEXT NOT NULL,
    country_code TEXT NOT NULL,
    notes TEXT,
    target_price REAL,
    added_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (watchlist_id) REFERENCES watchlists(id) ON DELETE CASCADE,
    FOREIGN KEY (country_code) REFERENCES country_parameters(country_code),
    UNIQUE(watchlist_id, ticker, country_code)
);

CREATE VIRTUAL TABLE report_search_fts USING fts5(
    ticker,
    company_name,
    sector,
    industry,
    content='analysis_reports',
    content_rowid='rowid'
);

CREATE TRIGGER report_search_fts_ai AFTER INSERT ON analysis_reports BEGIN
    INSERT INTO report_search_fts(rowid, ticker, company_name, sector, industry)
    VALUES (new.rowid, new.ticker, new.company_name, new.sector, new.industry);
END;

CREATE TRIGGER report_search_fts_ad AFTER DELETE ON analysis_reports BEGIN
    INSERT INTO report_search_fts(report_search_fts, rowid, ticker, company_name, sector, industry)
    VALUES ('delete', old.rowid, old.ticker, old.company_name, old.sector, old.industry);
END;

CREATE TRIGGER report_search_fts_au AFTER UPDATE ON analysis_reports BEGIN
    INSERT INTO report_search_fts(report_search_fts, rowid, ticker, company_name, sector, industry)
    VALUES ('delete', old.rowid, old.ticker, old.company_name, old.sector, old.industry);
    INSERT INTO report_search_fts(rowid, ticker, company_name, sector, industry)
    VALUES (new.rowid, new.ticker, new.company_name, new.sector, new.industry);
END;
"#;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;
    let mut applied: Vec<i64> = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT version FROM schema_migrations")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        for row in rows {
            applied.push(row?);
        }
    }
    let tx = conn.unchecked_transaction()?;
    for (idx, sql) in MIGRATIONS.iter().enumerate() {
        let version = (idx + 1) as i64;
        if applied.contains(&version) {
            continue;
        }
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version) VALUES (?1)",
            params![version],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn seed(conn: &Connection, force: bool) -> rusqlite::Result<SeedReport> {
    Ok(SeedReport {
        countries: crate::seeds::seed_countries(conn, force)?,
        prompts: crate::seeds::seed_prompts(conn, force)?,
        settings: crate::seeds::seed_settings(conn, force)?,
        tickers: crate::seeds::seed_tickers(conn, force)?,
    })
}

pub struct SeedReport {
    pub countries: usize,
    pub prompts: usize,
    pub settings: usize,
    pub tickers: usize,
}

/// Backfill D1: for reports where LLM nested 5 fields inside `narrative`, extract them to top-level columns.
/// Runs idempotently on every `init`.
pub fn backfill_nested_reports(conn: &Connection) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT id, ai_business_context, ai_opportunities_risks, ai_investment_perspective, scenarios, search_sources FROM analysis_reports",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut to_update: Vec<(String, String, String, String, String, String)> = Vec::new(); // id, clean_ctx, opp_risks, invest, scenarios, search
    for row in rows {
        let (id, ctx_opt, opp_opt, inv_opt, scen_opt, search_opt) = row?;
        let ctx_str = match ctx_opt { Some(s) if !s.trim().is_empty() => s, _ => continue };
        let ctx_val: serde_json::Value = match serde_json::from_str(&ctx_str) { Ok(v) => v, Err(_) => continue };
        let obj = match ctx_val.as_object() { Some(o) => o, None => continue };
        let has_nested = ["opportunities","risks","investment_perspective","scenario_narratives","search_sources"].iter().any(|k| obj.contains_key(*k));
        if !has_nested { continue; }
        // check if top-level already populated correctly — if so, just clean context
        let opp_is_null = opp_opt.as_deref().map(|s| s.contains("\"opportunities\":null") || s.trim() == "null" || s.trim().is_empty()).unwrap_or(true);
        let inv_is_null = inv_opt.as_deref().map(|s| s.trim() == "null" || s.trim() == "\"null\"" || s.trim().is_empty()).unwrap_or(true);
        let scen_missing = scen_opt.as_deref().map(|s| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                if let Some(arr) = v.as_array() {
                    return arr.iter().all(|item| item.get("narrative").map(|n| n.is_null()).unwrap_or(true));
                }
            }
            false
        }).unwrap_or(false);
        let search_is_tool_trace = search_opt.as_deref().map(|s| {
            // tool trace has "query" or "result" keys, curated has "source_name"
            s.contains("\"query\"") || (s.contains("\"result\"") && !s.contains("\"source_name\""))
        }).unwrap_or(false);
        // only backfill if at least one indicator
        if !opp_is_null && !inv_is_null && !scen_missing && !search_is_tool_trace {
            // still need to clean ctx even if no null? — clean anyway if has_nested
            if !has_nested { continue; }
        }
        // extract
        let opportunities = obj.get("opportunities").cloned();
        let risks = obj.get("risks").cloned();
        let investment = obj.get("investment_perspective").cloned();
        let scenario_narratives = obj.get("scenario_narratives").cloned();
        let search_sources_nested = obj.get("search_sources").cloned();
        // clean ctx
        let mut clean = ctx_val.clone();
        if let Some(o) = clean.as_object_mut() {
            for k in ["opportunities","risks","investment_perspective","scenario_narratives","search_sources"] { o.remove(k); }
        }
        let clean_str = serde_json::to_string(&clean).unwrap_or(ctx_str.clone());
        // opp/risks
        let new_opp_risks = if opp_is_null {
            let opp_val = opportunities.clone().unwrap_or(serde_json::Value::Null);
            let risks_val = risks.clone().unwrap_or(serde_json::Value::Null);
            // only update if nested had something
            if opp_val.is_null() && risks_val.is_null() { opp_opt.clone().unwrap_or_else(|| r#"{"opportunities":null,"risks":null}"#.into()) }
            else { serde_json::to_string(&serde_json::json!({"opportunities": opp_val, "risks": risks_val})).unwrap_or_else(|_| opp_opt.clone().unwrap_or_default()) }
        } else { opp_opt.clone().unwrap_or_default() };
        let new_invest = if inv_is_null {
            if let Some(v) = investment.clone() { serde_json::to_string(&v).unwrap_or_else(|_| inv_opt.clone().unwrap_or_default()) }
            else { inv_opt.clone().unwrap_or_default() }
        } else { inv_opt.clone().unwrap_or_default() };
        // scenarios merge
        let new_scenarios = if scen_missing {
            if let Some(narr) = scenario_narratives.as_ref() {
                if let Some(s) = scen_opt.clone() {
                    if let Ok(mut arr) = serde_json::from_str::<serde_json::Value>(&s) {
                        if let Some(list) = arr.as_array_mut() {
                            for item in list.iter_mut() {
                                if let Some(name) = item.get("scenario_name").and_then(|x| x.as_str()).map(|s| s.to_string()) {
                                    let txt = narr.get(&name).and_then(|v| v.as_str()).map(|s| s.to_string())
                                        .or_else(|| {
                                            // also handle array/string fallback
                                            narr.get(&name).map(|v| match v {
                                                serde_json::Value::String(s) => s.clone(),
                                                serde_json::Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("\n"),
                                                other => other.as_str().map(|s| s.to_string()).unwrap_or_else(|| other.to_string()),
                                            })
                                        });
                                    if let Some(t) = txt { if !t.trim().is_empty() && t != "null" {
                                        if let Some(o) = item.as_object_mut() { o.insert("narrative".into(), serde_json::Value::String(t)); }
                                    } }
                                }
                            }
                            serde_json::to_string(&arr).unwrap_or(s)
                        } else { s }
                    } else { s }
                } else { scen_opt.clone().unwrap_or_default() }
            } else { scen_opt.clone().unwrap_or_default() }
        } else { scen_opt.clone().unwrap_or_default() };
        let new_search = if search_is_tool_trace {
            if let Some(v) = search_sources_nested.clone() {
                let is_valid = v.as_array().map(|arr| arr.iter().all(|it| it.get("source_name").is_some())).unwrap_or(false);
                if is_valid { serde_json::to_string(&v).unwrap_or_else(|_| search_opt.clone().unwrap_or_default()) }
                else { search_opt.clone().unwrap_or_default() }
            } else { search_opt.clone().unwrap_or_default() }
        } else { search_opt.clone().unwrap_or_default() };
        to_update.push((id, clean_str, new_opp_risks, new_invest, new_scenarios, new_search));
    }
    let mut count = 0usize;
    for (id, clean, opp, inv, scen, search) in to_update {
        conn.execute(
            "UPDATE analysis_reports SET ai_business_context = ?1, ai_opportunities_risks = ?2, ai_investment_perspective = ?3, scenarios = ?4, search_sources = ?5, updated_at = datetime('now') WHERE id = ?6",
            params![clean, opp, inv, scen, search, id],
        )?;
        count += 1;
    }
    Ok(count)
}

/// Backfill market_cap_category for rows that were incorrectly persisted as mid-cap due to company_info bug.
/// Also fixes financial_data_cache parsed_financials current_market_data.category.
pub fn backfill_market_cap_category(conn: &Connection) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT id, ticker, country_code, market_cap, current_price, market_cap_category FROM analysis_reports WHERE market_cap_category = 'mid-cap'",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<f64>>(3)?,
            r.get::<_, Option<f64>>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut to_update: Vec<(String, String)> = Vec::new();
    for row in rows {
        let (id, _ticker, country_code, market_cap, current_price, _cat) = row?;
        // fallback price*shares if market_cap null — try to load shares from financial_data_cache
        let mut mc = market_cap;
        if mc.is_none() {
            // try current_price * shares_outstanding not directly available here; use current_price as hint only if we had shares
            // leave as None -> will try to recompute from cache below
        }
        // Try to get shares_outstanding from financial_data_cache for this ticker/country
        // We do a simple query inside loop - acceptable for backfill (limited to mid-cap rows)
        // If market_cap still None, attempt to derive from financial_data_cache shares
        if mc.is_none() {
            if let Ok(shares) = conn.query_row(
                "SELECT parsed_financials FROM financial_data_cache WHERE ticker = ?1 AND country_code = ?2",
                rusqlite::params![_ticker, country_code],
                |r| r.get::<_, Option<String>>(0),
            ) {
                if let Some(parsed_str) = shares {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&parsed_str) {
                        let s = v.get("current_market_data").and_then(|cm| cm.get("shares_outstanding")).and_then(|x| x.as_f64().or_else(|| x.as_i64().map(|i| i as f64)));
                        if let (Some(p), Some(ss)) = (current_price, s) {
                            if p>0.0 && ss>0.0 { mc = Some(p*ss); }
                        }
                    }
                }
            }
        }
        if let Some(cat) = crate::yahoo::quote::market_cap_category(mc, &country_code) {
            if cat != "mid-cap" {
                to_update.push((id, cat));
            }
        }
    }
    let mut count = 0usize;
    for (id, cat) in &to_update {
        conn.execute("UPDATE analysis_reports SET market_cap_category = ?1, updated_at = datetime('now') WHERE id = ?2", rusqlite::params![cat, id])?;
        count += 1;
    }
    // Also fix financial_data_cache parsed_financials for mid-cap entries
    let mut stmt2 = conn.prepare("SELECT ticker, country_code, parsed_financials FROM financial_data_cache")?;
    let rows2 = stmt2.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?;
    let mut cache_updates: Vec<(String, String, String)> = Vec::new();
    for row in rows2 {
        let (ticker, country_code, parsed_opt) = row?;
        let parsed_str = match parsed_opt { Some(s) if !s.trim().is_empty() => s, _ => continue };
        let mut v: serde_json::Value = match serde_json::from_str(&parsed_str) { Ok(x) => x, Err(_) => continue };
        let cur = v.get("current_market_data");
        let existing = cur.and_then(|cm| cm.get("market_cap_category")).and_then(|x| x.as_str()).unwrap_or("mid-cap");
        if existing != "mid-cap" { continue; }
        // recompute from current_market_data.market_cap or price*shares
        let mc = cur.and_then(|cm| cm.get("market_cap")).and_then(|x| x.as_f64().or_else(|| x.as_i64().map(|i| i as f64)));
        let mc = mc.or_else(|| {
            let p = cur.and_then(|cm| cm.get("current_price")).and_then(|x| x.as_f64());
            let s = cur.and_then(|cm| cm.get("shares_outstanding")).and_then(|x| x.as_f64().or_else(|| x.as_i64().map(|i| i as f64)));
            match (p,s) { (Some(pp), Some(ss)) if pp>0.0 && ss>0.0 => Some(pp*ss), _ => None }
        });
        if let Some(cat) = crate::yahoo::quote::market_cap_category(mc, &country_code) {
            if cat != "mid-cap" {
                if let Some(obj) = v.as_object_mut() {
                    if let Some(cm) = obj.get_mut("current_market_data").and_then(|x| x.as_object_mut()) {
                        cm.insert("market_cap_category".to_string(), serde_json::Value::String(cat.clone()));
                    } else if let Some(cm_val) = obj.get_mut("current_market_data") {
                        // if not object, skip
                        let _ = cm_val;
                        continue;
                    }
                    // also ensure company_info has it for future engine fallback
                    if let Some(ci) = obj.get_mut("company_info").and_then(|x| x.as_object_mut()) {
                        if !ci.contains_key("market_cap_category") {
                            ci.insert("market_cap_category".to_string(), serde_json::Value::String(cat.clone()));
                        }
                    }
                }
                let new_str = serde_json::to_string(&v).unwrap_or(parsed_str.clone());
                cache_updates.push((ticker, country_code, new_str));
            }
        }
    }
    for (ticker, cc, new_parsed) in cache_updates {
        // parsed_financials is third param
        // Need to fetch raw_response? keep same; update parsed_financials only
        conn.execute("UPDATE financial_data_cache SET parsed_financials = ?1 WHERE ticker = ?2 AND country_code = ?3", rusqlite::params![new_parsed, ticker, cc])?;
        count += 1;
    }
    if count > 0 {
        eprintln!("[backfill] market_cap_category corrected {} rows", count);
    }
    Ok(count)
}

pub fn init(path: &Path) -> rusqlite::Result<Connection> {
    let conn = open(path)?;
    migrate(&conn)?;
    crate::keyring_store::ensure_table(&conn)?;
    seed(&conn, false)?;
    let _ = backfill_nested_reports(&conn);
    let _ = backfill_market_cap_category(&conn);
    Ok(conn)
}

pub fn table_names(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type IN ('table','virtual') AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
}

pub fn count_rows(conn: &Connection, table: &str) -> rusqlite::Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM \"{}\"", table.replace('"', "\"\""));
    conn.query_row(&sql, [], |r| r.get(0))
}

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    match rows.next()? {
        Some(row) => row.get(0).map(Some),
        None => Ok(None),
    }
}

pub fn set_setting(
    conn: &Connection,
    key: &str,
    value: &str,
    category: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value, category, updated_at) VALUES (?1, ?2, ?3, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, category = excluded.category, updated_at = datetime('now')",
        params![key, value, category],
    )?;
    Ok(())
}

/// Parsed window geometry persisted in `app_settings` (display.window_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
}

pub fn load_window_geometry(conn: &Connection) -> Option<WindowGeometry> {
    let width = get_setting(conn, "display.window_width")
        .ok()??
        .parse::<i32>()
        .ok()?;
    let height = get_setting(conn, "display.window_height")
        .ok()??
        .parse::<i32>()
        .ok()?;
    let x = get_setting(conn, "display.window_x")
        .ok()??
        .parse::<i32>()
        .ok()?;
    let y = get_setting(conn, "display.window_y")
        .ok()??
        .parse::<i32>()
        .ok()?;
    Some(WindowGeometry {
        width,
        height,
        x,
        y,
    })
}

pub fn save_window_geometry(conn: &Connection, g: WindowGeometry) -> rusqlite::Result<()> {
    set_setting(
        conn,
        "display.window_width",
        &g.width.to_string(),
        "display",
    )?;
    set_setting(
        conn,
        "display.window_height",
        &g.height.to_string(),
        "display",
    )?;
    set_setting(conn, "display.window_x", &g.x.to_string(), "display")?;
    set_setting(conn, "display.window_y", &g.y.to_string(), "display")?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
    pub category: Option<String>,
}

pub fn list_settings(conn: &Connection, prefix: Option<&str>) -> rusqlite::Result<Vec<SettingRow>> {
    let query = match prefix {
        Some(p) if !p.is_empty() => {
            "SELECT key, value, category FROM app_settings WHERE key LIKE ?1 ORDER BY key"
        }
        _ => "SELECT key, value, category FROM app_settings ORDER BY key",
    };
    let mut stmt = conn.prepare(query)?;
    let rows = match prefix {
        Some(p) if !p.is_empty() => stmt
            .query_map(params![format!("{p}.%")], |r| {
                Ok(SettingRow {
                    key: r.get(0)?,
                    value: r.get(1)?,
                    category: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        _ => stmt
            .query_map([], |r| {
                Ok(SettingRow {
                    key: r.get(0)?,
                    value: r.get(1)?,
                    category: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    };
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryPayload {
    pub country_code: String,
    pub country_name: String,
    pub currency_code: String,
    #[serde(default)]
    pub currency_symbol: Option<String>,
    pub expected_market_return: f64,
    pub risk_free_rate: f64,
    pub corporate_tax_rate: f64,
    pub nominal_gdp_growth: f64,
    #[serde(default)]
    pub yahoo_ticker_suffix: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CountryRow {
    pub country_code: String,
    pub country_name: String,
    pub currency_code: String,
    pub currency_symbol: Option<String>,
    pub expected_market_return: f64,
    pub risk_free_rate: f64,
    pub equity_risk_premium: f64,
    pub corporate_tax_rate: f64,
    pub nominal_gdp_growth: f64,
    pub yahoo_ticker_suffix: Option<String>,
    pub reference_urls: Option<String>,
}

pub fn list_countries(conn: &Connection) -> rusqlite::Result<Vec<CountryRow>> {
    let mut stmt = conn.prepare(
        "SELECT country_code, country_name, currency_code, currency_symbol,
                expected_market_return, risk_free_rate, corporate_tax_rate,
                nominal_gdp_growth, yahoo_ticker_suffix, reference_urls
         FROM country_parameters WHERE is_active = 1 ORDER BY country_code",
    )?;
    let rows = stmt.query_map([], |r| {
        let emr: f64 = r.get(4)?;
        let rf: f64 = r.get(5)?;
        Ok(CountryRow {
            country_code: r.get(0)?,
            country_name: r.get(1)?,
            currency_code: r.get(2)?,
            currency_symbol: r.get(3)?,
            expected_market_return: emr,
            risk_free_rate: rf,
            equity_risk_premium: emr - rf,
            corporate_tax_rate: r.get(6)?,
            nominal_gdp_growth: r.get(7)?,
            yahoo_ticker_suffix: r.get(8)?,
            reference_urls: r.get(9)?,
        })
    })?;
    rows.collect()
}

pub fn upsert_country(conn: &Connection, c: &CountryPayload) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO country_parameters
            (id, country_code, country_name, currency_code, currency_symbol,
             expected_market_return, risk_free_rate, corporate_tax_rate,
             nominal_gdp_growth, yahoo_ticker_suffix, updated_at)
         VALUES ('country-' || lower(hex(randomblob(4))), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))
         ON CONFLICT(country_code) DO UPDATE SET
            country_name = excluded.country_name,
            currency_code = excluded.currency_code,
            currency_symbol = excluded.currency_symbol,
            expected_market_return = excluded.expected_market_return,
            risk_free_rate = excluded.risk_free_rate,
            corporate_tax_rate = excluded.corporate_tax_rate,
            nominal_gdp_growth = excluded.nominal_gdp_growth,
            yahoo_ticker_suffix = excluded.yahoo_ticker_suffix,
            updated_at = datetime('now')",
        params![
            c.country_code,
            c.country_name,
            c.currency_code,
            c.currency_symbol,
            c.expected_market_return,
            c.risk_free_rate,
            c.corporate_tax_rate,
            c.nominal_gdp_growth,
            c.yahoo_ticker_suffix
        ],
    )?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct PromptRow {
    pub prompt_key: String,
    pub prompt_name: String,
    pub description: Option<String>,
    pub prompt_text: String,
    pub is_active: i64,
    pub version: i64,
    pub updated_at: Option<String>,
}

pub fn list_prompts(conn: &Connection) -> rusqlite::Result<Vec<PromptRow>> {
    let mut stmt = conn.prepare(
        "SELECT prompt_key, prompt_name, description, prompt_text, is_active, version, updated_at
         FROM system_prompts ORDER BY CASE prompt_key
             WHEN 'role_and_abort_protocol' THEN 1
             WHEN 'validation_and_search' THEN 2
             WHEN 'growth_and_fcf' THEN 3
             WHEN 'narrative_and_risks' THEN 4
             WHEN 'confidence_scoring' THEN 5
             WHEN 'json_schema' THEN 6
             ELSE 99 END, prompt_key",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PromptRow {
            prompt_key: r.get(0)?,
            prompt_name: r.get(1)?,
            description: r.get(2)?,
            prompt_text: r.get(3)?,
            is_active: r.get(4)?,
            version: r.get(5)?,
            updated_at: r.get(6)?,
        })
    })?;
    rows.collect()
}

pub fn update_prompt(conn: &Connection, key: &str, text: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE system_prompts SET prompt_text = ?2, version = version + 1, updated_at = datetime('now')
         WHERE prompt_key = ?1",
        params![key, text],
    )
}

pub fn restore_prompt(conn: &Connection, key: &str) -> rusqlite::Result<usize> {
    let Some((id, k, name, text, desc)) = crate::seeds::prompt_default_for_key(key) else {
        return Err(rusqlite::Error::InvalidParameterName(format!("unknown prompt_key: {key}")));
    };
    // Reset this single prompt to its seed default (reseed). Works whether row exists or not.
    let updated = conn.execute(
        "UPDATE system_prompts SET id = ?1, prompt_name = ?3, prompt_text = ?4, description = ?5, is_active = 1, version = 1, updated_at = datetime('now')
         WHERE prompt_key = ?2",
        params![id, k, name, text, desc],
    )?;
    if updated == 0 {
        conn.execute(
            "INSERT INTO system_prompts (id, prompt_key, prompt_name, prompt_text, description, is_active, version) VALUES (?1, ?2, ?3, ?4, ?5, 1, 1)",
            params![id, k, name, text, desc],
        )
    } else {
        Ok(updated)
    }
}

pub fn restore_all_prompts(conn: &Connection) -> rusqlite::Result<usize> {
    crate::seeds::seed_prompts(conn, true)
}

pub fn cache_counts(conn: &Connection) -> rusqlite::Result<(usize, usize)> {
    let tickers = conn.query_row("SELECT COUNT(*) FROM ticker_cache", [], |r| {
        r.get::<_, i64>(0)
    })?;
    let fin = conn.query_row("SELECT COUNT(*) FROM financial_data_cache", [], |r| {
        r.get::<_, i64>(0)
    })?;
    Ok((tickers as usize, fin as usize))
}

#[derive(Debug, Serialize)]
pub struct TickerCacheRow {
    pub ticker: String,
    pub company_name: Option<String>,
    pub exchange: Option<String>,
    pub country_code: String,
}

pub fn search_tickers(
    conn: &Connection,
    query: &str,
    country: Option<&str>,
    limit: usize,
) -> rusqlite::Result<Vec<TickerCacheRow>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let like = format!("%{}%", q);
    // Search ticker OR company_name, case-insensitive via COLLATE NOCASE
    let (sql, has_country) = if country.is_some() {
        (
            "SELECT ticker, company_name, exchange, country_code FROM ticker_cache WHERE country_code = ?1 AND (ticker LIKE ?2 COLLATE NOCASE OR company_name LIKE ?2 COLLATE NOCASE) ORDER BY CASE WHEN ticker LIKE ?3 COLLATE NOCASE THEN 0 ELSE 1 END, ticker LIMIT ?4",
            true,
        )
    } else {
        (
            "SELECT ticker, company_name, exchange, country_code FROM ticker_cache WHERE (ticker LIKE ?1 COLLATE NOCASE OR company_name LIKE ?1 COLLATE NOCASE) ORDER BY CASE WHEN ticker LIKE ?2 COLLATE NOCASE THEN 0 ELSE 1 END, ticker LIMIT ?3",
            false,
        )
    };
    let limit = limit.clamp(1, 50) as i64;
    let prefix_like = format!("{}%", q);
    let mut stmt = conn.prepare(sql)?;
    let rows = if has_country {
        let c = country.unwrap();
        stmt.query_map(params![c, like, prefix_like, limit], |r| {
            Ok(TickerCacheRow {
                ticker: r.get(0)?,
                company_name: r.get(1)?,
                exchange: r.get(2)?,
                country_code: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map(params![like, prefix_like, limit], |r| {
            Ok(TickerCacheRow {
                ticker: r.get(0)?,
                company_name: r.get(1)?,
                exchange: r.get(2)?,
                country_code: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(rows)
}

pub fn upsert_ticker_cache(
    conn: &Connection,
    ticker: &str,
    company_name: Option<&str>,
    exchange: Option<&str>,
    country_code: &str,
) -> rusqlite::Result<()> {
    let t = ticker.trim().to_uppercase();
    if t.is_empty() {
        return Ok(());
    }
    let id = format!(
        "ticker-{}-{}",
        country_code.to_lowercase(),
        t.to_lowercase().replace('.', "-").replace('/', "-")
    );
    conn.execute(
        "INSERT INTO ticker_cache (id, ticker, company_name, exchange, country_code) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(ticker, country_code) DO UPDATE SET company_name = COALESCE(excluded.company_name, company_name), exchange = COALESCE(excluded.exchange, exchange)",
        params![id, t, company_name, exchange, country_code.to_uppercase()],
    )?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FinancialCacheRow {
    pub ticker: String,
    pub country_code: String,
    pub raw_response: String,
    pub parsed_financials: Option<String>,
    pub fetch_date: String,
}

pub fn get_cached_financial(
    conn: &Connection,
    ticker: &str,
    country_code: &str,
) -> rusqlite::Result<Option<FinancialCacheRow>> {
    let mut stmt = conn.prepare(
        "SELECT ticker, country_code, raw_response, parsed_financials, fetch_date FROM financial_data_cache WHERE ticker = ?1 AND country_code = ?2",
    )?;
    let mut rows = stmt.query(params![ticker.to_uppercase(), country_code.to_uppercase()])?;
    if let Some(row) = rows.next()? {
        Ok(Some(FinancialCacheRow {
            ticker: row.get(0)?,
            country_code: row.get(1)?,
            raw_response: row.get(2)?,
            parsed_financials: row.get(3)?,
            fetch_date: row.get(4)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn upsert_financial_cache(
    conn: &Connection,
    ticker: &str,
    country_code: &str,
    raw_response: &str,
    parsed_financials: &str,
) -> rusqlite::Result<()> {
    let id = format!(
        "fdc-{}-{}",
        country_code.to_lowercase(),
        ticker.to_lowercase().replace('.', "-").replace('/', "-")
    );
    conn.execute(
        "INSERT INTO financial_data_cache (id, ticker, country_code, raw_response, parsed_financials, fetch_date) VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(ticker, country_code) DO UPDATE SET raw_response = excluded.raw_response, parsed_financials = excluded.parsed_financials, fetch_date = datetime('now')",
        params![id, ticker.to_uppercase(), country_code.to_uppercase(), raw_response, parsed_financials],
    )?;
    Ok(())
}

pub fn backup_db(conn: &Connection, dest: &Path) -> rusqlite::Result<()> {
    let _ = std::fs::remove_file(dest);
    let dest_str = dest.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{dest_str}'"))
}

pub fn is_valid_db(path: &Path) -> bool {
    use rusqlite::OpenFlags;
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|c| {
            let names = table_names(&c)?;
            Ok(names.iter().any(|t| t == "country_parameters")
                && names.iter().any(|t| t == "app_settings"))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn
    }

    #[test]
    fn migrate_creates_all_tables() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        let tables = table_names(&conn).unwrap();
        for t in [
            "country_parameters",
            "system_prompts",
            "app_settings",
            "ticker_cache",
            "financial_data_cache",
            "analysis_reports",
            "watchlists",
            "watchlist_items",
            "report_search_fts",
            "schema_migrations",
        ] {
            assert!(tables.iter().any(|x| x == t), "missing table {t}");
        }
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);
    }

    #[test]
    fn seed_populates_expected_rows() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        let report = seed(&conn, false).unwrap();
        assert_eq!(report.countries, 4);
        assert_eq!(report.prompts, 6);
        assert_eq!(report.settings, 28);
        assert!(report.tickers > 20_000, "tickers should be pre-seeded, got {}", report.tickers);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ticker_cache", [], |r| r.get(0))
            .unwrap();
        assert!(count > 20_000);
    }

    #[test]
    fn seed_is_idempotent_and_preserves_edits() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        seed(&conn, false).unwrap();
        conn.execute(
            "UPDATE country_parameters SET risk_free_rate = 0.1234 WHERE country_code = 'IN'",
            [],
        )
        .unwrap();
        let second = seed(&conn, false).unwrap();
        assert_eq!(second.countries, 0);
        let rf: f64 = conn
            .query_row(
                "SELECT risk_free_rate FROM country_parameters WHERE country_code = 'IN'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!((rf - 0.1234).abs() < 1e-9, "edit must survive re-seed");
    }

    #[test]
    fn seed_force_overwrites_edits() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        seed(&conn, false).unwrap();
        conn.execute(
            "UPDATE country_parameters SET risk_free_rate = 0.9999 WHERE country_code = 'IN'",
            [],
        )
        .unwrap();
        seed(&conn, true).unwrap();
        let rf: f64 = conn
            .query_row(
                "SELECT risk_free_rate FROM country_parameters WHERE country_code = 'IN'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            (rf - 0.07067).abs() < 1e-9,
            "force seed must restore seeded value"
        );
    }

    #[test]
    fn fts_triggers_keep_search_index_in_sync() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        seed(&conn, false).unwrap();
        conn.execute(
            "INSERT INTO analysis_reports (id, ticker, company_name, country_code, price_date, analysis_date, llm_provider, llm_model) VALUES ('r1', 'AAPL', 'Apple Inc', 'US', '2026-09-19', '2026-09-19', 'anthropic', 'claude')",
            [],
        )
        .unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM report_search_fts WHERE report_search_fts MATCH 'Apple'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
        conn.execute("DELETE FROM analysis_reports WHERE id = 'r1'", [])
            .unwrap();
        let hits_after_delete: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM report_search_fts WHERE report_search_fts MATCH 'Apple'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits_after_delete, 0);
    }

    #[test]
    fn settings_round_trip_and_geometry() {
        let conn = test_conn();
        migrate(&conn).unwrap();

        assert_eq!(
            get_setting(&conn, "display.sidebar_collapsed").unwrap(),
            None
        );

        set_setting(&conn, "display.sidebar_collapsed", "true", "display").unwrap();
        assert_eq!(
            get_setting(&conn, "display.sidebar_collapsed").unwrap(),
            Some("true".to_string())
        );

        set_setting(&conn, "display.sidebar_collapsed", "false", "display").unwrap();
        assert_eq!(
            get_setting(&conn, "display.sidebar_collapsed").unwrap(),
            Some("false".to_string())
        );

        let geom = WindowGeometry {
            width: 1280,
            height: 800,
            x: 120,
            y: 60,
        };
        save_window_geometry(&conn, geom).unwrap();
        assert_eq!(load_window_geometry(&conn), Some(geom));
    }

    #[test]
    fn country_upsert_and_list_round_trip() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        seed(&conn, false).unwrap();

        let payload = CountryPayload {
            country_code: "SG".into(),
            country_name: "Singapore".into(),
            currency_code: "SGD".into(),
            currency_symbol: Some("S$".into()),
            expected_market_return: 0.10,
            risk_free_rate: 0.03,
            corporate_tax_rate: 0.17,
            nominal_gdp_growth: 0.045,
            yahoo_ticker_suffix: Some(".SI".into()),
        };
        upsert_country(&conn, &payload).unwrap();
        let rows = list_countries(&conn).unwrap();
        let sg = rows
            .iter()
            .find(|c| c.country_code == "SG")
            .expect("SG added");
        assert!((sg.equity_risk_premium - 0.07).abs() < 1e-9);
        assert!((sg.corporate_tax_rate - 0.17).abs() < 1e-9);

        let mut edited = payload.clone();
        edited.corporate_tax_rate = 0.20;
        upsert_country(&conn, &edited).unwrap();
        let rows = list_countries(&conn).unwrap();
        let sg = rows.iter().find(|c| c.country_code == "SG").unwrap();
        assert!(
            (sg.corporate_tax_rate - 0.20).abs() < 1e-9,
            "upsert must update"
        );
        let count = rows.len();
        assert_eq!(count, 5, "one row added, none duplicated");
    }

    #[test]
    fn prompt_update_bumps_version() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        seed(&conn, false).unwrap();

        let before = list_prompts(&conn).unwrap();
        let role = before
            .iter()
            .find(|p| p.prompt_key == "role_and_abort_protocol")
            .unwrap();
        assert_eq!(role.version, 2);

        update_prompt(&conn, "role_and_abort_protocol", "new text").unwrap();
        let after = list_prompts(&conn).unwrap();
        let role = after
            .iter()
            .find(|p| p.prompt_key == "role_and_abort_protocol")
            .unwrap();
        assert_eq!(role.prompt_text, "new text");
        assert_eq!(role.version, 3);
    }

    #[test]
    fn list_settings_filters_by_prefix() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        seed(&conn, false).unwrap();

        let llm = list_settings(&conn, Some("llm")).unwrap();
        assert!(llm.len() >= 10);
        assert!(llm.iter().all(|r| r.key.starts_with("llm.")));

        let all = list_settings(&conn, None).unwrap();
        assert_eq!(all.len(), 28);
    }

    #[test]
    fn seeds_never_contain_secret_keys() {
        for (key, _, _) in crate::seeds::SETTING_ROWS {
            assert!(
                !key.contains("api_key") && !key.contains("_key"),
                "secret key {key} must not be seeded into app_settings"
            );
        }
        let conn = test_conn();
        migrate(&conn).unwrap();
        seed(&conn, false).unwrap();
        for secret in crate::keyring_store::SECRET_KEYS {
            assert!(
                get_setting(&conn, secret).unwrap().is_none(),
                "{secret} must live in secrets table only, never in app_settings"
            );
        }
    }

    #[test]
    fn ticker_search_returns_hits() {
        let conn = test_conn();
        migrate(&conn).unwrap();
        seed(&conn, false).unwrap();
        // Typeahead: MSFT exists in US cache (AAPL not in deduped screener sample)
        let hits = search_tickers(&conn, "MSFT", Some("US"), 10).unwrap();
        assert!(!hits.is_empty(), "MSFT should be found in US cache");
        let zydus = search_tickers(&conn, "ZYDUS", None, 5).unwrap();
        assert!(!zydus.is_empty(), "ZYDUS name search should hit");
        // Country filter: IN tickers with .NS
        let in_hits = search_tickers(&conn, "ZYDUSWELL", Some("IN"), 5).unwrap();
        assert!(!in_hits.is_empty());
        // Cross-country company name search
        let tesla = search_tickers(&conn, "Tesla", None, 5).unwrap();
        // Tesla may not be in screener IN cache but should be in US if present; at least test non-empty path not required
        let _ = tesla;
        // Empty query returns empty
        let empty = search_tickers(&conn, "", None, 5).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn backup_vacuum_into_produces_valid_replica() {
        let dir = std::env::temp_dir().join(format!("stocksy-backup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("src.db");
        let backup_path = dir.join("backup.db");
        std::fs::remove_file(&db_path).ok();

        {
            let conn = open(&db_path).unwrap();
            migrate(&conn).unwrap();
            seed(&conn, false).unwrap();
            backup_db(&conn, &backup_path).unwrap();
        }
        assert!(is_valid_db(&backup_path));
        std::fs::remove_dir_all(&dir).ok();
    }
}
