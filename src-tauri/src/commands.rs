#![allow(clippy::too_many_arguments)]
use crate::{DbState, PKG_NAME, PKG_VERSION};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tauri::State;

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub db_path: String,
    pub schema_ok: bool,
    pub tables: usize,
}

#[derive(Serialize)]
pub struct DbHealth {
    pub migrated: bool,
    pub tables: usize,
    pub countries: usize,
    pub prompts: usize,
}

pub struct DbCounts {
    pub tables: usize,
    pub countries: usize,
    pub prompts: usize,
}

pub fn db_counts(conn: &Connection) -> DbCounts {
    let tables = crate::db::table_names(conn).map(|t| t.len()).unwrap_or(0);
    let countries = conn
        .query_row("SELECT COUNT(*) FROM country_parameters", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0) as usize;
    let prompts = conn
        .query_row("SELECT COUNT(*) FROM system_prompts", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0) as usize;
    DbCounts {
        tables,
        countries,
        prompts,
    }
}

#[tauri::command]
pub fn app_info(state: State<'_, DbState>) -> AppInfo {
    let guard = state.conn.lock().unwrap();
    let counts = db_counts(&guard);
    AppInfo {
        name: PKG_NAME,
        version: PKG_VERSION,
        db_path: state.path.display().to_string(),
        schema_ok: counts.tables > 0,
        tables: counts.tables,
    }
}

#[tauri::command]
pub fn db_health(state: State<'_, DbState>) -> DbHealth {
    let guard = state.conn.lock().unwrap();
    let counts = db_counts(&guard);
    DbHealth {
        migrated: counts.tables > 0,
        tables: counts.tables,
        countries: counts.countries,
        prompts: counts.prompts,
    }
}

#[tauri::command]
pub fn get_setting(state: State<'_, DbState>, key: String) -> Result<Option<String>, String> {
    let guard = state.conn.lock().unwrap();
    crate::db::get_setting(&guard, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(state: State<'_, DbState>, key: String, value: String) -> Result<(), String> {
    let category = key.split('.').next().unwrap_or("app").to_string();
    let guard = state.conn.lock().unwrap();
    crate::db::set_setting(&guard, &key, &value, &category).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct SecretStatus {
    pub key: String,
    pub is_saved: bool,
}

#[derive(Serialize)]
pub struct DataInfo {
    pub db_path: String,
    pub db_size_bytes: i64,
    pub settings: usize,
    pub countries: usize,
    pub prompts: usize,
    pub ticker_cache: usize,
    pub financial_cache: usize,
}

#[tauri::command]
pub fn list_settings(
    state: State<'_, DbState>,
    prefix: Option<String>,
) -> Vec<crate::db::SettingRow> {
    let guard = state.conn.lock().unwrap();
    crate::db::list_settings(&guard, prefix.as_deref()).unwrap_or_default()
}

#[tauri::command]
pub fn get_secret(state: State<'_, DbState>, key: String) -> Result<Option<String>, String> {
    let guard = state.conn.lock().unwrap();
    crate::keyring_store::get(&guard, &key)
}

#[tauri::command]
pub fn set_secret(state: State<'_, DbState>, key: String, value: String) -> Result<(), String> {
    let guard = state.conn.lock().unwrap();
    crate::keyring_store::set(&guard, &key, &value)
}

#[tauri::command]
pub fn delete_secret(state: State<'_, DbState>, key: String) -> Result<(), String> {
    let guard = state.conn.lock().unwrap();
    crate::keyring_store::delete(&guard, &key)
}

#[tauri::command]
pub fn secret_status(state: State<'_, DbState>) -> Vec<SecretStatus> {
    let guard = state.conn.lock().unwrap();
    crate::keyring_store::SECRET_KEYS
        .iter()
        .map(|key| SecretStatus {
            key: (*key).to_string(),
            is_saved: crate::keyring_store::status(&guard, key).unwrap_or(false),
        })
        .collect()
}

#[tauri::command]
pub fn countries_list(state: State<'_, DbState>) -> Vec<crate::db::CountryRow> {
    let guard = state.conn.lock().unwrap();
    crate::db::list_countries(&guard).unwrap_or_default()
}

#[tauri::command]
pub fn country_upsert(
    state: State<'_, DbState>,
    country: crate::db::CountryPayload,
) -> Result<(), String> {
    let guard = state.conn.lock().unwrap();
    crate::db::upsert_country(&guard, &country).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn prompts_list(state: State<'_, DbState>) -> Vec<crate::db::PromptRow> {
    let guard = state.conn.lock().unwrap();
    crate::db::list_prompts(&guard).unwrap_or_default()
}

#[tauri::command]
pub fn prompt_update(state: State<'_, DbState>, key: String, text: String) -> Result<(), String> {
    let guard = state.conn.lock().unwrap();
    crate::db::update_prompt(&guard, &key, &text).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn prompt_restore_default(state: State<'_, DbState>, key: String) -> Result<(), String> {
    let guard = state.conn.lock().unwrap();
    crate::db::restore_prompt(&guard, &key).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn prompt_restore_all(state: State<'_, DbState>) -> Result<usize, String> {
    let guard = state.conn.lock().unwrap();
    crate::db::restore_all_prompts(&guard).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dcf_defaults() -> Vec<crate::dcf::DcfDefault> {
    crate::dcf::defaults()
}

fn provider_secret_key(provider: &str) -> Option<String> {
    match provider {
        "anthropic" | "deepseek" | "openai" | "kimi" => Some(format!("llm.{provider}.api_key")),
        "custom" => Some("llm.custom.api_key".to_string()),
        _ => None,
    }
}

#[tauri::command]
pub async fn llm_fetch_models(
    state: State<'_, DbState>,
    provider: String,
    base_url: String,
    api_key: Option<String>,
) -> Result<Vec<String>, String> {
    let secret_key =
        provider_secret_key(&provider).ok_or_else(|| format!("unknown provider: {provider}"))?;
    let resolved = match api_key.as_deref() {
        Some(k) if !k.trim().is_empty() => Some(k.to_string()),
        _ => {
            let guard = state.conn.lock().unwrap();
            crate::keyring_store::get(&guard, &secret_key)?
        }
    };
    crate::llm::fetch_models(&provider, &base_url, resolved).await
}

#[tauri::command]
pub async fn llm_test_connection(
    state: State<'_, DbState>,
    provider: String,
    base_url: String,
    model: String,
    api_key: Option<String>,
) -> Result<crate::llm::TestResult, String> {
    let secret_key =
        provider_secret_key(&provider).ok_or_else(|| format!("unknown provider: {provider}"))?;
    let resolved = match api_key.as_deref() {
        Some(k) if !k.trim().is_empty() => Some(k.to_string()),
        _ => {
            let guard = state.conn.lock().unwrap();
            crate::keyring_store::get(&guard, &secret_key)?
        }
    };
    crate::llm::test_connection(&provider, &base_url, &model, resolved).await
}

#[tauri::command]
pub fn data_info(state: State<'_, DbState>) -> DataInfo {
    let guard = state.conn.lock().unwrap();
    let settings = crate::db::list_settings(&guard, None)
        .map(|v| v.len())
        .unwrap_or(0);
    let countries = crate::db::list_countries(&guard)
        .map(|v| v.len())
        .unwrap_or(0);
    let prompts = crate::db::list_prompts(&guard)
        .map(|v| v.len())
        .unwrap_or(0);
    let (ticker_cache, financial_cache) = crate::db::cache_counts(&guard).unwrap_or((0, 0));
    DataInfo {
        db_path: state.path.display().to_string(),
        db_size_bytes: std::fs::metadata(&state.path)
            .map(|m| m.len() as i64)
            .unwrap_or(0),
        settings,
        countries,
        prompts,
        ticker_cache,
        financial_cache,
    }
}

#[tauri::command]
pub fn stub_dashboard_stats() -> Result<serde_json::Value, String> {
    Ok(crate::stubs::dashboard_stats())
}

#[tauri::command]
pub fn dashboard_stats(state: State<'_, DbState>) -> Result<serde_json::Value, String> {
    let guard = state.conn.lock().unwrap();
    let total_reports: i64 = guard
        .query_row(
            "SELECT COUNT(*) FROM analysis_reports WHERE status = 'completed'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    // reports per model
    let mut per_model: Vec<serde_json::Value> = Vec::new();
    {
        let mut stmt = guard
            .prepare(
                "SELECT llm_model, COUNT(*) FROM analysis_reports WHERE status='completed' GROUP BY llm_model ORDER BY COUNT(*) DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (model, count) = row.map_err(|e| e.to_string())?;
            per_model.push(json!({"model": model, "count": count}));
        }
    }
    // provider token stats + estimated cost (provider average low)
    let mut provider_stats: Vec<serde_json::Value> = Vec::new();
    let mut total_input: i64 = 0;
    let mut total_output: i64 = 0;
    let mut total_tokens: i64 = 0;
    let mut total_estimated_usd: f64 = 0.0;
    {
        let mut stmt = guard
            .prepare(
                "SELECT llm_provider, COUNT(*), SUM(COALESCE(llm_thinking_tokens,0)), SUM(COALESCE(llm_output_tokens,0)), SUM(COALESCE(llm_thinking_tokens,0)+COALESCE(llm_output_tokens,0)), AVG(COALESCE(llm_duration_ms,0)) FROM analysis_reports WHERE status='completed' GROUP BY llm_provider ORDER BY COUNT(*) DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<f64>>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (provider, count, inp, out, total, avg_ms) = row.map_err(|e| e.to_string())?;
            let inp_v = inp.unwrap_or(0);
            let out_v = out.unwrap_or(0);
            let tot_v = total.unwrap_or(0);
            let avg = avg_ms.unwrap_or(0.0) as i64;
            let est = crate::pricing::estimate_usd(&provider, Some(inp_v), Some(out_v));
            total_input += inp_v;
            total_output += out_v;
            total_tokens += tot_v;
            total_estimated_usd += est;
            provider_stats.push(json!({
                "provider": provider,
                "count": count,
                "input_tokens": inp_v,
                "output_tokens": out_v,
                "total_tokens": tot_v,
                "avg_duration_ms": avg,
                "estimated_usd": est
            }));
        }
    }
    if total_estimated_usd == 0.0 {
        // fallback compute from sums if no provider rows (should not happen)
        total_estimated_usd = provider_stats.iter().map(|v| v.get("estimated_usd").and_then(|x| x.as_f64()).unwrap_or(0.0)).sum();
    }
    // reports per country
    let mut per_country: Vec<serde_json::Value> = Vec::new();
    {
        let mut stmt = guard
            .prepare(
                "SELECT country_code, COUNT(*) FROM analysis_reports WHERE status='completed' GROUP BY country_code ORDER BY COUNT(*) DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (country, count) = row.map_err(|e| e.to_string())?;
            per_country.push(json!({"country": country, "count": count}));
        }
    }
    // recent analyses (up to 10, completed only)
    let mut recent: Vec<serde_json::Value> = Vec::new();
    {
        let sql = format!(
            "{} WHERE status='completed' ORDER BY analysis_date DESC, price_date DESC LIMIT 10",
            REPORT_SELECT
        );
        let mut stmt = guard.prepare(&sql).map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |row| {
                Ok(report_row_to_json(
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, Option<f64>>(13)?,
                    row.get::<_, Option<f64>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, Option<f64>>(16)?,
                    row.get::<_, Option<f64>>(17)?,
                    row.get::<_, Option<f64>>(18)?,
                    row.get::<_, Option<String>>(19)?,
                    row.get::<_, Option<f64>>(20)?,
                    row.get::<_, Option<f64>>(21)?,
                    row.get::<_, Option<f64>>(22)?,
                    row.get::<_, Option<f64>>(23)?,
                    row.get::<_, Option<f64>>(24)?,
                    row.get::<_, Option<f64>>(25)?,
                    row.get::<_, Option<f64>>(26)?,
                    row.get::<_, Option<f64>>(27)?,
                    row.get::<_, Option<f64>>(28)?,
                    row.get::<_, Option<f64>>(29)?,
                    row.get::<_, Option<String>>(30)?,
                    row.get::<_, Option<String>>(31)?,
                    row.get::<_, Option<String>>(32)?,
                    row.get::<_, Option<f64>>(33)?,
                    row.get::<_, Option<String>>(34)?,
                    row.get::<_, Option<String>>(35)?,
                    row.get::<_, Option<String>>(36)?,
                    row.get::<_, Option<String>>(37)?,
                    row.get::<_, Option<String>>(38)?,
                    row.get::<_, Option<String>>(39)?,
                    row.get::<_, Option<String>>(40)?,
                    row.get::<_, Option<String>>(41)?,
                    row.get::<_, Option<String>>(42)?,
                    row.get::<_, Option<String>>(43)?,
                    row.get::<_, Option<String>>(44)?,
                    row.get::<_, String>(45)?,
                    row.get::<_, String>(46)?,
                    row.get::<_, Option<i64>>(47)?,
                    row.get::<_, Option<i64>>(48)?,
                    row.get::<_, Option<i64>>(49)?,
                    row.get::<_, i64>(50)?,
                    row.get::<_, Option<String>>(51)?,
                    row.get::<_, Option<String>>(52)?,
                    row.get::<_, Option<String>>(53)?,
                    row.get::<_, Option<String>>(54)?,
                    row.get::<_, Option<f64>>(55)?,
                    row.get::<_, Option<f64>>(56)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for r in mapped {
            recent.push(r.map_err(|e| e.to_string())?);
        }
    }
    // watchlist summary
    let mut watchlist_summary: Vec<serde_json::Value> = Vec::new();
    {
        // tolerate absence of watchlists
        if let Ok(mut stmt) = guard.prepare("SELECT id, name FROM watchlists ORDER BY name") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
                for row in rows.flatten() {
                    let (wid, wname) = row;
                    let tickers: i64 = guard
                        .query_row(
                            "SELECT COUNT(*) FROM watchlist_items WHERE watchlist_id = ?1",
                            rusqlite::params![wid],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    // gainers = count where linked report is undervalued, else 0
                    let gainers: i64 = guard
                        .query_row(
                            "SELECT COUNT(*) FROM watchlist_items wi JOIN analysis_reports ar ON ar.ticker = wi.ticker AND ar.country_code = wi.country_code AND ar.is_primary = 1 AND ar.status='completed' WHERE wi.watchlist_id = ?1 AND ar.valuation_verdict = 'undervalued'",
                            rusqlite::params![wid],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let avg_gap: Option<f64> = guard
                        .query_row(
                            "SELECT AVG(ar.valuation_gap_pct) FROM watchlist_items wi JOIN analysis_reports ar ON ar.ticker = wi.ticker AND ar.country_code = wi.country_code AND ar.is_primary=1 AND ar.status='completed' WHERE wi.watchlist_id = ?1",
                            rusqlite::params![wid],
                            |r| r.get(0),
                        )
                        .unwrap_or(None);
                    let gap_pct = avg_gap.unwrap_or(0.0) * 100.0;
                    watchlist_summary.push(json!({
                        "name": wname,
                        "tickers": tickers,
                        "gainers": gainers,
                        "latest_gap_pct": gap_pct
                    }));
                }
            }
        }
    }
    Ok(json!({
        "total_reports": total_reports,
        "reports_per_model": per_model,
        "reports_per_country": per_country,
        "recent_analyses": recent,
        "watchlist_summary": watchlist_summary,
        "provider_stats": provider_stats,
        "total_input_tokens": total_input,
        "total_output_tokens": total_output,
        "total_tokens": total_tokens,
        "total_estimated_usd": total_estimated_usd
    }))
}

#[tauri::command]
pub fn screener_rows(state: State<'_, DbState>) -> Result<serde_json::Value, String> {
    let guard = state.conn.lock().unwrap();
    let sql = format!("{} WHERE is_primary = 1 AND status = 'completed' ORDER BY analysis_date DESC LIMIT 1000", REPORT_SELECT);
    let mut stmt = guard.prepare(&sql).map_err(|e| e.to_string())?;
    let mapped = stmt
        .query_map([], |row| {
            // pull a subset needed for ScreenerRow, but reuse report_row_to_json tuple then extract
            let v = report_row_to_json(
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<f64>>(13)?,
                row.get::<_, Option<f64>>(14)?,
                row.get::<_, Option<String>>(15)?,
                row.get::<_, Option<f64>>(16)?,
                row.get::<_, Option<f64>>(17)?,
                row.get::<_, Option<f64>>(18)?,
                row.get::<_, Option<String>>(19)?,
                row.get::<_, Option<f64>>(20)?,
                row.get::<_, Option<f64>>(21)?,
                row.get::<_, Option<f64>>(22)?,
                row.get::<_, Option<f64>>(23)?,
                row.get::<_, Option<f64>>(24)?,
                row.get::<_, Option<f64>>(25)?,
                row.get::<_, Option<f64>>(26)?,
                row.get::<_, Option<f64>>(27)?,
                row.get::<_, Option<f64>>(28)?,
                row.get::<_, Option<f64>>(29)?,
                row.get::<_, Option<String>>(30)?,
                row.get::<_, Option<String>>(31)?,
                row.get::<_, Option<String>>(32)?,
                row.get::<_, Option<f64>>(33)?,
                row.get::<_, Option<String>>(34)?,
                row.get::<_, Option<String>>(35)?,
                row.get::<_, Option<String>>(36)?,
                row.get::<_, Option<String>>(37)?,
                row.get::<_, Option<String>>(38)?,
                row.get::<_, Option<String>>(39)?,
                row.get::<_, Option<String>>(40)?,
                row.get::<_, Option<String>>(41)?,
                row.get::<_, Option<String>>(42)?,
                row.get::<_, Option<String>>(43)?,
                row.get::<_, Option<String>>(44)?,
                row.get::<_, String>(45)?,
                row.get::<_, String>(46)?,
                row.get::<_, Option<i64>>(47)?,
                row.get::<_, Option<i64>>(48)?,
                row.get::<_, Option<i64>>(49)?,
                row.get::<_, i64>(50)?,
                row.get::<_, Option<String>>(51)?,
                row.get::<_, Option<String>>(52)?,
                row.get::<_, Option<String>>(53)?,
                row.get::<_, Option<String>>(54)?,
                row.get::<_, Option<f64>>(55)?,
                row.get::<_, Option<f64>>(56)?,
            );
            Ok(v)
        })
        .map_err(|e| e.to_string())?;
    let mut out: Vec<serde_json::Value> = Vec::new();
    for r in mapped {
        let v = r.map_err(|e| e.to_string())?;
        let ticker = v.get("ticker").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let company = v.get("company_name").and_then(|x| x.as_str()).or_else(|| v.get("company").and_then(|x| x.as_str())).unwrap_or("").to_string();
        let sector = v.get("sector").and_then(|x| x.as_str()).unwrap_or("—").to_string();
        let industry = v.get("industry").and_then(|x| x.as_str()).unwrap_or("—").to_string();
        let mcap_cat = v.get("market_cap_category").and_then(|x| x.as_str()).unwrap_or("—").to_string();
        let country = v.get("country_code").and_then(|x| x.as_str()).or_else(|| v.get("country").and_then(|x| x.as_str())).unwrap_or("").to_string();
        let verdict = v.get("valuation_verdict").and_then(|x| x.as_str()).or_else(|| v.get("verdict").and_then(|x| x.as_str())).unwrap_or("—").to_string();
        let wfv = v.get("weighted_fair_value").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let gap_raw = v.get("valuation_gap_pct").and_then(|x| x.as_f64()).or_else(|| v.get("gap_pct").and_then(|x| x.as_f64())).unwrap_or(0.0);
        // stored gap_pct is decimal (0.126 = 12.6%, 2.34 = 234%); remove legacy >2 normalization per user — 2.34 must stay 2.34 not 0.0234
        let gap_pct = gap_raw;
        let confidence = v.get("confidence_score").and_then(|x| x.as_f64()).or_else(|| v.get("confidence").and_then(|x| x.as_f64())).unwrap_or(0.0);
        let risk_reward = v.get("risk_reward_ratio").and_then(|x| x.as_f64()).unwrap_or(0.0);
        // revenue growth: first year growth from dcf_assumptions
        let dcf = v.get("dcf_assumptions");
        let revenue_growth = dcf
            .and_then(|d| d.get("raw").and_then(|r| r.get("revenue_growth_rates")).or_else(|| d.get("revenue_growth_rates")))
            .and_then(|arr| arr.as_array())
            .and_then(|arr| arr.first().and_then(|x| x.as_f64()))
            .unwrap_or(0.0);
        let confidence_details = v.get("confidence_details").cloned().unwrap_or(Value::Null);
        out.push(json!({
            "ticker": ticker,
            "company": company,
            "sector": sector,
            "industry": industry,
            "market_cap_category": mcap_cat,
            "country": country,
            "verdict": verdict,
            "weighted_fair_value": wfv,
            "gap_pct": gap_pct,
            "confidence": confidence,
            "risk_reward": risk_reward,
            "revenue_growth": revenue_growth,
            "confidence_details": confidence_details
        }));
    }
    Ok(serde_json::Value::Array(out))
}

// ── Watchlists (F6.12) ───────────────────────────────────────────────────────
#[tauri::command]
pub fn watchlists_list(state: State<'_, DbState>) -> Result<Vec<Value>, String> {
    let guard = state.conn.lock().unwrap();
    let mut stmt = guard.prepare("SELECT id, name, description, created_at, updated_at FROM watchlists ORDER BY name").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "description": r.get::<_, Option<String>>(2)?,
            "created_at": r.get::<_, Option<String>>(3)?,
            "updated_at": r.get::<_, Option<String>>(4)?,
        }))
    }).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows { out.push(r.map_err(|e| e.to_string())?); }
    Ok(out)
}

#[tauri::command]
pub fn watchlist_create(state: State<'_, DbState>, name: String, description: Option<String>) -> Result<Value, String> {
    let n = name.trim().to_string();
    if n.is_empty() { return Err("name empty".to_string()); }
    if n.len() > 64 { return Err("name too long".to_string()); }
    let id = uuid::Uuid::new_v4().to_string();
    let guard = state.conn.lock().unwrap();
    guard.execute("INSERT INTO watchlists (id, name, description) VALUES (?1, ?2, ?3)", rusqlite::params![id, n, description]).map_err(|e| e.to_string())?;
    Ok(json!({"id": id, "name": n, "description": description}))
}

#[tauri::command]
pub fn watchlist_update(state: State<'_, DbState>, id: String, name: String, description: Option<String>) -> Result<(), String> {
    let n = name.trim().to_string();
    if n.is_empty() { return Err("name empty".to_string()); }
    let guard = state.conn.lock().unwrap();
    let changed = guard.execute("UPDATE watchlists SET name=?1, description=?2, updated_at=datetime('now') WHERE id=?3", rusqlite::params![n, description, id]).map_err(|e| e.to_string())?;
    if changed == 0 { return Err("watchlist not found".to_string()); }
    Ok(())
}

#[tauri::command]
pub fn watchlist_delete(state: State<'_, DbState>, id: String) -> Result<(), String> {
    let guard = state.conn.lock().unwrap();
    let changed = guard.execute("DELETE FROM watchlists WHERE id=?1", rusqlite::params![id]).map_err(|e| e.to_string())?;
    if changed == 0 { return Err("watchlist not found".to_string()); }
    Ok(())
}

#[tauri::command]
pub fn watchlist_items(state: State<'_, DbState>, watchlist_id: String) -> Result<Vec<Value>, String> {
    let guard = state.conn.lock().unwrap();
    // enriched: join to primary report for gap/verdict + currency per-country
    let mut stmt = guard.prepare(
        "SELECT wi.id, wi.ticker, wi.country_code, wi.notes, wi.target_price, wi.added_at,
                ar.company_name, ar.sector, ar.industry, ar.current_price, ar.weighted_fair_value, ar.valuation_gap_pct, ar.valuation_verdict, ar.confidence_score,
                COALESCE(ar.currency_code, cp.currency_code) as currency_code
         FROM watchlist_items wi
         LEFT JOIN analysis_reports ar ON ar.ticker=wi.ticker AND ar.country_code=wi.country_code AND ar.is_primary=1 AND ar.status='completed'
         LEFT JOIN country_parameters cp ON cp.country_code = wi.country_code
         WHERE wi.watchlist_id=?1 ORDER BY wi.added_at DESC"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(rusqlite::params![watchlist_id], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "ticker": r.get::<_, String>(1)?,
            "country_code": r.get::<_, String>(2)?,
            "notes": r.get::<_, Option<String>>(3)?,
            "target_price": r.get::<_, Option<f64>>(4)?,
            "added_at": r.get::<_, Option<String>>(5)?,
            "company_name": r.get::<_, Option<String>>(6)?,
            "sector": r.get::<_, Option<String>>(7)?,
            "industry": r.get::<_, Option<String>>(8)?,
            "current_price": r.get::<_, Option<f64>>(9)?,
            "weighted_fair_value": r.get::<_, Option<f64>>(10)?,
            "valuation_gap_pct": r.get::<_, Option<f64>>(11)?,
            "valuation_verdict": r.get::<_, Option<String>>(12)?,
            "confidence_score": r.get::<_, Option<f64>>(13)?,
            "currency_code": r.get::<_, Option<String>>(14)?,
        }))
    }).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows { out.push(r.map_err(|e| e.to_string())?); }
    Ok(out)
}

#[tauri::command]
pub fn watchlist_add_item(state: State<'_, DbState>, watchlist_id: String, ticker: String, country_code: String, notes: Option<String>, target_price: Option<f64>) -> Result<Value, String> {
    let t = ticker.trim().to_uppercase();
    let cc = country_code.trim().to_uppercase();
    if t.is_empty() || cc.is_empty() { return Err("ticker/country empty".to_string()); }
    let id = uuid::Uuid::new_v4().to_string();
    let guard = state.conn.lock().unwrap();
    // verify watchlist exists
    let exists: i64 = guard.query_row("SELECT COUNT(*) FROM watchlists WHERE id=?1", rusqlite::params![watchlist_id], |r| r.get(0)).map_err(|e| e.to_string())?;
    if exists == 0 { return Err("watchlist not found".to_string()); }
    guard.execute("INSERT INTO watchlist_items (id, watchlist_id, ticker, country_code, notes, target_price) VALUES (?1,?2,?3,?4,?5,?6)", rusqlite::params![id, watchlist_id, t, cc, notes, target_price]).map_err(|e| {
        let s = e.to_string();
        if s.contains("UNIQUE") { "already in watchlist".to_string() } else { s }
    })?;
    Ok(json!({"id": id, "ticker": t, "country_code": cc}))
}

#[tauri::command]
pub fn watchlist_remove_item(state: State<'_, DbState>, item_id: String) -> Result<(), String> {
    let guard = state.conn.lock().unwrap();
    let changed = guard.execute("DELETE FROM watchlist_items WHERE id=?1", rusqlite::params![item_id]).map_err(|e| e.to_string())?;
    if changed == 0 { return Err("item not found".to_string()); }
    Ok(())
}

#[tauri::command]
pub fn watchlist_update_item(state: State<'_, DbState>, item_id: String, notes: Option<String>, target_price: Option<f64>) -> Result<(), String> {
    let guard = state.conn.lock().unwrap();
    let changed = guard.execute("UPDATE watchlist_items SET notes=?1, target_price=?2 WHERE id=?3", rusqlite::params![notes, target_price, item_id]).map_err(|e| e.to_string())?;
    if changed == 0 { return Err("item not found".to_string()); }
    Ok(())
}

#[tauri::command]
pub fn save_pdf_file(path: String, data: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, &data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_backup_file(path: String, data: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, &data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn backup_export(state: State<'_, DbState>) -> Result<Vec<u8>, String> {
    let guard = state.conn.lock().unwrap();
    crate::backup::export_bytes(&guard)
}

#[tauri::command]
pub fn backup_validate(data: Vec<u8>) -> Result<crate::backup::BackupMeta, String> {
    let (_, meta) = crate::backup::parse_and_validate(&data)?;
    Ok(meta)
}

#[tauri::command]
pub fn backup_restore(state: State<'_, DbState>, data: Vec<u8>) -> Result<crate::backup::BackupMeta, String> {
    let guard = state.conn.lock().unwrap();
    crate::backup::restore_bytes(&guard, &data)
}

#[tauri::command]
pub fn stub_screener_rows() -> Result<serde_json::Value, String> {
    Ok(crate::stubs::screener_rows())
}

#[tauri::command]
pub fn ticker_search(
    state: State<'_, DbState>,
    query: String,
    country: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<crate::db::TickerCacheRow>, String> {
    let guard = state.conn.lock().unwrap();
    crate::db::search_tickers(&guard, &query, country.as_deref(), limit.unwrap_or(8))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ticker_yahoo_search(
    state: State<'_, DbState>,
    query: String,
    country: Option<String>,
) -> Result<Vec<crate::db::TickerCacheRow>, String> {
    let rows = crate::ticker::yahoo_search(&query, country.as_deref()).await?;
    // Persist returned quotes into ticker_cache so next local search hits
    {
        let guard = state.conn.lock().unwrap();
        for r in &rows {
            let _ = crate::db::upsert_ticker_cache(
                &guard,
                &r.ticker,
                r.company_name.as_deref(),
                r.exchange.as_deref(),
                &r.country_code,
            );
        }
    }
    Ok(rows)
}

#[derive(Serialize, Debug, Clone)]
pub struct FetchFinancialResult {
    pub status: String,
    pub error_message: Option<String>,
    pub company_info: Value,
    pub current_market_data: Value,
    pub market_parameters: Value,
    pub financials: Value,
    pub cached: bool,
}

#[tauri::command]
pub fn fetch_financial_data(
    state: State<'_, DbState>,
    ticker: String,
    country_code: String,
    force_refresh: Option<bool>,
) -> Result<FetchFinancialResult, String> {
    let _force = force_refresh.unwrap_or(false);
    // Python is the sole Yahoo fetch method — source kept at `py-scripts/yahoo_stock_extract.py`
    let (mparams, country_name) = {
        let guard = state.conn.lock().map_err(|e| format!("db lock: {e}"))?;
        let mp = crate::yahoo::transform::get_market_params(&guard, &country_code)?;
        let cn = crate::yahoo::transform::get_country_name(&guard, &country_code);
        (mp, cn)
    };
    let result = crate::yahoo::transform::fetch_and_transform_with_params(
        &ticker,
        &country_code,
        mparams,
        country_name,
    );

    match result {
        Ok(fr) => {
            // Upsert into financial_data_cache
            {
                let guard = state.conn.lock().unwrap();
                let raw_str = serde_json::to_string(&fr.raw_response).unwrap_or_else(|_| "{}".to_string());
                let parsed_str = serde_json::to_string(&fr.parsed_financials).unwrap_or_else(|_| "{}".to_string());
                let _ = crate::db::upsert_financial_cache(&guard, &ticker, &country_code, &raw_str, &parsed_str);
                // Also enrich ticker_cache with company_name/exchange if improved
                let comp_name = fr.company_info.get("company_name").and_then(|v| v.as_str());
                let exch = fr.company_info.get("exchange").and_then(|v| v.as_str());
                let _ = crate::db::upsert_ticker_cache(&guard, &ticker, comp_name, exch, &country_code);
            }
            Ok(FetchFinancialResult {
                status: fr.status,
                error_message: fr.error_message,
                company_info: fr.company_info,
                current_market_data: fr.current_market_data,
                market_parameters: fr.market_parameters,
                financials: fr.financials,
                cached: false,
            })
        }
        Err(e) => {
            // On network failure, try to return cached if exists
            let guard = state.conn.lock().unwrap();
            if let Ok(Some(cached)) = crate::db::get_cached_financial(&guard, &ticker, &country_code) {
                if let Some(parsed) = cached.parsed_financials {
                    if let Ok(fin) = serde_json::from_str::<Value>(&parsed) {
                        let raw: Value = serde_json::from_str(&cached.raw_response).unwrap_or(Value::Null);
                        // Rebuild minimal company_info from cache raw if possible, else fallback
                        let comp = raw.get("assetProfile").and_then(|p| p.get("longName").and_then(|v| v.as_str()).map(|s| json!(s))).unwrap_or(json!(ticker.clone()));
                        return Ok(FetchFinancialResult {
                            status: "completed".to_string(),
                            error_message: Some(format!("serving cached (yahoo error: {e})")),
                            company_info: json!({"company_name": comp, "ticker": ticker.to_uppercase(), "country": country_code}),
                            current_market_data: json!({}),
                            market_parameters: json!({}),
                            financials: fin,
                            cached: true,
                        });
                    }
                }
            }
            Err(e)
        }
    }
}

#[tauri::command]
pub fn get_cached_financial_data(
    state: State<'_, DbState>,
    ticker: String,
    country_code: String,
) -> Result<Option<Value>, String> {
    let guard = state.conn.lock().unwrap();
    let row = crate::db::get_cached_financial(&guard, &ticker, &country_code).map_err(|e| e.to_string())?;
    if let Some(r) = row {
        if let Some(parsed) = r.parsed_financials {
            if let Ok(v) = serde_json::from_str::<Value>(&parsed) {
                return Ok(Some(v));
            }
        }
        // fallback raw
        if let Ok(v) = serde_json::from_str::<Value>(&r.raw_response) {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

#[tauri::command]
pub async fn is_historical_unchanged(
    state: State<'_, DbState>,
    ticker: String,
    country_code: String,
    provider: String,
) -> Result<bool, String> {
    let t = ticker.trim().to_uppercase();
    let cc = country_code.trim().to_uppercase();
    if t.is_empty() || cc.is_empty() {
        return Ok(false);
    }
    // resolve model for provider
    let model: String = {
        let guard = state.conn.lock().unwrap();
        crate::db::get_setting(&guard, &format!("llm.{}.model", provider.trim()))
            .ok()
            .flatten()
            .unwrap_or_default()
    };
    if model.trim().is_empty() {
        return Ok(false);
    }
    // check if report exists for this ticker+model
    {
        let guard = state.conn.lock().unwrap();
        let count: i64 = guard
            .query_row(
                "SELECT COUNT(*) FROM analysis_reports WHERE ticker = ?1 AND llm_model = ?2",
                rusqlite::params![t, model],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if count == 0 {
            return Ok(false);
        }
    }
    // get cached historical
    let cached_parsed: Option<String> = {
        let guard = state.conn.lock().unwrap();
        crate::db::get_cached_financial(&guard, &t, &cc)
            .ok()
            .flatten()
            .and_then(|c| c.parsed_financials)
    };
    if cached_parsed.is_none() {
        return Ok(false);
    }
    // fresh fetch for compare (no upsert)
    let mparams = {
        let guard = state.conn.lock().unwrap();
        crate::yahoo::transform::get_market_params(&guard, &cc).map_err(|e| e.to_string())?
    };
    let cname = {
        let guard = state.conn.lock().unwrap();
        crate::yahoo::transform::get_country_name(&guard, &cc)
    };
    let fresh = crate::yahoo::transform::fetch_and_transform_with_params(&t, &cc, mparams, cname)
        .map_err(|e| e.to_string())?;
    if fresh.status != "completed" {
        return Ok(false);
    }
    // compare only historical_data
    let normalize = |v: &Value| -> Option<Vec<Value>> {
        let hist = v.get("financials")?.get("historical_data")?.as_array()?.clone();
        if hist.is_empty() {
            return Some(vec![]);
        }
        let mut out: Vec<Value> = Vec::with_capacity(hist.len());
        for (idx, row) in hist.iter().enumerate() {
            let mut obj = row.as_object()?.clone();
            if idx == 0 {
                obj.insert("fiscal_year_end".to_string(), Value::String("TTM".to_string()));
            }
            let mut filtered = serde_json::Map::new();
            for k in ["fiscal_year_end", "revenue", "operating_income", "ebitda", "net_income", "total_debt", "total_equity", "cash_equivalents", "free_cash_flow"] {
                if let Some(val) = obj.get(k) {
                    filtered.insert(k.to_string(), val.clone());
                }
            }
            out.push(Value::Object(filtered));
        }
        Some(out)
    };
    let fresh_hist = normalize(&fresh.parsed_financials);
    let cached_val: Value = serde_json::from_str(&cached_parsed.unwrap_or_default()).map_err(|e| e.to_string())?;
    let cached_hist = normalize(&cached_val);
    if let (Some(c), Some(f)) = (cached_hist, fresh_hist) {
        Ok(c == f)
    } else {
        Ok(false)
    }
}

#[derive(Serialize)]
pub struct StalenessInfo {
    pub is_stale: bool,
    pub reason: String,
    pub fetch_date: Option<String>,
    pub price_date: Option<String>,
}

#[tauri::command]
pub fn check_financial_staleness(
    state: State<'_, DbState>,
    ticker: String,
    country_code: String,
) -> Result<StalenessInfo, String> {
    let t = ticker.trim().to_uppercase();
    let cc = country_code.trim().to_uppercase();
    let cached_opt = {
        let guard = state.conn.lock().unwrap();
        crate::db::get_cached_financial(&guard, &t, &cc).map_err(|e| e.to_string())?
    };
    match cached_opt {
        None => Ok(StalenessInfo { is_stale: true, reason: "no cached data".to_string(), fetch_date: None, price_date: None }),
        Some(r) => {
            // 90-day check via chrono NaiveDateTime parsing of fetch_date "YYYY-MM-DD HH:MM:SS"
            let fetch = r.fetch_date.clone();
            let is_old_by_date = {
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&fetch, "%Y-%m-%d %H:%M:%S") {
                    let now = chrono::Utc::now().naive_utc();
                    let diff = now.signed_duration_since(dt);
                    diff.num_days() > 90
                } else {
                    // fallback: try date only
                    if let Ok(d) = chrono::NaiveDate::parse_from_str(&fetch[..10.min(fetch.len())], "%Y-%m-%d") {
                        let now = chrono::Utc::now().date_naive();
                        (now - d).num_days() > 90
                    } else { false }
                }
            };
            if is_old_by_date {
                return Ok(StalenessInfo { is_stale: true, reason: "fetch older than 90d — re-analyze recommended".to_string(), fetch_date: Some(fetch), price_date: None });
            }
            // Try fresh Yahoo fetch to compare price_date / TTM fiscal year
            let fresh_stale: Option<(bool, String, Option<String>)> = {
                let guard = state.conn.lock().unwrap();
                let mparams = crate::yahoo::transform::get_market_params(&guard, &cc).ok();
                let cname = crate::yahoo::transform::get_country_name(&guard, &cc);
                drop(guard);
                if let Some(mp) = mparams {
                    if let Ok(fr) = crate::yahoo::transform::fetch_and_transform_with_params(&t, &cc, mp, cname) {
                        let fresh_price = fr.current_market_data.get("price_date").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let cached_parsed: Option<Value> = r.parsed_financials.as_ref().and_then(|s| serde_json::from_str::<Value>(s).ok());
                        let cached_price = cached_parsed.as_ref().and_then(|v| v.get("current_market_data")).and_then(|cm| cm.get("price_date")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                        // Also compare most recent fiscal_year_end
                        let fresh_fye = fr.financials.get("historical_data").and_then(|a| a.as_array()).and_then(|arr| arr.first()).and_then(|row| row.get("fiscal_year_end")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let cached_fye = cached_parsed.as_ref().and_then(|v| v.get("financials")).and_then(|f| f.get("historical_data")).and_then(|a| a.as_array()).and_then(|arr| arr.first()).and_then(|row| row.get("fiscal_year_end")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if !fresh_price.is_empty() && !cached_price.is_empty() && fresh_price != cached_price {
                            Some((true, format!("new market price_date {fresh_price} vs cached {cached_price}"), Some(fresh_price)))
                        } else if !fresh_fye.is_empty() && !cached_fye.is_empty() && fresh_fye != cached_fye {
                            Some((true, format!("new financial period {fresh_fye} vs cached {cached_fye}"), Some(fresh_price)))
                        } else {
                            Some((false, "cached fresh — no new price or financial period".to_string(), Some(fresh_price)))
                        }
                    } else { None }
                } else { None }
            };
            if let Some((is_stale, reason, fresh_price)) = fresh_stale {
                Ok(StalenessInfo { is_stale, reason, fetch_date: Some(fetch), price_date: fresh_price })
            } else {
                // Network failed or no market params — fallback to date check (already not old)
                Ok(StalenessInfo { is_stale: false, reason: "cached fresh (yahoo check unavailable)".to_string(), fetch_date: Some(fetch), price_date: None })
            }
        }
    }
}

// ── Report reads ────────────────────────────────────────────────────────────

fn parse_json_opt(s: Option<String>) -> Value {
    s.and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null)
}

fn report_row_to_json(
    id: String,
    ticker: String,
    company_name: String,
    exchange: Option<String>,
    country_code: String,
    sector: Option<String>,
    industry: Option<String>,
    market_cap_category: Option<String>,
    price_date: String,
    analysis_date: String,
    is_primary: i64,
    status: String,
    error_message: Option<String>,
    current_price: Option<f64>,
    market_cap: Option<f64>,
    currency_code: Option<String>,
    base_case_fair_value: Option<f64>,
    weighted_fair_value: Option<f64>,
    valuation_gap_pct: Option<f64>,
    valuation_verdict: Option<String>,
    wacc: Option<f64>,
    cost_of_equity: Option<f64>,
    cost_of_debt_after_tax: Option<f64>,
    raw_beta: Option<f64>,
    blume_adjusted_beta: Option<f64>,
    terminal_growth_rate: Option<f64>,
    terminal_margin: Option<f64>,
    upside_pct: Option<f64>,
    downside_pct: Option<f64>,
    risk_reward_ratio: Option<f64>,
    risk_reward_label: Option<String>,
    scenarios: Option<String>,
    sensitivity_matrix: Option<String>,
    confidence_score: Option<f64>,
    confidence_level: Option<String>,
    confidence_details: Option<String>,
    ai_business_context: Option<String>,
    ai_historical_performance: Option<String>,
    ai_growth_projections: Option<String>,
    ai_opportunities_risks: Option<String>,
    ai_investment_perspective: Option<String>,
    dcf_assumptions: Option<String>,
    country_params_snapshot: Option<String>,
    search_sources: Option<String>,
    sensitivity_base_parameters: Option<String>,
    llm_provider: String,
    llm_model: String,
    llm_thinking_tokens: Option<i64>,
    llm_output_tokens: Option<i64>,
    llm_duration_ms: Option<i64>,
    is_favorite: i64,
    user_notes: Option<String>,
    tags: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    bull_case_fair_value: Option<f64>,
    bear_case_fair_value: Option<f64>,
) -> Value {
    json!({
        "id": id,
        "ticker": ticker,
        "company_name": company_name,
        "exchange": exchange,
        "country_code": country_code,
        "sector": sector,
        "industry": industry,
        "market_cap_category": market_cap_category,
        "price_date": price_date,
        "analysis_date": analysis_date,
        "is_primary": is_primary != 0,
        "status": status,
        "error_message": error_message,
        "current_price": current_price,
        "market_cap": market_cap,
        "currency_code": currency_code,
        "base_case_fair_value": base_case_fair_value,
        "bull_case_fair_value": bull_case_fair_value,
        "bear_case_fair_value": bear_case_fair_value,
        "weighted_fair_value": weighted_fair_value,
        "valuation_gap_pct": valuation_gap_pct,
        "valuation_verdict": valuation_verdict,
        "wacc": wacc,
        "cost_of_equity": cost_of_equity,
        "cost_of_debt_after_tax": cost_of_debt_after_tax,
        "raw_beta": raw_beta,
        "blume_adjusted_beta": blume_adjusted_beta,
        "terminal_growth_rate": terminal_growth_rate,
        "terminal_margin": terminal_margin,
        "upside_pct": upside_pct,
        "downside_pct": downside_pct,
        "risk_reward_ratio": risk_reward_ratio,
        "risk_reward_label": risk_reward_label,
        "scenarios": parse_json_opt(scenarios),
        "sensitivity_matrix": parse_json_opt(sensitivity_matrix),
        "confidence_score": confidence_score,
        "confidence_level": confidence_level,
        "confidence_details": parse_json_opt(confidence_details),
        "ai_business_context": ai_business_context,
        "ai_historical_performance": ai_historical_performance,
        "ai_growth_projections": ai_growth_projections,
        "ai_opportunities_risks": parse_json_opt(ai_opportunities_risks),
        "ai_investment_perspective": ai_investment_perspective,
        "dcf_assumptions": parse_json_opt(dcf_assumptions),
        "country_params_snapshot": parse_json_opt(country_params_snapshot),
        "search_sources": parse_json_opt(search_sources),
        "sensitivity_base_parameters": parse_json_opt(sensitivity_base_parameters),
        "llm_provider": llm_provider,
        "llm_model": llm_model,
        "llm_thinking_tokens": llm_thinking_tokens,
        "llm_output_tokens": llm_output_tokens,
        "llm_duration_ms": llm_duration_ms,
        "is_favorite": is_favorite != 0,
        "user_notes": user_notes,
        "tags": parse_json_opt(tags),
        "created_at": created_at,
        "updated_at": updated_at,
        // legacy aliases for MyReports convenience
        "company": company_name.clone(),
        "country": country_code.clone(),
        "model": llm_model.clone(),
        "verdict": valuation_verdict.clone().unwrap_or_default(),
        "gap_pct": valuation_gap_pct,
        "confidence": confidence_score,
        "favorite": is_favorite != 0,
    })
}

fn query_report_by_sql(conn: &Connection, sql: &str, params: Vec<String>) -> Result<Value, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let mut rows = stmt.query(rusqlite::params_from_iter(params.iter())).map_err(|e| e.to_string())?;
    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let v = report_row_to_json(
            row.get::<_, String>(0).map_err(|e| e.to_string())?,
            row.get::<_, String>(1).map_err(|e| e.to_string())?,
            row.get::<_, String>(2).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(3).map_err(|e| e.to_string())?,
            row.get::<_, String>(4).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(5).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(6).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(7).map_err(|e| e.to_string())?,
            row.get::<_, String>(8).map_err(|e| e.to_string())?,
            row.get::<_, String>(9).map_err(|e| e.to_string())?,
            row.get::<_, i64>(10).map_err(|e| e.to_string())?,
            row.get::<_, String>(11).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(12).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(13).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(14).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(15).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(16).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(17).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(18).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(19).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(20).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(21).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(22).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(23).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(24).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(25).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(26).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(27).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(28).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(29).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(30).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(31).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(32).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(33).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(34).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(35).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(36).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(37).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(38).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(39).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(40).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(41).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(42).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(43).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(44).map_err(|e| e.to_string())?,
            row.get::<_, String>(45).map_err(|e| e.to_string())?,
            row.get::<_, String>(46).map_err(|e| e.to_string())?,
            row.get::<_, Option<i64>>(47).map_err(|e| e.to_string())?,
            row.get::<_, Option<i64>>(48).map_err(|e| e.to_string())?,
            row.get::<_, Option<i64>>(49).map_err(|e| e.to_string())?,
            row.get::<_, i64>(50).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(51).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(52).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(53).map_err(|e| e.to_string())?,
            row.get::<_, Option<String>>(54).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(55).map_err(|e| e.to_string())?,
            row.get::<_, Option<f64>>(56).map_err(|e| e.to_string())?,
        );
        Ok(v)
    } else {
        Err("report not found".to_string())
    }
}

const REPORT_SELECT: &str = "SELECT id, ticker, company_name, exchange, country_code, sector, industry, market_cap_category, price_date, analysis_date, is_primary, status, error_message, current_price, market_cap, currency_code, base_case_fair_value, weighted_fair_value, valuation_gap_pct, valuation_verdict, wacc, cost_of_equity, cost_of_debt_after_tax, raw_beta, blume_adjusted_beta, terminal_growth_rate, terminal_margin, upside_pct, downside_pct, risk_reward_ratio, risk_reward_label, scenarios, sensitivity_matrix, confidence_score, confidence_level, confidence_details, ai_business_context, ai_historical_performance, ai_growth_projections, ai_opportunities_risks, ai_investment_perspective, dcf_assumptions, country_params_snapshot, search_sources, sensitivity_base_parameters, llm_provider, llm_model, llm_thinking_tokens, llm_output_tokens, llm_duration_ms, is_favorite, user_notes, tags, created_at, updated_at, bull_case_fair_value, bear_case_fair_value FROM analysis_reports";

#[tauri::command]
pub fn list_reports(state: State<'_, DbState>) -> Result<Vec<Value>, String> {
    let guard = state.conn.lock().unwrap();
    let sql = format!("{REPORT_SELECT} WHERE is_primary = 1 ORDER BY analysis_date DESC, price_date DESC LIMIT 500");
    let mut stmt = guard.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(report_row_to_json(
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, i64>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<f64>>(13)?,
            row.get::<_, Option<f64>>(14)?,
            row.get::<_, Option<String>>(15)?,
            row.get::<_, Option<f64>>(16)?,
            row.get::<_, Option<f64>>(17)?,
            row.get::<_, Option<f64>>(18)?,
            row.get::<_, Option<String>>(19)?,
            row.get::<_, Option<f64>>(20)?,
            row.get::<_, Option<f64>>(21)?,
            row.get::<_, Option<f64>>(22)?,
            row.get::<_, Option<f64>>(23)?,
            row.get::<_, Option<f64>>(24)?,
            row.get::<_, Option<f64>>(25)?,
            row.get::<_, Option<f64>>(26)?,
            row.get::<_, Option<f64>>(27)?,
            row.get::<_, Option<f64>>(28)?,
            row.get::<_, Option<f64>>(29)?,
            row.get::<_, Option<String>>(30)?,
            row.get::<_, Option<String>>(31)?,
            row.get::<_, Option<String>>(32)?,
            row.get::<_, Option<f64>>(33)?,
            row.get::<_, Option<String>>(34)?,
            row.get::<_, Option<String>>(35)?,
            row.get::<_, Option<String>>(36)?,
            row.get::<_, Option<String>>(37)?,
            row.get::<_, Option<String>>(38)?,
            row.get::<_, Option<String>>(39)?,
            row.get::<_, Option<String>>(40)?,
            row.get::<_, Option<String>>(41)?,
            row.get::<_, Option<String>>(42)?,
            row.get::<_, Option<String>>(43)?,
            row.get::<_, Option<String>>(44)?,
            row.get::<_, String>(45)?,
            row.get::<_, String>(46)?,
            row.get::<_, Option<i64>>(47)?,
            row.get::<_, Option<i64>>(48)?,
            row.get::<_, Option<i64>>(49)?,
            row.get::<_, i64>(50)?,
            row.get::<_, Option<String>>(51)?,
            row.get::<_, Option<String>>(52)?,
            row.get::<_, Option<String>>(53)?,
            row.get::<_, Option<String>>(54)?,
            row.get::<_, Option<f64>>(55)?,
            row.get::<_, Option<f64>>(56)?,
        ))
    }).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows { out.push(r.map_err(|e| e.to_string())?); }
    Ok(out)
}

#[tauri::command]
pub fn get_report(state: State<'_, DbState>, ticker: String, version_id: Option<String>) -> Result<Value, String> {
    let guard = state.conn.lock().unwrap();
    if let Some(vid) = version_id.as_deref().filter(|s| !s.trim().is_empty()) {
        return query_report_by_sql(&guard, &format!("{REPORT_SELECT} WHERE id = ?1"), vec![vid.to_string()]);
    }
    let t = ticker.trim().to_uppercase();
    if t.is_empty() { return Err("ticker empty".to_string()); }
    // Latest: is_primary DESC then analysis_date DESC
    query_report_by_sql(&guard, &format!("{REPORT_SELECT} WHERE ticker = ?1 ORDER BY is_primary DESC, analysis_date DESC, price_date DESC LIMIT 1"), vec![t])
}

#[tauri::command]
pub fn get_report_by_id(state: State<'_, DbState>, id: String) -> Result<Value, String> {
    let guard = state.conn.lock().unwrap();
    query_report_by_sql(&guard, &format!("{REPORT_SELECT} WHERE id = ?1"), vec![id])
}

#[tauri::command]
pub fn get_report_versions(state: State<'_, DbState>, ticker: String) -> Result<Vec<Value>, String> {
    let t = ticker.trim().to_uppercase();
    let guard = state.conn.lock().unwrap();
    let mut stmt = guard.prepare("SELECT id, llm_provider, llm_model, price_date, analysis_date, is_primary, is_favorite, status FROM analysis_reports WHERE ticker = ?1 ORDER BY analysis_date DESC, price_date DESC").map_err(|e| e.to_string())?;
    let rows = stmt.query_map(rusqlite::params![t], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "llm_provider": r.get::<_, String>(1)?,
            "llm_model": r.get::<_, String>(2)?,
            "price_date": r.get::<_, String>(3)?,
            "analysis_date": r.get::<_, String>(4)?,
            "is_primary": r.get::<_, i64>(5)? != 0,
            "is_favorite": r.get::<_, i64>(6)? != 0,
            "status": r.get::<_, String>(7)?,
        }))
    }).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows { out.push(r.map_err(|e| e.to_string())?); }
    Ok(out)
}

#[tauri::command]
pub fn toggle_favorite(state: State<'_, DbState>, id: String) -> Result<Value, String> {
    let guard = state.conn.lock().unwrap();
    // Fetch current row's ticker / flags to decide primary switch
    let row: Option<(String, i64, i64)> = guard
        .query_row(
            "SELECT ticker, is_primary, is_favorite FROM analysis_reports WHERE id = ?1",
            rusqlite::params![id.clone()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
        )
        .ok();
    let Some((ticker, is_primary, is_favorite)) = row else {
        return Err("report not found".to_string());
    };
    if is_primary != 0 {
        // Already primary — just toggle favorite, keep primary as is
        let changed = guard
            .execute(
                "UPDATE analysis_reports SET is_favorite = CASE WHEN is_favorite=1 THEN 0 ELSE 1 END, updated_at = datetime('now') WHERE id = ?1",
                rusqlite::params![id.clone()],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 { return Err("report not found".to_string()); }
    } else if is_favorite == 0 {
        // Non-primary being marked as favorite → also make it primary (only one primary per ticker)
        let tx = guard.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE analysis_reports SET is_primary = 0 WHERE ticker = ?1 AND is_primary = 1",
            rusqlite::params![ticker],
        )
        .map_err(|e| e.to_string())?;
        let changed = tx
            .execute(
                "UPDATE analysis_reports SET is_favorite = 1, is_primary = 1, updated_at = datetime('now') WHERE id = ?1",
                rusqlite::params![id.clone()],
            )
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        if changed == 0 { return Err("report not found".to_string()); }
    } else {
        // Non-primary currently favorite → unfavorite, keep non-primary
        let changed = guard
            .execute(
                "UPDATE analysis_reports SET is_favorite = 0, updated_at = datetime('now') WHERE id = ?1",
                rusqlite::params![id.clone()],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 { return Err("report not found".to_string()); }
    }
    query_report_by_sql(&guard, &format!("{REPORT_SELECT} WHERE id = ?1"), vec![id])
}

#[tauri::command]
pub fn get_reports_by_ids(state: State<'_, DbState>, ids: Vec<String>) -> Result<Vec<Value>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    if ids.len() > 10 {
        return Err("too many ids (max 10)".to_string());
    }
    let guard = state.conn.lock().unwrap();
    // Fetch in input order — query each id to preserve order and reuse row mapping
    let mut out: Vec<Value> = Vec::with_capacity(ids.len());
    for id in ids {
        let v = query_report_by_sql(&guard, &format!("{REPORT_SELECT} WHERE id = ?1"), vec![id.clone()])
            .map_err(|e| format!("{}: {}", id, e))?;
        out.push(v);
    }
    Ok(out)
}

#[tauri::command]
pub fn delete_reports(state: State<'_, DbState>, ids: Vec<String>) -> Result<usize, String> {
    if ids.is_empty() {
        return Err("no ids".to_string());
    }
    if ids.len() > 50 {
        return Err("too many ids (max 50)".to_string());
    }
    let guard = state.conn.lock().unwrap();
    // Collect affected tickers before delete
    let mut tickers: std::collections::HashSet<String> = std::collections::HashSet::new();
    for id in &ids {
        if let Ok(ticker) = guard.query_row(
            "SELECT ticker FROM analysis_reports WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, String>(0),
        ) {
            tickers.insert(ticker);
        }
    }
    let tx = guard.unchecked_transaction().map_err(|e| e.to_string())?;
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM analysis_reports WHERE id IN ({})", placeholders);
    let deleted = {
        let mut stmt = tx.prepare(&sql).map_err(|e| e.to_string())?;
        stmt.execute(rusqlite::params_from_iter(ids.iter()))
            .map_err(|e| e.to_string())? as usize
    };
    // Re-elect primary per ticker if missing
    for ticker in tickers {
        let has_primary: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM analysis_reports WHERE ticker = ?1 AND is_primary = 1",
                rusqlite::params![ticker],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if has_primary == 0 {
            let _ = tx.execute(
                "UPDATE analysis_reports SET is_primary = 1 WHERE id = (SELECT id FROM analysis_reports WHERE ticker = ?1 ORDER BY analysis_date DESC LIMIT 1)",
                rusqlite::params![ticker],
            );
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(deleted)
}

#[tauri::command]
pub fn update_cmp(state: State<'_, DbState>, ticker: String) -> Result<Value, String> {
    let t = ticker.trim().to_uppercase();
    if t.is_empty() { return Err("ticker empty".to_string()); }
    // Resolve country_code from latest report, else fallback ticker_cache
    let country_code: String = {
        let guard = state.conn.lock().unwrap();
        if let Ok(cc) = guard.query_row("SELECT country_code FROM analysis_reports WHERE ticker = ?1 ORDER BY analysis_date DESC LIMIT 1", rusqlite::params![t.clone()], |r| r.get::<_, String>(0)) {
            cc
        } else if let Ok(cc) = guard.query_row("SELECT country_code FROM ticker_cache WHERE ticker = ?1 LIMIT 1", rusqlite::params![t.clone()], |r| r.get::<_, String>(0)) {
            cc
        } else if let Ok(cc) = guard.query_row("SELECT country_code FROM financial_data_cache WHERE ticker = ?1 LIMIT 1", rusqlite::params![t.clone()], |r| r.get::<_, String>(0)) {
            cc
        } else {
            return Err(format!("no report or cache for ticker {t} — cannot resolve country"));
        }
    };
    // Fetch fresh via same Yahoo extract (full fetch)
    let (mparams, cname) = {
        let guard = state.conn.lock().unwrap();
        let mp = crate::yahoo::transform::get_market_params(&guard, &country_code).map_err(|e| e.to_string())?;
        let cn = crate::yahoo::transform::get_country_name(&guard, &country_code);
        (mp, cn)
    };
    let fr = crate::yahoo::transform::fetch_and_transform_with_params(&t, &country_code, mparams, cname).map_err(|e| e.to_string())?;
    // Upsert into financial_data_cache but do NOT touch analysis_reports
    {
        let guard = state.conn.lock().unwrap();
        let raw_str = serde_json::to_string(&fr.raw_response).unwrap_or_else(|_| "{}".to_string());
        let parsed_str = serde_json::to_string(&fr.parsed_financials).unwrap_or_else(|_| "{}".to_string());
        let _ = crate::db::upsert_financial_cache(&guard, &t, &country_code, &raw_str, &parsed_str);
    }
    let price = fr.current_market_data.get("current_price").and_then(|v| v.as_f64());
    let price_date = fr.current_market_data.get("price_date").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let market_cap = fr.current_market_data.get("market_cap").and_then(|v| v.as_f64());
    Ok(json!({
        "ticker": t,
        "country_code": country_code,
        "current_price": price,
        "price_date": price_date,
        "market_cap": market_cap,
        "status": fr.status,
    }))
}

#[tauri::command]
pub fn update_price(state: State<'_, DbState>, ticker: String) -> Result<Value, String> {
    let t = ticker.trim().to_uppercase();
    if t.is_empty() { return Err("ticker empty".to_string()); }
    let country_code: String = {
        let guard = state.conn.lock().unwrap();
        if let Ok(cc) = guard.query_row("SELECT country_code FROM analysis_reports WHERE ticker = ?1 ORDER BY analysis_date DESC LIMIT 1", rusqlite::params![t.clone()], |r| r.get::<_, String>(0)) {
            cc
        } else if let Ok(cc) = guard.query_row("SELECT country_code FROM ticker_cache WHERE ticker = ?1 LIMIT 1", rusqlite::params![t.clone()], |r| r.get::<_, String>(0)) {
            cc
        } else if let Ok(cc) = guard.query_row("SELECT country_code FROM financial_data_cache WHERE ticker = ?1 LIMIT 1", rusqlite::params![t.clone()], |r| r.get::<_, String>(0)) {
            cc
        } else {
            return Err(format!("no report or cache for ticker {t} — cannot resolve country"));
        }
    };
    // Price-only fast path — no MarketParamsSnapshot / historical / cache write (ephemeral)
    let pr = crate::yahoo::transform::fetch_price_only(&t).map_err(|e| e.to_string())?;
    Ok(json!({
        "ticker": t,
        "country_code": country_code,
        "current_price": pr.price,
        "price_date": pr.price_date,
        "currency": pr.currency,
        "status": "price_ok",
    }))
}

#[derive(Debug, Deserialize)]
pub struct RecomputeOverride {
    pub revenue_growth_rates: Option<[f64; 5]>,
    pub operating_margins: Option<[f64; 5]>,
    pub terminal_margin: Option<f64>,
    pub terminal_growth_rate: Option<f64>,
    pub fcf_conversion_factor: Option<f64>,
    pub financial_services_ni_factor: Option<f64>,
    pub cyclicality_adjustment_bps: Option<i32>,
    pub wacc: Option<f64>,
}

#[tauri::command]
pub fn recompute_local(state: State<'_, DbState>, id: String, overrides: RecomputeOverride) -> Result<Value, String> {
    let guard = state.conn.lock().unwrap();
    let report = query_report_by_sql(&guard, &format!("{REPORT_SELECT} WHERE id = ?1"), vec![id.clone()])?;
    let ticker = report.get("ticker").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let country_code = report.get("country_code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    // Baseline dcf_assumptions: prefer stored dcf_assumptions raw, else reconstruct defaults
    let dcf_json = report.get("dcf_assumptions").cloned().unwrap_or(Value::Null);
    let raw_inputs = dcf_json.get("raw").cloned().unwrap_or_else(|| dcf_json.clone());
    let mut dcf_inputs: crate::engine::DcfInputs = if raw_inputs.is_null() || raw_inputs == Value::Null {
        // Fallback — try to parse top-level fields inside dcf_assumptions
        serde_json::from_value::<crate::engine::DcfInputs>(dcf_json.clone()).map_err(|e| format!("parse dcf_assumptions: {e}"))?
    } else if raw_inputs.get("revenue_growth_rates").is_some() {
        serde_json::from_value::<crate::engine::DcfInputs>(raw_inputs.clone()).map_err(|e| format!("parse raw dcf_inputs: {e}"))?
    } else {
        serde_json::from_value::<crate::engine::DcfInputs>(dcf_json.clone()).map_err(|e| format!("parse dcf_assumptions: {e}"))?
    };
    // For missing optional raw arrays, reconstruct from report-level if needed: use overrides baseline as is
    // Apply overrides
    if let Some(v) = overrides.revenue_growth_rates { dcf_inputs.revenue_growth_rates = v; }
    if let Some(v) = overrides.operating_margins { dcf_inputs.operating_margins = v; }
    if let Some(v) = overrides.terminal_margin { dcf_inputs.terminal_margin = v; }
    if let Some(v) = overrides.terminal_growth_rate { dcf_inputs.terminal_growth_rate = v; }
    if let Some(v) = overrides.fcf_conversion_factor { dcf_inputs.fcf_conversion_factor = Some(v); }
    if let Some(v) = overrides.financial_services_ni_factor { dcf_inputs.financial_services_ni_factor = Some(v); }
    if let Some(v) = overrides.cyclicality_adjustment_bps { dcf_inputs.cyclicality_adjustment_bps = Some(v); }
    if let Some(v) = overrides.wacc { dcf_inputs.wacc = Some(v); }

    // Build EngineInput from cached financials + report meta
    let cached = crate::db::get_cached_financial(&guard, &ticker, &country_code).map_err(|e| e.to_string())?.ok_or_else(|| "no financial_data_cache for ticker — run analysis first".to_string())?;
    let parsed: Value = cached.parsed_financials.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()).ok_or_else(|| "parsed_financials missing or invalid".to_string())?;
    let mut company_info_v = parsed.get("company_info").cloned().ok_or("company_info missing in cache")?;
    let current_market_data_v = parsed.get("current_market_data").cloned().ok_or("current_market_data missing")?;
    let market_params_v = parsed.get("market_parameters").cloned().ok_or("market_parameters missing")?;
    let financials_v = parsed.get("financials").cloned().ok_or("financials missing")?;
    // Backfill missing market_cap_category (cached company_info predates that field — derive from current_market_data)
    if let Some(obj) = company_info_v.as_object_mut() {
        if !obj.contains_key("market_cap_category") {
            let cat = current_market_data_v
                .get("market_cap_category")
                .cloned()
                .unwrap_or(Value::String("mid-cap".to_string()));
            obj.insert("market_cap_category".to_string(), cat);
        }
    }
    // Prefer report's country_params_snapshot for market_params (audit trail) if available
    let market_params_v = report.get("country_params_snapshot").and_then(|v| if v.is_null() { None } else { Some(v.clone()) }).unwrap_or(market_params_v);
    let company_info: crate::engine::CompanyInfo = serde_json::from_value(company_info_v).map_err(|e| format!("company_info parse: {e}"))?;
    let current_market_data: crate::engine::CurrentMarketData = serde_json::from_value(current_market_data_v).map_err(|e| format!("current_market_data parse: {e}"))?;
    let market_params: crate::engine::MarketParams = serde_json::from_value(market_params_v).map_err(|e| format!("market_params parse: {e}"))?;
    let financials: crate::engine::Financials = serde_json::from_value(financials_v).map_err(|e| format!("financials parse: {e}"))?;
    let input = crate::engine::EngineInput { company_info, current_market_data, market_params, financials, dcf_inputs };
    let out = crate::engine::run(input).map_err(|e| e.to_string())?;
    // Return DcfOutput as JSON — caller renders locally, no DB write
    Ok(serde_json::to_value(&out).map_err(|e| e.to_string())?)
}

#[derive(Serialize)]
pub struct DbTable {
    pub name: String,
    pub rows: usize,
}

#[derive(Serialize)]
pub struct DbTableData {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub total: usize,
    pub returned: usize,
}

fn open_readonly(path: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
}

#[tauri::command]
pub async fn run_analysis(
    app_handle: tauri::AppHandle,
    state: State<'_, DbState>,
    ticker: String,
    country_code: String,
    provider: Option<String>,
) -> Result<crate::pipeline::RunAnalysisResult, String> {
    let path = state.path.clone();
    crate::pipeline::run_analysis(path, Some(app_handle), ticker, country_code, provider).await
}

#[tauri::command]
pub async fn enqueue_analysis(
    app_handle: tauri::AppHandle,
    state: State<'_, DbState>,
    queue: State<'_, crate::queue::QueueHandle>,
    ticker: String,
    country_code: String,
    provider: Option<String>,
) -> Result<String, String> {
    let path = state.path.clone();
    let prov = provider.unwrap_or_else(|| "custom".into());
    // Use the shared queue's in-memory FIFO
    crate::queue::enqueue(app_handle, path, queue.0.clone(), ticker, country_code, prov).await
}

#[tauri::command]
pub async fn list_queue(queue: State<'_, crate::queue::QueueHandle>) -> Result<serde_json::Value, String> {
    let s = queue.0.lock().unwrap();
    let pending: Vec<_> = s.pending.iter().cloned().collect();
    let active: Vec<_> = s.active.values().cloned().collect();
    let recent: Vec<_> = s.recent.iter().cloned().collect();
    Ok(serde_json::json!({"pending": pending, "active": active, "recent": recent, "max_parallel": s.max_parallel}))
}

#[tauri::command]
pub async fn get_queue_state(queue: State<'_, crate::queue::QueueHandle>) -> Result<serde_json::Value, String> {
    let s = queue.0.lock().unwrap();
    Ok(serde_json::json!({"pending": s.pending.len(), "active": s.active.len(), "max_parallel": s.max_parallel}))
}

#[tauri::command]
pub async fn get_max_parallel(_state: State<'_, DbState>, queue: State<'_, crate::queue::QueueHandle>) -> Result<usize, String> {
    // Prefer in-memory, fallback to DB
    let mem = queue.0.lock().unwrap().max_parallel;
    Ok(mem)
}

#[tauri::command]
pub async fn set_max_parallel(
    state: State<'_, DbState>,
    queue: State<'_, crate::queue::QueueHandle>,
    app_handle: tauri::AppHandle,
    value: usize,
) -> Result<usize, String> {
    let v = value.clamp(1, 5);
    {
        let guard = state.conn.lock().unwrap();
        crate::db::set_setting(&guard, "analysis.max_parallel", &v.to_string(), "analysis").map_err(|e| e.to_string())?;
    }
    {
        let mut s = queue.0.lock().unwrap();
        s.max_parallel = v;
    }
    // Try to dispatch pending if we increased parallelism
    let path = state.path.clone();
    let q = queue.0.clone();
    crate::queue::try_dispatch(app_handle, path, q);
    Ok(v)
}

#[tauri::command]
pub fn debug_log_dir(state: State<'_, DbState>) -> String {
    crate::debug_log::log_dir_path(&state.path)
}

#[tauri::command]
pub fn debug_logs_list(state: State<'_, DbState>) -> Vec<crate::debug_log::LogFileMeta> {
    crate::debug_log::list_logs(&state.path)
}

#[tauri::command]
pub fn debug_log_read(
    state: State<'_, DbState>,
    run_id: String,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    let (summary, entries) = crate::debug_log::read_log(&state.path, &run_id, limit)?;
    Ok(serde_json::json!({"summary": summary, "run_id": run_id, "entries": entries}))
}

#[tauri::command]
pub fn debug_logs_clear(state: State<'_, DbState>) -> Result<usize, String> {
    crate::debug_log::clear_logs(&state.path)
}

fn value_to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::String(format!("<blob {} bytes>", b.len())),
    }
}

#[tauri::command]
pub fn db_browser_tables(state: State<'_, DbState>) -> Result<Vec<DbTable>, String> {
    let conn = open_readonly(&state.path).map_err(|e| e.to_string())?;
    let tables = crate::db::table_names(&conn).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(tables.len());
    for t in tables {
        let count = crate::db::count_rows(&conn, &t).unwrap_or(0) as usize;
        out.push(DbTable {
            name: t,
            rows: count,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn db_browser_query(
    state: State<'_, DbState>,
    table: String,
    limit: usize,
    offset: Option<usize>,
) -> Result<DbTableData, String> {
    let conn = open_readonly(&state.path).map_err(|e| e.to_string())?;
    let safe = table.replace('"', "\"\"");
    let total: i64 = conn
        .query_row(&format!("SELECT COUNT(*) FROM \"{safe}\""), [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    let total_usize = total as usize;
    let limit = limit.clamp(1, 500);
    let offset = offset.unwrap_or(0).min(total_usize);
    // LIMIT/OFFSET via bound params to stay read-only and avoid injection; rowid ordering keeps pages stable
    let sql = format!("SELECT * FROM \"{safe}\" LIMIT ?1 OFFSET ?2");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows_iter = stmt
        .query_map(rusqlite::params![limit as i64, offset as i64], |row| {
            let mut vals = Vec::with_capacity(columns.len());
            for i in 0..columns.len() {
                vals.push(value_to_json(row.get_ref(i)?));
            }
            Ok(vals)
        })
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for r in rows_iter {
        rows.push(r.map_err(|e| e.to_string())?);
    }
    let returned = rows.len();
    Ok(DbTableData {
        name: table,
        columns,
        rows,
        total: total_usize,
        returned,
    })
}
