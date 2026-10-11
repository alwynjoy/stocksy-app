# 07 DCF Engine Appendix — Deterministic Values, Migrations & Pipeline Internals

This appendix locks **deterministic midpoints, migrations, and pipeline internals** that the main `03` describes as bands or omits for brevity. Values here are authoritative — they match `src-tauri` code at `2026-10-10` and are covered by `cargo test 76`.

> Related: `03_design_dcf_engine.md` (formulas), `02_design_database.md` (seeds), `05_design_configuration.md` (settings). Decisions D1–D4/D8 are summarized in `00_design_index.md:35`.

---

## 1. WACC Deterministic Midpoints

Doc ranges → code midpoints (`engine/wacc.rs:22-57`):

| Band | Doc band | Code (midpoint) | Note |
|---|---|---|---|
| Mega-cap | −50 to −80 bps | **−65 bps** (`-0.0065`) | funding-cost concession |
| Large-cap | `0` | **`0`** | anchor |
| Mid-cap | +30 to +50 bps | **`+40 bps`** (`0.0040`) | |
| Small-cap | +50 to +100 bps | **`+75 bps`** (`0.0075`) | |
| Micro-cap | +100 to +150 bps | **`+125 bps`** (`0.0125`) | |
| **Debt/MCap >50%** | `+50 bps` | **`+0.005`** — **zeroed for financial services** (`wacc.rs:75`) | |
| **Cyclicality** | +30–50 bps (LLM) | `bps ∈{0,30,40,50}` else `0` (`mod.rs:256`), `bps/10000` (`wacc.rs:77`), also zeroed for finserv | `40` is valid though doc said 30–50 |

**Credit spread vs leverage** (`wacc.rs:42-57`, non-finite → `1.25%`):

| Leverage (Debt/EBITDA) | Spread (code) | Doc band |
|---|---|---|
| `<1.5×` | **0.75%** | 0.75–1.0% |
| `1.5–3.0×` | **1.25%** | 1.0–1.5% |
| `3.0–4.5×` | **2.00%** | 1.5–2.5% |
| `4.5–6.0×` | **3.25%** | 2.5–4.0% |
| `>6.0×` | **5.00%** | 4.0–6.0% |

Other WACC deterministics: beta `clamp[0.30,2.00]` → Blume `0.67/0.33` (`wacc.rs:62-63`); `Ke clamp[8%,20%]` (`wacc.rs:84`); `Leverage = debt/ebitda` else `op_income×1.15` else sentinel `99.0` (`wacc.rs:88-93` → max spread `5%`); `WACC clamp[7%,15%]` (`wacc.rs:103`); LLM `wacc` override `clamp[5%,20%]` (`mod.rs:314`).

---

## 2. FCF / NI-Factor Deterministic Tables

**Non-fin fallback** (`mod.rs:176-189`) — supersedes "no hardcoded industry table" sentence in `03 §4`:
`IT/Software 0.80, FMCG/Pharma 0.70, Auto/Industrial 0.58, Utilities/Infra 0.43, Other 0.65`.

**Financial-services universal 5-tier GICS-aware** (`mod.rs:211-253`, `fcf.rs:31,68`, promptly v5 `seeds.rs:405-450`):

| Bucket | Condition (sector/industry substring, case-insensitive) | NI factor (used when LLM field null) |
|---|---|---|
| Asset management | `asset management` in industry | **`0.75`** |
| Specialty credit | `credit services` or `financial - credit` | **`0.30`** |
| Regional bank | `banks - regional` | **`0.35`** |
| Diversified mega (`market_cap >50B`) | `banks - diversified` | **`0.50`** |
| Diversified large (`10–50B`) | same | **`0.45`** |
| Diversified small/mid (`<10B`) | same | **`0.35`** |
| Conservative default (no match) | — | **`0.40`** |

Clamp: `effective_ni_factor(v).clamp(0.20,0.90)` (`mod.rs:208`) — lower bound raised from `0.40` in earlier doc to **`0.20`**; tests `finserv_ni_factor_clamp_and_default` lock `0.10→0.20 / 0.95→0.90 / None→tier_or_0.40`. Also `fcf.rs:68` `unwrap_or(0.40)` for CAGR fallback.

Size premium is applied **universally incl. financial services** (`wacc.rs:25`, test `finserv_size_premium_applied`), with `_is_finserv` kept only for signature stability.

---

## 3. Terminal & Scenarios Deterministics

**Cliffs** (`terminal.rs:19-36`): `g = min(g, gdp)` → `g = max(g, Y5-2.5%)` → if `wacc-g<0.02` then `g=wacc-0.02` & `is_model_stable=false` → `g<0 → 0`. Per-scenario `wacc_scen` is clamped `[7%,15%]` before TV.

**Scenario overrides** (`scenarios.rs:58-72`): Optimistic `g = min(g+1%, GDP+1%).max(g)`, Pessimistic `g = max(g-1%, Y5-3.5%, 3.00%).min(g)` — the `Y5-3.5%` / `3.00%` floors are scenario-level specifics not in `03 §5` base cliffs.

---

## 4. Sensitivity Scaling Factor

`scaling_factor = base_fair_value / (equity/shares)` (`sensitivity.rs:40-45`) applied to every cell so `cents(base.fair_value) == cents(base_case_fair_value)` (`mod.rs:723`). Branch: finserv `equity = PV(TV)` alone; non-fin `+ cash − debt`. Shares recomputed `market_cap/current_price` (`sensitivity.rs:25`). Rounding: FV 2dp, pct 4dp, wacc/g 4dp. Stability: `WACC−g ≤ 0.0001` or `|pct|>1.0` or `rawFV≤0` → `is_model_stable=false`.

---

## 5. Seeds & Settings Migrations (authoritative numbers)

**`SETTING_ROWS` 28 entries** (`seeds.rs:322`): `llm.* (14)` + `analysis.max_parallel 1` + `display.* (3+2 optional)` + `search.* (active_provider, max_results 5, exclude_extensions [".pdf",".doc",".xls"], tavily URLs, scrapingdog URLs)` + `general.* (default_market US, pdf_save_folder)`. Former `search.brave_*` / generic custom slots are **not seeded**.

**LLM defaults** (with migrations `seeds.rs:476-481`): `thinking_budget 20000` (was 10000), `temperature 0.3` (was 0.1), `max_tokens 32000` (was 8192/16000). Pipeline enforces `thinking_budget < max_tokens` with ≥10_000 gap (`pipeline/mod.rs:724`): `clamped = (max_tokens-10_000).max(10_000)`.

**Prompts versions** (`seeds.rs:72-450`): fresh `force` seeds `version=2` for all 6; incremental migrations bump `json_schema` → `3` (`seeds.rs:449`) and `growth_and_fcf` → `5` (`seeds.rs:450`) to apply the 5-tier GICS text + streaming/rate-limit guidance. `list_prompts` uses `CASE prompt_key WHEN … END` ordering (`db.rs:715`) not alphabetical.

**LLM provider defaults** (`seeds.rs:323-332`): `anthropic claude-sonnet-4-5 @ api.anthropic.com`, `deepseek deepseek-flash @ api.deepseek.com`, `openai gpt-5 @ api.openai.com`, `kimi kimi-k2.6 @ api.moonshot.ai/v1` (migration `seeds.rs:483` `moonshot.cn → moonshot.ai`, `llm/mod.rs:316` verifies `…/ai/v1…`), `custom ""`.

---

## 6. Pipeline Internals (agentic loop, tools, JSON repair, caching, queue)

**Loop** (`pipeline/mod.rs:868-961`): `for turn in 0..10` (`tool_defs:14` `web_search` / `web_scrape`, `MAX_TOOL_RESULT_CHARS=12000` truncation `truncate_for_llm`, `_fallback_warning` bundled logic).

**Config** (`load_tool_config` `pipeline/mod.rs:600`): caches `ToolConfig` via `PathBuf` before `await` to avoid holding `Connection` across await.

**Streaming** (`llm/anthropic.rs:9-15`, `openai_compat.rs:9-15`): `reqwest` 300s/15s + keepalive 30s; SSE streaming with **120s idle timeout**; `tauri::Emitter` `llm://token` + `pipeline://step` + `debug://log_path`; `debug_logger` think buffering (`flush_thinking`).

**Retry / fallback** (`openai_compat.rs:221-311`, `anthropic.rs:148-186`, `pipeline/mod.rs:873-905`): provider-specific compat strips (`reasoning_effort`, `thinking_budget`, `temperature`, `max_tokens`, `max_completion_tokens`, `output_config.effort`) with 2 attempts; non-stream fallback once on `stream error`/`idle timeout`; per-turn 1500ms backoff; `507 OOM` / `401` / `429` are **fail-fast** (no retry) — `shorten_error` maps to `HTTP code + type` (160 chars).

**Agentic streaming fallback** (`openai_compat.rs:440-655`, `anthropic.rs:318-436`): on `stream_err` with empty content, retry non-stream with same compat strips, logged via `debug_logger`.

**Tool routing** (`pipeline/mod.rs:620-681`, `tools/mod.rs:52`, `tools/tavily.rs:10-46`, `tools/scrapingdog.rs:8-68`): active-provider + fallback to secondary with `_fallback_warning`; Tavily extract hardcodes `query:"find forward estimates"` / `extract_depth:"advanced"`; `exclude_extensions` gate skips scrape URLs before network.

**JSON repair** (`pipeline/mod.rs:168-542`): `strip_bom` → `clean_control_chars` (drop controls except `\n\r\t`) → `fix_trailing_commas` (string-aware) → `escape_literal_newlines_in_strings` → `auto_close_json` (string-aware brace/bracket) → `extract_json` (markdown ` ```json` fence + string-aware outer `{}` matcher + `rfind` fallback) — 5 candidates tried (`cleaned`, `auto_close(cleaned)`, `sanitized`, `auto_close(sanitized)`, `fix_commas→auto_close`) — first `Value` parse wins; then `Schema mismatch` mapping; `coalesce_field`/`clean_narrative` normalize DeepSeek nesting (opportunities/risks/etc inside `narrative`).

**Empty / truncated** (`pipeline/mod.rs:975-998`): `final_content.trim().is_empty() → parse_failed_empty` (`Empty JSON received from {model}`) + ephemeral `failure` (queue `Failed`, not DB) before truncation check; `is_truncated = output_tokens==max_tokens` → `parse_failed_truncated`; non-truncated parse error → `JSON incomplete: … Increase Max tokens in Settings → Advanced LLM`.

**Cache side-channel** (`pipeline/mod.rs:735-854`): `provider_override` from New Analysis dropdown wins over `llm.active_provider` (`pipeline/mod.rs:707`); financial cache decision: `report_exists_for_ticker && cached_opt && !report_exists_same_model → reuse cached payload` (inject fresh `market_parameters`) else fresh Yahoo `fetch_and_transform_with_params`; `already_exists = !used_cached && report_exists_same_model && is_same_historical_data(cache, fresh)` (normalize TTM→`"TTM"` + 9-field filter) → skip LLM; `used_cached → skip upsert_financial_cache`.

**Queue** (`queue.rs:66-307`, `lib.rs:175-193`, `commands.rs:1693-1721`): `QueueState {pending:VecDeque, active:HashMap, max_parallel: clamp 1-5, recent:20}`, `try_dispatch` FIFO tight loop while `active.len()<max_parallel` (no `Semaphore`), each job `tokio::spawn(pipeline::run_analysis_with_job)`; `set_max_parallel` persists in `app_settings` + in-memory and immediately `try_dispatch`es; `queue://update` fan-out to `AnalysisQueueContext` (`lib/api.ts:648`).

---

## 7. Yahoo Transform Details

**TTM handling** (`yahoo/transform.rs:149-160`): first row `TTM` (`ttm` object) + last-4 fiscal years descending `b.cmp(a)` filtered on `contains('-') && len==10`, capped `take(4)` → up to 5 rows (`TTM + 4Y`); `<3` → `None` → `yahoo fetch failed`. `T T M` counts toward the `≥3` validator (`engine/mod.rs:283`), so `2 annual + TTM` passes.

**Fallbacks**: `market_cap is None → price×shares_outstanding` (i64 or f64) (`transform.rs:90-103`, comment "per user decision"); suffix mismatch warning (`expected_suffix`: `.NS`/`.L`/`.AX` / `""`) (`transform.rs:60-88`); `price_date` `YYYY-MM-DD → DD/MM/YYYY`; `mcap_category` via `quote::market_cap_category` (India `/1e7` crore thresholds `>1,650,000cr mega` etc.).

---

## 8. Verification Checklist

- [ ] `cargo test 76` includes `finserv_ni_factor_clamp_and_default`, `finserv_generic_gics_fallbacks`, `finserv_size_premium_applied`, `fcf default 0.40`, `validation_insufficient/not_profitable` (test-local helper), `stubs_are_valid (dashboard+screener only)`.
- [ ] `cargo check` clean (only allowed `is_valid_stub` dead_code).
- [ ] `npm run typecheck` + `build` (help 4-tab, settings 7-tab, cost table 7 rows, queue rate-limits card).
- [ ] Tauri manual: Help `?tab=overview|setup|features|methodology` persists, File menu `Dashboard/My Reports/Screener/Watchlists/New Analysis`, Help only `User Guide & Help`, Settings provider `moonshot.ai`, `.sbk` backup round-trip.
