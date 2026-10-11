use rusqlite::{params, Connection};

type CountrySeed = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

const COUNTRY_ROWS: [CountrySeed; 4] = [
    (
        "country-in",
        "IN",
        "India",
        "INR",
        "₹",
        "0.16",
        "0.07067",
        "0.30",
        "0.1033",
        ".NS",
        r#"{"risk_free_rate":{"IN":"https://www.investing.com/rates-bonds/india-10-year-bond-yield"},"tax_rate":"https://pages.stern.nyu.edu/~adamodar/New_Home_Page/datafile/ctryprem.html","nominal_gdp_growth":{"IN":"https://www.ceicdata.com/en/indicator/india/nominal-gdp-growth"}}"#,
    ),
    (
        "country-us",
        "US",
        "United States",
        "USD",
        "$",
        "0.12",
        "0.04996",
        "0.25",
        "0.06562",
        "",
        r#"{"risk_free_rate":{"US":"https://www.investing.com/rates-bonds/u.s.-10-year-bond-yield"},"tax_rate":"https://pages.stern.nyu.edu/~adamodar/New_Home_Page/datafile/ctryprem.html","nominal_gdp_growth":{"US":"https://www.ceicdata.com/en/indicator/united-states/nominal-gdp-growth"}}"#,
    ),
    (
        "country-gb",
        "GB",
        "United Kingdom",
        "GBP",
        "£",
        "0.08",
        "0.05298",
        "0.25",
        "0.04059",
        ".L",
        r#"{"risk_free_rate":{"GB":"https://www.investing.com/rates-bonds/uk-10-year-bond-yield"},"tax_rate":"https://pages.stern.nyu.edu/~adamodar/New_Home_Page/datafile/ctryprem.html","nominal_gdp_growth":{"GB":"https://www.ceicdata.com/en/indicator/united-kingdom/nominal-gdp-growth"}}"#,
    ),
    (
        "country-au",
        "AU",
        "Australia",
        "AUD",
        "A$",
        "0.11",
        "0.05292",
        "0.30",
        "0.05251",
        ".AX",
        r#"{"risk_free_rate":{"AU":"https://www.investing.com/rates-bonds/australia-10-year-bond-yield"},"tax_rate":"https://pages.stern.nyu.edu/~adamodar/New_Home_Page/datafile/ctryprem.html","nominal_gdp_growth":{"AU":"https://www.ceicdata.com/en/indicator/australia/nominal-gdp-growth"}}"#,
    ),
];

const PROMPT_ROWS: [(&str, &str, &str, &str, &str); 6] = [
    (
        "prompt-role-and-abort",
        "role_and_abort_protocol",
        "Role & Abort Protocol",
        r#"You are ValorBot, an AI financial analyst providing structured INPUTS to a deterministic DCF engine running locally on Stocksy Desktop. You do NOT perform DCF computations yourself — the app's Rust engine handles all math (WACC, FCF projections, terminal value, enterprise value, scenario weighting, sensitivity matrix, and risk/reward ratio).

YOUR JOB is to:
1. Research forward estimates and qualitative context using the provided tools
2. Provide structured ASSUMPTIONS the Rust engine needs: revenue growth rates (years 1-5), operating margin projections (years 1-5), terminal operating margin, terminal growth rate, FCF conversion factor (or net-income proxy factor for financial services), cyclicality adjustment bps for the WACC equity risk premium
3. Score 5 confidence dimensions with evidence-based reasoning
4. Produce narrative sections for the investment report
5. Return exactly one JSON object matching the schema — no other output

ABORT (set "abort": true) if ANY of these are true:
- Forward estimates search fails AND historical revenue trend is erratic or declining
- is_financial_services = true but net_income is null in all historical years
- Fewer than 3 years of historical financials in input JSON

On abort: return minimal JSON with abort_reason code. Do NOT fabricate estimates."#,
        "AI identity, computation split with Rust engine, and abort rules.",
    ),
    (
        "prompt-validation-search",
        "validation_and_search",
        "Input Data, Tools & Search Strategy",
        r#"## INPUT DATA

You receive a JSON object containing:
- company_info: name, ticker, exchange, country, sector, industry, currency_code, is_financial_services, market_cap_category
- current_market_data: current_price, market_cap, price_date
- country_parameters (from local DB — DO NOT search for these): risk_free_rate, equity_risk_premium, corporate_tax_rate, nominal_gdp_growth
- financials: beta (raw), shares_outstanding, historical_data (newest first, min 3 years): fiscal_year_end, revenue, operating_income, ebitda, net_income, total_debt, total_equity, cash_equivalents, free_cash_flow

All monetary values are absolute currency units. Rates are decimals (0.07067 = 7.067%).

## TOOLS

web_search(query, max_results): Search for analyst estimates, forward guidance, earnings transcripts, sector growth outlooks, management commentary.

web_scrape(url): Extract content from a URL. NEVER scrape .pdf .doc .docx .ppt .xls .xlsx — skip these entirely.

Rules: use tools autonomously; do not search for data already in input JSON; max 3 search attempts; record all accessed URLs in search_sources.

## SEARCH STRATEGY BY COUNTRY

India: MoneyControl, Economic Times Markets, CRISIL/ICRA sector reports, BSE/NSE filings, screener.in, company IR pages
US: SEC EDGAR, earnings call transcripts, company IR page, financial news consensus
UK: LSE RNS announcements, Morningstar UK, company IR, Companies House
Australia: ASX announcements, AFR, Morningstar AU, company IR

Prefer: official filings/IR → earnings transcripts → broker summaries → sector reports

## ABORT DECISION AFTER SEARCH
- Estimates found: status = "success", proceed
- No estimates but clear historical trend: status = "success_with_limitations", mark each year as "historical_extrapolation"
- No estimates + erratic/declining history: abort, abort_reason = "forward_estimates_unavailable""#,
        "Input JSON structure, web_search/web_scrape tool rules, country-specific search guidance.",
    ),
    (
        "prompt-growth-fcf",
        "growth_and_fcf",
        "Growth, FCF & WACC Inputs",
        r#"## ASSUMPTIONS TO PROVIDE (Rust computes all DCF math from these)

### 1. Revenue Growth Rates — years 1 to 5 (decimals, array of 5)
Years 1-2: anchor to analyst consensus or company guidance from web_search. If unavailable, use recent historical CAGR and mark as "historical_extrapolation".
Years 3-5: converge from Year 2 rate toward nominal_gdp_growth + sector_premium.
  Tech/Pharma/Healthcare: +2-3% | Consumer/FMCG: +1-2% | Industrial/Auto: +0.5-1.5% | Utilities/Infra: 0-1%

IMPORTANT: If the most recent historical data point (historical_data[0]) is a TTM period (fiscal_year_end month != 03), its growth rate represents actual recent momentum — anchor Year 1 closer to this observed rate, not just analyst estimates which may lag.

### 2. Operating Margin Projections — years 1 to 5 (decimals, array of 5)
Assess from historical trend in input JSON, analyst targets, company guidance.

### 3. Terminal Margin (decimal)
Operating margin at end of explicit period used as basis for terminal FCF. Typically Year 5 margin or slight improvement if expansion is ongoing.

### 4. Terminal Growth Rate (decimal)
Perpetual growth rate for Gordon Growth Model.

MANDATORY: Show the cliff constraint check explicitly in terminal_growth_derivation:
  Step 1: Ceiling check:   g <= nominal_gdp_growth (e.g., 0.085 <= 0.1033 ✓)
  Step 2: Cliff check:     Year5_growth_rate - 0.025 = X; g >= X (e.g., 0.09 - 0.025 = 0.065; 0.085 >= 0.065 ✓)
  Step 3: Spread check:    estimated_WACC - g >= 0.020 (e.g., 0.132 - 0.085 = 0.047 >= 0.020 ✓)
If any check fails, adjust g until all three pass before emitting the value.

Derivation categories:
  Market leader / pricing power / recurring revenue: GDP - 0.010
  Established / differentiated / stable margins:     GDP - 0.015
  Commodity / fragmented / thin margins:             GDP - 0.020
  Declining industry:                                GDP - 0.030

### 5. FCF Conversion Factor (non-financial-services only, null for banks)
YOU determine based on: historical FCF/operating_income from input JSON (use if 3+ years available and in 0.20-0.95 range); capex intensity for this company and sector.
If you omit this or return null for a non-financial company, the Rust engine applies a sector fallback (IT:0.80, FMCG/Pharma:0.70, Auto/Industrial:0.58, Utilities:0.43).
Provide fcf_conversion_rationale (1-2 sentences).
Note on null TTM FCF: If historical_data[0] has fiscal_year_end == "TTM" and free_cash_flow == null (cash-flow not disclosed in the latest interim quarter for this ticker), exclude that TTM row from your FCF conversion baseline and calculate using completed annual periods only (historical_data[1..4]).

### 6. Financial Services NI Factor (financial services only, null otherwise)
If is_financial_services = true: provide an institution-calibrated factor in 0.20-0.85 range based on capital retention needs and regulatory constraints (FCFE = Net Income * factor, where retention = g/ROE):

- Asset Management, Wealth Management & Exchanges (fee-based, asset-light, minimal capital lockup): 0.70 - 0.85 (e.g. BlackRock, HDFC AMC, CME Group, Schroders)
- Diversified Tier-1 Money-Center Banks (CET1 / CAR > 14%, ROE > 15%, high revenue diversity): 0.45 - 0.55 (e.g. JPMorgan, HDFC Bank, Commonwealth Bank, HSBC, ICICI Bank)
- Regional, Mid-Cap & Community Banks (ROE 10-14%, regional credit concentration, higher funding volatility): 0.30 - 0.40 (e.g. US Bancorp, South Indian Bank, Virgin Money, Bank of Queensland)
- Credit Services & Specialized Lending (consumer credit, wholesale/infra finance, high leverage): 0.25 - 0.35 (e.g. Capital One, Synchrony, PFC, Muthoot Finance)
- Policy Lenders, State-Owned or Capital-Strained Banks (sovereign mandates, CET1 < 12%, higher historical credit costs): 0.20 - 0.30 (e.g. PNB, Canara Bank, state development lenders)

Explain your derivation in fcf_conversion_rationale based on the institution's ROE, Tier-1 capital ratio, and business model.
Rust clamps [0.20, 0.90], and applies an intelligent GICS sub-industry fallback if null (asset-mgmt 0.75, diversified mega 0.50 / large 0.45 / mid-small 0.35, regional 0.35, credit 0.30, default 0.40).

### 7. Cyclicality Adjustment Bps (integer: 0, 30, 40, or 50)
Rust adds this to equity risk premium in WACC. If you omit this, Rust defaults to 0.
  0: non-cyclical (staples, utilities, healthcare, software)
  30: mildly cyclical (consumer discretionary, IT services)
  40: moderately cyclical (auto, chemicals, capital goods)
  50: highly cyclical (metals, mining, commodities, shipping)
Provide cyclicality_rationale (1 sentence)."#,
        "Revenue growth, operating margins, terminal inputs with mandatory cliff constraint math, FCF factor (LLM-determined, Rust fallback), cyclicality bps.",
    ),
    (
        "prompt-narrative",
        "narrative_and_risks",
        "Narrative & Risks",
        r#"Produce narrative and risk content. Be specific — cite actual numbers from input JSON and URLs from research. No LaTeX, no HTML.

IMPORTANT SCHEMA PLACEMENT — READ CAREFULLY:
- `narrative` object MUST contain exactly 4 keys: business_description, historical_performance_insights, growth_projection_rationale, terminal_growth_derivation. Nothing else inside `narrative`.
- The following are TOP-LEVEL siblings of `narrative` — NEVER nest them inside `narrative`: scenario_narratives, opportunities, risks, investment_perspective, search_sources.

A. INSIDE `narrative` (exactly 4 strings):
1. business_description: 2-3 sentences on business model, primary revenue drivers, key markets, geographic exposure.

2. historical_performance_insights: ONE single plain-text string containing 3-5 bullet points separated by newline characters, using the • character. Do NOT return a JSON array — it must be a single string. Example:
"• Revenue CAGR of 12% over 3 years (FY23: ₹56.9B → FY26: ₹76.7B).\n• Operating margins expanded from 43.5% to 44.7%, reflecting operating leverage.\n• FCF conversion robust at ~72% of operating income.\n• Balance sheet pristine: net cash, negligible debt (₹325M)."

3. growth_projection_rationale: 1-2 paragraphs explaining Year 1-2 anchor credibility (cite source + URL) and Years 3-5 convergence logic. If historical_data[0] is a TTM, note the observed TTM growth rate and how it influenced Year 1.

4. terminal_growth_derivation: 1 paragraph showing ALL THREE constraint checks explicitly (Ceiling: g vs GDP; Cliff: Year5_growth - 2.5% = X, g >= X; Spread: WACC_estimate - g >= 2%). Then state the chosen category and rate.

B. TOP-LEVEL siblings (outside `narrative`):
5. opportunities (max 3, top-level array): catalyst_name, description, potential_impact, timeline (null if unknown), source_url (null if derived from financials).

6. risks (max 3, top-level array): risk_category, description, potential_impact, likelihood ("low"|"moderate"|"high"), source_url.

7. investment_perspective (top-level object):
   - verdict: 2-3 sentence qualitative view. Do NOT reference a specific fair value price — Rust computes that.
   - monitoring_metrics (2-3): metric_name, target_value (string), red_flag_condition (string).

8. scenario_narratives (top-level object): Write a 1-sentence BUSINESS STORY for each of the three scenarios based on the known offset directions. Do NOT reference fair value numbers — Rust computes those and attaches them later. Describe what business/macro conditions would drive each scenario.
   - optimistic: What conditions (deal wins, margin expansion, macro tailwinds) support +300bps revenue and +200bps margin above base?
   - expected: Brief restatement of the base case business trajectory.
   - pessimistic: What conditions (execution delays, demand slowdown, margin pressure) would cause -300bps revenue and -200bps margin?
   These narratives are displayed to the user alongside the Rust-computed fair values in the scenario cards.
   search_sources (top-level array, separate from tool trace) is defined in the JSON schema prompt."#,
        "Narrative sections: business context, historical insights (single string), growth rationale, terminal constraint math, scenario_narratives (qualitative, pre-computation), opportunities, risks, investment view. v2 clarifies top-level vs nested placement.",
    ),
    (
        "prompt-confidence",
        "confidence_scoring",
        "Confidence Scoring",
        r#"Score 5 confidence dimensions. Each score MUST cite a specific data point or research finding — no generic statements. Rust clamps each to its max bound after receiving your output.

Dimension                 | Max | Key criteria
--------------------------|-----|--------------------------------------------------
Cash Flow Predictability  |  30 | Recurring revenue %, FCF consistency, earnings volatility from historical data
Competitive Position      |  30 | Market share, moat, pricing power, brand strength from research
Financial Health          |  20 | Debt/EBITDA from input JSON, interest coverage, liquidity
Industry Visibility       |  15 | Regulatory clarity, demand predictability, cycle stability
Management Quality        |   5 | Capital allocation track record, governance

Reference scoring bands:
  Cash Flow Predictability: recurring >70% → 25-30 | 40-70% → 15-24 | <40% → 5-14
  Competitive Position: strong moat → 24-30 | moderate → 15-23 | commodity → 5-14
  Financial Health: Debt/EBITDA <2x → 16-20 | 2-4x → 8-15 | >4x → 0-7
  Industry Visibility: stable/regulated → 12-15 | moderate → 6-11 | disruptive → 0-5
  Management Quality: consistent → 4-5 | average → 2-3 | weak → 0-1

Overall = sum of 5 scores (0-100). Level: >=70 High | 40-69 Moderate | <40 Low
Emit: score, max_score, reasoning for each dimension."#,
        "5-dimension confidence rubric (30/30/20/15/5). Evidence required per score.",
    ),
    (
        "prompt-json-schema",
        "json_schema",
        "Structured JSON Output Schema",
        r#"Emit exactly one JSON object. No text before or after. No markdown fences. All rates as decimals. Null for missing optional fields. All strings must be valid JSON strings: escape " as \" and line breaks as \n (backslash-n) — never literal newline or unescaped quote inside any string value.

SUCCESS:
{
  "abort": false, "abort_reason": null,
  "status": "success",
  "limitations": [],
  "dcf_inputs": {
    "revenue_growth_rates": [0.18, 0.15, 0.12, 0.10, 0.09],
    "operating_margins":    [0.21, 0.22, 0.22, 0.23, 0.23],
    "terminal_margin": 0.23,
    "terminal_growth_rate": 0.065,
    "fcf_conversion_factor": 0.72,
    "financial_services_ni_factor": null,
    "cyclicality_adjustment_bps": 0,
    "fcf_conversion_rationale": "string",
    "cyclicality_rationale": "string"
  },
  "confidence": {
    "overall_score": 72, "overall_level": "high",
    "dimensions": {
      "cash_flow_predictability": { "score": 22, "max_score": 30, "reasoning": "string" },
      "competitive_position":     { "score": 24, "max_score": 30, "reasoning": "string" },
      "financial_health":         { "score": 15, "max_score": 20, "reasoning": "string" },
      "industry_visibility":      { "score": 9,  "max_score": 15, "reasoning": "string" },
      "management_quality":       { "score": 4,  "max_score": 5,  "reasoning": "string" }
    }
  },
  "narrative": {
    "business_description": "string",
    "historical_performance_insights": "Sentence 1 covering revenue/margin trend. Sentence 2 covering FCF/net income. Sentence 3 covering debt/leverage and latest quarter.",
    "growth_projection_rationale": "string",
    "terminal_growth_derivation": "string — must include explicit constraint checks: Ceiling g<=GDP, Cliff Year5-2.5%=X g>=X, Spread WACC-g>=2%"
  },
  "scenario_narratives": {
    "optimistic": "1-sentence business story: conditions that would drive +300bps revenue / +200bps margin above base.",
    "expected": "1-sentence base case: the central thesis for how the company grows.",
    "pessimistic": "1-sentence business story: conditions that would cause -300bps revenue / -200bps margin below base."
  },
  "opportunities": [{"catalyst_name":"string","description":"string","potential_impact":"string","timeline":null,"source_url":null}],
  "risks":         [{"risk_category":"string","description":"string","potential_impact":"string","likelihood":"moderate","source_url":null}],
  "investment_perspective": {
    "verdict": "string",
    "monitoring_metrics": [{"metric_name":"string","target_value":"string","red_flag_condition":"string"}]
  },
  "search_sources": [{"source_name":"string","source_url":null,"data_obtained":"string"}]
}

KEY CONSTRAINTS:
- `narrative` MUST contain exactly 4 keys: business_description, historical_performance_insights, growth_projection_rationale, terminal_growth_derivation. Do NOT add opportunities/risks/investment_perspective/scenario_narratives/search_sources inside `narrative` — they are TOP-LEVEL siblings.
- historical_performance_insights MUST be a plain string of 3-5 sentences (no •/- bullets, no JSON array). If you need line breaks use escaped \n (\n) — never literal newline or bullet character inside the string.
- scenario_narratives describes business conditions (not fair values); Rust attaches the computed fair values at save time
- overall_score in confidence MUST equal the exact arithmetic sum of the 5 dimension scores
- search_sources is a top-level array of {source_name, source_url, data_obtained} — record every URL you used
- All string values: escape " as \" and every line break as \n inside the JSON string. Never emit a literal newline character inside any string value. Never trailing comma before } or ].

ABORT (minimal):
{ "abort": true, "abort_reason": "forward_estimates_unavailable", "status": "failure", "company_name": "string", "ticker": "string" }

abort_reason codes: forward_estimates_unavailable | net_income_missing_for_finserv | insufficient_history"#,
        "Complete JSON output schema v2. narrative has exactly 4 keys; scenario_narratives/opportunities/risks/investment_perspective/search_sources are top-level. historical_performance_insights is a plain string paragraph (no bullets). overall_score must be arithmetic sum. Strings must use escaped \\n and \\\".",
    ),
];

pub const SETTING_ROWS: [(&str, &str, &str); 28] = [
    ("llm.active_provider", "anthropic", "llm"),
    ("llm.anthropic.base_url", "https://api.anthropic.com", "llm"),
    ("llm.anthropic.model", "claude-sonnet-4-5", "llm"),
    ("llm.deepseek.base_url", "https://api.deepseek.com", "llm"),
    ("llm.deepseek.model", "deepseek-flash", "llm"),
    ("llm.openai.base_url", "https://api.openai.com", "llm"),
    ("llm.openai.model", "gpt-5", "llm"),
    ("llm.kimi.base_url", "https://api.moonshot.ai/v1", "llm"),
    ("llm.kimi.model", "kimi-k2.6", "llm"),
    ("llm.custom.base_url", "", "llm"),
    ("llm.custom.model", "", "llm"),
    ("llm.thinking_budget", "20000", "llm"),
    ("llm.temperature", "0.3", "llm"),
    ("llm.max_tokens", "32000", "llm"),
    ("analysis.max_parallel", "1", "analysis"),
    ("display.sidebar_collapsed", "false", "display"),
    ("display.window_width", "1400", "display"),
    ("display.window_height", "900", "display"),
    ("search.enabled", "true", "search"),
    ("search.max_results", "5", "search"),
    (
        "search.exclude_extensions",
        "[\".pdf\",\".doc\",\".xls\"]",
        "search",
    ),
    ("search.active_provider", "tavily", "search"),
    ("search.tavily_search_url", "https://api.tavily.com/search", "search"),
    ("scrape.tavily_extract_url", "https://api.tavily.com/extract", "scrape"),
    ("search.scrapingdog_search_url", "https://api.scrapingdog.com/google", "search"),
    ("scrape.scrapingdog_scrape_url", "https://api.scrapingdog.com/scrape", "scrape"),
    ("general.default_market", "US", "general"),
    ("general.pdf_save_folder", "", "general"),
];

pub fn seed_countries(conn: &Connection, force: bool) -> rusqlite::Result<usize> {
    let sql = if force {
        "INSERT OR REPLACE INTO country_parameters (id, country_code, country_name, currency_code, currency_symbol, expected_market_return, risk_free_rate, corporate_tax_rate, nominal_gdp_growth, yahoo_ticker_suffix, reference_urls) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
    } else {
        "INSERT OR IGNORE INTO country_parameters (id, country_code, country_name, currency_code, currency_symbol, expected_market_return, risk_free_rate, corporate_tax_rate, nominal_gdp_growth, yahoo_ticker_suffix, reference_urls) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
    };
    let mut inserted = 0usize;
    for (id, code, name, currency, symbol, emr, rf, tax, gdp, suffix, urls) in COUNTRY_ROWS {
        inserted += conn.execute(
            sql,
            params![
                id,
                code,
                name,
                currency,
                symbol,
                emr.parse::<f64>().unwrap(),
                rf.parse::<f64>().unwrap(),
                tax.parse::<f64>().unwrap(),
                gdp.parse::<f64>().unwrap(),
                suffix,
                urls
            ],
        )?;
    }
    Ok(inserted)
}

pub fn prompt_default_for_key(key: &str) -> Option<(&'static str, &'static str, &'static str, &'static str, &'static str)> {
    for (id, k, name, text, desc) in PROMPT_ROWS {
        if k == key {
            return Some((id, k, name, text, desc));
        }
    }
    None
}

pub fn seed_prompts(conn: &Connection, force: bool) -> rusqlite::Result<usize> {
    let sql = if force {
        "INSERT OR REPLACE INTO system_prompts (id, prompt_key, prompt_name, prompt_text, description, is_active, version) VALUES (?1, ?2, ?3, ?4, ?5, 1, 2)"
    } else {
        "INSERT OR IGNORE INTO system_prompts (id, prompt_key, prompt_name, prompt_text, description, is_active, version) VALUES (?1, ?2, ?3, ?4, ?5, 1, 2)"
    };
    let mut inserted = 0usize;
    for (id, key, name, text, desc) in PROMPT_ROWS {
        inserted += conn.execute(sql, params![id, key, name, text, desc])?;
    }
    // Bump existing v1 prompts to v2 (force reseed for schema-placement fix)
    if !force {
        for (id, key, name, text, desc) in PROMPT_ROWS {
            if matches!(key, "narrative_and_risks" | "json_schema") {
                let _ = conn.execute(
                    "UPDATE system_prompts SET prompt_text = ?1, description = ?2, version = 2, updated_at = datetime('now') WHERE prompt_key = ?3 AND version = 1",
                    params![text, desc, key],
                );
                // also handle legacy id mismatch if any
                let _ = conn.execute(
                    "UPDATE system_prompts SET id = ?1, prompt_name = ?2 WHERE prompt_key = ?3",
                    params![id, name, key],
                );
            }
        }
        // Migrate json_schema from bullet-based v2 to escaped-\\n paragraph v3 (ICEMAKE-NS fix, no deps)
        if let Some((_, k, _, new_text, new_desc)) = prompt_default_for_key("json_schema") {
            let _ = conn.execute(
                "UPDATE system_prompts SET prompt_text = ?1, description = ?2, version = 3, updated_at = datetime('now') WHERE prompt_key = ?3 AND (prompt_text LIKE '%• Bullet 1.%' OR version = 2)",
                params![new_text, new_desc, k],
            );
        }
        // Migrate growth_and_fcf from 0.20-0.30 v2 to 0.60-0.85 v3 (NI factor 0.65)
        if let Some((_, k, _, new_text, new_desc)) = prompt_default_for_key("growth_and_fcf") {
            let _ = conn.execute(
                "UPDATE system_prompts SET prompt_text = ?1, description = ?2, version = 3, updated_at = datetime('now') WHERE prompt_key = ?3 AND (prompt_text LIKE '%0.20-0.30%' OR version = 2)",
                params![new_text, new_desc, k],
            );
        }
        // Migrate growth_and_fcf to 4: generic null TTM FCF note (TTM-free baseline)
        if let Some((_, k, _, new_text, new_desc)) = prompt_default_for_key("growth_and_fcf") {
            let _ = conn.execute(
                "UPDATE system_prompts SET prompt_text = ?1, description = ?2, version = 4, updated_at = datetime('now') WHERE prompt_key = ?3 AND (version = 3 OR prompt_text NOT LIKE '%Note on null TTM FCF:%')",
                params![new_text, new_desc, k],
            );
        }
        // Migrate growth_and_fcf to 5: universal 5-tier NI factor calibration (global banks)
        if let Some((_, k, _, new_text, new_desc)) = prompt_default_for_key("growth_and_fcf") {
            let _ = conn.execute(
                "UPDATE system_prompts SET prompt_text = ?1, description = ?2, version = 5, updated_at = datetime('now') WHERE prompt_key = ?3 AND (version = 4 OR prompt_text LIKE '%0.60-0.85%' OR prompt_text LIKE '%Well-capitalized bank%')",
                params![new_text, new_desc, k],
            );
        }
    }
    // Ensure json_schema and growth_and_fcf are version 3 even for fresh force inserts (PROMPT_ROWS still seeds v2)
    let _ = conn.execute("UPDATE system_prompts SET version = 3, updated_at = datetime('now') WHERE prompt_key = 'json_schema' AND version = 2", []);
    let _ = conn.execute("UPDATE system_prompts SET version = 5, updated_at = datetime('now') WHERE prompt_key = 'growth_and_fcf' AND version < 5", []);
    Ok(inserted)
}

pub fn seed_settings(conn: &Connection, force: bool) -> rusqlite::Result<usize> {
    let sql = if force {
        "INSERT OR REPLACE INTO app_settings (key, value, category) VALUES (?1, ?2, ?3)"
    } else {
        "INSERT OR IGNORE INTO app_settings (key, value, category) VALUES (?1, ?2, ?3)"
    };
    let mut inserted = 0usize;
    for (key, value, category) in SETTING_ROWS {
        inserted += conn.execute(sql, params![key, value, category])?;
    }
    // Backfill pdf save folder to OS Downloads if empty (fresh install or legacy DB)
    if !force {
        if let Ok(v) = crate::db::get_setting(conn, "general.pdf_save_folder") {
            if v.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                if let Some(dl) = dirs::download_dir() {
                    let _ = crate::db::set_setting(conn, "general.pdf_save_folder", &dl.to_string_lossy(), "general");
                }
            }
        }
    }
    // Migrate old defaults to new seeds if user never customized (preserves custom values)
    if !force {
        let migrations = [
            ("llm.thinking_budget", "10000", "20000"),
            ("llm.thinking_budget", "24000", "20000"),
            ("llm.thinking_budget", "15488", "20000"),
            ("llm.temperature", "0.1", "0.3"),
            ("llm.max_tokens", "8192", "32000"),
            ("llm.max_tokens", "16000", "32000"),
            ("llm.anthropic.model", "claude-sonet-4-5", "claude-sonnet-4-5"),
            ("llm.kimi.base_url", "https://api.moonshot.cn/v1", "https://api.moonshot.ai/v1"),
        ];
        for (key, old, new_val) in migrations {
            if let Ok(Some(cur)) = crate::db::get_setting(conn, key) {
                if cur == old {
                    let _ = crate::db::set_setting(conn, key, new_val, "llm");
                }
            }
        }
    }
    Ok(inserted)
}

// ── Ticker cache seed from py-scripts/tickers_*.json (pre-seeded per Stage 3) ──

const TICKERS_US_JSON: &str = include_str!("../../py-scripts/tickers_US.json");
const TICKERS_IN_JSON: &str = include_str!("../../py-scripts/tickers_IN.json");
const TICKERS_GB_JSON: &str = include_str!("../../py-scripts/tickers_GB.json");
const TICKERS_AU_JSON: &str = include_str!("../../py-scripts/tickers_AU.json");

fn parse_tickers_json(json_str: &str) -> Vec<(String, String, String)> {
    // Each file is { region, type, reported_total, count, deduped, tickers: [{symbol,name,exchange,...}] }
    let v: serde_json::Value = serde_json::from_str(json_str).unwrap_or(serde_json::Value::Null);
    let arr = v.get("tickers").and_then(|a| a.as_array());
    let mut out = Vec::new();
    if let Some(list) = arr {
        for item in list {
            let symbol = item.get("symbol").and_then(|s| s.as_str()).unwrap_or("").trim();
            let name = item.get("name").and_then(|s| s.as_str()).unwrap_or("").trim();
            let exchange = item.get("exchange").and_then(|s| s.as_str()).unwrap_or("").trim();
            if symbol.is_empty() {
                continue;
            }
            out.push((symbol.to_string(), name.to_string(), exchange.to_string()));
        }
    }
    out
}

pub fn seed_tickers(conn: &Connection, force: bool) -> rusqlite::Result<usize> {
    // If not forced and cache already populated, skip (idempotent)
    if !force {
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ticker_cache", [], |r| r.get(0))
            .unwrap_or(0);
        if count > 0 {
            return Ok(0);
        }
    }
    // When forced, wipe existing seeded rows (keep manually added? For now clear all)
    if force {
        conn.execute("DELETE FROM ticker_cache", [])?;
    }

    let sql = "INSERT OR IGNORE INTO ticker_cache (id, ticker, company_name, exchange, country_code) VALUES (?1, ?2, ?3, ?4, ?5)";
    let mut inserted = 0usize;
    let sources: [(&str, &str); 4] = [
        (TICKERS_US_JSON, "US"),
        (TICKERS_IN_JSON, "IN"),
        (TICKERS_GB_JSON, "GB"),
        (TICKERS_AU_JSON, "AU"),
    ];
    for (json_str, country) in sources {
        let tickers = parse_tickers_json(json_str);
        for (symbol, name, exchange) in tickers {
            let id = format!("ticker-{}-{}", country.to_lowercase(), symbol.to_lowercase().replace('.', "-").replace('/', "-"));
            let n = conn.execute(
                sql,
                params![id, symbol, if name.is_empty() { None::<String> } else { Some(name) }, if exchange.is_empty() { None::<String> } else { Some(exchange) }, country],
            )?;
            inserted += n;
        }
    }
    Ok(inserted)
}
