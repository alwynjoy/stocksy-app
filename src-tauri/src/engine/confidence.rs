//! Confidence — clamp 30/30/20/15/5, sum → level

use super::ConfidenceOutput;

pub fn clamp_and_score(
    cash: i32,
    competitive: i32,
    health: i32,
    visibility: i32,
    management: i32,
) -> ConfidenceOutput {
    let c = cash.clamp(0, 30);
    let cp = competitive.clamp(0, 30);
    let h = health.clamp(0, 20);
    let v = visibility.clamp(0, 15);
    let m = management.clamp(0, 5);
    let overall = c + cp + h + v + m;
    let level = if overall >= 70 {
        "high"
    } else if overall >= 40 {
        "moderate"
    } else {
        "low"
    }
    .to_string();
    ConfidenceOutput {
        overall_score: overall,
        overall_level: level,
        cash_flow_predictability: c,
        competitive_position: cp,
        financial_health: h,
        industry_visibility: v,
        management_quality: m,
    }
}

pub fn empty() -> ConfidenceOutput {
    clamp_and_score(0, 0, 0, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clamp() {
        let c = clamp_and_score(40, 40, 30, 20, 10);
        assert_eq!(c.cash_flow_predictability, 30);
        assert_eq!(c.competitive_position, 30);
        assert_eq!(c.financial_health, 20);
        assert_eq!(c.industry_visibility, 15);
        assert_eq!(c.management_quality, 5);
        assert_eq!(c.overall_score, 100);
        assert_eq!(c.overall_level, "high");
    }
}
