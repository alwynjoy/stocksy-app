use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

pub const BACKUP_MAGIC: &[u8; 8] = b"STOCKYBK";
pub const BACKUP_VERSION: u8 = 1;
const MAX_UNCOMPRESSED: usize = 100 * 1024 * 1024; // 100MB guard

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryParamDump {
    pub id: String,
    pub country_code: String,
    pub country_name: String,
    pub currency_code: String,
    pub currency_symbol: Option<String>,
    pub expected_market_return: f64,
    pub risk_free_rate: f64,
    pub corporate_tax_rate: f64,
    pub nominal_gdp_growth: f64,
    pub yahoo_ticker_suffix: Option<String>,
    pub reference_urls: Option<String>,
    pub is_active: Option<i64>,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDump {
    pub id: String,
    pub prompt_key: String,
    pub prompt_name: String,
    pub prompt_text: String,
    pub description: Option<String>,
    pub is_active: i64,
    pub version: i64,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingDump {
    pub key: String,
    pub value: String,
    pub category: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialCacheDump {
    pub id: String,
    pub ticker: String,
    pub country_code: String,
    pub source: String,
    pub raw_response: String,
    pub parsed_financials: Option<String>,
    pub fetch_date: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistDump {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistItemDump {
    pub id: String,
    pub watchlist_id: String,
    pub ticker: String,
    pub country_code: String,
    pub notes: Option<String>,
    pub target_price: Option<f64>,
    pub added_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportDump {
    pub id: String,
    pub ticker: String,
    pub company_name: String,
    pub exchange: Option<String>,
    pub country_code: String,
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub market_cap_category: Option<String>,
    pub price_date: String,
    pub analysis_date: String,
    pub is_primary: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub current_price: Option<f64>,
    pub market_cap: Option<f64>,
    pub currency_code: Option<String>,
    pub base_case_fair_value: Option<f64>,
    pub weighted_fair_value: Option<f64>,
    pub valuation_gap_pct: Option<f64>,
    pub valuation_verdict: Option<String>,
    pub wacc: Option<f64>,
    pub cost_of_equity: Option<f64>,
    pub cost_of_debt_after_tax: Option<f64>,
    pub raw_beta: Option<f64>,
    pub blume_adjusted_beta: Option<f64>,
    pub terminal_growth_rate: Option<f64>,
    pub terminal_margin: Option<f64>,
    pub upside_pct: Option<f64>,
    pub downside_pct: Option<f64>,
    pub risk_reward_ratio: Option<f64>,
    pub risk_reward_label: Option<String>,
    pub scenarios: Option<String>,
    pub sensitivity_matrix: Option<String>,
    pub confidence_score: Option<f64>,
    pub confidence_level: Option<String>,
    pub confidence_details: Option<String>,
    pub ai_business_context: Option<String>,
    pub ai_historical_performance: Option<String>,
    pub ai_growth_projections: Option<String>,
    pub ai_opportunities_risks: Option<String>,
    pub ai_investment_perspective: Option<String>,
    pub dcf_assumptions: Option<String>,
    pub country_params_snapshot: Option<String>,
    pub search_sources: Option<String>,
    pub sensitivity_base_parameters: Option<String>,
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_thinking_tokens: Option<i64>,
    pub llm_output_tokens: Option<i64>,
    pub llm_duration_ms: Option<i64>,
    pub is_favorite: i64,
    pub user_notes: Option<String>,
    pub tags: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub bull_case_fair_value: Option<f64>,
    pub bear_case_fair_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTables {
    pub country_parameters: Vec<CountryParamDump>,
    pub system_prompts: Vec<PromptDump>,
    pub app_settings: Vec<SettingDump>,
    pub financial_data_cache: Vec<FinancialCacheDump>,
    pub watchlists: Vec<WatchlistDump>,
    pub watchlist_items: Vec<WatchlistItemDump>,
    pub analysis_reports: Vec<ReportDump>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFile {
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: String,
    pub tables: BackupTables,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMeta {
    pub version: u8,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: String,
    pub counts: BackupCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupCounts {
    pub country_parameters: usize,
    pub system_prompts: usize,
    pub app_settings: usize,
    pub financial_data_cache: usize,
    pub watchlists: usize,
    pub watchlist_items: usize,
    pub analysis_reports: usize,
}

fn current_schema_version(conn: &Connection) -> i64 {
    conn.query_row("SELECT COALESCE(MAX(version),0) FROM schema_migrations", [], |r| r.get(0))
        .unwrap_or(0)
}

pub fn export_bytes(conn: &Connection) -> Result<Vec<u8>, String> {
    let tables = collect_tables(conn)?;
    let bf = BackupFile {
        app_version: crate::PKG_VERSION.to_string(),
        schema_version: current_schema_version(conn),
        created_at: chrono::Utc::now().to_rfc3339(),
        tables,
    };
    let mut payload = rmp_serde::to_vec_named(&bf).map_err(|e| e.to_string())?;
    if payload.len() > MAX_UNCOMPRESSED {
        return Err(format!("backup too large: {} bytes", payload.len()));
    }
    let mut out = Vec::with_capacity(9 + payload.len());
    out.extend_from_slice(BACKUP_MAGIC);
    out.push(BACKUP_VERSION);
    out.append(&mut payload);
    Ok(out)
}

fn collect_tables(conn: &Connection) -> Result<BackupTables, String> {
    let country_parameters = {
        let mut stmt = conn
            .prepare("SELECT id, country_code, country_name, currency_code, currency_symbol, expected_market_return, risk_free_rate, corporate_tax_rate, nominal_gdp_growth, yahoo_ticker_suffix, reference_urls, is_active, updated_at, created_at FROM country_parameters")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(CountryParamDump {
                    id: r.get(0)?, country_code: r.get(1)?, country_name: r.get(2)?, currency_code: r.get(3)?,
                    currency_symbol: r.get(4)?, expected_market_return: r.get(5)?, risk_free_rate: r.get(6)?,
                    corporate_tax_rate: r.get(7)?, nominal_gdp_growth: r.get(8)?, yahoo_ticker_suffix: r.get(9)?,
                    reference_urls: r.get(10)?, is_active: r.get(11)?, updated_at: r.get(12)?, created_at: r.get(13)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?
    };
    let system_prompts = {
        let mut stmt = conn.prepare("SELECT id, prompt_key, prompt_name, prompt_text, description, is_active, version, updated_at, created_at FROM system_prompts").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok(PromptDump { id: r.get(0)?, prompt_key: r.get(1)?, prompt_name: r.get(2)?, prompt_text: r.get(3)?, description: r.get(4)?, is_active: r.get(5)?, version: r.get(6)?, updated_at: r.get(7)?, created_at: r.get(8)? })).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?
    };
    let app_settings = {
        let mut stmt = conn.prepare("SELECT key, value, category, updated_at FROM app_settings").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok(SettingDump { key: r.get(0)?, value: r.get(1)?, category: r.get(2)?, updated_at: r.get(3)? })).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?
    };
    let financial_data_cache = {
        let mut stmt = conn.prepare("SELECT id, ticker, country_code, source, raw_response, parsed_financials, fetch_date, created_at FROM financial_data_cache").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok(FinancialCacheDump { id: r.get(0)?, ticker: r.get(1)?, country_code: r.get(2)?, source: r.get(3)?, raw_response: r.get(4)?, parsed_financials: r.get(5)?, fetch_date: r.get(6)?, created_at: r.get(7)? })).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?
    };
    let watchlists = {
        let mut stmt = conn.prepare("SELECT id, name, description, created_at, updated_at FROM watchlists").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok(WatchlistDump { id: r.get(0)?, name: r.get(1)?, description: r.get(2)?, created_at: r.get(3)?, updated_at: r.get(4)? })).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?
    };
    let watchlist_items = {
        let mut stmt = conn.prepare("SELECT id, watchlist_id, ticker, country_code, notes, target_price, added_at FROM watchlist_items").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok(WatchlistItemDump { id: r.get(0)?, watchlist_id: r.get(1)?, ticker: r.get(2)?, country_code: r.get(3)?, notes: r.get(4)?, target_price: r.get(5)?, added_at: r.get(6)? })).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?
    };
    let analysis_reports = {
        let mut stmt = conn.prepare("SELECT id, ticker, company_name, exchange, country_code, sector, industry, market_cap_category, price_date, analysis_date, is_primary, status, error_message, current_price, market_cap, currency_code, base_case_fair_value, weighted_fair_value, valuation_gap_pct, valuation_verdict, wacc, cost_of_equity, cost_of_debt_after_tax, raw_beta, blume_adjusted_beta, terminal_growth_rate, terminal_margin, upside_pct, downside_pct, risk_reward_ratio, risk_reward_label, scenarios, sensitivity_matrix, confidence_score, confidence_level, confidence_details, ai_business_context, ai_historical_performance, ai_growth_projections, ai_opportunities_risks, ai_investment_perspective, dcf_assumptions, country_params_snapshot, search_sources, sensitivity_base_parameters, llm_provider, llm_model, llm_thinking_tokens, llm_output_tokens, llm_duration_ms, is_favorite, user_notes, tags, created_at, updated_at, bull_case_fair_value, bear_case_fair_value FROM analysis_reports").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok(ReportDump {
            id: r.get(0)?, ticker: r.get(1)?, company_name: r.get(2)?, exchange: r.get(3)?, country_code: r.get(4)?, sector: r.get(5)?, industry: r.get(6)?, market_cap_category: r.get(7)?, price_date: r.get(8)?, analysis_date: r.get(9)?, is_primary: r.get(10)?, status: r.get(11)?, error_message: r.get(12)?, current_price: r.get(13)?, market_cap: r.get(14)?, currency_code: r.get(15)?, base_case_fair_value: r.get(16)?, weighted_fair_value: r.get(17)?, valuation_gap_pct: r.get(18)?, valuation_verdict: r.get(19)?, wacc: r.get(20)?, cost_of_equity: r.get(21)?, cost_of_debt_after_tax: r.get(22)?, raw_beta: r.get(23)?, blume_adjusted_beta: r.get(24)?, terminal_growth_rate: r.get(25)?, terminal_margin: r.get(26)?, upside_pct: r.get(27)?, downside_pct: r.get(28)?, risk_reward_ratio: r.get(29)?, risk_reward_label: r.get(30)?, scenarios: r.get(31)?, sensitivity_matrix: r.get(32)?, confidence_score: r.get(33)?, confidence_level: r.get(34)?, confidence_details: r.get(35)?, ai_business_context: r.get(36)?, ai_historical_performance: r.get(37)?, ai_growth_projections: r.get(38)?, ai_opportunities_risks: r.get(39)?, ai_investment_perspective: r.get(40)?, dcf_assumptions: r.get(41)?, country_params_snapshot: r.get(42)?, search_sources: r.get(43)?, sensitivity_base_parameters: r.get(44)?, llm_provider: r.get(45)?, llm_model: r.get(46)?, llm_thinking_tokens: r.get(47)?, llm_output_tokens: r.get(48)?, llm_duration_ms: r.get(49)?, is_favorite: r.get(50)?, user_notes: r.get(51)?, tags: r.get(52)?, created_at: r.get(53)?, updated_at: r.get(54)?, bull_case_fair_value: r.get(55)?, bear_case_fair_value: r.get(56)?,
        })).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?
    };
    Ok(BackupTables { country_parameters, system_prompts, app_settings, financial_data_cache, watchlists, watchlist_items, analysis_reports })
}

pub fn parse_and_validate(data: &[u8]) -> Result<(BackupFile, BackupMeta), String> {
    if data.len() < 10 { return Err("file too small — not a Stocksy backup".to_string()); }
    if &data[0..8] != BACKUP_MAGIC { return Err("invalid backup file — bad magic (not .sbk)".to_string()); }
    let ver = data[8];
    if ver != BACKUP_VERSION { return Err(format!("unsupported backup version {ver} — expected {BACKUP_VERSION}")); }
    let payload = &data[9..];
    if payload.len() > MAX_UNCOMPRESSED { return Err("backup payload too large".to_string()); }
    let bf: BackupFile = rmp_serde::from_slice(payload).map_err(|e| format!("invalid backup payload: {e}"))?;
    // sanity: at least one country or prompt if not empty backup? allow empty but warn not error
    let counts = BackupCounts {
        country_parameters: bf.tables.country_parameters.len(),
        system_prompts: bf.tables.system_prompts.len(),
        app_settings: bf.tables.app_settings.len(),
        financial_data_cache: bf.tables.financial_data_cache.len(),
        watchlists: bf.tables.watchlists.len(),
        watchlist_items: bf.tables.watchlist_items.len(),
        analysis_reports: bf.tables.analysis_reports.len(),
    };
    // FK sanity: watchlist_items references
    if counts.watchlist_items > 0 {
        let wids: std::collections::HashSet<&str> = bf.tables.watchlists.iter().map(|w| w.id.as_str()).collect();
        for it in &bf.tables.watchlist_items {
            if !wids.contains(it.watchlist_id.as_str()) {
                return Err(format!("invalid backup: watchlist_item {} references missing watchlist {}", it.id, it.watchlist_id));
            }
        }
    }
    let meta = BackupMeta { version: ver, app_version: bf.app_version.clone(), schema_version: bf.schema_version, created_at: bf.created_at.clone(), counts };
    Ok((bf, meta))
}

pub fn restore_bytes(conn: &Connection, data: &[u8]) -> Result<BackupMeta, String> {
    let (bf, meta) = parse_and_validate(data)?;
    let cur_ver = current_schema_version(conn);
    if bf.schema_version > cur_ver {
        return Err(format!("backup from newer app (schema {} > local {}) — update Stocksy first", bf.schema_version, cur_ver));
    }
    // Transaction with FK off
    conn.execute_batch("PRAGMA foreign_keys=OFF;").map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    // Delete child -> parent
    for sql in [
        "DELETE FROM watchlist_items",
        "DELETE FROM watchlists",
        "DELETE FROM analysis_reports",
        "DELETE FROM financial_data_cache",
        "DELETE FROM system_prompts",
        "DELETE FROM country_parameters",
        "DELETE FROM app_settings",
    ] {
        tx.execute(sql, []).map_err(|e| e.to_string())?;
    }
    // Insert parent -> child
    for c in &bf.tables.country_parameters {
        tx.execute("INSERT INTO country_parameters (id, country_code, country_name, currency_code, currency_symbol, expected_market_return, risk_free_rate, corporate_tax_rate, nominal_gdp_growth, yahoo_ticker_suffix, reference_urls, is_active, updated_at, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![c.id, c.country_code, c.country_name, c.currency_code, c.currency_symbol, c.expected_market_return, c.risk_free_rate, c.corporate_tax_rate, c.nominal_gdp_growth, c.yahoo_ticker_suffix, c.reference_urls, c.is_active.unwrap_or(1), c.updated_at, c.created_at]).map_err(|e| e.to_string())?;
    }
    for p in &bf.tables.system_prompts {
        tx.execute("INSERT INTO system_prompts (id, prompt_key, prompt_name, prompt_text, description, is_active, version, updated_at, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![p.id, p.prompt_key, p.prompt_name, p.prompt_text, p.description, p.is_active, p.version, p.updated_at, p.created_at]).map_err(|e| e.to_string())?;
    }
    for s in &bf.tables.app_settings {
        tx.execute("INSERT INTO app_settings (key, value, category, updated_at) VALUES (?1,?2,?3,?4)",
            params![s.key, s.value, s.category, s.updated_at]).map_err(|e| e.to_string())?;
    }
    for f in &bf.tables.financial_data_cache {
        tx.execute("INSERT INTO financial_data_cache (id, ticker, country_code, source, raw_response, parsed_financials, fetch_date, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![f.id, f.ticker, f.country_code, f.source, f.raw_response, f.parsed_financials, f.fetch_date, f.created_at]).map_err(|e| e.to_string())?;
    }
    for w in &bf.tables.watchlists {
        tx.execute("INSERT INTO watchlists (id, name, description, created_at, updated_at) VALUES (?1,?2,?3,?4,?5)",
            params![w.id, w.name, w.description, w.created_at, w.updated_at]).map_err(|e| e.to_string())?;
    }
    for it in &bf.tables.watchlist_items {
        tx.execute("INSERT INTO watchlist_items (id, watchlist_id, ticker, country_code, notes, target_price, added_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![it.id, it.watchlist_id, it.ticker, it.country_code, it.notes, it.target_price, it.added_at]).map_err(|e| e.to_string())?;
    }
    for r in &bf.tables.analysis_reports {
        tx.execute("INSERT INTO analysis_reports (id, ticker, company_name, exchange, country_code, sector, industry, market_cap_category, price_date, analysis_date, is_primary, status, error_message, current_price, market_cap, currency_code, base_case_fair_value, weighted_fair_value, valuation_gap_pct, valuation_verdict, wacc, cost_of_equity, cost_of_debt_after_tax, raw_beta, blume_adjusted_beta, terminal_growth_rate, terminal_margin, upside_pct, downside_pct, risk_reward_ratio, risk_reward_label, scenarios, sensitivity_matrix, confidence_score, confidence_level, confidence_details, ai_business_context, ai_historical_performance, ai_growth_projections, ai_opportunities_risks, ai_investment_perspective, dcf_assumptions, country_params_snapshot, search_sources, sensitivity_base_parameters, llm_provider, llm_model, llm_thinking_tokens, llm_output_tokens, llm_duration_ms, is_favorite, user_notes, tags, created_at, updated_at, bull_case_fair_value, bear_case_fair_value) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36,?37,?38,?39,?40,?41,?42,?43,?44,?45,?46,?47,?48,?49,?50,?51,?52,?53,?54,?55,?56,?57)",
            params![r.id, r.ticker, r.company_name, r.exchange, r.country_code, r.sector, r.industry, r.market_cap_category, r.price_date, r.analysis_date, r.is_primary, r.status, r.error_message, r.current_price, r.market_cap, r.currency_code, r.base_case_fair_value, r.weighted_fair_value, r.valuation_gap_pct, r.valuation_verdict, r.wacc, r.cost_of_equity, r.cost_of_debt_after_tax, r.raw_beta, r.blume_adjusted_beta, r.terminal_growth_rate, r.terminal_margin, r.upside_pct, r.downside_pct, r.risk_reward_ratio, r.risk_reward_label, r.scenarios, r.sensitivity_matrix, r.confidence_score, r.confidence_level, r.confidence_details, r.ai_business_context, r.ai_historical_performance, r.ai_growth_projections, r.ai_opportunities_risks, r.ai_investment_perspective, r.dcf_assumptions, r.country_params_snapshot, r.search_sources, r.sensitivity_base_parameters, r.llm_provider, r.llm_model, r.llm_thinking_tokens, r.llm_output_tokens, r.llm_duration_ms, r.is_favorite, r.user_notes, r.tags, r.created_at, r.updated_at, r.bull_case_fair_value, r.bear_case_fair_value]).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA foreign_keys=ON;").map_err(|e| e.to_string())?;
    // rebuild FTS
    let _ = conn.execute_batch("INSERT INTO report_search_fts(report_search_fts) VALUES('rebuild');");
    Ok(meta)
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
    fn roundtrip_seven_tables() {
        let conn = test_conn();
        crate::db::migrate(&conn).unwrap();
        crate::db::seed(&conn, false).unwrap();
        // add watchlist + report
        conn.execute("INSERT INTO watchlists (id, name) VALUES ('w1','MyList')", []).unwrap();
        conn.execute("INSERT INTO watchlist_items (id, watchlist_id, ticker, country_code) VALUES ('wi1','w1','AAPL','US')", []).unwrap();
        conn.execute("INSERT INTO analysis_reports (id, ticker, company_name, country_code, price_date, analysis_date, llm_provider, llm_model) VALUES ('r1','AAPL','Apple','US','2026-01-01','2026-01-01','openai','gpt-5')", []).unwrap();
        let bytes = export_bytes(&conn).unwrap();
        assert!(bytes.starts_with(BACKUP_MAGIC));
        assert_eq!(bytes[8], BACKUP_VERSION);
        let (_, meta) = parse_and_validate(&bytes).unwrap();
        assert!(meta.counts.country_parameters >= 4);
        assert!(meta.counts.system_prompts == 6);
        assert!(meta.counts.app_settings >= 20);
        assert_eq!(meta.counts.watchlists, 1);
        assert_eq!(meta.counts.analysis_reports, 1);
        // restore into fresh conn
        let conn2 = test_conn();
        crate::db::migrate(&conn2).unwrap();
        let meta2 = restore_bytes(&conn2, &bytes).unwrap();
        assert_eq!(meta2.counts.watchlists, 1);
        // verify counts
        let c: i64 = conn2.query_row("SELECT COUNT(*) FROM analysis_reports", [], |r| r.get(0)).unwrap();
        assert_eq!(c, 1);
        let c: i64 = conn2.query_row("SELECT COUNT(*) FROM app_settings", [], |r| r.get(0)).unwrap();
        assert!(c >= 20);
    }

    #[test]
    fn rejects_bad_magic() {
        let conn = test_conn();
        crate::db::migrate(&conn).unwrap();
        let mut bytes = export_bytes(&conn).unwrap();
        bytes[0] = b'X';
        assert!(parse_and_validate(&bytes).is_err());
    }

    #[test]
    fn rejects_bad_version() {
        let conn = test_conn();
        crate::db::migrate(&conn).unwrap();
        let mut bytes = export_bytes(&conn).unwrap();
        bytes[8] = 99;
        assert!(parse_and_validate(&bytes).is_err());
    }

    #[test]
    fn rejects_future_schema() {
        let conn = test_conn();
        crate::db::migrate(&conn).unwrap();
        let bytes = export_bytes(&conn).unwrap();
        // decode, bump schema_version, re-encode
        let mut bf: BackupFile = rmp_serde::from_slice(&bytes[9..]).unwrap();
        bf.schema_version = 999;
        let payload = rmp_serde::to_vec_named(&bf).unwrap();
        let mut out = Vec::new();
        out.extend_from_slice(BACKUP_MAGIC);
        out.push(BACKUP_VERSION);
        out.extend_from_slice(&payload);
        // parse ok, restore should reject
        assert!(parse_and_validate(&out).is_ok());
        let conn2 = test_conn();
        crate::db::migrate(&conn2).unwrap();
        assert!(restore_bytes(&conn2, &out).is_err());
    }
}
