# DECISIONS — Stocksy Desktop

Locked architectural rulings. Companion to `00`–`07` in this directory. Any doc/spec conflict is
resolved by the ruling below (later decisions override earlier docs).

| # | Decision | Ruling | Overrides |
|---|---|---|---|
| D1 | LLM provider set | **Anthropic + DeepSeek + OpenAI + Kimi + Custom (4+1)**. Relabel: F3.1→Anthropic, F3.2→OpenAI, F3.3→DeepSeek, F3.4→Kimi, F3.5→Custom (Local/Custom). Later feature IDs unchanged | doc/01 F3.3 (Google Gemini not first-class; reachable via Custom) |
| D2 | API key storage | **SQLite `secrets` table with XOR obfuscation** — machine-derived XOR key (`stocksy-desktop:{HOME}:{USER}`, 256-byte repeating), value base64-encoded before storage. Prevents plaintext keys in the DB file; no OS keychain or external crate dependency. Module: `keyring_store.rs` (misnamed; no actual keyring used). 9 managed keys enumerated in `SECRET_KEYS`. | doc/05 "Tauri Stronghold" (archived/deprecated in v2); supersedes earlier "OS keychain via `keyring`" ruling |
| D3 | DCF engine ground truth | **Port `my-dcf.json` deterministic formulas + doc/03 §3–§10**, locked by golden tests | — |
| D4 | DCF Defaults settings | **Math fixed in Rust** per doc/05; formula constants surfaced **read-only on the Help page** (`#/help` via `dcf_defaults()`), not in Settings. Country params remain the only macro-tunable inputs in Settings (Stage 2 alignment: Web Search/Scrape always-on, no toggle) | doc/01 F9.6 (trimmed) |
| D5 | Report identity | `(ticker, llm_model, price_date)` unique key; interactive overrides never persisted | doc/02 §4 |
| D6 | SQLite access | `rusqlite` direct behind Tauri commands (WAL). Not `tauri-plugin-sql` | — |
| D7 | Frontend source | New **single-user** desktop codebase per doc/04. **Nothing reusable from the stocksy.pro web app** (different product). Native desktop constructs only (native menus, dialogs, keyboard-first, drag regions, window-state). **Light theme only — no dark mode** | doc/04 only |
| D8 | DCF input contract | Rust Yahoo port reproduces `input_findata_example*.json` end-to-end (`company_info` + `current_market_data` + `market_parameters` + `financials`, 5yr incl. TTM). `is_financial_services` derived in Rust from sector/industry **keyword matching**; `market_cap_category` per F1.5 country thresholds; `market_parameters` always from `country_parameters`; rest ported from `yahoo_stock_extract.py` `metrics_mapping` + TTM | yahoo_stock_extract.py + F1.5/F1.6 |
| D9 | `system_prompts` seed keys | Seed the **6 modular modules from doc/05** (`role_and_abort_protocol`, `validation_and_search`, `growth_and_fcf`, `confidence_scoring`, `narrative_and_risks`, `json_schema`). Defaults are structural placeholders; real "ValorBot" content ported from `my-dcf.json` in **Stage 5** | doc/02 §5 (4-key list superseded) |
| D10 | Financial-services discount & NI factor | **Equity cash flows → discount at Cost of Equity (Ke), not WACC** (bank FCF proxy = Net Income × factor is post-interest). `DcfOutput.wacc` stores the actual discount used (Ke for `is_financial_services`, WACC otherwise). NI factor: **default 0.65, clamp 0.40–0.90**, LLM guidance **0.60–0.85** (0.75–0.85 well-capitalized CAR>16%, 0.65–0.75 average, 0.55–0.65 constrained) | doc/03 §4 (0.25) + §3 (WACC for all) |

## F3 relabel mapping

| Feature ID | Doc/01 old | Effective (D1) |
|---|---|---|
| F3.1 | Anthropic Claude | Anthropic (Claude) |
| F3.2 | OpenAI GPT | OpenAI (GPT) |
| F3.3 | Google Gemini | DeepSeek |
| F3.4 | Local/Custom | Kimi (Moonshot) |
| F3.5 | Streaming AI Responses | Custom / Local (OpenAI-compatible: Ollama, LM Studio, MLX) |

> Note: F3.5 feature content ("Streaming AI Responses") is *not* dropped — streaming is
> implemented for all providers; the *identifier slot* F3.5 is rebound to Custom. Reconcile
> numbering in doc/01 during Stage 2.

## Dev utilities

Database inspection is handled by the **DB Browser** dev page in the app (left sidebar, `#/db-browser`, dev builds only). It lists all tables with row counts in a left pane and renders the selected table's rows read-only in a scrollable right pane; double-click a cell to view full text. Read access goes through a dedicated read-only SQLite connection, so the GUI can never mutate data.

DB file: `STOCKSY_DB_PATH` env var overrides; default is
`<OS data dir>/com.stocksy.desktop/stocksy.db`.