//! WACC — beta cap, Blume, size premium, capital-structure penalty, cyclicality, credit spread, Kd, weights.
//! Exact deterministic per docs/design/03 §3 + my-dcf.json calc framework.

use super::EngineInput;

#[derive(Debug, Clone)]
pub struct WaccOutput {
    pub wacc: f64,
    pub ke: f64,
    pub kd_after_tax: f64,
    pub kd_pre_tax: f64,
    pub adjusted_beta: f64,
    pub base_mrp: f64,
    pub adjusted_mrp: f64,
    pub size_adj: f64,
    pub debt_adj: f64,
    pub cyclicality_adj: f64,
    pub credit_spread: f64,
    pub leverage: f64,
}

/// Size premium deterministic midpoints (bps → decimal):
/// mega -65bps (-0.0065), large 0, mid +40bps (+0.004), small +75bps (+0.0075), micro +125bps (+0.0125)
/// Applied universally including financial services (funding cost / deposit volatility).
fn size_premium(category: &str, _is_finserv: bool) -> f64 {
    let c = category.to_lowercase();
    if c.contains("mega") {
        -0.0065
    } else if c.contains("large") {
        0.0
    } else if c.contains("mid") {
        0.0040
    } else if c.contains("small") {
        0.0075
    } else if c.contains("micro") {
        0.0125
    } else {
        0.0
    }
}

fn credit_spread(leverage: f64) -> f64 {
    if !leverage.is_finite() {
        return 0.0125;
    }
    if leverage < 1.5 {
        0.0075
    } else if leverage < 3.0 {
        0.0125
    } else if leverage < 4.5 {
        0.0200
    } else if leverage < 6.0 {
        0.0325
    } else {
        0.0500
    }
}

pub fn compute(input: &EngineInput, _fcf_factor_used: f64, cyc_bps: i32) -> WaccOutput {
    // Step 1: cap raw beta
    let raw = input.financials.beta;
    let capped = raw.clamp(0.30, 2.00);
    let adjusted_beta = 0.67 * capped + 0.33 * 1.0;

    // Base MRP
    let base_mrp = input.market_params.market_risk_premium;

    // Size adjustment
    let size_adj = size_premium(&input.company_info.market_cap_category, input.company_info.is_financial_services);

    // Capital structure penalty: +50bps if Debt/MCap >50%
    let debt = input.financials.historical_data.first().and_then(|r| r.total_debt).unwrap_or(0.0);
    let mcap = input.current_market_data.market_cap;
    let debt_adj = if mcap > 0.0 && debt / mcap > 0.50 { 0.005 } else { 0.0 };
    let debt_adj = if input.company_info.is_financial_services { 0.0 } else { debt_adj };

    // Cyclicality
    let cyc_adj = (cyc_bps as f64) / 10000.0;
    let cyc_adj = if input.company_info.is_financial_services { 0.0 } else { cyc_adj };

    let adjusted_mrp = base_mrp + size_adj + debt_adj + cyc_adj;

    // Ke
    let ke_raw = input.market_params.risk_free_rate + adjusted_beta * adjusted_mrp;
    let ke = ke_raw.clamp(0.08, 0.20);

    // Leverage → spread → Kd
    let latest_ebitda = input.financials.historical_data.first()
        .and_then(|r| r.ebitda)
        .or_else(|| input.financials.historical_data.first().and_then(|r| r.operating_income.map(|v| v * 1.15)))
        .unwrap_or(0.0);
    let leverage = if latest_ebitda.abs() > 1e-9 { debt / latest_ebitda } else { 99.0 };
    let spread = credit_spread(leverage);
    let kd_pre = input.market_params.risk_free_rate + spread;
    let kd_after = kd_pre * (1.0 - input.market_params.corporate_tax_rate);

    // Weights
    let equity = mcap;
    let total = equity + debt;
    let w_e = if total > 0.0 { equity / total } else { 1.0 };
    let w_d = if total > 0.0 { debt / total } else { 0.0 };

    let wacc_raw = w_e * ke + w_d * kd_after;
    // Target clamp 7-15% - but we keep exact for tests; clamped wacc used for valuation
    let wacc = wacc_raw.clamp(0.07, 0.15);

    WaccOutput {
        wacc,
        ke,
        kd_after_tax: kd_after,
        kd_pre_tax: kd_pre,
        adjusted_beta,
        base_mrp,
        adjusted_mrp,
        size_adj,
        debt_adj,
        cyclicality_adj: cyc_adj,
        credit_spread: spread,
        leverage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{CompanyInfo, CurrentMarketData, DcfInputs, EngineInput, Financials, HistoricalRow, MarketParams};

    fn mk_input(beta: f64, cat: &str, mcap: f64, debt: f64, ebitda: f64, is_fin: bool, cyc: i32) -> EngineInput {
        EngineInput {
            company_info: CompanyInfo {
                company_name: "T".into(), ticker: "T".into(), exchange: "NSE".into(),
                country: "India".into(), sector: "IT Services".into(), industry: "Software".into(),
                currency_code: "INR".into(), is_financial_services: is_fin, market_cap_category: cat.into(),
            },
            current_market_data: CurrentMarketData { current_price: 100.0, market_cap: mcap, price_date: "01/01/2026".into(), shares_outstanding: None, market_cap_category: None },
            market_params: MarketParams { risk_free_rate: 0.0656, market_risk_premium: 0.0726, corporate_tax_rate: 0.30, nominal_gdp_growth: 0.08733 },
            financials: Financials { beta, historical_data: vec![
                HistoricalRow { fiscal_year_end: "03/2026".into(), revenue: Some(1e9), operating_income: Some(2e8), ebitda: Some(ebitda), net_income: Some(1e8), total_debt: Some(debt), total_equity: Some(1e9), cash_equivalents: Some(1e8), free_cash_flow: Some(1e8) },
                HistoricalRow { fiscal_year_end: "03/2025".into(), revenue: Some(9e8), operating_income: Some(1.8e8), ebitda: Some(ebitda), net_income: Some(9e7), total_debt: Some(debt), total_equity: Some(9e8), cash_equivalents: Some(9e7), free_cash_flow: Some(9e7) },
                HistoricalRow { fiscal_year_end: "03/2024".into(), revenue: Some(8e8), operating_income: Some(1.6e8), ebitda: Some(ebitda), net_income: Some(8e7), total_debt: Some(debt), total_equity: Some(8e8), cash_equivalents: Some(8e7), free_cash_flow: Some(8e7) },
            ]},
            dcf_inputs: DcfInputs { revenue_growth_rates: [0.1;5], operating_margins: [0.2;5], terminal_margin: 0.2, terminal_growth_rate: 0.05, fcf_conversion_factor: Some(0.7), financial_services_ni_factor: None, cyclicality_adjustment_bps: Some(cyc), fcf_conversion_rationale: None, cyclicality_rationale: None, wacc: None },
        }
    }

    #[test]
    fn beta_cap_and_blume() {
        let inp = mk_input(3.0, "large-cap", 1e11, 1e9, 5e9, false, 0);
        let out = compute(&inp, 0.7, 0);
        // capped 2.0 → adj = 0.67*2+0.33=1.67
        assert!((out.adjusted_beta - 1.67).abs() < 1e-9);
        let inp2 = mk_input(0.1, "large-cap", 1e11, 1e9, 5e9, false, 0);
        let out2 = compute(&inp2, 0.7, 0);
        assert!((out2.adjusted_beta - (0.67*0.3+0.33)).abs() < 1e-9);
    }

    #[test]
    fn size_premium_mid() {
        let inp = mk_input(1.0, "mid-cap", 1e11, 1e9, 5e9, false, 0);
        let out = compute(&inp, 0.7, 0);
        assert!((out.size_adj - 0.004).abs() < 1e-9);
    }

    #[test]
    fn cyclicality_40bps_enters_mrp() {
        // mid-cap cyclical with beta 1.0 → adjBeta 1.0, base MRP 0.0726 + size 0.004 + cyc 0.004 = 0.0806
        let inp = mk_input(1.0, "mid-cap", 3.95e11, 1.27e10, 1.14e10, false, 40);
        let out = compute(&inp, 0.7, 40);
        assert!((out.cyclicality_adj - 0.0040).abs() < 1e-9);
        assert!((out.adjusted_mrp - (0.0726+0.004+0.004)).abs() < 1e-9);
        let adj_beta = 0.67*1.0+0.33; // 1.0
        let ke_exp = (0.0656_f64 + adj_beta*0.0806_f64).clamp(0.08,0.20);
        assert!((out.ke - ke_exp).abs() < 1e-6, "ke {} vs exp {}", out.ke, ke_exp);
    }

    #[test]
    fn debt_penalty_over_50pct() {
        // mcap 1e9 debt 6e8 → 60% → penalty
        let inp = mk_input(1.0, "large-cap", 1e9, 600_000_000.0, 1e9, false, 0);
        let out = compute(&inp, 0.7, 0);
        assert!((out.debt_adj - 0.005).abs() < 1e-9);
    }

    #[test]
    fn credit_spread_table() {
        assert!((credit_spread(1.0)-0.0075).abs() < 1e-9);
        assert!((credit_spread(2.0)-0.0125).abs() < 1e-9);
        assert!((credit_spread(4.0)-0.0200).abs() < 1e-9);
        assert!((credit_spread(5.0)-0.0325).abs() < 1e-9);
        assert!((credit_spread(7.0)-0.0500).abs() < 1e-9);
    }

    #[test]
    fn finserv_size_premium_applied() {
        // Small-cap finserv should now get +75bps (universal)
        let inp_small = mk_input(1.0, "small-cap", 1e11, 1e9, 5e9, true, 0);
        let out_small = compute(&inp_small, 0.7, 0);
        assert!((out_small.size_adj - 0.0075).abs() < 1e-9, "small-cap finserv must get +75bps");
        let inp_mid = mk_input(1.0, "mid-cap", 1e11, 1e9, 5e9, true, 0);
        let out_mid = compute(&inp_mid, 0.7, 0);
        assert!((out_mid.size_adj - 0.0040).abs() < 1e-9, "mid-cap finserv must get +40bps");
        let inp_mega = mk_input(1.0, "mega-cap", 1e11, 1e9, 5e9, true, 0);
        let out_mega = compute(&inp_mega, 0.7, 0);
        assert!((out_mega.size_adj - (-0.0065)).abs() < 1e-9, "mega-cap finserv must get -65bps");
        // Also verify Ke reflects it (size premium flows to MRP)
        let inp_large = mk_input(1.0, "large-cap", 1e11, 1e9, 5e9, true, 0);
        let out_large = compute(&inp_large, 0.7, 0);
        assert!((out_large.size_adj - 0.0).abs() < 1e-9);
        assert!(out_small.ke > out_large.ke, "small-cap Ke must be higher than large-cap for same beta");
    }
}
