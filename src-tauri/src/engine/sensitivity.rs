//! Sensitivity 3x3 — ±50bps WACC × ±50bps growth, scaled terminal, finserv branch exact per my-dcf.json

use super::SensitivityCell;

/// Compute 9 cells. Mirrors n8n `3x3_Sensitivity_Matrix` exactly:
/// - shares = market_cap / current_price
/// - scalingFactor = base_fair_value / baseCalculatedFV where baseCalculatedFV = equity / shares
///   equity = PV(Terminal) (+cash-debt for non-fin)
/// - Each cell: TV = FCF5*(1+g)/(WACC-g), PV = TV/(1+WACC)^5, equity similarly, rawFV = equity/shares
///   fair_value = rawFV * scalingFactor (rounded to 2 decimals), pctChange = (fair - base)/base rounded 4 decimals
/// - is_model_stable = spread>0.0001 && rawFV>0 && abs(pctChange)<=1.0
pub fn compute(
    wacc: f64,
    terminal_growth: f64,
    fcf_year_5: f64,
    latest_cash: f64,
    latest_debt: f64,
    shares: f64,
    base_fair_value: f64,
    is_financial_services: bool,
    market_cap: f64,
    current_price: f64,
) -> Vec<SensitivityCell> {
    // shares calc fallback per n8n (more reliable than LLM)
    let shares_calc = if current_price > 0.0 { market_cap / current_price } else { shares };
    let shares_use = if shares_calc.is_finite() && shares_calc > 0.0 { shares_calc } else { shares };

    // base scaling factor
    let base_spread = wacc - terminal_growth;
    // guard
    let base_spread = if base_spread.abs() < 1e-12 { 0.0001 } else { base_spread };
    let base_terminal_fcf = fcf_year_5 * (1.0 + terminal_growth);
    let base_terminal_value = base_terminal_fcf / base_spread;
    let base_pv_terminal = base_terminal_value / (1.0 + wacc).powf(5.0);
    let base_equity = if is_financial_services {
        base_pv_terminal
    } else {
        base_pv_terminal + latest_cash - latest_debt
    };
    let base_calculated_fv = if shares_use > 0.0 { base_equity / shares_use } else { base_fair_value };
    let scaling_factor = if base_calculated_fv.abs() > 1e-12 {
        base_fair_value / base_calculated_fv
    } else {
        1.0
    };

    let wacc_steps = [-0.005, 0.0, 0.005];
    let growth_steps = [-0.005, 0.0, 0.005];

    let mut out = Vec::with_capacity(9);
    for wi in 0..3 {
        for gi in 0..3 {
            let w = wacc + wacc_steps[wi];
            let g = terminal_growth + growth_steps[gi];
            let spread = w - g;
            let is_spread_valid = spread > 0.0001;

            let (fair_value, pct_change, is_stable) = if is_spread_valid {
                let terminal_fcf = fcf_year_5 * (1.0 + g);
                let terminal_value = terminal_fcf / spread;
                let pv_terminal = terminal_value / (1.0 + w).powf(5.0);
                let equity = if is_financial_services {
                    pv_terminal
                } else {
                    pv_terminal + latest_cash - latest_debt
                };
                let raw_fv = if shares_use > 0.0 { equity / shares_use } else { 0.0 };
                if raw_fv > 0.0 {
                    let scaled = raw_fv * scaling_factor;
                    let fv = (scaled * 100.0).round() / 100.0;
                    let pct = (fv - base_fair_value) / base_fair_value;
                    let pct_rounded = (pct * 10000.0).round() / 10000.0;
                    (fv, Some(pct_rounded), pct_rounded.abs() <= 1.0)
                } else {
                    (0.0, None, false)
                }
            } else {
                (0.0, None, false)
            };

            // round wacc/g to 4 decimals
            let w_rounded = (w * 10000.0).round() / 10000.0;
            let g_rounded = (g * 10000.0).round() / 10000.0;

            out.push(SensitivityCell {
                wacc_level: wi as i32 - 1,
                growth_level: gi as i32 - 1,
                wacc_value: w_rounded,
                growth_value: g_rounded,
                fair_value,
                pct_change_from_base: pct_change,
                is_base_case: wi == 1 && gi == 1,
                is_model_stable: is_stable,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_cell_stable() {
        let cells = compute(0.10, 0.05, 1e9, 5e8, 1e9, 1e9, 150.0, false, 1e11, 100.0);
        assert_eq!(cells.len(), 9);
        let base = cells.iter().find(|c| c.is_base_case).unwrap();
        assert!(base.is_model_stable);
        assert!(base.fair_value > 0.0);
    }

    #[test]
    fn finserv_branch() {
        let cells = compute(0.09, 0.08, 1e9, 5e8, 10e9, 1e9, 100.0, true, 1e11, 100.0);
        // high growth → spread 0.01 → still valid for base
        assert_eq!(cells.len(), 9);
    }

    #[test]
    fn negative_spread_unstable() {
        // wacc 0.08 growth 0.09 → base spread -0.01 → but sensitivity will test cells where spread negative
        let cells = compute(0.08, 0.087, 1e9, 0.0, 0.0, 1e9, 100.0, false, 1e11, 100.0);
        // At least one cell with negative spread should be unstable
        assert!(cells.iter().any(|c| !c.is_model_stable));
    }
}
