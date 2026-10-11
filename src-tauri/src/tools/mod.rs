//! Tool router — bundled per provider: Tavily (primary) + ScrapingDog (secondary, toggle search.active_provider)
//! Each provider has api_key + search_url + scrape_url. Fallback: if active missing, try secondary with warning.

pub mod scrapingdog;
pub mod tavily;

use rusqlite::Connection;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct SearchArgs {
    pub query: String,
    pub max_results: usize,
}

#[derive(Debug, Clone)]
pub struct ScrapeArgs {
    pub url: String,
    pub ai_query: Option<String>,
}

pub fn should_skip_url(url: &str, exclude_exts: &[String]) -> bool {
    let lower = url.to_lowercase();
    for ext in exclude_exts {
        if lower.contains(&ext.to_lowercase()) {
            return true;
        }
    }
    false
}

fn load_exclude(conn: &Connection) -> Vec<String> {
    let raw = crate::db::get_setting(conn, "search.exclude_extensions")
        .ok()
        .flatten()
        .unwrap_or_else(|| r#"[".pdf",".doc",".xls"]"#.to_string());
    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_else(|_| vec![".pdf".into(), ".doc".into(), ".xls".into()])
}

fn get_secret(conn: &Connection, key: &str) -> Option<String> {
    crate::keyring_store::get(conn, key).ok().flatten().filter(|v| !v.trim().is_empty())
}

fn get_setting(conn: &Connection, key: &str, fallback: &str) -> String {
    crate::db::get_setting(conn, key)
        .ok()
        .flatten()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn active_provider(conn: &Connection) -> String {
    crate::db::get_setting(conn, "search.active_provider")
        .ok()
        .flatten()
        .map(|v| v.trim().to_lowercase())
        .filter(|v| v == "tavily" || v == "scrapingdog")
        .unwrap_or_else(|| "tavily".to_string())
}

/// Route web_search: active provider, fallback to secondary with warning
pub async fn route_search(conn: &Connection, args: &SearchArgs) -> Value {
    let exclude = load_exclude(conn);
    let active = active_provider(conn);
    let try_tavily = || {
        get_secret(conn, "search.tavily_key").map(|k| {
            let url = get_setting(conn, "search.tavily_search_url", "https://api.tavily.com/search");
            (k, url)
        })
    };
    let try_dog = || {
        get_secret(conn, "search.scrapingdog_key").map(|k| {
            let url = get_setting(conn, "search.scrapingdog_search_url", "https://api.scrapingdog.com/google");
            (k, url)
        })
    };
    if active == "tavily" {
        if let Some((key, url)) = try_tavily() {
            return tavily::search(&url, &key, args, &exclude).await;
        }
        if let Some((key, url)) = try_dog() {
            // fallback warning embedded as error wrapper then attempt
            let mut v = scrapingdog::search(&url, &key, args).await;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_fallback_warning".into(), Value::String("API key missing for Tavily, attempting secondary ScrapingDog".into()));
            }
            return v;
        }
    } else {
        if let Some((key, url)) = try_dog() {
            return scrapingdog::search(&url, &key, args).await;
        }
        if let Some((key, url)) = try_tavily() {
            let mut v = tavily::search(&url, &key, args, &exclude).await;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_fallback_warning".into(), Value::String("API key missing for ScrapingDog, attempting secondary Tavily".into()));
            }
            return v;
        }
    }
    serde_json::json!({"error": format!("no search provider configured — active is {active}; add key for Tavily or ScrapingDog in Settings → AI Agent Tools")})
}

/// Route web_scrape: active provider, fallback to secondary
pub async fn route_scrape(conn: &Connection, args: &ScrapeArgs) -> Value {
    let exclude = load_exclude(conn);
    if should_skip_url(&args.url, &exclude) {
        return serde_json::json!({"error": format!("skipped document URL per exclude_extensions: {}", args.url)});
    }
    let active = active_provider(conn);
    let try_tavily = || {
        get_secret(conn, "search.tavily_key").map(|k| {
            let url = get_setting(conn, "scrape.tavily_extract_url", "https://api.tavily.com/extract");
            (k, url)
        })
    };
    let try_dog = || {
        get_secret(conn, "search.scrapingdog_key").map(|k| {
            let url = get_setting(conn, "scrape.scrapingdog_scrape_url", "https://api.scrapingdog.com/scrape");
            (k, url)
        })
    };
    if active == "tavily" {
        if let Some((key, url)) = try_tavily() {
            return tavily::extract(&url, &key, args).await;
        }
        if let Some((key, url)) = try_dog() {
            let mut v = scrapingdog::scrape(&url, &key, args).await;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_fallback_warning".into(), Value::String("API key missing for Tavily, attempting secondary ScrapingDog".into()));
            }
            return v;
        }
    } else {
        if let Some((key, url)) = try_dog() {
            return scrapingdog::scrape(&url, &key, args).await;
        }
        if let Some((key, url)) = try_tavily() {
            let mut v = tavily::extract(&url, &key, args).await;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_fallback_warning".into(), Value::String("API key missing for ScrapingDog, attempting secondary Tavily".into()));
            }
            return v;
        }
    }
    serde_json::json!({"error": format!("no scrape provider configured — active is {active}; add key for Tavily or ScrapingDog in Settings → AI Agent Tools")})
}

pub(crate) mod urlencoding {
    pub fn encode(s: &str) -> String {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_pdf_doc() {
        let exts = vec![".pdf".to_string(), ".doc".to_string()];
        assert!(should_skip_url("https://example.com/file.pdf", &exts));
        assert!(should_skip_url("https://example.com/doc.docx", &exts));
        assert!(!should_skip_url("https://example.com/article", &exts));
    }
}
