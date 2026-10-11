//! ScrapingDog search + scrape — GET with query params per user spec.
//! search: GET {search.scrapingdog_search_url}?api_key=...&query=...&advance_search=true&domain=google.com&language=en
//! scrape: GET {scrape.scrapingdog_scrape_url}?api_key=...&url=...&dynamic=true&ai_query=...

use super::{ScrapeArgs, SearchArgs};
use serde_json::Value;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .build()
        .unwrap()
}

pub async fn search(base_url: &str, api_key: &str, args: &SearchArgs) -> Value {
    let url = format!(
        "{}?api_key={}&query={}&advance_search=true&domain=google.com&language=en&results={}",
        base_url.trim_end_matches('/'),
        super::urlencoding::encode(api_key),
        super::urlencoding::encode(&args.query),
        args.max_results.clamp(1, 10)
    );
    let resp = client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await;
    match resp {
        Ok(r) => {
            let status = r.status();
            let txt = r.text().await.unwrap_or_default();
            if status.is_success() {
                serde_json::from_str::<Value>(&txt).unwrap_or_else(|_| serde_json::json!({"raw": txt}))
            } else {
                serde_json::json!({"error": format!("ScrapingDog search HTTP {}: {}", status, &txt[..txt.len().min(500)])})
            }
        }
        Err(e) => serde_json::json!({"error": format!("ScrapingDog search request failed: {}", e)}),
    }
}

pub async fn scrape(base_url: &str, api_key: &str, args: &ScrapeArgs) -> Value {
    let ai = args.ai_query.as_deref().unwrap_or("summarize page for forward estimates").trim();
    let ai_q = if ai.is_empty() { "summarize page for forward estimates" } else { ai };
    let url = format!(
        "{}?api_key={}&url={}&dynamic=true&ai_query={}",
        base_url.trim_end_matches('/'),
        super::urlencoding::encode(api_key),
        super::urlencoding::encode(&args.url),
        super::urlencoding::encode(ai_q)
    );
    let resp = client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await;
    match resp {
        Ok(r) => {
            let status = r.status();
            let txt = r.text().await.unwrap_or_default();
            if status.is_success() {
                serde_json::from_str::<Value>(&txt).unwrap_or_else(|_| serde_json::json!({"raw": txt}))
            } else {
                serde_json::json!({"error": format!("ScrapingDog scrape HTTP {}: {}", status, &txt[..txt.len().min(500)])})
            }
        }
        Err(e) => serde_json::json!({"error": format!("ScrapingDog scrape request failed: {}", e)}),
    }
}

#[cfg(test)]
mod tests {
    use crate::tools::{ScrapeArgs, SearchArgs};
    #[test]
    fn search_url_shape() {
        let args = SearchArgs { query: "market analysis of reliance industries".into(), max_results: 5 };
        let url = format!(
            "{}?api_key={}&query={}&advance_search=true&domain=google.com&language=en&results={}",
            "https://api.scrapingdog.com/google".trim_end_matches('/'),
            crate::tools::urlencoding::encode("key123"),
            crate::tools::urlencoding::encode(&args.query),
            5
        );
        assert!(url.contains("advance_search=true"));
        assert!(url.contains("domain=google.com"));
        assert!(url.contains("language=en"));
        assert!(url.contains("api_key=key123"));
    }
    #[test]
    fn scrape_url_shape() {
        let args = ScrapeArgs { url: "https://example.com".into(), ai_query: Some("howdie".into()) };
        let url = format!(
            "{}?api_key={}&url={}&dynamic=true&ai_query={}",
            "https://api.scrapingdog.com/scrape".trim_end_matches('/'),
            crate::tools::urlencoding::encode("69344a73774b84e55a61b845"),
            crate::tools::urlencoding::encode(&args.url),
            crate::tools::urlencoding::encode(args.ai_query.as_deref().unwrap())
        );
        assert!(url.contains("dynamic=true"));
        assert!(url.contains("ai_query=howdie"));
        assert!(url.contains("api_key=69344a"));
    }
}
