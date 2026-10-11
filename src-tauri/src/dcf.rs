//! Read-only DCF formula constants surfaced in Settings (decision D4).
//! The math itself is fixed in Rust (Stage 4); nothing here is editable.

use serde::Serialize;

#[derive(Serialize)]
pub struct DcfDefault {
    pub name: String,
    pub value: String,
    pub note: String,
}

pub fn defaults() -> Vec<DcfDefault> {
    vec![
        DcfDefault {
            name: "Projection horizon".into(),
            value: "5 years".into(),
            note: "Explicit revenue / operating-margin / FCF forecast (F2.4); FCF chart spans 5 years (F7.8).".into(),
        },
        DcfDefault {
            name: "Scenario weights".into(),
            value: "0.25 / 0.55 / 0.20".into(),
            note: "Optimistic / Expected / Pessimistic weighted fair value (F2.10 §7).".into(),
        },
        DcfDefault {
            name: "Sensitivity grid".into(),
            value: "3 × 3 at ±0.5%".into(),
            note: "WACC and terminal-growth each shifted by ±50 bps around base (§9).".into(),
        },
        DcfDefault {
            name: "Beta cap".into(),
            value: "0.30 – 2.00".into(),
            note: "Raw beta clamped before the Blume adjustment (§3).".into(),
        },
        DcfDefault {
            name: "Blume adjustment".into(),
            value: "0.67 × β + 0.33 × 1.0".into(),
            note: "Adjusted beta shrinks the observed beta toward the market (§4).".into(),
        },
        DcfDefault {
            name: "Cost of equity clamp".into(),
            value: "8 – 20%".into(),
            note: "Ke = Rf + β × MRP, clamped before WACC assembly (§3).".into(),
        },
        DcfDefault {
            name: "WACC clamp".into(),
            value: "7 – 15%".into(),
            note: "Target capital-structure WACC cap (§3).".into(),
        },
        DcfDefault {
            name: "Financial-services FCF proxy".into(),
            value: "Net income × 0.60 – 0.85 (0.65, clamp 0.40 – 0.90)".into(),
            note: "Equity cash flow proxy for banks/NBFCs; discounted at Cost of Equity (Ke), not WACC (§3).".into(),
        },
        DcfDefault {
            name: "Confidence dimension caps".into(),
            value: "30 / 30 / 20 / 15 / 5".into(),
            note: "Cash-flow predictability / competitive position / financial health / industry visibility / management quality.".into(),
        },
    ]
}
