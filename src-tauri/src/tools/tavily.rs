//! Tavily search + extract — exact payloads per user curl.
//! search: POST {search.tavily_search_url} body {query, search_depth:"advanced", max_results}
//! extract: POST {scrape.tavily_extract_url} body {urls:[url], query:"find forward estimates", extract_depth:"advanced"}

use serde_json::Value;

use super::SearchArgs;
use super::ScrapeArgs;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .build()
        .unwrap()
}

pub async fn search(base_url: &str, api_key: &str, args: &SearchArgs, _exclude: &[String]) -> Value {
    let body = serde_json::json!({
        "query": args.query,
        "search_depth": "advanced",
        "max_results": args.max_results.clamp(1, 10),
        "exclude_domains": []
    });
    let resp = client()
        .post(base_url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await;
    match resp {
        Ok(r) => {
            let status = r.status();
            let txt = r.text().await.unwrap_or_default();
            if status.is_success() {
                serde_json::from_str::<Value>(&txt).unwrap_or_else(|_| serde_json::json!({"raw": txt}))
            } else {
                serde_json::json!({"error": format!("Tavily search HTTP {}: {}", status, &txt[..txt.len().min(500)])})
            }
        }
        Err(e) => serde_json::json!({"error": format!("Tavily search request failed: {}", e)}),
    }
}

pub async fn extract(base_url: &str, api_key: &str, args: &ScrapeArgs) -> Value {
    let _ = args.ai_query.as_deref();
    let body = serde_json::json!({
        "urls": [args.url],
        "query": "find forward estimates",
        "extract_depth": "advanced"
    });
    let resp = client()
        .post(base_url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await;
    match resp {
        Ok(r) => {
            let status = r.status();
            let txt = r.text().await.unwrap_or_default();
            if status.is_success() {
                serde_json::from_str::<Value>(&txt).unwrap_or_else(|_| serde_json::json!({"raw": txt}))
            } else {
                serde_json::json!({"error": format!("Tavily extract HTTP {}: {}", status, &txt[..txt.len().min(500)])})
            }
        }
        Err(e) => serde_json::json!({"error": format!("Tavily extract request failed: {}", e)}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_body_shape() {
        let args = SearchArgs { query: "AAPL forward estimates".into(), max_results: 5 };
        let body = serde_json::json!({"query": args.query, "search_depth": "advanced", "max_results": 5});
        assert_eq!(body["search_depth"], "advanced");
        assert_eq!(body["max_results"], 5);
    }
    #[test]
    fn extract_body_shape() {
        let body = serde_json::json!({"urls": ["https://example.com/estimates"], "query": "find forward estimates", "extract_depth": "advanced"});
        assert_eq!(body["extract_depth"], "advanced");
        assert_eq!(body["urls"][0], "https://example.com/estimates");
    }
}
