use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YahooQuote {
    pub symbol: String,
    #[serde(default)]
    pub shortname: Option<String>,
    #[serde(default)]
    pub longname: Option<String>,
    #[serde(default, rename = "exchDisp")]
    pub exch_disp: Option<String>,
    #[serde(default)]
    pub exchange: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YahooSearchResponse {
    #[serde(default)]
    quotes: Vec<YahooQuote>,
}

fn country_from_symbol(symbol: &str) -> String {
    let s = symbol.to_uppercase();
    if s.ends_with(".NS") || s.ends_with(".BO") {
        "IN".to_string()
    } else if s.ends_with(".L") {
        "GB".to_string()
    } else if s.ends_with(".AX") || s.ends_with(".XA") || s.ends_with(".AU") {
        "AU".to_string()
    } else {
        "US".to_string()
    }
}

fn display_name(q: &YahooQuote) -> Option<String> {
    q.longname.clone().or_else(|| q.shortname.clone())
}

pub async fn yahoo_search(query: &str, country_hint: Option<&str>) -> Result<Vec<crate::db::TickerCacheRow>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!(
        "https://query2.finance.yahoo.com/v1/finance/search?q={}",
        urlencoding(q)
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .user_agent("stocksy-desktop/0.1")
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    // Yahoo requires a User-Agent; no auth needed
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("yahoo search failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("yahoo search HTTP {status}: {}", &body[..body.len().min(300)]));
    }
    let parsed: YahooSearchResponse = resp
        .json()
        .await
        .map_err(|e| format!("yahoo search parse failed: {e}"))?;
    let mut out = Vec::new();
    for q in parsed.quotes.into_iter().take(10) {
        let symbol = q.symbol.trim().to_uppercase();
        if symbol.is_empty() {
            continue;
        }
        // Filter to equity-like quotes only (skip futures etc if desired, but keep all for now)
        let company = display_name(&q);
        let exch = q.exch_disp.clone().or(q.exchange.clone());
        let country = country_hint
            .map(|c| c.to_uppercase())
            .unwrap_or_else(|| country_from_symbol(&symbol));
        out.push(crate::db::TickerCacheRow {
            ticker: symbol,
            company_name: company,
            exchange: exch,
            country_code: country,
        });
    }
    Ok(out)
}

// Minimal URL encoding (avoid adding extra crate if already available via reqwest)
fn urlencoding(s: &str) -> String {
    // Use percent-encoding for query param
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn country_inference() {
        assert_eq!(country_from_symbol("RELIANCE.NS"), "IN");
        assert_eq!(country_from_symbol("TCS.BO"), "IN");
        assert_eq!(country_from_symbol("BHP.AX"), "AU");
        assert_eq!(country_from_symbol("VOD.L"), "GB");
        assert_eq!(country_from_symbol("AAPL"), "US");
        assert_eq!(country_from_symbol("ZNO.XA"), "AU");
    }
}
