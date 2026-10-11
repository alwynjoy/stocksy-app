//! FCF projection — 5 years revenue × margin → NOPAT → FCF (+ finserv proxy)

use super::EngineInput;

#[derive(Debug, Clone)]
pub struct FcfOutput {
    pub revenue: [f64; 5],
    pub operating_income: [f64; 5],
    pub fcf: [f64; 5],
    pub revenue_cagr_5y: f64,
    pub fcf_cagr_5y: f64,
}

pub fn project(input: &EngineInput, fcf_factor: f64, ni_factor: Option<f64>) -> FcfOutput {
    let latest_rev = input.financials.historical_data.first().and_then(|r| r.revenue).unwrap_or(0.0);
    let tax = input.market_params.corporate_tax_rate;

    let mut revenue = [0.0; 5];
    let mut op_income = [0.0; 5];
    let mut fcf = [0.0; 5];

    let mut prev = latest_rev;
    for i in 0..5 {
        let g = input.dcf_inputs.revenue_growth_rates[i];
        let rev = prev * (1.0 + g);
        revenue[i] = rev;
        let margin = input.dcf_inputs.operating_margins[i];
        let oi = rev * margin;
        op_income[i] = oi;

        let f = if input.company_info.is_financial_services {
            // NI proxy: need net_income projection. Use NI growth = revenue growth on latest NI
            let latest_ni = input.financials.historical_data.first().and_then(|r| r.net_income).unwrap_or(0.0);
            // Project NI similarly: NI[t] = NI[t-1] * (1+g) where NI[t-1] chain
            // Simpler: use ratio: NI projection via revenue growth chain from latest_ni
            let ni_proj = if i == 0 {
                latest_ni * (1.0 + g)
            } else {
                // use previous ni proxy? Derive from previous iteration via g again
                // Keep deterministic: NI[t] = latest_ni * product(1+g0..gi) 
                // Compute product
                let mut prod = 1.0;
                for k in 0..=i {
                    prod *= 1.0 + input.dcf_inputs.revenue_growth_rates[k];
                }
                latest_ni * prod
            };
            let factor = ni_factor.unwrap_or(0.40);
            ni_proj * factor
        } else {
            let nopat = oi * (1.0 - tax);
            nopat * fcf_factor
        };
        fcf[i] = f;
        prev = rev;
    }

    let cagr = |arr: [f64;5], base: f64| -> f64 {
        if base.abs() < 1e-9 || arr[4].abs() < 1e-9 {
            0.0
        } else {
            (arr[4] / base).powf(1.0/5.0) - 1.0
        }
    };
    let rev_cagr = cagr(revenue, latest_rev);
    let fcf_cagr = {
        let base_fcf = if input.company_info.is_financial_services {
            input.financials.historical_data.first().and_then(|r| r.net_income).unwrap_or(0.0) * ni_factor.unwrap_or(0.40)
        } else {
            let oi0 = latest_rev * input.dcf_inputs.operating_margins[0]; // not historical; use projected comparison
            oi0 * (1.0 - tax) * fcf_factor
        };
        // Use historical fcf if available?
        // For exact, compute from first projected fcf[0] chain not base; fallback to first fcf
        let base = if fcf[0].abs() > 1e-9 { fcf[0] } else { base_fcf };
        cagr(fcf, base)
    };

    FcfOutput { revenue, operating_income: op_income, fcf, revenue_cagr_5y: rev_cagr, fcf_cagr_5y: fcf_cagr }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{CompanyInfo, CurrentMarketData, DcfInputs, EngineInput, Financials, HistoricalRow, MarketParams};

    fn mk(is_fin: bool) -> EngineInput {
        EngineInput {
            company_info: CompanyInfo { company_name:"T".into(), ticker:"T".into(), exchange:"NSE".into(), country:"India".into(), sector:"IT Services".into(), industry:"Software".into(), currency_code:"INR".into(), is_financial_services: is_fin, market_cap_category: "large-cap".into() },
            current_market_data: CurrentMarketData { current_price: 100.0, market_cap: 1e11, price_date:"01/01/2026".into(), shares_outstanding: Some(1e9), market_cap_category: None },
            market_params: MarketParams { risk_free_rate:0.0656, market_risk_premium:0.0726, corporate_tax_rate:0.30, nominal_gdp_growth:0.08733 },
            financials: Financials { beta:1.0, historical_data: vec![
                HistoricalRow { fiscal_year_end:"03/2026".into(), revenue:Some(1e10), operating_income:Some(2e9), ebitda:Some(2.2e9), net_income:Some(1e9), total_debt:Some(1e9), total_equity:Some(5e9), cash_equivalents:Some(5e8), free_cash_flow:Some(1e9)},
                HistoricalRow { fiscal_year_end:"03/2025".into(), revenue:Some(9e9), operating_income:Some(1.8e9), ebitda:Some(2e9), net_income:Some(9e8), total_debt:Some(1e9), total_equity:Some(4.5e9), cash_equivalents:Some(4e8), free_cash_flow:Some(9e8)},
                HistoricalRow { fiscal_year_end:"03/2024".into(), revenue:Some(8e9), operating_income:Some(1.6e9), ebitda:Some(1.8e9), net_income:Some(8e8), total_debt:Some(1e9), total_equity:Some(4e9), cash_equivalents:Some(3e8), free_cash_flow:Some(8e8)},
            ]},
            dcf_inputs: DcfInputs { revenue_growth_rates:[0.10,0.10,0.08,0.06,0.05], operating_margins:[0.20,0.20,0.20,0.20,0.20], terminal_margin:0.20, terminal_growth_rate:0.06, fcf_conversion_factor: Some(0.80), financial_services_ni_factor: Some(0.65), cyclicality_adjustment_bps: Some(0), fcf_conversion_rationale:None, cyclicality_rationale: None, wacc: None },
        }
    }

    #[test]
    fn non_fin_fcf() {
        let inp = mk(false);
        let out = project(&inp, 0.80, None);
        // rev0 1e10*1.1=1.1e10 oi=2.2e9 nopat=1.54e9 fcf=1.232e9
        assert!((out.fcf[0] - 1.232e9).abs() < 1.0);
    }

    #[test]
    fn finserv_proxy() {
        let inp = mk(true);
        let out = project(&inp, 0.65, Some(0.65));
        // NI 1e9*1.1=1.1e9 *0.65=715M (0.65 default)
        assert!((out.fcf[0] - 715_000_000.0).abs() < 1.0);
    }

    #[test]
    fn finserv_proxy_default_65() {
        let mut inp = mk(true);
        inp.dcf_inputs.financial_services_ni_factor = None;
        let out = project(&inp, 0.65, None);
        // None → 0.40 global conservative fallback (was 0.65)
        assert!((out.fcf[0] - 440_000_000.0).abs() < 1.0);
    }
}
