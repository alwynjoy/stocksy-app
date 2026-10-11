# Stocksy Desktop — Design Document Index

> All design documents for the cross-platform DCF stock analyzer desktop application.
> **Multi-country** (India, US, UK, AU) · **Yahoo Finance** (free, primary) · **Flexible LLM** (cloud + local)

---

## Review Status

| Document | Status |
|---|---|
| **01 Feature List** | ✅ **Approved** (88 features) |
| **02 Database Design** | 📝 Updated to match approved features |
| **03 DCF Engine** | 📝 Updated to match approved features |
| **04 UI/UX Design** | 📝 Updated to match approved features |
| **05 Configuration** | 📝 Updated to match approved features |
| **06 Implementation Stages** | 📝 Updated to match approved features |

---

## Documents

| # | Document | Purpose | Key Content |
|---|---|---|---|
| **01** | [Feature List](01_feature_list.md) | **WHAT** we build | 88 features, 9 categories. 63 P0, 17 P1, 8 P2. AI vs Local compute split. ✅ Approved. |
| **02** | [Database Design](02_design_database.md) | **HOW** data is stored | 9 tables + 1 FTS. Multi-country. Ticker cache. System prompts table. Report key = ticker+model+price_date. |
| **03** | [DCF Engine & Pipeline](03_design_dcf_engine.md) | **HOW** DCF is computed | Pipeline diagram. 🤖 AI vs 🔧 Local split. Multiple system prompts. Interactive assumptions (not persisted). Queue-gated freshness (implicit). |
| **04** | [UI/UX Design](04_design_ui.md) | **HOW** the app looks | Light theme default. Country selector. Update CMP button. Clickable screener charts. Self-comparison. 4-tab Help User Guide + Queue & Parallelism. |
| **05** | [Configuration](05_design_configuration.md) | **HOW** settings work | Yahoo Finance only. Cloud LLM (API key + Fetch Models). Local/Custom LLM (URL + key). Bundled Tavily + ScrapingDog (AI Agent Tools). Multiple system prompts. |
| **06** | [Implementation Stages](06_implementation_stages.md) | **WHEN** we build | 10 stages, 18 weeks. Mapped features reflect Removals (F1.7 manual, F2.16 freshness, F4.3 Brave, F8.5 updater). |
| **07** | [DCF Engine Appendix](07_design_dcf_engine_appendix.md) | **APPENDIX** deterministics | WACC/NI/terminal midpoints, seeds/settings migrations, pipeline streaming/retry/JSON-repair/cache/queue internals. |

---

## Key Architecture Decisions (Approved)

| Decision | Choice |
|---|---|
| **Data source** | Yahoo Finance (free, global) — no API key required. **No manual fallback** — sole source; insufficient → ephemeral `failure`. |
| **Countries** | Multi-country from day one: India, US, UK, Australia (extensible) |
| **LLM** | Cloud: Anthropic / DeepSeek / OpenAI / Kimi (user's own API key, D1 4+1). Custom: any OpenAI-compatible endpoint (Ollama/LM Studio/MLX via Custom). |
| **Theme** | Light mode default — modern financial app aesthetic |
| **Report identity** | Unique by: ticker + LLM model + price date (`is_primary` pinned for screener) |
| **System prompts** | 6 modular modules (1–6) concatenated with `\n\n---\n\n` (fixed CASE order, versions `2→3→5` via `seeds.rs`) |
| **Assumption overrides** | Interactive on report page via **Recompute** (`recompute_local`) — NOT persisted, banner-gated |
| **Data freshness** | Implicit: same-model fresh Yahoo fetch + deep equality (`is_same_historical_data`) → skip LLM; different-model → reuse cache (+ fresh country params) |
| **Search/Scrape** | Bundled **Tavily + ScrapingDog** (`active_provider` radio), always-on (`MAX_TOOL_RESULT_CHARS 12000`, SSE 120s idle) |
| **Queue** | `analysis.max_parallel` 1–5, FIFO `pending/active/recent:20`, `queue://update` fan-out |
| **Backup** | `.sbk` (`STOCKYBK` MessagePack, 7 tables) as app path; `VACUUM INTO` as test/legacy path |

---

## Quick Stats

- **Total Features**: 88 (63 P0 + 17 P1 + 8 P2)
- **Database Tables**: 9 + 1 FTS virtual table
- **UI Pages**: 7 main pages + Settings
- **Implementation Stages**: 10 stages over ~18 weeks
- **Tech Stack**: Tauri v2 + React 18 + TypeScript + SQLite + Rust
