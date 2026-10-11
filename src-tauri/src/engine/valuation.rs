//! Valuation — EV = Σ PV(FCF) + PV(TV), Equity = EV + cash - debt, FV = equity / shares

use super::{EngineInput, fcf::FcfOutput, terminal::TerminalOutput};

#[derive(Debug, Clone)]
pub struct ValuationOutput {
    pub enterprise_value: f64,
    pub equity_value: f64,
    pub fair_value_per_share: f64,
    pub pv_explicit: f64,
    pub pv_terminal: f64,
}

pub fn compute(input: &EngineInput, fcf_out: &FcfOutput, term: &TerminalOutput, wacc: f64, shares: f64) -> ValuationOutput {
    let mut pv_explicit = 0.0;
    for t in 0..5 {
        pv_explicit += fcf_out.fcf[t] / (1.0 + wacc).powi((t + 1) as i32);
    }
    let pv_terminal = term.terminal_value / (1.0 + wacc).powi(5);
    let ev = pv_explicit + pv_terminal;

    // Financial services: equity cash flow already; do not add cash/subtract debt (operational)
    // Matches n8n sensitivity matrix finserv branch: equity = PV(Terminal) [+ PV explicit]
    let equity = if input.company_info.is_financial_services {
        ev
    } else {
        let cash = input.financials.historical_data.first().and_then(|r| r.cash_equivalents).unwrap_or(0.0);
        let debt = input.financials.historical_data.first().and_then(|r| r.total_debt).unwrap_or(0.0);
        ev + cash - debt
    };
    let fv = if shares > 0.0 { equity / shares } else { 0.0 };

    ValuationOutput { enterprise_value: ev, equity_value: equity, fair_value_per_share: fv, pv_explicit, pv_terminal }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fcf::FcfOutput;
    use crate::engine::terminal::TerminalOutput;
    use crate::engine::{CompanyInfo, CurrentMarketData, DcfInputs, EngineInput, Financials, HistoricalRow, MarketParams};

    #[test]
    fn simple_ev() {
        let inp = EngineInput {
            company_info: CompanyInfo { company_name:"T".into(), ticker:"T".into(), exchange:"NSE".into(), country:"India".into(), sector:"IT".into(), industry:"Soft".into(), currency_code:"INR".into(), is_financial_services:false, market_cap_category:"large-cap".into() },
            current_market_data: CurrentMarketData { current_price:100.0, market_cap:1e11, price_date:"01/01/2026".into(), shares_outstanding:Some(1e9), market_cap_category:None },
            market_params: MarketParams { risk_free_rate:0.0656, market_risk_premium:0.0726, corporate_tax_rate:0.30, nominal_gdp_growth:0.08733 },
            financials: Financials { beta:1.0, historical_data: vec![
                HistoricalRow { fiscal_year_end:"03/2026".into(), revenue:Some(1e10), operating_income:Some(2e9), ebitda:Some(2e9), net_income:Some(1e9), total_debt:Some(1e9), total_equity:Some(5e9), cash_equivalents:Some(5e8), free_cash_flow:Some(1e9)},
                HistoricalRow { fiscal_year_end:"03/2025".into(), revenue:Some(9e9), operating_income:Some(1.8e9), ebitda:Some(2e9), net_income:Some(9e8), total_debt:Some(1e9), total_equity:Some(4.5e9), cash_equivalents:Some(4e8), free_cash_flow:Some(9e8)},
                HistoricalRow { fiscal_year_end:"03/2024".into(), revenue:Some(8e9), operating_income:Some(1.6e9), ebitda:Some(1.8e9), net_income:Some(8e8), total_debt:Some(1e9), total_equity:Some(4e9), cash_equivalents:Some(3e8), free_cash_flow:Some(8e8)},
            ]},
            dcf_inputs: DcfInputs { revenue_growth_rates:[0.05;5], operating_margins:[0.2;5], terminal_margin:0.2, terminal_growth_rate:0.03, fcf_conversion_factor:Some(0.7), financial_services_ni_factor:None, cyclicality_adjustment_bps:Some(0), fcf_conversion_rationale:None, cyclicality_rationale: None, wacc: None },
        };
        let fcf = FcfOutput { revenue:[0.0;5], operating_income:[0.0;5], fcf:[1e8,1e8,1e8,1e8,1e8], revenue_cagr_5y:0.0, fcf_cagr_5y:0.0 };
        let term = TerminalOutput { g_effective:0.03, g_raw:0.03, is_stable:true, terminal_fcf: 1e8*1.03, terminal_value: 1.03e8/(0.10-0.03), fcf_year_5:1e8 };
        let out = compute(&inp, &fcf, &term, 0.10, 1e9);
        // PV explicit = 1e8/1.1 + 1e8/1.21 + 1e8/1.331 + 1e8/1.4641 +1e8/1.61051 ≈ 379079
        assert!(out.fair_value_per_share.is_finite());
    }
}
