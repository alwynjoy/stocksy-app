//! Risk/Reward — upside/downside → R/R + label

#[derive(Debug, Clone)]
pub struct RiskRewardOutput {
    pub upside_pct: f64,
    pub downside_pct: f64,
    pub ratio: f64,
    pub label: String,
}

pub fn compute(optimistic_fv: f64, pessimistic_fv: f64, cmp: f64) -> RiskRewardOutput {
    if cmp.abs() < 1e-12 {
        return RiskRewardOutput { upside_pct: 0.0, downside_pct: 0.0, ratio: 99.9, label: "Higher Risk".into() };
    }
    let upside = (optimistic_fv - cmp) / cmp;
    let downside = (pessimistic_fv - cmp) / cmp; // negative
    let denom = upside - downside;
    let ratio = if denom.abs() < 1e-12 {
        99.9
    } else {
        downside.abs() / denom
    };
    let label = if ratio <= 0.20 {
        "Excellent"
    } else if ratio <= 0.40 {
        "Very Good"
    } else if ratio <= 0.50 {
        "Good"
    } else if ratio <= 0.60 {
        "Balanced"
    } else {
        "Higher Risk"
    }
    .to_string();

    RiskRewardOutput { upside_pct: upside, downside_pct: downside, ratio, label }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels() {
        let r = compute(150.0, 90.0, 100.0);
        assert_eq!(r.label, "Excellent"); // up 0.5 down -0.1 denom 0.6 ratio 0.166
        let r2 = compute(110.0, 50.0, 100.0);
        assert_eq!(r2.label, "Higher Risk"); // up 0.1 down -0.5 denom 0.6 ratio 0.833
        let r3 = compute(130.0, 85.0, 100.0); // up 0.3 down -0.15 denom 0.45 ratio 0.333 → Very Good
        assert_eq!(r3.label, "Very Good");
    }
}
