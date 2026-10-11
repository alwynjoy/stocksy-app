use rusqlite::Connection;
use serde_json::{json, Value};

use super::quote::{is_financial_services, market_cap_category};

#[derive(Debug, Clone, serde::Serialize)]
pub struct FetchResult {
    pub company_info: Value,
    pub current_market_data: Value,
    pub market_parameters: Value,
    pub financials: Value,
    pub status: String,
    pub error_message: Option<String>,
    pub raw_response: Value,
    pub parsed_financials: Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MarketParamsSnapshot {
    pub risk_free_rate: f64,
    pub market_risk_premium: f64,
    pub corporate_tax_rate: f64,
    pub nominal_gdp_growth: f64,
}

pub fn get_market_params(conn: &Connection, country_code: &str) -> Result<MarketParamsSnapshot, String> {
    let mut stmt = conn
        .prepare(
            "SELECT expected_market_return, risk_free_rate, corporate_tax_rate, nominal_gdp_growth FROM country_parameters WHERE country_code = ?1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query(rusqlite::params![country_code])
        .map_err(|e| e.to_string())?;
    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let emr: f64 = row.get(0).map_err(|e| e.to_string())?;
        let rf: f64 = row.get(1).map_err(|e| e.to_string())?;
        let tax: f64 = row.get(2).map_err(|e| e.to_string())?;
        let gdp: f64 = row.get(3).map_err(|e| e.to_string())?;
        Ok(MarketParamsSnapshot {
            risk_free_rate: rf,
            market_risk_premium: emr - rf,
            corporate_tax_rate: tax,
            nominal_gdp_growth: gdp,
        })
    } else {
        Err(format!("country_parameters not found for {country_code}"))
    }
}

pub fn get_country_name(conn: &Connection, country_code: &str) -> String {
    conn.query_row(
        "SELECT country_name FROM country_parameters WHERE country_code = ?1",
        rusqlite::params![country_code],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| country_code.to_string())
}

fn expected_suffix(country_code: &str) -> &'static str {
    match country_code {
        "IN" => ".NS",
        "GB" => ".L",
        "AU" => ".AX",
        _ => "",
    }
}

fn transform_from_python_json(
    py: &Value,
    ticker: &str,
    country_code: &str,
    mparams: &MarketParamsSnapshot,
    country_name: &str,
) -> Option<FetchResult> {
    let t = ticker.trim().to_uppercase();
    let cc = country_code.trim().to_uppercase();
    // Suffix mismatch warning — log but do not block
    let expected = expected_suffix(&cc);
    let ticker_has_suffix = t.contains('.');
    let suffix_ok = if expected.is_empty() {
        !ticker_has_suffix
    } else {
        t.ends_with(expected)
    };
    if !suffix_ok {
        eprintln!("[warn] ticker suffix mismatch: ticker={} country={} expected suffix={:?}", t, cc, expected);
    }
    let price = py.get("price").and_then(|v| v.as_f64());
    let mut market_cap = py.get("market_cap").and_then(|v| v.as_f64());
    // Fallback: price * shares_outstanding when market_cap is None (per user decision)
    if market_cap.is_none() {
        // shares_outstanding may be i64 or f64 in python json
        let shares = py.get("shares_outstanding").or_else(|| py.get("shares")).and_then(|v| {
            if v.is_null() { None } else { v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)) }
        });
        if let (Some(p), Some(s)) = (price, shares) {
            if p > 0.0 && s > 0.0 && p.is_finite() && s.is_finite() {
                market_cap = Some(p * s);
                eprintln!("[info] market_cap fallback price*shares for {}: {} * {} = {}", t, p, s, p*s);
            }
        }
    }
    let currency = py.get("currency").and_then(|v| v.as_str()).unwrap_or("INR").to_string();
    let exchange = py.get("exchange").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let beta = py.get("beta").and_then(|v| v.as_f64()).unwrap_or(0.2);
    let sector = py.get("sector").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let industry = py.get("industry").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let date_str = py.get("date").and_then(|v| v.as_str()).unwrap_or("");
    let price_date = {
        if date_str.len() >= 10 {
            let parts: Vec<&str> = date_str.split('-').collect();
            if parts.len() == 3 {
                format!("{}/{}/{}", parts[2], parts[1], parts[0])
            } else {
                date_str.to_string()
            }
        } else {
            date_str.to_string()
        }
    };
    let company_name = py.get("company_name").and_then(|v| v.as_str()).unwrap_or(&t).to_string();
    let is_fin = is_financial_services(Some(&sector), Some(&industry));
    let mcap_cat = market_cap_category(market_cap, &cc);
    let shares_outstanding = py.get("shares_outstanding").and_then(|v| {
        if v.is_null() { None } else { v.as_i64().map(|i| json!(i)).or_else(|| v.as_f64().map(|f| json!(f))) }
    }).unwrap_or(Value::Null);

    let mut historical: Vec<Value> = Vec::new();
    let to_row = |fye: String, obj: &Value| -> Value {
        let get = |k: &str| -> Value {
            obj.get(k).and_then(|v| {
                if v.is_null() { Some(Value::Null) } else { v.as_f64().map(|f| json!(f)).or_else(|| v.as_i64().map(|i| json!(i))) }
            }).unwrap_or(Value::Null)
        };
        json!({
            "fiscal_year_end": fye,
            "revenue": get("revenue"),
            "operating_income": get("operating_income"),
            "ebitda": get("ebitda"),
            "net_income": get("net_income"),
            "total_debt": get("total_debt"),
            "total_equity": get("total_equity"),
            "cash_equivalents": get("cash_equivalents"),
            "free_cash_flow": get("free_cash_flow"),
        })
    };
    if let Some(ttm) = py.get("ttm").and_then(|v| v.as_object()) {
        let fye = "TTM".to_string();
        let ttm_val = Value::Object(ttm.clone());
        historical.push(to_row(fye, &ttm_val));
    }
    let mut yearly_keys: Vec<String> = py.as_object().unwrap().keys().filter(|k| k.contains('-') && k.len() == 10 && *k != "ttm").cloned().collect();
    yearly_keys.sort_by(|a,b| b.cmp(a));
    for k in yearly_keys.iter().take(4) {
        if let Some(obj) = py.get(k) {
            let fye = format!("{}/{}", &k[5..7], &k[0..4]);
            historical.push(to_row(fye, obj));
        }
    }
    if historical.len() < 3 {
        return None;
    }
    let (status, error_message) = {
        let mut s = "completed".to_string();
        let mut e = None;
        if historical.len() < 3 {
            s = "error_insufficient_history".to_string();
            e = Some(format!("Less than 3 years of history (got {})", historical.len()));
        } else if price.is_none() || price.unwrap() <= 0.0 || market_cap.is_none() || market_cap.unwrap() <= 0.0 {
            s = "error_no_data".to_string();
            e = Some("Missing or non-positive price/market cap".to_string());
        } else {
            let mut consec = 0;
            for row in &historical {
                if let Some(v) = row.get("operating_income").and_then(|x| x.as_f64()) {
                    if v < 0.0 { consec += 1; if consec >=2 { s="error_not_profitable".to_string(); e=Some("2+ consecutive years of negative operating income".to_string()); break; } } else { consec=0; }
                } else { consec=0; }
            }
            if s=="completed" && is_fin {
                let all_null = historical.iter().all(|r| r.get("net_income").map(|v| v.is_null()).unwrap_or(true));
                if all_null { s="error_no_data".to_string(); e=Some("net_income missing for financial services".to_string()); }
            }
        }
        (s,e)
    };

    let company_info = json!({
        "company_name": company_name,
        "ticker": t,
        "exchange": exchange,
        "country": country_name,
        "sector": sector,
        "industry": industry,
        "currency_code": currency,
        "is_financial_services": is_fin,
    });
    let current_market_data = json!({
        "current_price": price.map(|v| json!(v)).unwrap_or(Value::Null),
        "market_cap": market_cap.map(|v| json!(v)).unwrap_or(Value::Null),
        "market_cap_category": mcap_cat.map(|s| json!(s)).unwrap_or(Value::Null),
        "price_date": json!(price_date),
        "shares_outstanding": shares_outstanding,
    });
    let market_parameters = json!({
        "risk_free_rate": mparams.risk_free_rate,
        "market_risk_premium": mparams.market_risk_premium,
        "corporate_tax_rate": mparams.corporate_tax_rate,
        "nominal_gdp_growth": mparams.nominal_gdp_growth,
    });
    let financials = json!({
        "beta": beta,
        "historical_data": historical,
    });
    let parsed = json!({
        "company_info": company_info.clone(),
        "current_market_data": current_market_data.clone(),
        "market_parameters": market_parameters.clone(),
        "financials": financials.clone(),
    });
    let raw = py.clone();
    Some(FetchResult {
        company_info,
        current_market_data,
        market_parameters,
        financials,
        status,
        error_message,
        raw_response: raw,
        parsed_financials: parsed,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PriceResult {
    pub price: f64,
    pub price_date: String,
    pub currency: String,
}

pub fn fetch_price_only(ticker: &str) -> Result<PriceResult, String> {
    let t = ticker.trim().to_uppercase();
    if t.is_empty() { return Err("ticker empty".to_string()); }
    let v = crate::yahoo::client::fetch_yahoo_price_via_python(&t).ok_or_else(|| format!("price fetch failed for {t}: python get_price_only returned no data"))?;
    let price = v.get("price").and_then(|x| x.as_f64()).ok_or_else(|| format!("price missing for {t}"))?;
    if !price.is_finite() || price <= 0.0 { return Err(format!("invalid price {price} for {t}")); }
    let currency = v.get("currency").and_then(|x| x.as_str()).unwrap_or("INR").to_string();
    let date_str = v.get("date").and_then(|x| x.as_str()).unwrap_or("");
    let price_date = if date_str.len() >= 10 {
        let parts: Vec<&str> = date_str.split('-').collect();
        if parts.len() == 3 { format!("{}/{}/{}", parts[2], parts[1], parts[0]) } else { date_str.to_string() }
    } else { date_str.to_string() };
    Ok(PriceResult { price, price_date, currency })
}

/// Python is the sole Yahoo fetch method — source kept at `py-scripts/yahoo_stock_extract.py`
pub fn fetch_and_transform_with_params(
    ticker: &str,
    country_code: &str,
    mparams: MarketParamsSnapshot,
    country_name: String,
) -> Result<FetchResult, String> {
    let t = ticker.trim().to_uppercase();
    if t.is_empty() {
        return Err("ticker empty".to_string());
    }
    let cc = country_code.trim().to_uppercase();
    if let Some(py) = crate::yahoo::client::fetch_yahoo_extract_via_python(&t) {
        if let Some(fr) = transform_from_python_json(&py, &t, &cc, &mparams, &country_name) {
            if fr.financials.get("historical_data").and_then(|v| v.as_array()).map(|a| a.len() >= 3).unwrap_or(false) {
                return Ok(fr);
            }
        }
    }
    Err(format!("yahoo fetch failed for {t}: python yahoo_stock_extract returned no data"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn validate_historical(historical: &[Value], price: Option<f64>, mcap: Option<f64>) -> (String, Option<String>) {
        if historical.len() < 3 {
            return ("error_insufficient_history".to_string(), Some(format!("Less than 3 years of history (got {})", historical.len())));
        }
        if price.is_none() || price.unwrap() <= 0.0 || mcap.is_none() || mcap.unwrap() <= 0.0 {
            return ("error_no_data".to_string(), Some("Missing or non-positive price/market cap".to_string()));
        }
        let mut consec_neg = 0;
        for row in historical.iter() {
            let op = row.get("operating_income").and_then(|v| v.as_f64());
            if let Some(v) = op {
                if v < 0.0 {
                    consec_neg += 1;
                    if consec_neg >= 2 { return ("error_not_profitable".to_string(), Some("2+ consecutive years of negative operating income".to_string())); }
                } else { consec_neg = 0; }
            } else { consec_neg = 0; }
        }
        ("completed".to_string(), None)
    }

    #[test]
    fn validation_insufficient() {
        let hist: Vec<Value> = vec![json!({"fiscal_year_end":"03/2025","operating_income":100.0})];
        let (s, _) = validate_historical(&hist, Some(100.0), Some(1e9));
        assert_eq!(s, "error_insufficient_history");
    }
    #[test]
    fn validation_not_profitable() {
        let hist = vec![
            json!({"fiscal_year_end":"03/2025","operating_income":-10.0}),
            json!({"fiscal_year_end":"03/2024","operating_income":-5.0}),
            json!({"fiscal_year_end":"03/2023","operating_income":10.0}),
        ];
        let (s, _) = validate_historical(&hist, Some(100.0), Some(1e9));
        assert_eq!(s, "error_not_profitable");
    }
}
