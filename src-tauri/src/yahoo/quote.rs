use chrono::{DateTime, Utc};

pub fn is_financial_services(sector: Option<&str>, industry: Option<&str>) -> bool {
    const KEYWORDS: &[&str] = &[
        "bank",
        "banking",
        "financial services",
        "financial",
        "finance",
        "insurance",
        "investment",
        "asset management",
        "nbfc",
        "capital markets",
        "securities",
        "mortgage",
        "consumer finance",
    ];
    let combined = format!(
        "{} {}",
        sector.unwrap_or("").to_lowercase(),
        industry.unwrap_or("").to_lowercase()
    );
    let c = combined.to_lowercase();
    KEYWORDS.iter().any(|k| c.contains(k))
}

pub fn market_cap_category(market_cap: Option<f64>, country_code: &str) -> Option<String> {
    let v = market_cap?;
    if !v.is_finite() || v <= 0.0 { return None; }
    match country_code {
        "IN" => {
            let cr = v / 1e7;
            if cr > 1_650_000.0 { Some("mega-cap".to_string()) }
            else if cr >= 82_500.0 { Some("large-cap".to_string()) }
            else if cr >= 16_500.0 { Some("mid-cap".to_string()) }
            else if cr >= 2_475.0 { Some("small-cap".to_string()) }
            else { Some("micro-cap".to_string()) }
        }
        "GB" => {
            if v >= 100e9 { Some("mega-cap".to_string()) }
            else if v >= 4e9 { Some("large-cap".to_string()) }
            else if v >= 500e6 { Some("mid-cap".to_string()) }
            else if v >= 50e6 { Some("small-cap".to_string()) }
            else { Some("micro-cap".to_string()) }
        }
        _ => {
            if v >= 200e9 { Some("mega-cap".to_string()) }
            else if v >= 10e9 { Some("large-cap".to_string()) }
            else if v >= 2e9 { Some("mid-cap".to_string()) }
            else if v >= 250e6 { Some("small-cap".to_string()) }
            else { Some("micro-cap".to_string()) }
        }
    }
}

pub fn format_date_ddmmyyyy(epoch_secs: i64) -> String {
    DateTime::<Utc>::from_timestamp(epoch_secs, 0)
        .unwrap_or_else(|| Utc::now())
        .format("%d/%m/%Y")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fin_services_keywords() {
        assert!(is_financial_services(Some("Financial Services"), Some("Banks - Regional")));
        assert!(is_financial_services(Some("Financial Services"), Some("Capital Markets")));
        assert!(!is_financial_services(Some("Basic Materials"), Some("Chemicals")));
        assert!(is_financial_services(None, Some("Insurance - Life")));
        assert!(!is_financial_services(Some("Technology"), Some("Software")));
    }
    #[test]
    fn category_in() {
        assert_eq!(market_cap_category(Some(395_520_000_000.0), "IN"), Some("mid-cap".to_string()));
        assert_eq!(market_cap_category(Some(3_562_326_400_000.0), "IN"), Some("large-cap".to_string()));
    }
    #[test]
    fn category_us_gb() {
        assert_eq!(market_cap_category(Some(250e9), "US"), Some("mega-cap".to_string()));
        assert_eq!(market_cap_category(Some(15e9), "US"), Some("large-cap".to_string()));
        assert_eq!(market_cap_category(Some(5e9), "US"), Some("mid-cap".to_string()));
        assert_eq!(market_cap_category(Some(1e9), "US"), Some("small-cap".to_string()));
        assert_eq!(market_cap_category(Some(100e6), "US"), Some("micro-cap".to_string()));
        assert_eq!(market_cap_category(Some(150e9), "GB"), Some("mega-cap".to_string()));
        assert_eq!(market_cap_category(Some(10e9), "GB"), Some("large-cap".to_string()));
        assert_eq!(market_cap_category(Some(1e9), "GB"), Some("mid-cap".to_string()));
    }
}
