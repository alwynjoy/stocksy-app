//! Terminal Value — Gordon Growth with cliffs: g ≤ GDP, g ≥ Y5-2.5%, WACC-g ≥2%

use super::{EngineInput, fcf::FcfOutput};

#[derive(Debug, Clone)]
pub struct TerminalOutput {
    pub g_effective: f64,
    pub g_raw: f64,
    pub is_stable: bool,
    pub terminal_fcf: f64,
    pub terminal_value: f64,
    pub fcf_year_5: f64,
}

pub fn compute(input: &EngineInput, fcf_out: &FcfOutput, wacc: f64) -> TerminalOutput {
    let g_raw = input.dcf_inputs.terminal_growth_rate;
    let gdp = input.market_params.nominal_gdp_growth;
    let y5_growth = input.dcf_inputs.revenue_growth_rates[4];

    // Cliff 1: g ≤ GDP (base case cannot exceed GDP)
    let mut g = g_raw.min(gdp);
    // Cliff 2: g ≥ Y5 - 2.5%
    let floor = y5_growth - 0.025;
    if g < floor {
        g = floor;
    }
    // Cliff 3: WACC - g ≥ 2% → reduce g
    let mut is_stable = true;
    if wacc - g < 0.02 {
        g = wacc - 0.02;
        is_stable = false;
    }
    // Ensure g not negative crazy
    if g < 0.0 {
        g = 0.0;
    }

    let fcf5 = fcf_out.fcf[4];
    let term_fcf = fcf5 * (1.0 + g);
    let tv = if (wacc - g).abs() > 1e-12 {
        term_fcf / (wacc - g)
    } else {
        0.0
    };

    TerminalOutput { g_effective: g, g_raw, is_stable, terminal_fcf: term_fcf, terminal_value: tv, fcf_year_5: fcf5 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fcf::FcfOutput;
    use crate::engine::{CompanyInfo, CurrentMarketData, DcfInputs, EngineInput, Financials, HistoricalRow, MarketParams};

    fn mk(g_raw: f64, gdp: f64, y5: f64, _wacc: f64, fcf5: f64) -> (EngineInput, FcfOutput) {
        let inp = EngineInput {
            company_info: CompanyInfo { company_name:"T".into(), ticker:"T".into(), exchange:"NSE".into(), country:"India".into(), sector:"IT".into(), industry:"Soft".into(), currency_code:"INR".into(), is_financial_services:false, market_cap_category:"large-cap".into() },
            current_market_data: CurrentMarketData { current_price:100.0, market_cap:1e11, price_date:"01/01/2026".into(), shares_outstanding:Some(1e9), market_cap_category:None },
            market_params: MarketParams { risk_free_rate:0.0656, market_risk_premium:0.0726, corporate_tax_rate:0.30, nominal_gdp_growth: gdp },
            financials: Financials { beta:1.0, historical_data: vec![
                HistoricalRow { fiscal_year_end:"03/2026".into(), revenue:Some(1e10), operating_income:Some(2e9), ebitda:Some(2e9), net_income:Some(1e9), total_debt:Some(1e9), total_equity:Some(5e9), cash_equivalents:Some(5e8), free_cash_flow:Some(1e9)},
                HistoricalRow { fiscal_year_end:"03/2025".into(), revenue:Some(9e9), operating_income:Some(1.8e9), ebitda:Some(2e9), net_income:Some(9e8), total_debt:Some(1e9), total_equity:Some(4.5e9), cash_equivalents:Some(4e8), free_cash_flow:Some(9e8)},
                HistoricalRow { fiscal_year_end:"03/2024".into(), revenue:Some(8e9), operating_income:Some(1.6e9), ebitda:Some(1.8e9), net_income:Some(8e8), total_debt:Some(1e9), total_equity:Some(4e9), cash_equivalents:Some(3e8), free_cash_flow:Some(8e8)},
            ]},
            dcf_inputs: DcfInputs { revenue_growth_rates:[0.05,0.05,0.05,0.05,y5], operating_margins:[0.2;5], terminal_margin:0.2, terminal_growth_rate: g_raw, fcf_conversion_factor:Some(0.7), financial_services_ni_factor:None, cyclicality_adjustment_bps:Some(0), fcf_conversion_rationale:None, cyclicality_rationale: None, wacc: None },
        };
        let fcf = FcfOutput { revenue:[0.0;5], operating_income:[0.0;5], fcf:[1e8,1e8,1e8,1e8,fcf5], revenue_cagr_5y:0.0, fcf_cagr_5y:0.0 };
        (inp, fcf)
    }

    #[test]
    fn cliff_gdp_cap() {
        let (inp, fcf) = mk(0.10, 0.08733, 0.05, 0.12, 1e9);
        let out = compute(&inp, &fcf, 0.12);
        assert!((out.g_effective - 0.08733).abs() < 1e-9);
    }

    #[test]
    fn cliff_floor() {
        let (inp, fcf) = mk(0.01, 0.08733, 0.05, 0.12, 1e9);
        let out = compute(&inp, &fcf, 0.12);
        // floor = 0.05-0.025=0.025 → g should be max(0.01,0.025)=0.025
        assert!((out.g_effective - 0.025).abs() < 1e-9);
    }

    #[test]
    fn cliff_spread_200bps() {
        let (inp, fcf) = mk(0.11, 0.08733, 0.05, 0.12, 1e9);
        // g capped to GDP 0.08733, Wacc-g=0.0327 >0.02 so stable
        let out = compute(&inp, &fcf, 0.08);
        // gdp 0.08733 but wacc 0.08 → wacc-g = -0.007 → reduce g to 0.06
        assert!((out.g_effective - 0.06).abs() < 1e-9);
        assert!(!out.is_stable);
    }
}
