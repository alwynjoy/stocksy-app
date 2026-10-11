//! Stage-1 stub data: page actions return mock JSON until the real
//! pipeline (Stages 3–5) lands. Every function here shapes data exactly
//! like its production counterpart so the UI contract is stable.

use serde_json::{json, Value};

pub fn dashboard_stats() -> Value {
    json!({
        "total_reports": 12,
        "reports_per_model": [
            { "model": "claude-sonnet", "count": 4 },
            { "model": "gpt-4o", "count": 5 },
            { "model": "deepseek-reasoner", "count": 3 }
        ],
        "reports_per_country": [
            { "country": "US", "count": 5 },
            { "country": "IN", "count": 4 },
            { "country": "GB", "count": 2 },
            { "country": "AU", "count": 1 }
        ],
        "recent_analyses": [
            {
                "id": "r-aap1", "ticker": "AAPL", "company": "Apple Inc.", "country": "US",
                "price_date": "2026-09-19", "model": "claude-sonnet",
                "verdict": "Undervalued", "weighted_fair_value": 261.4,
                "gap_pct": 12.6, "confidence": 84, "favorite": true
            },
            {
                "id": "r-msf1", "ticker": "MSFT", "company": "Microsoft Corp.", "country": "US",
                "price_date": "2026-09-18", "model": "gpt-4o",
                "verdict": "Fairly Valued", "weighted_fair_value": 448.0,
                "gap_pct": 2.1, "confidence": 77, "favorite": false
            },
            {
                "id": "r-baj1", "ticker": "BAJFINANCE", "company": "Bajaj Finance Ltd.", "country": "IN",
                "price_date": "2026-09-17", "model": "deepseek-reasoner",
                "verdict": "Overvalued", "weighted_fair_value": 6100.0,
                "gap_pct": -8.4, "confidence": 71, "favorite": false
            },
            {
                "id": "r-tsl1", "ticker": "TSLA", "company": "Tesla, Inc.", "country": "US",
                "price_date": "2026-09-16", "model": "gpt-4o",
                "verdict": "Undervalued", "weighted_fair_value": 301.2,
                "gap_pct": 9.9, "confidence": 63, "favorite": true
            }
        ],
        "watchlist_summary": [
            { "name": "Tech Picks", "tickers": 5, "gainers": 3, "latest_gap_pct": 8.9 },
            { "name": "Dividend", "tickers": 3, "gainers": 1, "latest_gap_pct": -2.3 }
        ]
    })
}

pub fn screener_rows() -> Value {
    json!([
        { "ticker": "AAPL", "company": "Apple Inc.", "sector": "Technology", "industry": "Consumer Electronics", "market_cap_category": "mega-cap", "country": "US", "verdict": "Undervalued", "weighted_fair_value": 261.4, "gap_pct": 12.6, "confidence": 84, "risk_reward": 2.4, "revenue_growth": 0.062 },
        { "ticker": "MSFT", "company": "Microsoft Corp.", "sector": "Technology", "industry": "Software", "market_cap_category": "mega-cap", "country": "US", "verdict": "Fairly Valued", "weighted_fair_value": 448.0, "gap_pct": 2.1, "confidence": 77, "risk_reward": 1.2, "revenue_growth": 0.149 },
        { "ticker": "TSLA", "company": "Tesla, Inc.", "sector": "Consumer Cyclical", "industry": "Auto Manufacturers", "market_cap_category": "large-cap", "country": "US", "verdict": "Undervalued", "weighted_fair_value": 301.2, "gap_pct": 9.9, "confidence": 63, "risk_reward": 1.8, "revenue_growth": 0.041 },
        { "ticker": "NVDA", "company": "NVIDIA Corporation", "sector": "Technology", "industry": "Semiconductors", "market_cap_category": "mega-cap", "country": "US", "verdict": "Undervalued", "weighted_fair_value": 152.9, "gap_pct": 18.2, "confidence": 86, "risk_reward": 3.1, "revenue_growth": 0.241 },
        { "ticker": "BAJFINANCE", "company": "Bajaj Finance Ltd.", "sector": "Financial Services", "industry": "Credit Services", "market_cap_category": "large-cap", "country": "IN", "verdict": "Overvalued", "weighted_fair_value": 6100.0, "gap_pct": -8.4, "confidence": 71, "risk_reward": 0.6, "revenue_growth": 0.203 },
        { "ticker": "HDB", "company": "HDFC Bank Ltd.", "sector": "Financial Services", "industry": "Banks - Regional", "market_cap_category": "large-cap", "country": "IN", "verdict": "Fairly Valued", "weighted_fair_value": 1780.0, "gap_pct": 1.5, "confidence": 80, "risk_reward": 1.1, "revenue_growth": 0.117 },
        { "ticker": "SHEL", "company": "Shell plc", "sector": "Energy", "industry": "Oil & Gas Integrated", "market_cap_category": "large-cap", "country": "GB", "verdict": "Undervalued", "weighted_fair_value": 31.2, "gap_pct": 14.0, "confidence": 74, "risk_reward": 2.2, "revenue_growth": 0.019 },
        { "ticker": "CBA", "company": "Commonwealth Bank", "sector": "Financial Services", "industry": "Banks - Diversified", "market_cap_category": "large-cap", "country": "AU", "verdict": "Overvalued", "weighted_fair_value": 118.4, "gap_pct": -11.2, "confidence": 68, "risk_reward": 0.4, "revenue_growth": 0.053 }
    ])
}

#[allow(dead_code)]
fn is_valid_stub(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Array(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stubs_are_valid_json_objects_arrays() {
        assert!(is_valid_stub(&dashboard_stats()));
        assert!(is_valid_stub(&screener_rows()));
    }

    #[test]
    fn dashboard_stats_has_expected_keys() {
        let v = dashboard_stats();
        let obj = v.as_object().expect("dashboard stats must be an object");
        for key in [
            "total_reports",
            "reports_per_model",
            "reports_per_country",
            "recent_analyses",
        ] {
            assert!(obj.contains_key(key), "missing key {key}");
        }
        assert_eq!(v["total_reports"].as_i64(), Some(12));
    }

    #[test]
    fn screener_rows_carry_contract_fields() {
        let value = screener_rows();
        let rows = value.as_array().expect("screener rows must be array");
        assert!(rows.len() >= 4, "need enough rows for the UI table");
        let first = rows[0].as_object().expect("row must be object");
        for key in [
            "ticker",
            "company",
            "verdict",
            "weighted_fair_value",
            "gap_pct",
            "confidence",
        ] {
            assert!(first.contains_key(key), "missing key {key}");
        }
    }
}
