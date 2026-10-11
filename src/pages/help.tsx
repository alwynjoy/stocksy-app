import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { dcfDefaults, getSetting, setSetting, type DcfDefault } from "../lib/api";
import { Card, ErrorNote, Loading, Table } from "../components/ui";

const HELP_TAB_SETTING = "ui.help_active_tab";
type HelpTab = "overview" | "setup" | "features" | "methodology";

const HELP_TABS: Array<{ id: HelpTab; title: string }> = [
  { id: "overview", title: "What this app does" },
  { id: "setup", title: "Setup Instructions" },
  { id: "features", title: "Key Features" },
  { id: "methodology", title: "DCF Methodology" },
];

// concise provider meta mirrors src/pages/settings — kept in sync manually, no secret involved
const PROVIDER_ROWS: Array<{ id: string; name: string; baseUrl: string; model: string }> = [
  { id: "anthropic", name: "Anthropic", baseUrl: "https://api.anthropic.com", model: "claude-sonnet-4-5" },
  { id: "deepseek", name: "DeepSeek", baseUrl: "https://api.deepseek.com", model: "deepseek-flash" },
  { id: "openai", name: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-5" },
  { id: "kimi", name: "Kimi (Moonshot)", baseUrl: "https://api.moonshot.ai/v1", model: "kimi-k2.6" },
  { id: "custom", name: "Custom (OpenAI-compatible)", baseUrl: "https://…/v1", model: "— any /v1/chat/completions endpoint —" },
];

function Prose({ children }: { children: React.ReactNode }) {
  return <div className="space-y-3 text-sm leading-relaxed text-app-text">{children}</div>;
}

function Bullet({ children }: { children: React.ReactNode }) {
  return <li className="leading-relaxed">{children}</li>;
}

function OverviewTab() {
  return (
    <div className="space-y-6">
      <Card title="Mission">
        <Prose>
          <p>
            <span className="font-semibold">Stocksy Desktop</span> is institutional-grade AI stock research for
            patient investors — a local-first desktop app that pairs autonomous qualitative research with a
            deterministic, compiled-Rust DCF engine. You own your data, your keys, and your reports.
          </p>
          <ul className="list-disc space-y-1.5 pl-5 text-app-muted">
            <Bullet>No telemetry, no cloud storage. SQLite database at <code className="rounded bg-app-bg px-1 py-0.5 text-xs">stocksy.db</code> on your machine.</Bullet>
            <Bullet>Every valuation is reproducible: same inputs → same fair value, down to the cent.</Bullet>
            <Bullet>Built for India (NSE/BSE), US, UK, AU — with localized macro parameters and Yahoo Finance coverage.</Bullet>
          </ul>
        </Prose>
      </Card>

      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Hybrid architecture">
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
            <li><span className="font-medium text-app-text">AI researcher</span> — multi-turn agentic loop (≤10 turns) calls <code className="rounded bg-app-bg px-1 py-0.5 text-xs">web_search</code> / <code className="rounded bg-app-bg px-1 py-0.5 text-xs">web_scrape</code> to gather filings, concalls, and forward estimates.</li>
            <li><span className="font-medium text-app-text">Deterministic engine</span> — WACC, 5-yr FCF, terminal value, EV → equity, scenarios, sensitivity, and confidence run in Rust. LLM never does math.</li>
            <li><span className="font-medium text-app-text">What-if</span> — local recompute from the report page, no LLM cost.</li>
          </ul>
        </Card>
        <Card title="Multi-country & privacy">
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
            <li><span className="font-medium text-app-text">Countries:</span> India, United States, United Kingdom, Australia — each with risk-free rate, ERP, tax, and GDP growth. Add more in Settings → Country Parameters.</li>
            <li><span className="font-medium text-app-text">Keys:</span> stored in the SQLite <code className="rounded bg-app-bg px-1 py-0.5 text-xs">secrets</code> table (XOR-obfuscated), never in the backup file.</li>
            <li><span className="font-medium text-app-text">Cache:</span> Yahoo quote + financials cached locally; offline-friendly after first fetch.</li>
          </ul>
        </Card>
      </div>

      <Card title="Value proposition at a glance">
        <div className="grid gap-3 md:grid-cols-3">
          <div className="rounded-md border border-app-border bg-app-bg px-3 py-3">
            <div className="text-xs font-semibold uppercase tracking-wide text-app-muted">Research</div>
            <div className="mt-1 text-sm text-app-text">Forward-looking growth, margins, Moat & risks — sourced and cited.</div>
          </div>
          <div className="rounded-md border border-app-border bg-app-bg px-3 py-3">
            <div className="text-xs font-semibold uppercase tracking-wide text-app-muted">Valuation</div>
            <div className="mt-1 text-sm text-app-text">5-yr DCF with two models: FCFF/WACC (general) vs Equity/Ke (Banks & NBFCs).</div>
          </div>
          <div className="rounded-md border border-app-border bg-app-bg px-3 py-3">
            <div className="text-xs font-semibold uppercase tracking-wide text-app-muted">Workflow</div>
            <div className="mt-1 text-sm text-app-text">Batch, queue, screener, watchlists, PDF, backup, and full agentic trace.</div>
          </div>
        </div>
      </Card>

      <Card title="Ballpark cost — 100 reports (preferred models)">
        <p className="mb-3 text-xs text-app-muted">
          Observed averages for a typical report (10-turn agentic loop + engine). Actual varies with ticker complexity and tool calls. Local LLM = no API cost. Values are ballpark.
        </p>
        <Table headers={["Preferred model", "100 reports", "Notes"]}>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">Anthropic — Sonnet 5.5</td>
            <td className="px-3 py-2 font-medium tabular-nums text-app-text">$8–$11</td>
            <td className="px-3 py-2 text-xs text-app-muted">Good reasoning</td>
          </tr>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">DeepSeek — v4 Pro</td>
            <td className="px-3 py-2 font-medium tabular-nums text-app-text">$3–$4</td>
            <td className="px-3 py-2 text-xs text-app-muted">Good reasoning</td>
          </tr>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">OpenAI — GPT 5-mini</td>
            <td className="px-3 py-2 font-medium tabular-nums text-app-text">$3–$5</td>
            <td className="px-3 py-2 text-xs text-app-muted">Good reasoning</td>
          </tr>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">Kimi — 2.6</td>
            <td className="px-3 py-2 font-medium tabular-nums text-app-text">$7–$9</td>
            <td className="px-3 py-2 text-xs text-app-muted">Good reasoning</td>
          </tr>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">Gemini — 3.8 Flash</td>
            <td className="px-3 py-2 font-medium tabular-nums text-app-text">$3–$4</td>
            <td className="px-3 py-2 text-xs text-app-muted">Good reasoning</td>
          </tr>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">Qwen Flash</td>
            <td className="px-3 py-2 font-medium tabular-nums text-app-text">$3–$4</td>
            <td className="px-3 py-2 text-xs text-app-muted">Good reasoning</td>
          </tr>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">Local LLM — Free</td>
            <td className="px-3 py-2 font-medium tabular-nums text-app-positive">Free</td>
            <td className="px-3 py-2 text-xs text-app-muted">
              Good reasoning — prefer <code className="rounded bg-app-bg px-1 py-0.5 text-xs">Qwen 3.6</code> or{" "}
              <code className="rounded bg-app-bg px-1 py-0.5 text-xs">Gemma 4 MoE</code> via Ollama / LM Studio / MLX
            </td>
          </tr>
        </Table>
        <p className="mt-3 text-xs text-app-muted">All recommended models have good reasoning — pick by budget and availability.</p>
      </Card>
    </div>
  );
}

function SetupTab() {
  return (
    <div className="space-y-6">
      <Card title="Step 1 — Cloud AI providers">
        <p className="mb-3 text-sm text-app-muted">
          Settings → <span className="font-medium text-app-text">LLM Provider</span>. Pick an active provider, enter base URL, model, and API key → Save → Fetch Models (↻) → Test connection.
        </p>
        <Table headers={["Provider", "Default base URL", "Default model"]}>
          {PROVIDER_ROWS.map((r) => (
            <tr key={r.id}>
              <td className="px-3 py-2 font-medium text-app-text">{r.name}</td>
              <td className="px-3 py-2 font-mono text-xs text-app-text">{r.baseUrl}</td>
              <td className="px-3 py-2 font-mono text-xs text-app-muted">{r.model}</td>
            </tr>
          ))}
        </Table>
        <p className="mt-3 text-xs text-app-muted">
          Keys are saved to the SQLite <code className="rounded bg-app-bg px-1 py-0.5 text-xs">secrets</code> table (XOR-obfuscated, excluded from backups). The model dropdown is populated live via <code className="rounded bg-app-bg px-1 py-0.5">llm_fetch_models</code>.
        </p>
      </Card>

      <Card title="Step 2 — Custom / Local LLMs (OpenAI-compatible)">
        <p className="mb-3 text-sm text-app-muted">
          Any endpoint that serves <code className="rounded bg-app-bg px-1 py-0.5 text-xs">POST /v1/chat/completions</code> works. Select{" "}
          <span className="font-medium text-app-text">Custom</span> as provider.
        </p>
        <Table headers={["Runtime", "Typical base URL", "Notes"]}>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">Ollama</td>
            <td className="px-3 py-2 font-mono text-xs text-app-text">http://localhost:11434/v1</td>
            <td className="px-3 py-2 text-xs text-app-muted">Run <code className="rounded bg-app-bg px-1 py-0.5">ollama serve</code>; model e.g. <code className="rounded bg-app-bg px-1 py-0.5">qwen3:8b</code></td>
          </tr>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">LM Studio</td>
            <td className="px-3 py-2 font-mono text-xs text-app-text">http://localhost:1234/v1</td>
            <td className="px-3 py-2 text-xs text-app-muted">Enable local server in LM Studio</td>
          </tr>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">MLX / RotorQuant</td>
            <td className="px-3 py-2 font-mono text-xs text-app-text">http://localhost:8000/v1</td>
            <td className="px-3 py-2 text-xs text-app-muted">macOS MLX; custom build on 8000</td>
          </tr>
          <tr>
            <td className="px-3 py-2 font-medium text-app-text">Cloud gateways</td>
            <td className="px-3 py-2 font-mono text-xs text-app-text">https://api.groq.com/openai/v1</td>
            <td className="px-3 py-2 text-xs text-app-muted">Groq, OpenRouter, etc. — use provider’s key & base URL</td>
          </tr>
        </Table>
        <p className="mt-3 text-xs text-app-muted">
          For locals, leave API key blank or use any placeholder; set Max tokens / Thinking budget under{" "}
          <span className="font-medium text-app-text">Advanced</span> (applies to whichever provider is active).
        </p>
      </Card>

      <Card title="Step 3 — AI Agent Tools (search + scrape)">
        <p className="mb-3 text-sm text-app-muted">
          Settings → <span className="font-medium text-app-text">AI Agent Tools</span>. Each provider bundles search + scrape under one key. Pick Active Provider (Tavily default, ScrapingDog fallback).
        </p>
        <div className="grid gap-3 md:grid-cols-2">
          <div className="rounded-md border border-app-border bg-app-bg px-3 py-3">
            <div className="text-sm font-semibold text-app-text">Tavily — Search & Scrape</div>
            <div className="mt-1 font-mono text-xs text-app-muted">search: https://api.tavily.com/search<br />extract: https://api.tavily.com/extract<br />key: tvly-…</div>
          </div>
          <div className="rounded-md border border-app-border bg-app-bg px-3 py-3">
            <div className="text-sm font-semibold text-app-text">ScrapingDog — Search & Scrape</div>
            <div className="mt-1 font-mono text-xs text-app-muted">search: https://api.scrapingdog.com/google<br />scrape: https://api.scrapingdog.com/scrape<br />key: 69344a…</div>
          </div>
        </div>
        <p className="mt-3 text-xs text-app-muted">If the active key is missing, analysis falls back to the other provider with a warning.</p>
      </Card>

      <Card title="Step 4 — Test & run">
        <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
          <li><span className="font-medium text-app-text">Fetch Models</span> (↻) lists models for the selected provider/base URL.</li>
          <li><span className="font-medium text-app-text">Test connection</span> sends a minimal chat completion and reports latency.</li>
          <li>New Analysis → pick Market + model → Analyze (single) or Batch CSV. Queue runs at <code className="rounded bg-app-bg px-1 py-0.5 text-xs">analysis.max_parallel</code> (1–5).</li>
        </ul>
      </Card>
    </div>
  );
}

function FeaturesTab() {
  return (
    <div className="space-y-6">
      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Single vs Batch analysis">
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
            <li><span className="font-medium text-app-text">Single</span> — ticker + market + provider → queued run.</li>
            <li><span className="font-medium text-app-text">Batch</span> — CSV with <code className="rounded bg-app-bg px-1 py-0.5 text-xs">ticker,country,provider</code>; sample file one click. Up to 100 rows.</li>
            <li>Parallel workers 1–5 (Settings → Queue & Parallelism); FIFO, max 100 log files retained.</li>
          </ul>
        </Card>
        <Card title="Multi-provider reports">
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
            <li>Same ticker can have multiple versions (Anthropic vs OpenAI vs local) on Report page.</li>
            <li>Mark <span className="font-medium text-app-text">Primary</span> (used by Screener) and <span className="font-medium text-app-text">Favorite</span> (pin).</li>
            <li>Gap, verdict, confidence, risk/reward, and sources per version.</li>
          </ul>
        </Card>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <Card title="What-if recomputation">
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
            <li>On Report → What-if: tweak growth, margins, terminal g, WACC — instant local re-run.</li>
            <li>No LLM call, no cost. Deterministic revaluation from overridden <code className="rounded bg-app-bg px-1 py-0.5 text-xs">dcf_inputs</code>.</li>
          </ul>
        </Card>
        <Card title="Screener & Watchlists">
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
            <li><span className="font-medium text-app-text">Screener</span> — filters on gap, verdict, confidence, sector, market-cap, country; shows primary reports only.</li>
            <li><span className="font-medium text-app-text">Watchlists</span> — custom lists with price targets & notes; compare view.</li>
          </ul>
        </Card>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Backup & restore">
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
            <li>Settings → Database & Logging → <span className="font-medium text-app-text">Backup (.sbk)</span> saves 7 tables (reports, countries, prompts, settings, cache, watchlists).</li>
            <li>Backups exclude API keys. Restore validates magic + schema before replacing tables.</li>
          </ul>
        </Card>
        <Card title="Execution logs">
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
            <li>Per-run JSONL in the app log folder (turn-by-turn LLM thinking, tool calls, engine inputs/outputs).</li>
            <li>Debug Logs panel filters by kind; secrets redacted. Capped at 100 files.</li>
          </ul>
        </Card>
      </div>
    </div>
  );
}

function MethodologyTab({
  rows,
  err,
}: {
  rows: DcfDefault[] | null;
  err: string | null;
}) {
  return (
    <div className="space-y-6">
      <Card title="Two valuation models">
        <div className="grid gap-4 md:grid-cols-2">
          <div className="rounded-md border border-app-border bg-app-bg px-3 py-3">
            <div className="text-sm font-semibold text-app-text">Non-financial (FCFF → WACC)</div>
            <p className="mt-1 text-sm text-app-muted">
              5-yr explicit: Revenue × margin → Operating income → NOPAT (× 1-tax) → FCF (× conversion factor) →
              discount at <span className="font-medium text-app-text">WACC</span> → Enterprise Value + Cash − Debt → Equity → per-share.
            </p>
          </div>
          <div className="rounded-md border border-app-border bg-app-bg px-3 py-3">
            <div className="text-sm font-semibold text-app-text">Financial services — Banks & NBFCs (Equity → Ke)</div>
            <p className="mt-1 text-sm text-app-muted">
              Net Income chain (not revenue) → Distributable proxy{" "}
              <code className="rounded bg-app-bg px-1 py-0.5 text-xs">NI × factor</code> (default 0.65, clamp 0.40–0.90, fallback from sector) →
              discount at <span className="font-medium text-app-text">Cost of Equity (Ke)</span> → direct Equity Value (no debt/cash adjustment).
            </p>
          </div>
        </div>
        <p className="mt-3 text-xs text-app-muted">
          Engine entry: <code className="rounded bg-app-bg px-1 py-0.5">engine::run()</code> — LLM supplies only assumptions; all math here.
        </p>
      </Card>

      <Card title="WACC breakdown">
        <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
          <li>
            <span className="font-medium text-app-text">Beta:</span> raw beta clamped <code className="rounded bg-app-bg px-1 py-0.5 text-xs">[0.30, 2.00]</code> → Blume{" "}
            <code className="rounded bg-app-bg px-1 py-0.5 text-xs">0.67×β + 0.33×1.0</code>.
          </li>
          <li>
            <span className="font-medium text-app-text">Ke:</span> <code className="rounded bg-app-bg px-1 py-0.5 text-xs">Rf + β_adj × MRP</code> with market-cap size premium, then clamped 8–20%.
          </li>
          <li>
            <span className="font-medium text-app-text">WACC:</span> target capital structure with cyclicality (+0–50 bps), leverage credit spread, and 7–15% clamp. Ke/MRP/GDP come from Country Parameters.
          </li>
        </ul>
      </Card>

      <Card title="Terminal value & cliff constraints">
        <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
          <li>
            Gordon growth on Year-5 FCF: <code className="rounded bg-app-bg px-1 py-0.5 text-xs">TV = FCF5 × (1+g) / (WACC − g)</code>.
          </li>
          <li>
            <span className="font-medium text-app-text">3 cliffs:</span> (1) <code className="rounded bg-app-bg px-1 py-0.5 text-xs">g ≤ nominal GDP growth</code>, (2){" "}
            <code className="rounded bg-app-bg px-1 py-0.5 text-xs">g ≥ min(Y5 growth, GDP)</code> floor, (3){" "}
            <code className="rounded bg-app-bg px-1 py-0.5 text-xs">WACC − g ≥ 2.00%</code> stability spread (else <code className="rounded bg-app-bg px-1 py-0.5 text-xs">g</code> reduced).
          </li>
        </ul>
      </Card>

      <Card title="Scenarios, sensitivity, risk/reward">
        <ul className="list-disc space-y-1.5 pl-5 text-sm text-app-muted">
          <li>
            <span className="font-medium text-app-text">Scenarios:</span> Optimistic 25% / Expected 55% / Pessimistic 20% (offset perturbations on growth/margin/WACC/g).
          </li>
          <li>
            <span className="font-medium text-app-text">Sensitivity:</span> 3×3 grid at ±0.5% on WACC and terminal g.
          </li>
          <li>
            <span className="font-medium text-app-text">Verdict:</span> gap = (Weighted FV − CMP)/CMP: &gt;+10% Undervalued, &lt;−10% Overvalued, else Fairly Valued. Risk/reward from scenario spreads.
          </li>
        </ul>
      </Card>

      <Card title="DCF engine defaults">
        {err ? (
          <ErrorNote message={err} />
        ) : !rows ? (
          <Loading label="Loading DCF defaults…" />
        ) : (
          <>
            <Table headers={["Name", "Value", "Note"]}>
              {rows.map((r) => (
                <tr key={r.name}>
                  <td className="px-3 py-2 font-medium text-app-text">{r.name}</td>
                  <td className="px-3 py-2 text-app-text">{r.value}</td>
                  <td className="px-3 py-2 text-xs text-app-muted">{r.note}</td>
                </tr>
              ))}
            </Table>
            <p className="mt-3 text-xs text-app-muted">Constants are fixed in the engine (decision D4) — surfaced read-only here.</p>
          </>
        )}
      </Card>
    </div>
  );
}

export default function Help() {
  const location = useLocation();
  const navigate = useNavigate();
  const [active, setActive] = useState<HelpTab | null>(null);
  const [rows, setRows] = useState<DcfDefault[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  // persist + URL sync — mirrors Settings pattern
  useEffect(() => {
    let cancel = false;
    const params = new URLSearchParams(location.search);
    const urlTab = params.get("tab") as HelpTab | null;
    if (urlTab && HELP_TABS.some((t) => t.id === urlTab)) {
      setActive(urlTab);
      setSetting(HELP_TAB_SETTING, urlTab).catch(() => {});
      return () => {
        cancel = true;
      };
    }
    getSetting(HELP_TAB_SETTING)
      .then((v) => {
        if (cancel) return;
        const found = HELP_TABS.find((t) => t.id === v);
        setActive(found ? found.id : "overview");
      })
      .catch(() => {
        if (!cancel) setActive("overview");
      });
    return () => {
      cancel = true;
    };
  }, [location.search]);

  // DCF defaults — loaded eagerly so methodology tab has no extra wait; cheap (9 rows)
  useEffect(() => {
    let cancel = false;
    dcfDefaults()
      .then((d) => {
        if (!cancel) setRows(d);
      })
      .catch(() => {
        if (!cancel) setErr("Could not load DCF defaults.");
      });
    return () => {
      cancel = true;
    };
  }, []);

  const selectTab = (id: HelpTab) => {
    setActive(id);
    setSetting(HELP_TAB_SETTING, id).catch(() => {});
    navigate(`/help?tab=${id}`, { replace: true });
  };

  if (!active) return <Loading label="Loading help…" />;

  return (
    <div className="max-w-5xl space-y-6">
      <div>
        <h1 className="text-lg font-semibold text-app-text">User Guide & Help</h1>
        <p className="text-sm text-app-muted">Stocksy Desktop — local-first AI research + deterministic DCF valuation.</p>
      </div>

      <nav className="flex gap-1 overflow-x-auto border-b border-app-border pb-0" aria-label="Help sections">
        {HELP_TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => selectTab(t.id)}
            className={`whitespace-nowrap border-b-2 px-3 py-2 text-sm transition ${
              active === t.id
                ? "border-app-text font-medium text-app-text"
                : "border-transparent text-app-muted hover:text-app-text"
            }`}
            aria-current={active === t.id ? "page" : undefined}
          >
            {t.title}
          </button>
        ))}
      </nav>

      {active === "overview" && <OverviewTab />}
      {active === "setup" && <SetupTab />}
      {active === "features" && <FeaturesTab />}
      {active === "methodology" && <MethodologyTab rows={rows} err={err} />}
    </div>
  );
}
