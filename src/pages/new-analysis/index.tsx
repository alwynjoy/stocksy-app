import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { AlertCircle, CheckCircle2, Clock, Loader2, Search } from "lucide-react";
import {
  countriesList,
  getSecret,
  getSetting,
  isHistoricalUnchanged,
  secretStatus,
  setSetting,
  tickerSearch,
  tickerYahooSearch,
  type CountryRow,
  type TickerHit,
} from "../../lib/api";
import { Button, Card, ErrorNote, Field } from "../../components/ui";
import { inputCls } from "../../lib/format";
import { QueueJob, useAnalysisQueue } from "../../contexts/AnalysisQueueContext";
import { MAX_BATCH, SAMPLE_CSV, parseBatchCsv as parseBatchCsvLib } from "../../lib/batchCsv";

const COUNTRIES = [
  { code: "US", label: "US — United States", currency: "USD" },
  { code: "IN", label: "IN — India (NSE)", currency: "INR" },
  { code: "GB", label: "GB — United Kingdom (LSE)", currency: "GBP" },
  { code: "AU", label: "AU — Australia (ASX)", currency: "AUD" },
];

type ProviderId = "anthropic" | "openai" | "deepseek" | "kimi" | "custom";

const PROVIDERS: Array<{ id: ProviderId; label: string }> = [
  { id: "anthropic", label: "Anthropic Claude" },
  { id: "openai", label: "OpenAI GPT" },
  { id: "deepseek", label: "DeepSeek" },
  { id: "kimi", label: "Kimi (Moonshot)" },
  { id: "custom", label: "Custom (OpenAI-compatible)" },
];



function ThinkingBox({ text, status }: { text: string; status: QueueJob["status"] }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    // keep at bottom as new tokens stream in
    el.scrollTop = el.scrollHeight;
  }, [text]);
  return (
    <div
      ref={ref}
      className="max-h-64 overflow-auto whitespace-pre-wrap border-t border-app-border px-3 py-2 font-mono text-xs text-app-muted"
    >
      {text || (status === "running" ? "Waiting for LLM tokens…" : "No tokens yet.")}
    </div>
  );
}

const isProviderId = (v: string | null): v is ProviderId =>
  v != null && PROVIDERS.some((p) => p.id === v);



export default function NewAnalysis() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const [ticker, setTicker] = useState("");
  const [country, setCountry] = useState("US");
  const [marketOptions, setMarketOptions] = useState(COUNTRIES);
  const [provider, setProvider] = useState<ProviderId>("anthropic");
  const [savedModel, setSavedModel] = useState<string | null>(null);
  const [savedBaseUrl, setSavedBaseUrl] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [thinkingExpanded, setThinkingExpanded] = useState(true);
  const { jobs: queueJobs, pending, active, enqueue } = useAnalysisQueue();

  // Batch state — CSV below single-shot, same Analyze button queues all at once
  const [batchRows, setBatchRows] = useState<BatchRowLocal[] | null>(null);
  const [batchFileName, setBatchFileName] = useState<string | null>(null);
  const [downloadingSample, setDownloadingSample] = useState(false);
  const batchInputRef = useRef<HTMLInputElement | null>(null);
  const validBatch = batchRows ? batchRows.filter((r) => r.valid) : [];
  const invalidBatch = batchRows ? batchRows.filter((r) => !r.valid) : [];
  const batchActive = validBatch.length > 0;

  // Batch type used locally (reuse lib type but keep ProviderId typing)
  type BatchRowLocal = { line: number; ticker: string; country: string; provider: ProviderId; rawTicker: string; rawCountry: string; rawProvider: string; valid: boolean; error?: string };

  // Ticker typeahead state
  const [hits, setHits] = useState<TickerHit[]>([]);
  const [showDrop, setShowDrop] = useState(false);
  const [searching, setSearching] = useState(false);
  const [yahooSearching, setYahooSearching] = useState(false);
  const [yahooHits, setYahooHits] = useState<TickerHit[] | null>(null);
  const [yahooError, setYahooError] = useState<string | null>(null);
  const [selectedHit, setSelectedHit] = useState<TickerHit | null>(null);
  const boxRef = useRef<HTMLDivElement | null>(null);
  const debounceRef = useRef<number | null>(null);

  const loadSaved = useCallback(
    (_p: ProviderId, m: string | null, u: string | null) => {
      setSavedModel(m && m.trim() ? m.trim() : null);
      setSavedBaseUrl(u && u.trim() ? u.trim() : null);
    },
    [],
  );

  useEffect(() => {
    let cancel = false;
    let ap: ProviderId = "anthropic";
    getSetting("llm.active_provider")
      .then((v) => {
        if (isProviderId(v)) ap = v;
      })
      .catch(() => {})
      .finally(() => {
        if (cancel) return;
        setProvider(ap);
        getSetting(`llm.${ap}.model`)
          .then((m) =>
            getSetting(`llm.${ap}.base_url`).then((u) => {
              if (!cancel) loadSaved(ap, m, u);
            }),
          )
          .catch(() => {});
      });
    return () => {
      cancel = true;
    };
  }, [loadSaved]);

  // Default market from Miscellaneous → general.default_market (dynamic Country Parameters)
  useEffect(() => {
    let cancel = false;
    Promise.all([countriesList().catch(() => [] as CountryRow[]), getSetting("general.default_market").catch(() => null)]).then(([list, def]) => {
      if (cancel) return;
      if (Array.isArray(list) && list.length > 0) {
        // Build dynamic market options from country_parameters if available
        // Keep sort by country_code for stability
        const dynamic = [...list].sort((a, b) => a.country_code.localeCompare(b.country_code)).map((c) => ({
          code: c.country_code,
          label: `${c.country_code} — ${c.country_name}`,
          currency: c.currency_code,
        }));
        // Update COUNTRIES-driven dropdown via state
        setMarketOptions(dynamic);
      }
      // Apply default market if no prefill ticker and no explicit ?country
      const qCountry = searchParams.get("country");
      if (qCountry) return; // prefill effect handles it
      if (searchParams.get("ticker")) return;
      if (hasPrefilledRef.current) return;
      if (def && typeof def === "string" && def.trim()) {
        const v = def.trim().toUpperCase();
        // Validate against dynamic list if loaded, else hardcoded COUNTRIES
        const valid = (list && list.some((c) => c.country_code === v)) || COUNTRIES.some((c) => c.code === v);
        if (valid) setCountry(v);
      }
    });
    return () => {
      cancel = true;
    };
  }, [searchParams]);

  const onProviderChange = (p: ProviderId) => {
    setProvider(p);
    setErr(null);
    // Persist so pipeline fallback matches dropdown; runAnalysis also passes provider explicitly to avoid race
    setSetting("llm.active_provider", p).catch(() => {});
    getSetting("llm." + p + ".model")
      .then((m) => getSetting("llm." + p + ".base_url").then((u) => loadSaved(p, m, u)))
      .catch(() => {
        setSavedModel(null);
        setSavedBaseUrl(null);
      });
  };

  // Prefill from Report → Reanalyse (?ticker=&company=&country=&provider=&model=)
  const hasPrefilledRef = useRef(false);
  useEffect(() => {
    if (hasPrefilledRef.current) return;
    const qTicker = searchParams.get("ticker");
    if (!qTicker) return;
    hasPrefilledRef.current = true;
    const qCompany = searchParams.get("company");
    const qCountry = searchParams.get("country");
    const qProvider = searchParams.get("provider");
    const qModel = searchParams.get("model");
    if (qCountry && COUNTRIES.some((c) => c.code === qCountry.toUpperCase())) {
      setCountry(qCountry.toUpperCase());
    }
    const hitCountry = qCountry && COUNTRIES.some((c) => c.code === qCountry.toUpperCase()) ? qCountry.toUpperCase() : "US";
    if (qProvider && isProviderId(qProvider)) {
      setProvider(qProvider);
      setSetting("llm.active_provider", qProvider).catch(() => {});
      if (qModel) {
        setSavedModel(qModel);
        // Persist model for that provider so Analyze uses correct one immediately
        setSetting(`llm.${qProvider}.model`, qModel).catch(() => {});
        // Also load baseUrl for display
        getSetting(`llm.${qProvider}.base_url`)
          .then((u) => setSavedBaseUrl(u && u.trim() ? u.trim() : null))
          .catch(() => {});
      } else {
        getSetting(`llm.${qProvider}.model`)
          .then((m) => getSetting(`llm.${qProvider}.base_url`).then((u) => loadSaved(qProvider, m, u)))
          .catch(() => {});
      }
    } else if (qModel) {
      setSavedModel(qModel);
    }
    const displayName = qCompany && qCompany.trim() ? qCompany.trim() : qTicker;
    setTicker(displayName);
    setSelectedHit({
      ticker: qTicker.toUpperCase(),
      company_name: qCompany || displayName,
      exchange: null,
      country_code: hitCountry,
    });
  }, [searchParams, loadSaved]);

  // Close dropdown on outside click
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) {
        setShowDrop(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, []);

  const doLocalSearch = useCallback(
    (q: string, c: string) => {
      const trimmed = q.trim();
      if (trimmed.length === 0) {
        setHits([]);
        setSearching(false);
        return;
      }
      setSearching(true);
      setYahooHits(null);
      setYahooError(null);
      tickerSearch(trimmed, c, 8)
        .then((r) => {
          setHits(r);
          setShowDrop(true);
        })
        .catch(() => setHits([]))
        .finally(() => setSearching(false));
    },
    [],
  );

  // Debounced typeahead on each letter — suppressed while a company is selected
  useEffect(() => {
    if (debounceRef.current) window.clearTimeout(debounceRef.current);
    const q = ticker;
    // When a company is selected, don't show dropdown/search until text changes
    if (
      selectedHit &&
      ((selectedHit.company_name && selectedHit.company_name === q.trim()) ||
        selectedHit.ticker.toUpperCase() === q.trim().toUpperCase())
    ) {
      setHits([]);
      setShowDrop(false);
      return;
    }
    debounceRef.current = window.setTimeout(() => {
      doLocalSearch(q, country);
    }, 180);
    return () => {
      if (debounceRef.current) window.clearTimeout(debounceRef.current);
    };
  }, [ticker, country, doLocalSearch, selectedHit]);

  const pickHit = (h: TickerHit) => {
    // User requirement: company name inserted in text box, not symbol
    setTicker(h.company_name || h.ticker);
    setCountry(h.country_code);
    setSelectedHit(h);
    setShowDrop(false);
    setHits([]);
    setYahooHits(null);
    setErr(null);
  };

  const onTickerChange = (v: string) => {
    setTicker(v);
    if (
      selectedHit &&
      selectedHit.ticker.toUpperCase() !== v.trim().toUpperCase() &&
      (selectedHit.company_name || "") !== v.trim()
    ) {
      setSelectedHit(null);
    }
  };

  const triggerYahoo = async () => {
    const q = ticker.trim();
    if (q.length < 1) {
      setYahooError("Enter a ticker or company name to search.");
      return;
    }
    setYahooSearching(true);
    setYahooError(null);
    try {
      const rows = await tickerYahooSearch(q, country);
      setYahooHits(rows);
      if (rows.length === 0) setYahooError("Yahoo found no matching tickers.");
      else setShowDrop(false);
    } catch (e) {
      setYahooError(typeof e === "string" ? e : "Yahoo search failed — check network.");
    } finally {
      setYahooSearching(false);
    }
  };

  const pickYahooHit = (h: TickerHit) => {
    // Already persisted by backend ticker_yahoo_search — show company name in box
    setTicker(h.company_name || h.ticker);
    setCountry(h.country_code);
    setSelectedHit(h);
    setYahooHits(null);
    setShowDrop(false);
    setErr(null);
  };

  // ── Batch helpers ──────────────────────────────────────────────────
  const downloadSample = async () => {
    setDownloadingSample(true);
    try {
      const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      if (isTauri) {
        try {
          const { save } = await import("@tauri-apps/plugin-dialog");
          const { downloadDir } = await import("@tauri-apps/api/path");
          let defDir: string | null = null;
          try { defDir = await downloadDir(); } catch { defDir = null; }
          const defaultPath = defDir ? `${defDir.replace(/\/$/, "")}/stocksy-sample-batch.csv` : "stocksy-sample-batch.csv";
          const picked = await save({ defaultPath, filters: [{ name: "CSV", extensions: ["csv"] }] }) as string | null;
          if (picked) {
            const data = Array.from(new TextEncoder().encode(SAMPLE_CSV));
            const { invoke } = await import("@tauri-apps/api/core");
            await invoke("save_backup_file", { path: picked, data });
            return;
          }
          return; // cancelled — don't fallback to blob
        } catch {
          // fall through to blob
        }
      }
      // browser fallback — direct download (also fallback if Tauri save failed)
      const blob = new Blob([SAMPLE_CSV], { type: "text/csv;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "stocksy-sample-batch.csv";
      document.body.appendChild(a);
      a.click();
      setTimeout(() => { document.body.removeChild(a); URL.revokeObjectURL(url); }, 2000);
    } catch {
      setErr("Failed to download sample CSV — try again.");
    } finally {
      setDownloadingSample(false);
    }
  };

  const parseBatchCsv = (text: string): BatchRowLocal[] => {
    return parseBatchCsvLib(text, marketOptions.map((c) => c.code), PROVIDERS.map((p) => p.id), MAX_BATCH) as BatchRowLocal[];
  };

  const handleBatchFile = async (file: File | null) => {
    if (!file) return;
    setErr(null);
    const text = await file.text().catch(() => "");
    const rows = parseBatchCsv(text);
    if (rows.length === 0) {
      setErr("CSV empty or invalid — check header ticker,country,provider.");
      setBatchRows(null);
      setBatchFileName(null);
      return;
    }
    if (rows.length >= MAX_BATCH) {
      // we capped, but inform
      setErr(`Batch capped at ${MAX_BATCH} rows — extra rows ignored. Throttled by Settings → Queue & Parallelism.`);
    }
    setBatchRows(rows);
    setBatchFileName(file.name);
  };

  const clearBatch = () => {
    setBatchRows(null);
    setBatchFileName(null);
    if (batchInputRef.current) batchInputRef.current.value = "";
  };

  // shared pre-checker for batch + single
  const hasAnyTool = async (): Promise<boolean> => {
    const TOOL_KEYS = ["search.tavily_key", "search.scrapingdog_key"] as const;
    try {
      const statuses = await secretStatus().catch(() => [] as { key: string; is_saved: boolean }[]);
      if (statuses.some((s) => (TOOL_KEYS as readonly string[]).includes(s.key) && s.is_saved)) return true;
      const toolSecrets = await Promise.all(TOOL_KEYS.map((k) => getSecret(k).catch(() => null)));
      return toolSecrets.some((v) => v != null && String(v).trim().length > 0);
    } catch { return false; }
  };

  const analyze = async () => {
    // ── Batch path: same Analyze button queues all valid rows at once ──
    if (batchActive) {
      const distinctProviders = Array.from(new Set(validBatch.map((r) => r.provider)));
      try {
        for (const p of distinctProviders) {
          const label = PROVIDERS.find((x) => x.id === p)?.label ?? p;
          const sec = await getSecret(`llm.${p}.api_key`).catch(() => null);
          if (!sec || !sec.trim()) {
            setErr(`API key for ${label} not configured. Go to Settings → LLM Provider to add it. Batch aborted.`);
            return;
          }
        }
        if (!(await hasAnyTool())) {
          setErr("No AI Agent Tools key configured. Go to Settings → AI Agent Tools and add Tavily or ScrapingDog key before analyzing.");
          return;
        }
      } catch {
        setErr("Could not verify API keys — try again or check Settings.");
        return;
      }
      setErr(null);
      let queued = 0; let already = 0; let enqueueFailed = 0;
      for (const row of validBatch) {
        try {
          const unchanged = await isHistoricalUnchanged(row.ticker, row.country, row.provider).catch(() => false);
          if (unchanged) { already += 1; continue; }
        } catch { /* allow */ }
        try {
          await enqueue(row.ticker, row.country, row.provider);
          queued += 1;
        } catch { enqueueFailed += 1; }
      }
      const skippedInvalid = invalidBatch.length;
      const parts = [`Queued ${queued}`];
      if (already) parts.push(`${already} already exists — skipped`);
      if (skippedInvalid) parts.push(`${skippedInvalid} invalid rows skipped`);
      if (enqueueFailed) parts.push(`${enqueueFailed} failed`);
      setErr(`${parts.join(" · ")}${validBatch.length === MAX_BATCH ? ` · (cap ${MAX_BATCH})` : ""}`.trim());
      return;
    }
    const trimmed = ticker.trim();
    if (!trimmed) {
      setErr("Enter a ticker to analyze.");
      return;
    }
    const effectiveTicker = selectedHit ? selectedHit.ticker : trimmed.toUpperCase();
    const effectiveCountry = selectedHit ? selectedHit.country_code : country;
    if (selectedHit && effectiveCountry !== country) setCountry(effectiveCountry);
    if (!selectedHit && trimmed.length < 1) {
      setErr("Pick a company from the list or use Search Yahoo to find it.");
      return;
    }
    // ── Pre-checker: provider key + at least one search/scrape tool ──────────
    try {
      const providerLabel = PROVIDERS.find((p) => p.id === provider)?.label ?? provider;
      const providerKey = `llm.${provider}.api_key`;
      const providerSecret = await getSecret(providerKey).catch(() => null);
      if (!providerSecret || !providerSecret.trim()) {
        setErr(
          `API key for ${providerLabel} not configured. Go to Settings → LLM Provider to add it.`,
        );
        return;
      }
      if (!(await hasAnyTool())) {
        setErr(
          "No AI Agent Tools key configured. Go to Settings → AI Agent Tools and add Tavily or ScrapingDog key before analyzing.",
        );
        return;
      }
    } catch {
      setErr("Could not verify API keys — try again or check Settings.");
      return;
    }
    setErr(null);
    // Guard: same model already exists and historical_data unchanged → block
    try {
      const unchanged = await isHistoricalUnchanged(effectiveTicker, effectiveCountry, provider);
      if (unchanged) {
        setErr(`Report already exists for ${effectiveTicker} with ${savedModel || provider} and no new historical financials — re-analyse blocked. Use a different model or wait for new financials.`);
        return;
      }
    } catch {
      // if check fails, allow to proceed
    }
    try {
      await enqueue(effectiveTicker, effectiveCountry, provider);
    } catch (e) {
      setErr(typeof e === "string" ? e : "Enqueue failed — check Settings, network, or try again.");
    }
  };

  return (
    <div className="w-full space-y-6">
      <div className="w-full">
        <Card title="New Analysis">
        <div className="grid grid-cols-1 gap-4 md:grid-cols-12">
          <div className="md:col-span-5">
            <Field label="Company">
            <div ref={boxRef} className="relative">
              <div className="relative">
                <input
                  className={`${inputCls} pr-8`}
                  value={ticker}
                  onChange={(e) => onTickerChange(e.target.value)}
                  onFocus={() => {
                    // Don't reopen dropdown when a company is already selected — wait for text change
                    const isSelected =
                      !!selectedHit &&
                      ((selectedHit.company_name && selectedHit.company_name === ticker.trim()) ||
                        selectedHit.ticker.toUpperCase() === ticker.trim().toUpperCase());
                    if (isSelected) return;
                    if (ticker.trim().length > 0) setShowDrop(true);
                  }}
                  placeholder="e.g. AAPL or Apple"
                  autoFocus
                  autoComplete="off"
                  autoCorrect="off"
                  autoCapitalize="off"
                  spellCheck={false}
                  name="stocksy-ticker-search"
                  id="stocksy-ticker-search"
                  data-1p-ignore="true"
                  data-form-type="other"
                  aria-autocomplete="list"
                  role="combobox"
                  aria-expanded={showDrop}
                  aria-controls="ticker-dropdown"
                />
                <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-app-muted">
                  {searching ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Search className="h-4 w-4" />
                  )}
                </span>
              </div>

              {/* Typeahead dropdown — company name primary, SYMBOL · EXCHANGE secondary; Yahoo search always at end */}
              {showDrop && ticker.trim().length > 0 && (
                <div id="ticker-dropdown" className="absolute left-0 right-0 z-20 mt-1 max-h-72 overflow-auto rounded-md border border-app-border bg-app-surface shadow-lg">
                  {hits.length > 0 && (
                    <ul className="divide-y divide-app-border">
                      {hits.map((h) => (
                        <li key={`${h.country_code}:${h.ticker}`}>
                          <button
                            type="button"
                            onClick={() => pickHit(h)}
                            className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left hover:bg-app-sidebar"
                          >
                            <span className="min-w-0">
                              <span className="block truncate text-sm font-medium text-app-text">
                                {h.company_name || h.ticker}
                              </span>
                              <span className="block truncate text-xs text-app-muted">
                                {h.ticker} {h.exchange ? `· ${h.exchange}` : ""}
                              </span>
                            </span>
                            <span className="shrink-0 rounded bg-app-sidebar px-1.5 py-0.5 text-xs font-medium text-app-muted">
                              {h.country_code}
                            </span>
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                  {searching && <div className="px-3 py-3 text-center text-xs text-app-muted">Searching…</div>}
                  {!searching && hits.length === 0 && (
                    <div className="px-3 py-2 text-xs text-app-muted">No local match for “{ticker.trim()}”.</div>
                  )}
                  {/* Always-on Yahoo search at list end — spinner inside same row */}
                  <div className="border-t border-app-border p-2">
                    <button
                      type="button"
                      onClick={triggerYahoo}
                      disabled={yahooSearching}
                      className="flex w-full items-center justify-center gap-1.5 rounded-md bg-app-text px-3 py-2 text-xs font-medium text-app-surface hover:opacity-90 disabled:opacity-50"
                    >
                      {yahooSearching ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Search className="h-3.5 w-3.5" />
                      )}
                      Search Yahoo for “{ticker.trim()}”
                    </button>
                    {yahooError && <p className="mt-2 text-xs text-app-negative">{yahooError}</p>}
                  </div>
                </div>
              )}
            </div>
            {selectedHit ? (
              <p className="mt-1 text-xs text-app-positive">
                Selected: {selectedHit.company_name || selectedHit.ticker} · {selectedHit.exchange || "—"} · {selectedHit.country_code}
              </p>
            ) : (
              <p className="mt-1 text-xs text-app-muted">Type a ticker or company name — matches appear as you type.</p>
            )}
            {/* Yahoo results after explicit search */}
            {yahooHits && yahooHits.length > 0 && (
              <div className="mt-2 rounded-md border border-app-border bg-app-surface">
                <div className="border-b border-app-border px-3 py-1.5 text-xs font-medium text-app-text">
                  Yahoo results — pick to store & use
                </div>
                <ul className="divide-y divide-app-border">
                  {yahooHits.map((h) => (
                    <li key={`y-${h.ticker}`}>
                      <button
                        type="button"
                        onClick={() => pickYahooHit(h)}
                        className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left hover:bg-app-sidebar"
                      >
                        <span className="min-w-0">
                          <span className="block truncate text-sm font-medium text-app-text">
                            {h.company_name || h.ticker}
                          </span>
                          <span className="block truncate text-xs text-app-muted">
                            {h.ticker} {h.exchange ? `· ${h.exchange}` : ""}
                          </span>
                        </span>
                        <span className="shrink-0 rounded bg-app-sidebar px-1.5 py-0.5 text-xs text-app-muted">{h.country_code}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            </Field>
          </div>
          <div className="md:col-span-3">
            <Field label="Market">
            <select
              className={inputCls}
              value={country}
              onChange={(e) => setCountry(e.target.value)}
            >
              {marketOptions.map((c) => (
                <option key={c.code} value={c.code}>
                  {c.label}
                </option>
              ))}
            </select>
            </Field>
          </div>
          <div className="md:col-span-4">
            <Field label="AI provider">
            <div className="space-y-1.5">
              <select
                className={inputCls}
                value={provider}
                onChange={(e) => onProviderChange(e.target.value as ProviderId)}
              >
                {PROVIDERS.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </select>
              <p className="text-xs text-app-muted">
                {savedModel ? (
                  <>
                    Model: <span className="font-medium text-app-text">{savedModel}</span>
                  </>
                ) : (
                  <span className="text-app-negative">No model set</span>
                )}
                {" · "}
                <a href="#/settings?tab=llm" className="underline hover:text-app-text">
                  to change model goto Settings
                </a>
                {savedBaseUrl ? ` · ${savedBaseUrl}` : ""}
              </p>
            </div>
            </Field>
          </div>
        </div>

        <div className="my-6 flex items-center gap-3">
          <div className="h-px flex-1 bg-app-border" />
          <span className="rounded-full border border-app-border bg-app-surface px-3 py-0.5 text-xs font-medium text-app-muted">OR</span>
          <div className="h-px flex-1 bg-app-border" />
        </div>
        <div className="space-y-3">
          <h3 className="text-sm font-medium text-app-text">Batch Analysis</h3>
          <p className="text-xs text-app-muted">
            Upload a CSV with <span className="font-medium text-app-text">ticker, country, provider</span> (Yahoo ticker e.g. RELIANCE.NS, country IN/US/GB/AU, provider anthropic/openai/deepseek/kimi/custom). Each row can have a different provider. Max {MAX_BATCH} rows — throttled by Settings → Queue & Parallelism.
          </p>
          <div className="flex flex-wrap items-center gap-2">
            <input ref={batchInputRef} type="file" accept=".csv,text/csv,.txt" className="hidden" onChange={(e) => handleBatchFile(e.target.files?.[0] ?? null)} />
            <Button variant="ghost" onClick={() => batchInputRef.current?.click()}>Upload CSV</Button>
            <Button variant="ghost" onClick={downloadSample} disabled={downloadingSample}>{downloadingSample ? "Downloading…" : "Download sample CSV"}</Button>
            {batchFileName && <span className="text-xs text-app-muted">{batchFileName} · {batchRows?.length ?? 0} rows</span>}
          </div>
          {batchRows && (
            <div className="space-y-2">
              <div className="flex flex-wrap gap-2 text-xs">
                <span className="rounded bg-app-positive/10 px-2 py-0.5 text-app-positive">{validBatch.length} valid</span>
                {invalidBatch.length > 0 && <span className="rounded bg-app-negative/10 px-2 py-0.5 text-app-negative">{invalidBatch.length} invalid will be skipped</span>}
                <span className="text-app-muted">{batchRows.length} total · {validBatch.length} will be queued via Analyze</span>
              </div>
              <div className="max-h-64 overflow-auto rounded-md border border-app-border">
                <table className="w-full text-left text-xs">
                  <thead className="sticky top-0 bg-app-surface text-app-muted">
                    <tr><th className="px-2 py-1">#</th><th className="px-2 py-1">ticker</th><th className="px-2 py-1">country</th><th className="px-2 py-1">provider</th><th className="px-2 py-1">status</th></tr>
                  </thead>
                  <tbody className="divide-y divide-app-border">
                    {batchRows.map((r) => (
                      <tr key={`${r.line}-${r.ticker}-${r.country}-${r.provider}`} className={r.valid ? "" : "bg-app-negative/5"}>
                        <td className="px-2 py-1 text-app-muted">{r.line}</td>
                        <td className="px-2 py-1 font-medium text-app-text">{r.rawTicker || "—"}</td>
                        <td className="px-2 py-1">{r.rawCountry || "—"}</td>
                        <td className="px-2 py-1">{r.rawProvider || "—"}</td>
                        <td className={`px-2 py-1 ${r.valid ? "text-app-positive" : "text-app-negative"}`}>{r.valid ? "✓" : r.error}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <p className="text-xs text-app-muted">Invalid rows are skipped. Duplicate rows are marked duplicate. Each row’s provider can differ.</p>
            </div>
          )}
        </div>

        {err && (
          <div className="mt-4">
            <ErrorNote message={err} />
          </div>
        )}

        <div className="mt-5 flex flex-wrap items-center gap-3">
          <Button onClick={analyze}>
            {batchActive ? `Analyze Batch (${validBatch.length})` : "Analyze"}
          </Button>
          {batchActive && (
            <Button variant="ghost" onClick={clearBatch}>
              Clear batch
            </Button>
          )}
          <span className="text-xs text-app-muted">
            {batchActive ? `${validBatch.length} in batch · ${invalidBatch.length} invalid will be skipped · ${active.length} running · ${pending.length} queued` : `4-stage pipeline · ${active.length} running · ${pending.length} queued`}
            {pending.length > 0 ? " · FIFO queue" : ""}
          </span>
          {pending.length > 0 && <span className="rounded bg-app-warning/10 px-2 py-0.5 text-xs text-app-warning">{pending.length} queued</span>}
          {active.length > 0 && <span className="rounded bg-app-positive/10 px-2 py-0.5 text-xs text-app-positive">{active.length} running</span>}
        </div>
        {batchActive && <p className="mt-2 text-xs text-app-warning">Batch mode — single Company/Market inputs are ignored while a batch file is loaded. Clear batch to analyze a single ticker.</p>}
      </Card>
      </div>

      {/* Queue stacked cards — each card full width, pipeline and thinking stacked full width */}
      {queueJobs.length > 0 && (
        <div className="w-full space-y-4">
          <h3 className="text-sm font-medium text-app-text">
            Queue — {active.length} running · {pending.length} queued · {queueJobs.filter((j: QueueJob) => j.status === "completed" || j.status === "failed" || j.status === "already_exists").length} recent
          </h3>
          {queueJobs.map((job: QueueJob) => {
            const isTerminal = job.status === "completed" || job.status === "failed" || job.status === "already_exists";
            return (
            <Card
              key={job.id}
              title={`${job.ticker} · ${job.country_code} · ${job.provider}${job.model ? ` · ${job.model}` : ""}`}
              action={
                <span
                  className={`rounded px-2 py-0.5 text-xs font-medium ${
                    job.status === "running"
                      ? "bg-app-warning/10 text-app-warning"
                      : job.status === "queued"
                        ? "bg-app-muted/20 text-app-muted"
                        : job.status === "completed"
                          ? "bg-app-positive/10 text-app-positive"
                          : job.status === "already_exists"
                            ? "bg-app-positive/10 text-app-positive"
                            : "bg-app-negative/10 text-app-negative"
                  }`}
                >
                  {job.status === "completed" ? "success" : job.status}
                </span>
              }
            >
              {isTerminal ? (
                (job.status === "completed" || job.status === "already_exists") && job.report_id ? (
                  <div
                    role="button"
                    tabIndex={0}
                    onClick={() => navigate(`/report/${job.ticker}?version=${encodeURIComponent(job.report_id!)}`)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        navigate(`/report/${job.ticker}?version=${encodeURIComponent(job.report_id!)}`);
                      }
                    }}
                    className="flex cursor-pointer items-center justify-between gap-3 rounded -mx-2 px-2 py-2 text-sm hover:bg-app-sidebar"
                    title={`Open ${job.ticker} — ${job.model}`}
                  >
                    <span className="min-w-0 truncate text-app-positive font-medium inline-flex items-center gap-1">{job.status === "already_exists" ? "already_exists" : "success"} <span className="text-xs">↗</span> view report</span>
                    <span className="shrink-0 text-xs text-app-muted truncate" title={job.report_id}>{job.report_id}{job.debug_log_id ? ` · ${job.debug_log_id}.jsonl` : ""}</span>
                  </div>
                ) : (
                  <div className="flex items-center justify-between gap-3 text-sm">
                    <span className="min-w-0 truncate">
                      {job.status === "completed" && <span className="text-app-positive font-medium">success</span>}
                      {job.status === "already_exists" && <span className="text-app-positive font-medium">already_exists</span>}
                      {job.status === "failed" && <span className="text-app-negative font-medium" title={job.message ?? undefined}>{job.message ? job.message : "failed"}</span>}
                    </span>
                    <span className="shrink-0 text-xs text-app-muted">
                      {job.debug_log_id && <><code className="rounded bg-app-bg px-1 py-0.5">{job.debug_log_id}.jsonl</code> · </>}
                      <a href="#/settings?tab=data" onClick={(e) => e.stopPropagation()} className="underline hover:text-app-text">logs</a>
                    </span>
                  </div>
                )
              ) : (
              <div className="grid gap-4 md:grid-cols-10">
                {/* Left: pipeline - 30% */}
                <div className="w-full md:col-span-3">
                  <div className="mb-2 text-xs font-medium text-app-text">Pipeline</div>
                    <ul className="space-y-2">
                      {job.steps.map((step: QueueJob["steps"][number]) => (
                        <li
                          key={step.id}
                          className={`flex items-center justify-between gap-2 rounded-md px-3 py-2 text-sm ${
                            step.status === "failed"
                              ? "bg-app-negative/10 border border-app-negative"
                              : step.status === "running"
                                ? "bg-app-warning/10 border border-app-warning"
                                : step.status === "completed"
                                  ? "bg-app-positive/10 border border-app-positive"
                                  : "bg-app-sidebar border border-transparent"
                          }`}
                        >
                          <span className="flex min-w-0 items-center gap-2">
                            {step.status === "pending" && <Clock className="h-4 w-4 shrink-0 text-app-muted" />}
                            {step.status === "running" && <Loader2 className="h-4 w-4 shrink-0 animate-spin text-app-warning" />}
                            {step.status === "completed" && <CheckCircle2 className="h-4 w-4 shrink-0 text-app-positive" />}
                            {step.status === "failed" && <AlertCircle className="h-4 w-4 shrink-0 text-app-negative" />}
                            <span className={`truncate ${step.status === "failed" ? "text-app-negative font-medium" : "text-app-text"}`}>{step.label}</span>
                          </span>
                          <span className="max-w-[11rem] shrink-0 truncate text-right text-xs tabular-nums" title={step.status === "failed" && step.message ? step.message : undefined}>
                            {step.status === "failed" && step.message ? (
                              <span className="text-app-negative">{step.message}</span>
                            ) : (
                              <span className={step.status === "running" ? "text-app-warning" : step.status === "completed" ? "text-app-positive" : "text-app-muted"}>{step.status}</span>
                            )}
                          </span>
                        </li>
                      ))}
                    </ul>
                  {job.report_id && (
                    <div className="mt-3 rounded-md border border-app-border bg-app-surface px-3 py-2 text-xs">
                      <div className="font-medium text-app-text">{job.status === "already_exists" ? "Already exists — no re-run" : job.status === "completed" ? "Completed" : job.status === "failed" ? "Failed" : "Running"}</div>
                      <div className="truncate text-app-muted" title={job.report_id}>Report: {job.report_id}</div>
                      {job.message && <div className="mt-1 truncate text-app-muted" title={job.message}>{job.message}</div>}
                    </div>
                  )}
                  {job.message && !job.report_id && <div className="mt-3 truncate text-xs text-app-muted" title={job.message}>{job.message}</div>}
                  {job.debug_log_id && (
                    <div className="mt-2 text-[11px] text-app-muted">
                      Debug: <code className="rounded bg-app-bg px-1 py-0.5">{job.debug_log_id}.jsonl</code> · <a href="#/settings?tab=data" className="underline hover:text-app-text">view</a>
                    </div>
                  )}
                </div>
                {/* Right: AI streaming - 70% */}
                <div className="w-full md:col-span-7">
                  <div className="mb-2 text-xs font-medium text-app-text">AI Streaming</div>
                  <details className="rounded-md border border-app-border bg-app-surface" open={thinkingExpanded} onToggle={(e) => setThinkingExpanded((e.target as HTMLDetailsElement).open)}>
                    <summary className="cursor-pointer list-none px-3 py-2 text-xs font-medium text-app-text">
                      Thinking {job.thinking ? `· ${job.thinking.length} chars` : job.status === "running" ? "· streaming…" : "· —"} {thinkingExpanded ? "▾" : "▸"}
                    </summary>
                    <ThinkingBox text={job.thinking} status={job.status} />
                  </details>
                  {job.status === "queued" && <div className="mt-3 text-xs text-app-muted">Queued — starts when a slot frees (max parallel from Settings → Queue).</div>}
                </div>
              </div>
              )}
            </Card>
            );
          })}
        </div>
      )}


    </div>
  );
}
