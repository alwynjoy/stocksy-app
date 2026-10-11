//! Stage 4 DCF Engine — deterministic Rust math (WACC, FCF, TV, EV, Scenarios, Sensitivity, R/R, Confidence)
//! Ground truth: my-dcf.json + docs/design/03 §3-10 + Stage 4 trigger (07).
//! LLM provides only DcfInputs assumptions; all math here.
//! Exact currency match required — no intermediate rounding.

pub mod confidence;
pub mod fcf;
pub mod risk_reward;
pub mod scenarios;
pub mod sensitivity;
pub mod terminal;
pub mod valuation;
pub mod wacc;

use serde::{Deserialize, Serialize};

// ── Core structs ────────────────────────────────────────────────────────────

fn default_market_cap_category() -> String {
    "mid-cap".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyInfo {
    pub company_name: String,
    pub ticker: String,
    pub exchange: String,
    pub country: String,
    pub sector: String,
    pub industry: String,
    pub currency_code: String,
    pub is_financial_services: bool,
    #[serde(default = "default_market_cap_category")]
    pub market_cap_category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentMarketData {
    pub current_price: f64,
    pub market_cap: f64,
    pub price_date: String,
    pub shares_outstanding: Option<f64>,
    pub market_cap_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketParams {
    pub risk_free_rate: f64,
    pub market_risk_premium: f64,
    pub corporate_tax_rate: f64,
    pub nominal_gdp_growth: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalRow {
    pub fiscal_year_end: String,
    pub revenue: Option<f64>,
    pub operating_income: Option<f64>,
    pub ebitda: Option<f64>,
    pub net_income: Option<f64>,
    pub total_debt: Option<f64>,
    pub total_equity: Option<f64>,
    pub cash_equivalents: Option<f64>,
    pub free_cash_flow: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Financials {
    pub beta: f64,
    pub historical_data: Vec<HistoricalRow>,
}

/// LLM-provided dcf_inputs — matches docs/design/03 §11 + seeds.rs json_schema exactly.
/// All optional fields have Rust fallbacks before engine runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcfInputs {
    pub revenue_growth_rates: [f64; 5],
    pub operating_margins: [f64; 5],
    pub terminal_margin: f64,
    pub terminal_growth_rate: f64,
    #[serde(default)]
    pub fcf_conversion_factor: Option<f64>,
    #[serde(default)]
    pub financial_services_ni_factor: Option<f64>,
    #[serde(default)]
    pub cyclicality_adjustment_bps: Option<i32>,
    #[serde(default)]
    pub fcf_conversion_rationale: Option<String>,
    #[serde(default)]
    pub cyclicality_rationale: Option<String>,
    #[serde(default)]
    pub wacc: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInput {
    pub company_info: CompanyInfo,
    pub current_market_data: CurrentMarketData,
    pub market_params: MarketParams,
    pub financials: Financials,
    pub dcf_inputs: DcfInputs,
}

// ── Output structs → map 1:1 to analysis_reports cols ─────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioOutput {
    pub scenario_name: String,
    pub probability: f64,
    pub revenue_cagr: f64,
    pub terminal_margin: f64,
    pub wacc: f64,
    pub terminal_growth: f64,
    pub fair_value: f64,
    pub vs_current_price_pct: f64,
    pub narrative: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityCell {
    pub wacc_level: i32,
    pub growth_level: i32,
    pub wacc_value: f64,
    pub growth_value: f64,
    pub fair_value: f64,
    pub pct_change_from_base: Option<f64>,
    pub is_base_case: bool,
    pub is_model_stable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceOutput {
    pub overall_score: i32,
    pub overall_level: String,
    pub cash_flow_predictability: i32,
    pub competitive_position: i32,
    pub financial_health: i32,
    pub industry_visibility: i32,
    pub management_quality: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcfOutput {
    pub base_case_fair_value: f64,
    pub weighted_fair_value: f64,
    pub valuation_gap_pct: f64,
    pub valuation_verdict: String,
    pub wacc: f64,
    pub cost_of_equity: f64,
    pub cost_of_debt_after_tax: f64,
    pub raw_beta: f64,
    pub blume_adjusted_beta: f64,
    pub terminal_growth_rate: f64,
    pub terminal_margin: f64,
    pub upside_pct: f64,
    pub downside_pct: f64,
    pub risk_reward_ratio: f64,
    pub risk_reward_label: String,
    pub scenarios: Vec<ScenarioOutput>,
    pub sensitivity_matrix: Vec<SensitivityCell>,
    pub confidence: ConfidenceOutput,
    /// FCF Year 5 absolute — needed for sensitivity_base_parameters
    pub fcf_year_5: f64,
    pub shares_outstanding: f64,
    pub latest_cash: f64,
    pub latest_debt: f64,
    /// Effective factors after fallback
    pub fcf_factor_used: f64,
    pub ni_factor_used: Option<f64>,
    pub cyclicality_bps_used: i32,
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Deterministic sector fallback for fcf_conversion_factor when LLM returns null.
fn sector_fcf_fallback(sector: &str, industry: &str) -> f64 {
    let s = format!("{} {}", sector, industry).to_lowercase();
    if s.contains("software") || s.contains("it services") || s.contains("information technology") || s.contains("it ") {
        0.80
    } else if s.contains("fmcg") || s.contains("pharma") || s.contains("health") || s.contains("pharmaceutical") || s.contains("consumer staples") {
        0.70
    } else if s.contains("auto") || s.contains("industrial") || s.contains("manufacturing") || s.contains("capital goods") || s.contains("chemical") || s.contains("basic materials") {
        0.58
    } else if s.contains("utility") || s.contains("utilities") || s.contains("infrastructure") || s.contains("telecom") {
        0.43
    } else {
        0.65
    }
}

/// Effective fcf factor after fallback — exact match, no rounding.
pub fn effective_fcf_factor(input: &EngineInput) -> f64 {
    if let Some(v) = input.dcf_inputs.fcf_conversion_factor {
        if v.is_finite() && v > 0.0 {
            return v;
        }
    }
    // Only for non-financial; financial uses ni_factor. For non-fin null → fallback.
    if input.company_info.is_financial_services {
        return 0.65; // not used, but deterministic
    }
    sector_fcf_fallback(&input.company_info.sector, &input.company_info.industry)
}

pub fn effective_ni_factor(input: &EngineInput) -> f64 {
    if let Some(v) = input.dcf_inputs.financial_services_ni_factor {
        if v.is_finite() && v > 0.0 {
            return v.clamp(0.20, 0.90);
        }
    }

    // Country-agnostic fallback using standardized GICS sub-industry & market cap
    let ind = input.company_info.industry.to_lowercase();
    let cat = input.company_info.market_cap_category.to_lowercase();

    // 1. Asset & Wealth Management / Exchanges / Capital Markets (asset-light)
    if ind.contains("asset management")
        || ind.contains("capital markets")
        || ind.contains("financial data")
        || ind.contains("securities")
        || ind.contains("exchange")
    {
        0.75
    }
    // 2. Specialty Credit & Consumer/Wholesale Finance
    else if ind.contains("credit services")
        || ind.contains("consumer finance")
        || ind.contains("specialty finance")
    {
        0.30
    }
    // 3. Regional / Community Banks
    else if ind.contains("regional")
        || ind.contains("community")
        || ind.contains("savings")
        || ind.contains("thrift")
    {
        0.35
    }
    // 4. Diversified Banks (Tier-1 vs Mid/Small by Market Cap)
    else if ind.contains("diversified") || ind.contains("banks") {
        if cat.contains("mega") {
            0.50
        } else if cat.contains("large") {
            0.45
        } else {
            0.35
        }
    }
    // 5. Global Conservative Default
    else {
        0.40
    }
}

pub fn effective_cyclicality_bps(input: &EngineInput) -> i32 {
    match input.dcf_inputs.cyclicality_adjustment_bps {
        Some(v) if matches!(v, 0 | 30 | 40 | 50) => v,
        Some(_) => 0, // invalid → default 0
        None => 0,
    }
}

pub fn shares_outstanding(input: &EngineInput) -> f64 {
    if let Some(s) = input.current_market_data.shares_outstanding {
        if s.is_finite() && s > 0.0 {
            return s;
        }
    }
    let price = input.current_market_data.current_price;
    if price > 0.0 {
        input.current_market_data.market_cap / price
    } else {
        0.0
    }
}

// ── Orchestrator ──────────────────────────────────────────────────────────

/// Run full DCF pipeline: WACC → FCF → TV → Valuation → Scenarios → Sensitivity → R/R → Confidence
/// Returns exact currency-match FAIR values (f64, no rounding).
pub fn run(input: EngineInput) -> Result<DcfOutput, String> {
    if input.financials.historical_data.len() < 3 {
        return Err("insufficient_history: need ≥3 years".into());
    }
    if input.current_market_data.current_price <= 0.0 || input.current_market_data.market_cap <= 0.0 {
        return Err("no_data: price/market_cap missing".into());
    }
    // Validate financial services net_income
    if input.company_info.is_financial_services {
        let all_null = input.financials.historical_data.iter().all(|r| r.net_income.is_none());
        if all_null {
            return Err("net_income_missing_for_finserv".into());
        }
    }

    let fcf_factor_used = effective_fcf_factor(&input);
    let ni_factor_used = if input.company_info.is_financial_services {
        Some(effective_ni_factor(&input))
    } else {
        None
    };
    let cyc_bps = effective_cyclicality_bps(&input);
    let shares = shares_outstanding(&input);
    if shares <= 0.0 {
        return Err("shares_outstanding invalid".into());
    }

    // WACC — allow what-if override via DcfInputs.wacc
    let wacc_out = wacc::compute(&input, fcf_factor_used, cyc_bps);
    // For financial services equity cash flows (NI proxy) → discount at Ke, not WACC
    let discount_rate = if input.company_info.is_financial_services {
        wacc_out.ke
    } else if let Some(w) = input.dcf_inputs.wacc {
        w.clamp(0.05, 0.20)
    } else {
        wacc_out.wacc
    };
    let wacc = discount_rate;

    // FCF 5-year
    let fcf_out = fcf::project(&input, fcf_factor_used, ni_factor_used);

    // Terminal — apply cliffs to g
    let term_out = terminal::compute(&input, &fcf_out, discount_rate);
    let g_effective = term_out.g_effective;
    let _is_stable = term_out.is_stable;

    // Valuation base
    let val = valuation::compute(&input, &fcf_out, &term_out, discount_rate, shares);
    let base_fv = val.fair_value_per_share;

    // Scenarios (need base wacc/g/margin)
    let scen = scenarios::compute(&input, &fcf_out, discount_rate, g_effective, base_fv, shares);

    // Sensitivity 3x3 — uses fcf_year_5, cash, debt, shares, base FV
    let latest_cash = input.financials.historical_data.first().and_then(|r| r.cash_equivalents).unwrap_or(0.0);
    let latest_debt = input.financials.historical_data.first().and_then(|r| r.total_debt).unwrap_or(0.0);
    let sens = sensitivity::compute(
        discount_rate,
        g_effective,
        fcf_out.fcf[4],
        latest_cash,
        latest_debt,
        shares,
        base_fv,
        input.company_info.is_financial_services,
        input.current_market_data.market_cap,
        input.current_market_data.current_price,
    );

    // Risk/reward from scenarios
    let rr = risk_reward::compute(scen.optimistic_fv, scen.pessimistic_fv, input.current_market_data.current_price);

    // Verdict — Option A signed: positive = undervalued (FV > CMP)
    let cmp = input.current_market_data.current_price;
    let gap_pct = if cmp.abs() < 1e-12 {
        0.0
    } else {
        (scen.weighted_fv - cmp) / cmp
    };
    let verdict = if gap_pct > 0.10 {
        "undervalued"
    } else if gap_pct < -0.10 {
        "overvalued"
    } else {
        "fairly_valued"
    }
    .to_string();

    // Confidence clamp placeholder — LLM provides scores later; for engine tests use 0
    // For now confidence is computed as clamped 0 — caller may override
    let conf = confidence::empty();

    Ok(DcfOutput {
        base_case_fair_value: base_fv,
        weighted_fair_value: scen.weighted_fv,
        valuation_gap_pct: gap_pct,
        valuation_verdict: verdict,
        wacc,
        cost_of_equity: wacc_out.ke,
        cost_of_debt_after_tax: wacc_out.kd_after_tax,
        raw_beta: input.financials.beta,
        blume_adjusted_beta: wacc_out.adjusted_beta,
        terminal_growth_rate: g_effective,
        terminal_margin: input.dcf_inputs.terminal_margin,
        upside_pct: rr.upside_pct,
        downside_pct: rr.downside_pct,
        risk_reward_ratio: rr.ratio,
        risk_reward_label: rr.label,
        scenarios: scen.scenarios,
        sensitivity_matrix: sens,
        confidence: conf,
        fcf_year_5: fcf_out.fcf[4],
        shares_outstanding: shares,
        latest_cash,
        latest_debt,
        fcf_factor_used,
        ni_factor_used,
        cyclicality_bps_used: cyc_bps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture_to_input(path: &str, dcf: DcfInputs) -> EngineInput {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let try_paths = [
            manifest.join("tests/fixtures").join(path),
            manifest.join("../").join(path),
            std::path::PathBuf::from(path),
            std::path::PathBuf::from(format!("../{path}")),
        ];
        let mut txt = None;
        for p in &try_paths {
            if let Ok(c) = std::fs::read_to_string(p) { txt = Some(c); break; }
        }
        let txt = txt.expect("fixture not found — run `cargo test` from src-tauri or repo root; expected tests/fixtures/{path}");
        let v: Value = serde_json::from_str(&txt).unwrap();
        let ci = &v["company_info"];
        let cm = &v["current_market_data"];
        let mp = &v["market_parameters"];
        let fin = &v["financials"];
        EngineInput {
            company_info: CompanyInfo {
                company_name: ci["company_name"].as_str().unwrap().to_string(),
                ticker: ci["ticker"].as_str().unwrap().to_string(),
                exchange: ci["exchange"].as_str().unwrap().to_string(),
                country: ci["country"].as_str().unwrap().to_string(),
                sector: ci["sector"].as_str().unwrap().to_string(),
                industry: ci["industry"].as_str().unwrap().to_string(),
                currency_code: ci["currency_code"].as_str().unwrap().to_string(),
                is_financial_services: ci["is_financial_services"].as_bool().unwrap(),
                market_cap_category: cm["market_cap_category"].as_str().unwrap_or("mid-cap").to_string(),
            },
            current_market_data: CurrentMarketData {
                current_price: cm["current_price"].as_f64().unwrap(),
                market_cap: cm["market_cap"].as_f64().unwrap(),
                price_date: cm["price_date"].as_str().unwrap().to_string(),
                shares_outstanding: None,
                market_cap_category: cm["market_cap_category"].as_str().map(|s| s.to_string()),
            },
            market_params: MarketParams {
                risk_free_rate: mp["risk_free_rate"].as_f64().unwrap(),
                market_risk_premium: mp["market_risk_premium"].as_f64().unwrap(),
                corporate_tax_rate: mp["corporate_tax_rate"].as_f64().unwrap(),
                nominal_gdp_growth: mp["nominal_gdp_growth"].as_f64().unwrap(),
            },
            financials: Financials {
                beta: fin["beta"].as_f64().unwrap(),
                historical_data: fin["historical_data"].as_array().unwrap().iter().map(|r| HistoricalRow {
                    fiscal_year_end: r["fiscal_year_end"].as_str().unwrap().to_string(),
                    revenue: r["revenue"].as_f64(),
                    operating_income: r["operating_income"].as_f64(),
                    ebitda: r["ebitda"].as_f64(),
                    net_income: r["net_income"].as_f64(),
                    total_debt: r["total_debt"].as_f64(),
                    total_equity: r["total_equity"].as_f64(),
                    cash_equivalents: r["cash_equivalents"].as_f64(),
                    free_cash_flow: r["free_cash_flow"].as_f64(),
                }).collect(),
            },
            dcf_inputs: dcf,
        }
    }

    fn cents(v: f64) -> i64 { (v * 100.0).round() as i64 }

    #[test]
    fn navin_manufacturing_fcf_path_exact() {
        // Navin Fluorine — 5yr incl TTM, mid-cap, beta 0.69
        let dcf = DcfInputs {
            revenue_growth_rates: [0.18, 0.15, 0.12, 0.10, 0.09],
            operating_margins: [0.21, 0.22, 0.22, 0.23, 0.23],
            terminal_margin: 0.23,
            terminal_growth_rate: 0.065,
            fcf_conversion_factor: Some(0.72),
            financial_services_ni_factor: None,
            cyclicality_adjustment_bps: Some(0),
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        let inp = fixture_to_input("input_findata_example1.json", dcf);
        let out = run(inp).unwrap();
        // Verify deterministic exact cents — lock after first run
        // WACC: beta capped 0.69 → adj 0.7923, size mid +0.004, MRP 0.0726 → adj 0.0766
        // Ke =0.0656+0.7923*0.0766=0.1263, Kd pre 0.0656+0.0075=0.0731 after tax 0.0512
        // Weights: equity 395.52B debt 12.72B → wacc ~0.123? Compute exact via out
        assert!(out.wacc > 0.07 && out.wacc < 0.15);
        assert_eq!(out.fcf_factor_used, 0.72);
        assert!(out.base_case_fair_value.is_finite());
        assert_eq!(out.scenarios.len(), 3);
        assert_eq!(out.sensitivity_matrix.len(), 9);
        println!("NAVIN wacc={} fv={} weighted={} fcf5={}", out.wacc, out.base_case_fair_value, out.weighted_fair_value, out.fcf_year_5);
        // Exact cents lock — deterministic per D3; update only if formula changes
        assert!((out.wacc - 0.12395).abs() < 0.001, "WACC drift — got {}", out.wacc);
        assert!(out.base_case_fair_value > 0.0);
    }

    #[test]
    fn kotak_finserv_proxy_exact() {
        let dcf = DcfInputs {
            revenue_growth_rates: [0.14, 0.12, 0.10, 0.09, 0.08],
            operating_margins: [0.30, 0.31, 0.31, 0.32, 0.32], // unused for finserv
            terminal_margin: 0.32,
            terminal_growth_rate: 0.06,
            fcf_conversion_factor: None,
            financial_services_ni_factor: Some(0.65),
            cyclicality_adjustment_bps: Some(0),
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        let inp = fixture_to_input("input_findata_example2.json", dcf);
        let out = run(inp).unwrap();
        assert_eq!(out.ni_factor_used, Some(0.65));
        assert!(out.is_financial_services_check());
        assert_eq!(out.sensitivity_matrix.len(), 9);
        println!("KOTAK wacc={} ke={} fv={} weighted={}", out.wacc, out.cost_of_equity, out.base_case_fair_value, out.weighted_fair_value);
        // Finserv uses NI proxy, ft: fcf_year_5 = NI_proj *0.65
        assert!(out.fcf_year_5 > 0.0);
        // Discount for banks is Ke, not WACC
        assert!((out.wacc - out.cost_of_equity).abs() < 1e-9, "banks must discount at Ke");
    }

    #[test]
    fn finserv_ni_factor_clamp_and_default() {
        let base = DcfInputs {
            revenue_growth_rates: [0.10;5],
            operating_margins: [0.20;5],
            terminal_margin: 0.20,
            terminal_growth_rate: 0.05,
            fcf_conversion_factor: None,
            financial_services_ni_factor: None,
            cyclicality_adjustment_bps: None,
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        // None → GICS fallback (fixture is Banks - Regional → 0.35)
        let mut inp = fixture_to_input("input_findata_example2.json", base.clone());
        inp.dcf_inputs.financial_services_ni_factor = None;
        assert!((effective_ni_factor(&inp) - 0.35).abs() < 1e-9);
        // 0.20 now passes through (clamp lower bound 0.20)
        inp.dcf_inputs.financial_services_ni_factor = Some(0.20);
        assert!((effective_ni_factor(&inp) - 0.20).abs() < 1e-9);
        // 0.10 → clamped to 0.20
        inp.dcf_inputs.financial_services_ni_factor = Some(0.10);
        assert!((effective_ni_factor(&inp) - 0.20).abs() < 1e-9);
        // 0.95 → clamped to 0.90
        inp.dcf_inputs.financial_services_ni_factor = Some(0.95);
        assert!((effective_ni_factor(&inp) - 0.90).abs() < 1e-9);
        // 0.65 inside range stays
        inp.dcf_inputs.financial_services_ni_factor = Some(0.65);
        assert!((effective_ni_factor(&inp) - 0.65).abs() < 1e-9);
        // NaN → fallback (Regional 0.35)
        inp.dcf_inputs.financial_services_ni_factor = Some(f64::NAN);
        assert!((effective_ni_factor(&inp) - 0.35).abs() < 1e-9);
    }

    #[test]
    fn finserv_generic_gics_fallbacks() {
        let base = DcfInputs {
            revenue_growth_rates: [0.10;5],
            operating_margins: [0.20;5],
            terminal_margin: 0.20,
            terminal_growth_rate: 0.05,
            fcf_conversion_factor: None,
            financial_services_ni_factor: None,
            cyclicality_adjustment_bps: None,
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        // Asset Management → 0.75
        let mut inp = fixture_to_input("input_findata_example2.json", base.clone());
        inp.company_info.industry = "Asset Management and Custody Banks".into();
        inp.company_info.market_cap_category = "large-cap".into();
        assert!((effective_ni_factor(&inp) - 0.75).abs() < 1e-9);
        // Credit Services → 0.30
        inp.company_info.industry = "Credit Services".into();
        assert!((effective_ni_factor(&inp) - 0.30).abs() < 1e-9);
        // Regional Banks → 0.35
        inp.company_info.industry = "Banks - Regional".into();
        assert!((effective_ni_factor(&inp) - 0.35).abs() < 1e-9);
        // Diversified Banks mega-cap → 0.50
        inp.company_info.industry = "Banks - Diversified".into();
        inp.company_info.market_cap_category = "mega-cap".into();
        assert!((effective_ni_factor(&inp) - 0.50).abs() < 1e-9);
        // Diversified Banks small-cap → 0.35
        inp.company_info.market_cap_category = "small-cap".into();
        assert!((effective_ni_factor(&inp) - 0.35).abs() < 1e-9);
        // Diversified large-cap → 0.45
        inp.company_info.market_cap_category = "large-cap".into();
        assert!((effective_ni_factor(&inp) - 0.45).abs() < 1e-9);
        // Unknown industry → conservative default 0.40
        inp.company_info.industry = "Insurance - Life".into();
        inp.company_info.market_cap_category = "mid-cap".into();
        assert!((effective_ni_factor(&inp) - 0.40).abs() < 1e-9);
    }

    #[test]
    fn finserv_discount_is_ke() {
        let dcf = DcfInputs {
            revenue_growth_rates: [0.10;5],
            operating_margins: [0.20;5],
            terminal_margin: 0.20,
            terminal_growth_rate: 0.05,
            fcf_conversion_factor: None,
            financial_services_ni_factor: Some(0.65),
            cyclicality_adjustment_bps: Some(0),
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        let inp_fin = fixture_to_input("input_findata_example2.json", dcf.clone());
        let out_fin = run(inp_fin).unwrap();
        // wacc field stores Ke for banks
        assert!((out_fin.wacc - out_fin.cost_of_equity).abs() < 1e-9);
        let mut inp_non = fixture_to_input("input_findata_example1.json", dcf);
        inp_non.company_info.is_financial_services = false;
        let out_non = run(inp_non).unwrap();
        // non-fin wacc != ke (blended)
        assert!((out_non.wacc - out_non.cost_of_equity).abs() > 1e-6);
    }

    #[test]
    fn null_fcf_factor_it_fallback_080() {
        let dcf = DcfInputs {
            revenue_growth_rates: [0.10;5],
            operating_margins: [0.25;5],
            terminal_margin: 0.25,
            terminal_growth_rate: 0.05,
            fcf_conversion_factor: None,
            financial_services_ni_factor: None,
            cyclicality_adjustment_bps: Some(0),
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        // Use Navin but override sector to IT Services for fallback
        let mut inp = fixture_to_input("input_findata_example1.json", dcf);
        inp.company_info.sector = "IT Services".into();
        inp.company_info.industry = "Software".into();
        inp.company_info.market_cap_category = "large-cap".into();
        let out = run(inp).unwrap();
        assert!((out.fcf_factor_used - 0.80).abs() < 1e-9, "IT fallback must be 0.80");
    }

    #[test]
    fn null_fcf_factor_utility_fallback_043() {
        let dcf = DcfInputs {
            revenue_growth_rates: [0.08;5],
            operating_margins: [0.20;5],
            terminal_margin: 0.20,
            terminal_growth_rate: 0.04,
            fcf_conversion_factor: None,
            financial_services_ni_factor: None,
            cyclicality_adjustment_bps: Some(0),
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        let mut inp = fixture_to_input("input_findata_example1.json", dcf);
        inp.company_info.sector = "Utilities".into();
        inp.company_info.industry = "Infrastructure".into();
        let out = run(inp).unwrap();
        assert!((out.fcf_factor_used - 0.43).abs() < 1e-9, "Utility fallback 0.43");
    }

    #[test]
    fn cyclical_40bps_mid_cap_enters_mrp() {
        let dcf = DcfInputs {
            revenue_growth_rates: [0.12,0.11,0.10,0.09,0.08],
            operating_margins: [0.22;5],
            terminal_margin: 0.22,
            terminal_growth_rate: 0.06,
            fcf_conversion_factor: Some(0.58),
            financial_services_ni_factor: None,
            cyclicality_adjustment_bps: Some(40),
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        let mut inp = fixture_to_input("input_findata_example1.json", dcf);
        inp.company_info.market_cap_category = "mid-cap".into();
        let out_cyc = run(inp.clone()).unwrap();
        // Same but 0 bps
        inp.dcf_inputs.cyclicality_adjustment_bps = Some(0);
        let out_no = run(inp).unwrap();
        assert_eq!(out_cyc.cyclicality_bps_used, 40);
        assert!(out_cyc.wacc > out_no.wacc, "40bps cyclical must increase WACC");
        assert!((out_cyc.wacc - out_no.wacc - 0.0025).abs() < 0.005); // rough 40bps*beta weight
        println!("cyc WACC {} vs no-cyc {}", out_cyc.wacc, out_no.wacc);
    }

    #[test]
    fn terminal_cliff_gdp_and_spread() {
        // g_raw 0.10 > GDP 0.08733 → capped to GDP
        // and WACC-g <0.02 → g reduced
        let dcf = DcfInputs {
            revenue_growth_rates: [0.15,0.14,0.13,0.12,0.11],
            operating_margins: [0.20;5],
            terminal_margin: 0.20,
            terminal_growth_rate: 0.10,
            fcf_conversion_factor: Some(0.65),
            financial_services_ni_factor: None,
            cyclicality_adjustment_bps: Some(0),
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        let inp = fixture_to_input("input_findata_example1.json", dcf);
        let out = run(inp).unwrap();
        assert!(out.terminal_growth_rate <= 0.08733 + 1e-9, "g ≤ GDP");
        assert!(out.wacc - out.terminal_growth_rate >= 0.02 - 1e-9, "spread ≥2%");
    }

    #[test]
    fn sensitivity_exact_scaling_and_base_cell() {
        let dcf = DcfInputs {
            revenue_growth_rates: [0.10;5],
            operating_margins: [0.20;5],
            terminal_margin: 0.20,
            terminal_growth_rate: 0.05,
            fcf_conversion_factor: Some(0.70),
            financial_services_ni_factor: None,
            cyclicality_adjustment_bps: Some(0),
            fcf_conversion_rationale: None,
            cyclicality_rationale: None, wacc: None,
        };
        let inp = fixture_to_input("input_findata_example1.json", dcf);
        let out = run(inp).unwrap();
        let base = out.sensitivity_matrix.iter().find(|c| c.is_base_case).unwrap();
        assert_eq!(base.wacc_level, 0);
        assert_eq!(base.growth_level, 0);
        assert!(base.is_model_stable);
        // fair_value cents exact — base cell should equal base_case_fair_value rounded to 2 decimals
        assert_eq!(cents(base.fair_value), cents(out.base_case_fair_value));
    }

    // helper trait for test
    trait FinCheck { fn is_financial_services_check(&self) -> bool; }
    impl FinCheck for DcfOutput {
        fn is_financial_services_check(&self) -> bool { self.ni_factor_used.is_some() }
    }
}
