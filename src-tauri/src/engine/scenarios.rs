//! Scenarios — 25/55/20 with exact offsets per 03 §7

use super::{EngineInput, fcf::FcfOutput, terminal, valuation, ScenarioOutput};

#[derive(Debug, Clone)]
pub struct ScenarioComputed {
    pub scenarios: Vec<ScenarioOutput>,
    pub optimistic_fv: f64,
    pub pessimistic_fv: f64,
    pub weighted_fv: f64,
}

fn fv_for_scenario(
    input: &EngineInput,
    _base_fcf: &FcfOutput,
    base_wacc: f64,
    base_g: f64,
    base_margin: f64,
    rev_delta: f64,
    margin_delta: f64,
    wacc_delta: f64,
    g_delta: f64,
    shares: f64,
) -> (f64, f64) {
    // Clone inputs with deltas
    let mut scen_input = input.clone();
    for i in 0..5 {
        scen_input.dcf_inputs.revenue_growth_rates[i] += rev_delta;
        scen_input.dcf_inputs.operating_margins[i] += margin_delta;
        // clamp growth not negative crazy — allow negative but not below -0.5
        scen_input.dcf_inputs.revenue_growth_rates[i] = scen_input.dcf_inputs.revenue_growth_rates[i].clamp(-0.5, 1.0);
        scen_input.dcf_inputs.operating_margins[i] = scen_input.dcf_inputs.operating_margins[i].clamp(0.0, 0.6);
    }
    scen_input.dcf_inputs.terminal_margin = (base_margin + margin_delta).clamp(0.0, 0.6);
    scen_input.dcf_inputs.terminal_growth_rate = base_g + g_delta;

    let fcf_factor = super::effective_fcf_factor(&scen_input);
    let ni_factor = if scen_input.company_info.is_financial_services { Some(super::effective_ni_factor(&scen_input)) } else { None };
    let fcf_out = super::fcf::project(&scen_input, fcf_factor, ni_factor);
    let wacc_scen = (base_wacc + wacc_delta).clamp(0.07, 0.15);
    let term = terminal::compute(&scen_input, &fcf_out, wacc_scen);
    let val = valuation::compute(&scen_input, &fcf_out, &term, wacc_scen, shares);
    (val.fair_value_per_share, term.g_effective)
}

pub fn compute(
    input: &EngineInput,
    fcf_out: &FcfOutput,
    wacc: f64,
    g: f64,
    base_fv: f64,
    shares: f64,
) -> ScenarioComputed {
    let base_margin = input.dcf_inputs.terminal_margin;
    let gdp = input.market_params.nominal_gdp_growth;
    let y5 = input.dcf_inputs.revenue_growth_rates[4];

    // Optimistic: g = min(base+0.01, GDP+0.01) with base leeway (Option A)
    let g_opt_raw = g + 0.01;
    let g_opt = g_opt_raw.min(gdp + 0.01).max(g);
    let g_opt_delta = g_opt - input.dcf_inputs.terminal_growth_rate;
    let (fv_opt, geff_opt) = fv_for_scenario(input, fcf_out, wacc, g, base_margin, 0.03, 0.02, -0.005, g_opt_delta, shares);

    // Expected
    let fv_exp = base_fv;

    // Pessimistic: g = max(base-0.01, Y5-0.035, 0.03) capped to base
    let g_pes_raw = g - 0.01;
    let g_pes_floor = (y5 - 0.035).max(0.03);
    let g_pes = g_pes_raw.max(g_pes_floor).min(g);
    let g_pes_delta = g_pes - input.dcf_inputs.terminal_growth_rate;
    let (fv_pes, geff_pes) = fv_for_scenario(input, fcf_out, wacc, g, base_margin, -0.03, -0.02, 0.01, g_pes_delta, shares);

    let weighted = 0.25 * fv_opt + 0.55 * fv_exp + 0.20 * fv_pes;

    let cmp = input.current_market_data.current_price;
    let _cagr = |fv: f64| -> f64 {
        if base_fv.abs() < 1e-9 { 0.0 } else { (fv / base_fv).powf(1.0/5.0) - 1.0 }
    };
    let rev_cagr_opt = {
        let mut prod = 1.0;
        for i in 0..5 { prod *= 1.0 + input.dcf_inputs.revenue_growth_rates[i] + 0.03; }
        prod.powf(1.0/5.0) - 1.0
    };
    let rev_cagr_pes = {
        let mut prod = 1.0;
        for i in 0..5 { prod *= 1.0 + input.dcf_inputs.revenue_growth_rates[i] - 0.03; }
        prod.powf(1.0/5.0) - 1.0
    };
    let rev_cagr_exp = {
        let mut prod = 1.0;
        for i in 0..5 { prod *= 1.0 + input.dcf_inputs.revenue_growth_rates[i]; }
        prod.powf(1.0/5.0) - 1.0
    };

    let scenarios = vec![
        ScenarioOutput { scenario_name: "optimistic".into(), probability: 0.25, revenue_cagr: rev_cagr_opt, terminal_margin: (base_margin+0.02).clamp(0.0,0.6), wacc: (wacc-0.005).clamp(0.07,0.15), terminal_growth: geff_opt, fair_value: fv_opt, vs_current_price_pct: if cmp>0.0 { (fv_opt-cmp)/cmp } else {0.0}, narrative: None },
        ScenarioOutput { scenario_name: "expected".into(), probability: 0.55, revenue_cagr: rev_cagr_exp, terminal_margin: base_margin, wacc, terminal_growth: g, fair_value: fv_exp, vs_current_price_pct: if cmp>0.0 { (fv_exp-cmp)/cmp } else {0.0}, narrative: None },
        ScenarioOutput { scenario_name: "pessimistic".into(), probability: 0.20, revenue_cagr: rev_cagr_pes, terminal_margin: (base_margin-0.02).clamp(0.0,0.6), wacc: (wacc+0.01).clamp(0.07,0.15), terminal_growth: geff_pes, fair_value: fv_pes, vs_current_price_pct: if cmp>0.0 { (fv_pes-cmp)/cmp } else {0.0}, narrative: None },
    ];

    ScenarioComputed { scenarios, optimistic_fv: fv_opt, pessimistic_fv: fv_pes, weighted_fv: weighted }
}
