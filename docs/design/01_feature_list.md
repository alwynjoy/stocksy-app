# Stocksy Desktop: Feature List

This document outlines the complete feature set for **Stocksy Desktop**, a cross-platform DCF stock analyzer supporting **multiple countries** (India, US, UK, Australia, and more). This is a single-user personal desktop app that fetches financial data primarily from **Yahoo Finance** (free), provides AI-powered DCF analysis via configurable LLM providers (cloud or local), and includes a powerful stock screener.

> **Primary data source**: Yahoo Finance (free, global coverage via yfinance-style extraction — see [py-scripts/yahoo_stock_extract.py](../..//py-scripts/yahoo_stock_extract.py) for reference implementation).
>
> **Data source**: Yahoo Finance is the sole source — no manual fallback (insufficient data → ephemeral `failure`/`error_*`, not persisted).

The features are grouped by category and assigned priority levels:
- **P0**: Must-have for MVP
- **P1**: Important, include in early releases
- **P2**: Nice-to-have, add later

---

## Summary of Features

| Category | Total Features | P0 | P1 | P2 |
| :--- | :---: | :---: | :---: | :---: |
| **FC1. Stock Search & Data** | 8 | 7 | 1 | 0 |
| **FC2. DCF Analysis Engine** | 16 | 16 | 0 | 0 |
| **FC3. AI & LLM Integration** | 8 | 8 | 0 | 0 |
| **FC4. Web Search & Scraping** | 7 | 4 | 3 | 0 |
| **FC5. Local Data Storage** | 8 | 5 | 2 | 1 |
| **FC6. Stock Screener** | 12 | 4 | 6 | 2 |
| **FC7. Visualization & UI** | 13 | 10 | 2 | 1 |
| **FC8. Desktop Features** | 7 | 2 | 3 | 2 |
| **FC9. Configuration & Settings** | 9 | 7 | 2 | 0 |
| **Total** | **88** | **63** | **17** | **8** |

---

## FC1. Stock Search & Data

| ID | Feature | Description | Priority |
| :--- | :--- | :--- | :--- |
| **F1.1** | Ticker Cache & Search | All tickers for a selected country are prefetched from Yahoo Finance and cached locally. Search by name/ticker with autocomplete from cache. New listings fetched on demand for newly listed stocks. | P0 |
| **F1.2** | Financial Data Retrieval | Income statement (annual + TTM), balance sheet, cash flow, beta, shares outstanding — retrieved from Yahoo Finance. Reference implementation: [yahoo_stock_extract.py](file:///Users/aljoge/devworks/stocksy-app/yahoo_stock_extract.py). | P0 |
| **F1.3** | TTM Computation | TTM data is included in the Yahoo Finance extraction script. This feature handles edge cases where TTM needs recomputation from interim quarters. | P2 |
| **F1.4** | Data Quality Validation | Require ≥3 years historical data (5 years preferred), positive price/market cap, reject 2 consecutive years negative operating income. | P0 |
| **F1.5** | Market Cap Classification | Country-appropriate thresholds. India: Mega (>₹1,650,000 Cr), Large, Mid, Small, Micro (<₹2,475 Cr). US/UK/AU: equivalent thresholds in local currency. | P0 |
| **F1.6** | Financial Services Detection | Detect financial services firms by sector/industry classification — use net income proxy model instead of FCF. | P0 |
| **F1.9** | Local Data Caching & Refresh | Cache financial data locally. Yahoo is refetched only when a same-model re-analysis is triggered (fresh vs cached `historical_data` deep equality controls whether the LLM is skipped); different-model analyses reuse the cached financial payload with refreshed country `market_parameters`. | P0 |

> **Removed**: F1.7 (Manual Data Input / JSON Import) — not required; Yahoo Finance is the sole source. Insufficient data returns an ephemeral `failure` (tracked in queue, not in `analysis_reports`).
> **Removed**: F1.8 (Screener.in Excel Import) — not required.
> **Removed**: F1.10 (API Budget Tracking) — not required since Yahoo Finance is free.

---

## FC2. DCF Analysis Engine

> **Computation split**: Features marked **🤖 AI** are driven by AI/LLM output. Features marked **🔧 Local** are computed locally in Rust using AI-provided inputs. Features marked **🤖+🔧 Hybrid** involve AI providing assumptions which are then used in local computation.

| ID | Feature | Description | Compute | Priority |
| :--- | :--- | :--- | :---: | :--- |
| **F2.1** | WACC Computation | Blume-adjusted beta, size premium, capital structure penalty, cyclicality adjustment, credit spread brackets | 🤖+🔧 Hybrid | P0 |
| **F2.2** | Cost of Equity (CAPM) | Ke = Rf + (Adjusted Beta × Adjusted MRP), constrained 8-20% | 🔧 Local | P0 |
| **F2.3** | Cost of Debt | Kd = (Rf + Credit Spread) × (1 - Tax Rate) | 🔧 Local | P0 |
| **F2.4** | FCF Projections | 5-year explicit forecast with industry-specific conversion factors | 🤖+🔧 Hybrid | P0 |
| **F2.5** | Revenue Growth Assumptions | Years 1-2 from analyst consensus/web research, Years 3-5 convergence to GDP + sector premium | 🤖 AI | P0 |
| **F2.6** | Terminal Value | Gordon Growth Model with cliff constraints (g ≤ Nominal GDP, g ≥ Year5 Growth - 2.5%, WACC - g ≥ 2.00%) | 🔧 Local | P0 |
| **F2.7** | Enterprise Value | Sum of discounted FCFs + discounted Terminal Value | 🔧 Local | P0 |
| **F2.8** | Equity Value & Intrinsic Value | Equity value = EV + Cash - Total Debt; Intrinsic value per share = Equity Value / Shares Outstanding | 🔧 Local | P0 |
| **F2.9** | 3-Scenario Modeling | Optimistic (25%), Expected (55%), Pessimistic (20%) with associated modifiers | 🤖+🔧 Hybrid | P0 |
| **F2.10** | Weighted Fair Value | 0.25 × Optimistic + 0.55 × Expected + 0.20 × Pessimistic | 🔧 Local | P0 |
| **F2.11** | Valuation Verdict | Undervalued / fairly_valued / overvalued based on valuation gap % | 🔧 Local | P0 |
| **F2.12** | 3×3 Sensitivity Analysis | WACC × Growth grid, mark base case, flag unstable models | 🔧 Local | P0 |
| **F2.13** | 5-Dimension Confidence Scoring | Cash Flow Predictability, Competitive Position, Financial Health, Industry Visibility, Management Quality = Overall /100 | 🤖 AI | P0 |
| **F2.14** | Risk/Reward Ratio | \|Downside%\| / (Upside% - Downside%) categorized into risk levels | 🔧 Local | P0 |
| **F2.15** | Interactive Assumption Controls | All DCF assumptions displayed on the report page with user-friendly controls (sliders, inputs) to adjust any parameter and recompute locally via `recompute_local` (triggered by **Recompute** button). Changes are for exploration only — **not stored back** to the saved report. | 🔧 Local | P0 |

> **Removed**: F2.16 (Smart Data Freshness Check / `Check for Updates` button) — not surfaced in UI. Freshness is implicit: same-model re-analysis performs a fresh Yahoo fetch and deep-equality comparison of `historical_data` (`is_same_historical_data` — TTM label normalized to `"TTM"`) to decide whether to skip the LLM (`already_exists`). The Report page exposes `Update CMP` only.

---

## FC3. AI & LLM Integration

> **Architecture** (D1 4+1): 5 providers — Anthropic, DeepSeek, OpenAI, Kimi (Moonshot), plus 1 Custom OpenAI-compatible endpoint. Each has its own `base_url` + API key (secrets table) + Fetch Models + Test Connection. One provider is marked active via `llm.active_provider`. Streaming is supported on all providers.

| ID | Feature | Description | Priority |
| :--- | :--- | :--- | :--- |
| **F3.1** | Cloud LLM: Anthropic (Claude) | User enters API key → Fetch Models button lists available models (Sonnet, Opus, Haiku) → User selects model. Extended thinking budget configurable. | P0 |
| **F3.2** | Cloud LLM: OpenAI (GPT) | User enters API key → Fetch Models button lists available models (GPT-4o, GPT-4-turbo, etc.) → User selects model. | P0 |
| **F3.3** | Cloud LLM: DeepSeek | User enters API key → Fetch Models button lists available models (deepseek-reasoner, deepseek-chat) → User selects model. | P0 |
| **F3.4** | Cloud LLM: Kimi (Moonshot) | User enters API key → Fetch Models button lists available models (moonshot-v1-auto, kimi-*) → User selects model. | P0 |
| **F3.5** | Custom / Local LLM (OpenAI-compatible) | Single unified configuration: user inputs any OpenAI-compatible `base_url` + optional API key. Covers Ollama (`localhost:11434`), LM Studio (`localhost:1234`), MLX, or any self-hosted endpoint. Fetch Models button lists available models. Streaming is implemented here as well (streaming is a cross-provider capability, not a provider slot). | P0 |
| **F3.6** | Configurable System Prompts | Multiple system prompts broken down by DCF task delegation (6 modular modules: `role_and_abort_protocol`, `validation_and_search`, `growth_and_fcf`, `confidence_scoring`, `narrative_and_risks`, `json_schema` per D9). Each prompt is independently editable by the user. Maps to specific AI-driven tasks in F2.x. | P0 |
| **F3.7** | Structured JSON Output | AI outputs schema-compliant JSON for scenarios, sensitivity inputs, confidence scores, and narrative sections | P0 |
| **F3.8** | AI Narrative Analysis | Business context, historical performance, growth projections, opportunities/risks, investment perspective — generated as structured markdown sections | P0 |

> **Relabel per D1**: doc/01 previously listed F3.3=Google Gemini and F3.4=Local/Custom; D1 rebinds F3.3→DeepSeek, F3.4→Kimi, F3.5→Custom (streaming remains cross-provider). Google Gemini remains reachable via Custom endpoint.

---

## FC4. Web Search & Scraping

> All search/scrape providers require API key configuration in Settings → **AI Agent Tools** (F9.3). Search & scrape are **always-on** (no toggle) — the LLM autonomously calls `web_search`/`web_scrape` as needed.

| ID | Feature | Description | Priority |
| :--- | :--- | :--- | :--- |
| **F4.1** | Tavily API Search & Extract | Search for analyst estimates, forward guidance, earnings calls, sector reports (`tavily_search` at `https://api.tavily.com/search`); content extraction (`tavily_extract` at `https://api.tavily.com/extract`, hardcoded `extract_depth: advanced` / `query: "find forward estimates"`). API key in `secrets`. | P0 |
| **F4.2** | ScrapingDog Search & Scrape | Bundled alternative to Brave/Jina: Google Search (`https://api.scrapingdog.com/google`) + Scrape (`https://api.scrapingdog.com/scrape?dynamic=true`). Shares the bundled `Tavily ↔ ScrapingDog` fallback — if the active provider's key is missing, the other provider is tried and a `_fallback_warning` is injected. | P0 |
| **F4.4** | AI Agent Search Integration | AI autonomously searches/scrapes during analysis for forward estimates. Up to 10 agentic turns; each tool result truncated to `MAX_TOOL_RESULT_CHARS=12000` before re-injection; streaming via `pipeline://step` SSE (120s idle timeout, 300s total, 1 retry on `stream error`/`idle timeout` with 1500ms backoff — 507 OOM / 401 / 429 fail-fast). | P0 |
| **F4.5** | Source Citations | Display URLs, relevance scores, snippets used by AI in the report (`search_sources` accumulated across turns). | P1 |
| **F4.6** | Document Filter | Auto-exclude `.pdf`, `.doc`, `.xls` URLs (configurable via `search.exclude_extensions`) from `web_scrape`; skipped before any network call. | P1 |

> **Removed**: F4.3 (Brave Search) — replaced by ScrapingDog (bundled). No `search.brave_key` / `search.brave_search_url` is seeded.
> **Removed**: F4.7 generic **Custom Search/Scrape Provider** — covered by the bundled Tavily + ScrapingDog pair; `search.custom_provider_url` / `search.custom_provider_key` / `scrape.custom_provider_key` are not seeded.

---

## FC5. Local Data Storage

| ID | Feature | Description | Priority |
| :--- | :--- | :--- | :--- |
| **F5.1** | SQLite Database | Single file, ACID, WAL mode local database | P0 |
| **F5.2** | Country Parameters | Risk-free rate, MRP, corporate tax rate, GDP growth. Pre-seeded for multiple countries (India, US, UK, AU). User-editable. | P0 |
| **F5.3** | Saved Reports | Complete DCF reports with all data preserved | P0 |
| **F5.4** | Report Versioning | Each report keyed by stock ticker + LLM model + price date (from Yahoo Finance). Multiple reports per stock supported — different models, different dates. This forms the unique record identity for comparison. | P0 |
| **F5.5** | Dual Search System | (1) **Ticker search**: Search from cached ticker list for a selected country — to find stocks for new analysis or check if a report exists. (2) **Report search**: FTS5 full-text search on stock symbol, name, sector, industry within generated reports. No user search history tracking. | P0 |
| **F5.7** | Report Export | Export stock report as JSON. Financial data is not exported separately. | P1 |
| **F5.8** | Report Import | Import stock reports from the web app database as JSON. No other import formats required. | P1 |
| **F5.9** | Database Backup | Expose DB file location, manual backup/restore | P2 |

> **Removed**: F5.5 (Search History) — no value in retaining user search history.
> **Removed**: F5.10 (API Usage Logging) — not required since Yahoo Finance is free.
> **Modified**: F5.5-F5.6 merged into new F5.5 (Dual Search System).

---

## FC6. Stock Screener

| ID | Feature | Description | Priority |
| :--- | :--- | :--- | :--- |
| **F6.1** | Batch DCF Analysis | Run DCF on a batch of stocks with progress tracking | P0 |
| **F6.2** | Screener Data Table | Sortable, filterable, virtualized table showing all stock reports generated by the user | P0 |
| **F6.3** | Practical Range Filters | Filters focused on decision-making value: confidence score, risk-reward ratio, valuation gap %, revenue growth. Range sliders applicable within these metrics. | P0 |
| **F6.4** | Category Filters | Sector/industry, market cap category, valuation verdict (undervalued/fairly/overvalued), country. Practical filters that help users screen and make investment decisions. | P0 |
| **F6.5** | Built-in Filter Presets | Deep Value, Quality at Fair Price, Low Risk Value, Growth Leaders | P1 |
| **F6.6** | Custom Filter Presets | Save/load user-defined filter combinations | P1 |
| **F6.7** | Multi-column Sorting | Complex sorting on screener data | P1 |
| **F6.8** | Dynamic Macro Adjustment | Sliders for macro variables that instantly recalculate all stocks | P2 |
| **F6.10** | Interactive Screener Charts | Valuation gap histogram, sector breakdown, market cap scatter, confidence distribution. Charts are clickable — click a segment/bar to see which stocks it represents and jump directly to a specific stock report. | P1 |
| **F6.11** | Stock Comparison | Compare stocks across: valuation gap, confidence score, risk-reward ratio, revenue growth. **Also**: compare a single stock's own reports across different models or dates (e.g., Claude vs GPT analysis, or Jan vs Mar analysis). | P1 |
| **F6.12** | Watchlists | Create named lists, add stocks, notes, target prices | P1 |
| **F6.13** | Batch Progress UI | Progress bar, ETA, pause/resume/cancel, error log | P1 |

> **Removed**: F6.9 (Reverse DCF) — not required.
> **Removed**: F6.14 (Incremental Updates) — covered by F2.16 (Smart Data Freshness Check with re-analyze button).

---

## FC7. Visualization & UI

| ID | Feature | Description | Priority |
| :--- | :--- | :--- | :--- |
| **F7.1** | Modern Light Theme | Clean, light-mode design similar to modern desktop financial apps — professional, slick, and readable. Not dark institutional. | P0 |
| **F7.2** | Left Navigation Pane | Collapsible sidebar: Dashboard, New Analysis, My Reports, Screener, Watchlists | P0 |
| **F7.3** | Top Ribbon Menu | Native menus: **File** = Dashboard / My Reports / Screener / Watchlists / New Analysis; **Settings** = LLM Provider / AI Agent Tools / Country Parameters / System Prompts / Database & Logging / Queue & Parallelism / Miscellaneous (+ `Edit` with Undo/Redo/Cut/Copy/Paste, `Stocksy Desktop` app menu with Settings/Quit). Help = `User Guide & Help` (`#/help`). No `Check for Updates`. | P0 |
| **F7.4** | Price Scenario Slider | Fixed slider displaying Bear, Base, Optimistic fair values vs Current Market Price. Includes a button to **update CMP** directly from Yahoo Finance in real-time. | P0 |
| **F7.5** | 3×3 Sensitivity Heatmap | Color-coded WACC × Growth grid | P0 |
| **F7.6** | 5-Dimension Confidence Meter | Progress bars with dynamic fill colors | P0 |
| **F7.7** | 5-Year Forecast Chart | Revenue growth vs operating margin line chart | P0 |
| **F7.8** | FCF Projection Bar Chart | Visualized FCF projections over 5 years | P0 |
| **F7.9** | Historical Financials Charts | Revenue, operating income, net income, FCF trends | P0 |
| **F7.12** | AI Analysis Panel | Rendered markdown with structured sections | P0 |
| **F7.13** | Screener Radar Chart | 5 confidence dimensions comparison across stocks | P1 |
| **F7.15** | Responsive Layout | Adaptive to window resize | P0 |
| **F7.16** | PDF Report Export | Downloadable PDF with all charts, heatmap, confidence, AI narrative | P1 |

> **Removed**: F7.10 (Stock Price Chart) — not required.
> **Removed**: F7.11 (Financial Statements Table) — not required.
> **Removed**: F7.14 (Light Theme Option) — not needed since light mode is now the default.
> **Removed**: F7.17 (Keyboard Shortcuts) — deprioritized.

---

## FC8. Desktop Features

| ID | Feature | Description | Priority |
| :--- | :--- | :--- | :--- |
| **F8.1** | Cross-platform | macOS, Windows, Linux | P0 |
| **F8.2** | Native File Dialogs | OS-native save/open for import/export | P0 |
| **F8.3** | Window State Persistence | Remember size, position, sidebar state | P1 |
| **F8.4** | Desktop Notifications | Batch completion, long-running task alerts | P1 |
| **F8.6** | System Tray | Minimize to tray, quick access menu | P2 |
| **F8.7** | Splash Screen | Loading screen during initialization | P2 |

> **Removed**: F8.5 (Auto-updater / `Check for Updates` via GitHub Releases) — not implemented. No `tauri-plugin-updater`, no `tauri.conf.json` updater config, no `check_updates` command or Help menu item.

---

## FC9. Configuration & Settings

| ID | Feature | Description | Priority |
| :--- | :--- | :--- | :--- |
| **F9.1** | LLM Provider Selection | Choose provider among Anthropic / DeepSeek / OpenAI / Kimi / Custom (D1 4+1) — enter API key, Fetch Models button, select model, Test Connection, Set as Default. Each provider has its own `base_url`. | P0 |
| **F9.2** | Financial Data Config | Yahoo Finance is the sole data source (free). Configuration for country selection and data refresh preferences. | P0 |
| **F9.3** | AI Agent Tools — Web Search & Extract | Tavily **Search** (`https://api.tavily.com/search`) + **Extract** (`https://api.tavily.com/extract`) and **ScrapingDog** Search (`https://api.scrapingdog.com/google`) + Scrape (`https://api.scrapingdog.com/scrape`) — bundled pair, `search.active_provider` radio `tavily` (recommended) / `scrapingdog`. API keys in `secrets` (`search.tavily_key`, `search.scrapingdog_key`). Always-on, no enable toggle. URLs are editable in `app_settings` for future-proofing. `search.max_results` (default 5) + `search.exclude_extensions` (`[".pdf",".doc",".xls"]`) also live here. | P0 |
| **F9.4** | *Merged into F9.3* | Former `Jina Reader / Brave / Custom provider` — replaced by the bundled Tavily + ScrapingDog pair above. Not seeded. | — |
| **F9.5** | Country Parameters Editor | Edit risk-free rate, MRP, tax rate, GDP growth per country. Pre-seeded for India, US, UK, AU. Add-country supported. | P0 |
| **F9.6** | DCF Defaults (Help, read-only) | Formula constants are fixed in Rust (D4) and surfaced **read-only on the Help page** (`#/help`), not in Settings. Includes projection years, terminal growth defaults, WACC/sensitivity constants etc. | P0 |
| **F9.7** | Appearance Config | Theme configuration (light mode default) | P0 |
| **F9.8** | Data Management | View DB location/size + row counts, manual backup/restore of `.sbk` (`STOCKYBK` MessagePack) via native dialogs; debug logs. No `clear cache` button — cache is managed via DB (ticker `>20k` rows, financial cache per `(ticker,country)`). | P1 |
| **F9.9** | System Prompt Editor | View/edit 6 modular system prompts (`role_and_abort_protocol`, `validation_and_search`, `growth_and_fcf`, `confidence_scoring`, `narrative_and_risks`, `json_schema` per D9) | P1 |
