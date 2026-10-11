import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  backupExport,
  backupRestore,
  backupValidate,
  countriesList,
  countryUpsert,
  debugLogDir,
  debugLogRead,
  debugLogsClear,
  debugLogsList,
  deleteSecret,
  getAppInfo,
  getDataInfo,
  getSecret,
  getSetting,
  llmFetchModels,
  llmTestConnection,
  promptRestoreDefault,
  promptsList,
  promptUpdate,
  saveBackupFile,
  secretStatus,
  setSecret,
  setSetting,
  type BackupMeta,
  type CountryPayload,
  type CountryRow,
  type DataInfo,
  type DebugLogMeta,
  type DebugLogRead,
  type LlmTestResult,
  type PromptRow,
} from "../../lib/api";
import { Button, Card, ErrorNote, Field, Loading, StatCard, Table } from "../../components/ui";
import { fmtNum, inputCls } from "../../lib/format";

const ACTIVE_TAB_SETTING = "ui.settings_active_tab";
const SIDEBAR_SETTING = "display.sidebar_collapsed";

type TabId = "llm" | "tools" | "countries" | "prompts" | "appearance" | "data" | "analysis";

const TABS: Array<{ id: TabId; title: string }> = [
  { id: "llm", title: "LLM Provider" },
  { id: "tools", title: "AI Agent Tools" },
  { id: "countries", title: "Country Parameters" },
  { id: "prompts", title: "System Prompts" },
  { id: "data", title: "Database & Logging" },
  { id: "analysis", title: "Queue & Parallelism" },
  { id: "appearance", title: "Miscellaneous" },
];

type ProviderId = "anthropic" | "deepseek" | "openai" | "kimi" | "custom";

const PROVIDER_IDS: ProviderId[] = ["anthropic", "deepseek", "openai", "kimi", "custom"];

const PROVIDER_META: Record<
  ProviderId,
  { name: string; baseUrl: string; model: string }
> = {
  anthropic: {
    name: "Anthropic",
    baseUrl: "https://api.anthropic.com",
    model: "claude-sonnet-4-5",
  },
  deepseek: {
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-flash",
  },
  openai: {
    name: "OpenAI",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-5",
  },
  kimi: {
    name: "Kimi (Moonshot)",
    baseUrl: "https://api.moonshot.ai/v1",
    model: "kimi-k2.6",
  },
  custom: {
    name: "Custom (OpenAI-compatible)",
    baseUrl: "https://…/v1",
    model: "",
  },
};

function secKey(provider: ProviderId): string {
  return `llm.${provider}.api_key`;
}

function SageBadge({
  saved,
  onRemove,
}: {
  saved: boolean;
  onRemove: () => void;
}) {
  if (!saved) {
    return <span className="mb-2 text-xs text-app-muted">not set</span>;
  }
  return (
    <span className="mb-2 inline-flex items-center gap-1.5 self-end rounded-md bg-app-positive/10 px-2 py-1 text-xs font-medium text-app-positive">
      saved
      <button
        type="button"
        onClick={onRemove}
        className="underline hover:text-app-text"
      >
        remove
      </button>
    </span>
  );
}

function SettingsHeader() {
  const [app, setApp] = useState<{ name: string; version: string; db_path: string } | null>(null);
  useEffect(() => {
    let cancel = false;
    getAppInfo()
      .then((a) => {
        if (!cancel)
          setApp({ name: a.name, version: a.version, db_path: a.db_path });
      })
      .catch(() => {});
    return () => {
      cancel = true;
    };
  }, []);
  return (
    <div>
      <h1 className="text-lg font-semibold text-app-text">Settings</h1>
      <p className="text-sm text-app-muted">
        {app
          ? `${app.name} v${app.version} · db: ${app.db_path}`
          : "App info unavailable (run via `tauri dev`)."}
      </p>
    </div>
  );
}

function LlmProviderCard({
  provider,
  secrets,
}: {
  provider: ProviderId;
  secrets: Record<string, boolean>;
}) {
  const meta = PROVIDER_META[provider];
  const secretKey = secKey(provider);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [ok, setOk] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [fetched, setFetched] = useState<string[]>([]);
  const [fetching, setFetching] = useState(false);
  const [testing, setTesting] = useState(false);
  const [test, setTest] = useState<LlmTestResult | null>(null);

  useEffect(() => {
    let cancel = false;
    Promise.all([
      getSetting(`llm.${provider}.base_url`),
      getSetting(`llm.${provider}.model`),
      getSecret(secretKey),
    ])
      .then(([u, m, k]) => {
        if (cancel) return;
        setBaseUrl(u ?? "");
        setModel(m ?? "");
        setApiKey(k ?? "");
      })
      .catch(() => {})
      .finally(() => {
        if (!cancel) setLoaded(true);
      });
    return () => {
      cancel = true;
    };
  }, [provider, secretKey]);

  const save = async () => {
    setSaving(true);
    setErr(null);
    setOk(false);
    try {
      await setSetting(`llm.${provider}.base_url`, baseUrl);
      await setSetting(`llm.${provider}.model`, model);
      if (apiKey.trim()) await setSecret(secretKey, apiKey);
      setOk(true);
    } catch {
      setErr("Save failed — run via `tauri dev`?");
    } finally {
      setSaving(false);
    }
  };

  const removeKey = async () => {
    setApiKey("");
    try {
      await deleteSecret(secretKey);
    } catch {
      setErr("Could not remove key from the database.");
    }
  };

  // Effective key: unsaved input takes priority over saved secrets so Fetch/Test
  // work immediately after typing without requiring Save first.
  const effectiveKey = apiKey.trim() || undefined;

  const fetchModels = async () => {
    if (fetching) return; // prevent double-click, but always allow explicit refresh
    if (!effectiveKey && !secrets[secretKey]) {
      setErr("Enter and save your API key first, then click ↻ to load models.");
      return;
    }
    setFetching(true);
    setErr(null);
    try {
      const url = baseUrl.trim() || meta.baseUrl;
      const list = await llmFetchModels(provider, url, effectiveKey);
      setFetched(list);
    } catch (e) {
      setErr(`Fetch models failed: ${typeof e === "string" ? e : "request error"}`);
    } finally {
      setFetching(false);
    }
  };

  const pickFetched = (m: string) => {
    if (!m) return;
    setModel(m);
    // Persist model immediately on selection — no extra Save click needed
    setSetting(`llm.${provider}.model`, m).catch(() => {});
  };

  const testConnection = async () => {
    setTesting(true);
    setTest(null);
    setErr(null);
    try {
      // Always pass current unsaved key so Test works before/after Save
      setTest(
        await llmTestConnection(provider, baseUrl || meta.baseUrl, model, effectiveKey),
      );
    } catch (e) {
      setTest({
        ok: false,
        message: typeof e === "string" ? e : "Request error",
        latency_ms: 0,
      });
    } finally {
      setTesting(false);
    }
  };

  if (!loaded) {
    return (
      <Card title={meta.name}>
        <Loading label="Loading provider…" />
      </Card>
    );
  }

  return (
    <Card title={meta.name}>
      <div className="grid grid-cols-2 gap-4">
        <Field label="Base URL">
          <input
            className={inputCls}
            value={baseUrl}
            placeholder={meta.baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
          />
        </Field>
        <Field label="Model">
          <div className="flex gap-1.5">
            <select
              className={`${inputCls} flex-1`}
              value={model}
              onChange={(e) => pickFetched(e.target.value)}
            >
              {/* Blank / placeholder state */}
              {!model && (
                <option value="">
                  {fetched.length > 0 ? "— select a model —" : "— click ↻ to load —"}
                </option>
              )}
              {/* Fetched list from provider */}
              {fetched.map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
              {/* Keep saved model visible if not yet in fetched list */}
              {model && !fetched.includes(model) && (
                <option value={model}>{model}</option>
              )}
            </select>
            {/* Explicit refresh button — avoids onFocus race with async fetch */}
            <button
              type="button"
              title={fetched.length > 0 ? `Reload models (${fetched.length} loaded)` : "Load models"}
              onClick={fetchModels}
              disabled={fetching}
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-app-border bg-white text-app-muted hover:border-app-text hover:text-app-text disabled:opacity-40"
            >
              {fetching ? (
                <span className="inline-block h-3.5 w-3.5 animate-spin rounded-full border-2 border-app-muted border-t-transparent" />
              ) : (
                <span className="text-sm leading-none">↻</span>
              )}
            </button>
          </div>
          {fetched.length > 0 && !fetching && (
            <p className="mt-1 text-xs text-app-muted">{fetched.length} models loaded — select one above.</p>
          )}
        </Field>
      </div>
      <div className="mt-4 flex items-end gap-2">
        <div className="flex-1">
          <Field label="API key">
            <div className="relative">
              <input
                type={showKey ? "text" : "password"}
                className={`${inputCls} pr-14`}
                value={apiKey}
                autoComplete="off"
                placeholder={showKey ? "Paste your API key here (⌘V)" : "sk-… (click show to paste)"}
                onChange={(e) => setApiKey(e.target.value)}
              />
              <button
                type="button"
                tabIndex={-1}
                onClick={() => setShowKey((v) => !v)}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-xs text-app-muted hover:text-app-text select-none"
              >
                {showKey ? "hide" : "show"}
              </button>
            </div>
          </Field>
        </div>
        <SageBadge saved={Boolean(secrets[secretKey])} onRemove={removeKey} />
      </div>

      <div className="mt-4 flex flex-wrap items-center gap-2">
        <Button onClick={save} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </Button>
        <Button variant="ghost" onClick={testConnection} disabled={testing || !model}>
          {testing ? "Testing…" : "Test connection"}
        </Button>
        {ok && <span className="text-sm text-app-positive">Saved ✓</span>}
        {err && <span className="text-sm text-app-negative">{err}</span>}
      </div>

      {test && (
        <div
          className={`mt-4 rounded-md border px-3 py-2 text-sm ${
            test.ok
              ? "border-app-positive bg-app-positive/10 text-app-positive"
              : "border-app-negative bg-app-negative/10 text-app-negative"
          }`}
        >
          {test.ok ? "OK" : "Failed"} — {test.message} ({test.latency_ms} ms)
        </div>
      )}
    </Card>
  );
}

function AdvancedLlmSettings() {
  const [budget, setBudget] = useState("24000");
  const [temp, setTemp] = useState("0.3");
  const [maxTok, setMaxTok] = useState("16000");
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [ok, setOk] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancel = false;
    Promise.all([
      getSetting("llm.thinking_budget"),
      getSetting("llm.temperature"),
      getSetting("llm.max_tokens"),
    ])
      .then(([b, t, m]) => {
        if (cancel) return;
        if (b != null) setBudget(b);
        if (t != null) setTemp(t);
        if (m != null) setMaxTok(m);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancel) setLoaded(true);
      });
    return () => {
      cancel = true;
    };
  }, []);

  const save = async () => {
    setSaving(true);
    setErr(null);
    setOk(false);
    const bNum = parseInt(budget, 10);
    const mNum = parseInt(maxTok, 10);
    const tNum = parseFloat(temp);
    if (!Number.isFinite(bNum) || bNum < 0) {
      setErr("Thinking budget must be a non-negative integer.");
      setSaving(false);
      return;
    }
    if (!Number.isFinite(mNum) || mNum <= 0) {
      setErr("Max tokens must be a positive integer.");
      setSaving(false);
      return;
    }
    if (!Number.isFinite(tNum) || tNum < 0 || tNum > 1) {
      setErr("Temperature must be between 0 and 1.");
      setSaving(false);
      return;
    }
    try {
      await Promise.all([
        setSetting("llm.thinking_budget", String(bNum)),
        setSetting("llm.temperature", String(tNum)),
        setSetting("llm.max_tokens", String(mNum)),
      ]);
      setOk(true);
    } catch {
      setErr("Save failed — run via `tauri dev`?");
    } finally {
      setSaving(false);
    }
  };

  if (!loaded) {
    return (
      <Card title="Advanced">
        <Loading label="Loading advanced settings…" />
      </Card>
    );
  }

  return (
    <Card title="Advanced">
      <p className="mb-4 text-xs text-app-muted">
        These apply to whichever provider is selected above.
      </p>
      <div className="grid grid-cols-3 gap-4">
        <Field label="Thinking budget">
          <input
            className={inputCls}
            type="number"
            inputMode="numeric"
            min={0}
            step={1000}
            value={budget}
            onChange={(e) => setBudget(e.target.value)}
            placeholder="24000"
          />
          <p className="mt-1 text-xs text-app-muted">Extended thinking tokens (if supported)</p>
        </Field>
        <Field label="Temperature">
          <input
            className={inputCls}
            type="number"
            inputMode="decimal"
            min={0}
            max={1}
            step={0.05}
            value={temp}
            onChange={(e) => setTemp(e.target.value)}
            placeholder="0.3"
          />
          <p className="mt-1 text-xs text-app-muted">0–1, lower = more deterministic</p>
        </Field>
        <Field label="Max tokens">
          <input
            className={inputCls}
            type="number"
            inputMode="numeric"
            min={1}
            step={512}
            value={maxTok}
            onChange={(e) => setMaxTok(e.target.value)}
            placeholder="16000"
          />
          <p className="mt-1 text-xs text-app-muted">Maximum response tokens</p>
        </Field>
      </div>
      <div className="mt-4 flex items-center gap-3">
        <Button onClick={save} disabled={saving}>
          {saving ? "Saving…" : "Save advanced"}
        </Button>
        {ok && <span className="text-sm text-app-positive">Saved ✓</span>}
        {err && <span className="text-sm text-app-negative">{err}</span>}
      </div>
    </Card>
  );
}

function LlmProviderTab() {
  const [secrets, setSecrets] = useState<Record<string, boolean>>({});
  const [active, setActive] = useState<ProviderId>("anthropic");

  useEffect(() => {
    let cancel = false;
    Promise.all([secretStatus(), getSetting("llm.active_provider")])
      .then(([st, ap]) => {
        if (cancel) return;
        const map: Record<string, boolean> = {};
        st.forEach((s) => {
          map[s.key] = s.is_saved;
        });
        setSecrets(map);
        if (ap && (PROVIDER_IDS as string[]).includes(ap)) {
          setActive(ap as ProviderId);
        }
      })
      .catch(() => {});
    return () => {
      cancel = true;
    };
  }, []);

  const setActiveProvider = (p: ProviderId) => {
    setActive(p);
    setSetting("llm.active_provider", p).catch(() => {});
  };

  return (
    <div className="space-y-6">
      <Card title="Active provider">
        <p className="mb-3 text-xs text-app-muted">
          The provider used for new analyses by default.
        </p>
        <select
          className={inputCls}
          value={active}
          onChange={(e) => setActiveProvider(e.target.value as ProviderId)}
        >
          {PROVIDER_IDS.map((id) => (
            <option key={id} value={id}>
              {PROVIDER_META[id].name}
            </option>
          ))}
        </select>
      </Card>
      <AdvancedLlmSettings />
      {PROVIDER_IDS.map((id) => (
        <LlmProviderCard key={id} provider={id} secrets={secrets} />
      ))}
    </div>
  );
}


function AiAgentToolsTab() {
  // Bundled per-provider: Tavily (primary) + ScrapingDog (secondary), toggle via search.active_provider radio (default tavily)
  const [activeProvider, setActiveProvider] = useState<"tavily" | "scrapingdog">("tavily");
  const [tavily, setTavily] = useState("");
  const [tavilySearchUrl, setTavilySearchUrl] = useState("");
  const [tavilyExtractUrl, setTavilyExtractUrl] = useState("");
  const [scrapingdog, setScrapingdog] = useState("");
  const [dogSearchUrl, setDogSearchUrl] = useState("");
  const [dogScrapeUrl, setDogScrapeUrl] = useState("");
  const [saved, setSaved] = useState<Record<string, boolean>>({});
  const [show, setShow] = useState<Record<string, boolean>>({});
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [ok, setOk] = useState(false);

  useEffect(() => {
    let cancel = false;
    Promise.all([
      getSetting("search.active_provider"),
      getSecret("search.tavily_key"),
      getSecret("search.scrapingdog_key"),
      getSetting("search.tavily_search_url"),
      getSetting("scrape.tavily_extract_url"),
      getSetting("search.scrapingdog_search_url"),
      getSetting("scrape.scrapingdog_scrape_url"),
      secretStatus(),
    ])
      .then(([ap, tk, dk, tsu, teu, dsu, dcu, st]) => {
        if (cancel) return;
        const apNorm = (ap && ap.trim().toLowerCase() === "scrapingdog") ? "scrapingdog" as const : "tavily" as const;
        setActiveProvider(apNorm);
        setTavily(tk ?? "");
        setScrapingdog(dk ?? "");
        setTavilySearchUrl(tsu ?? "");
        setTavilyExtractUrl(teu ?? "");
        setDogSearchUrl(dsu ?? "");
        setDogScrapeUrl(dcu ?? "");
        const map: Record<string, boolean> = {};
        st.forEach((s) => { map[s.key] = s.is_saved; });
        setSaved(map);
      })
      .catch(() => {})
      .finally(() => { if (!cancel) setLoaded(true); });
    return () => { cancel = true; };
  }, []);

  const toggleShow = (key: string) =>
    setShow((prev) => ({ ...prev, [key]: !prev[key] }));

  const pickProvider = (v: "tavily" | "scrapingdog") => {
    setActiveProvider(v);
    setSetting("search.active_provider", v).catch(() => {});
  };

  const save = async () => {
    setSaving(true);
    setErr(null);
    setOk(false);
    try {
      await setSetting("search.active_provider", activeProvider);
      await setSetting("search.tavily_search_url", tavilySearchUrl);
      await setSetting("scrape.tavily_extract_url", tavilyExtractUrl);
      await setSetting("search.scrapingdog_search_url", dogSearchUrl);
      await setSetting("scrape.scrapingdog_scrape_url", dogScrapeUrl);
      if (tavily.trim()) await setSecret("search.tavily_key", tavily.trim());
      if (scrapingdog.trim()) await setSecret("search.scrapingdog_key", scrapingdog.trim());
      const st = await secretStatus();
      const map: Record<string, boolean> = {};
      st.forEach((s) => { map[s.key] = s.is_saved; });
      setSaved(map);
      setOk(true);
    } catch {
      setErr("Save failed — run via `tauri dev`?");
    } finally {
      setSaving(false);
    }
  };

  if (!loaded) {
    return (
      <Card title="AI Agent Tools">
        <Loading label="Loading AI agent tools…" />
      </Card>
    );
  }

  const KeyField = ({
    label,
    stateKey,
    value,
    onChange,
    onRemove,
    placeholder,
  }: {
    label: string;
    stateKey: string;
    value: string;
    onChange: (v: string) => void;
    onRemove: () => void;
    placeholder?: string;
  }) => (
    <div className="flex items-end gap-2">
      <div className="flex-1">
        <Field label={label}>
          <div className="relative">
            <input
              type={show[stateKey] ? "text" : "password"}
              className={`${inputCls} pr-14`}
              value={value}
              autoComplete="off"
              placeholder={show[stateKey] ? "Paste key here (⌘V)" : placeholder ?? "…"}
              onChange={(e) => onChange(e.target.value)}
            />
            <button
              type="button"
              tabIndex={-1}
              onClick={() => toggleShow(stateKey)}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-xs text-app-muted hover:text-app-text select-none"
            >
              {show[stateKey] ? "hide" : "show"}
            </button>
          </div>
        </Field>
      </div>
      <SageBadge saved={Boolean(saved[stateKey])} onRemove={onRemove} />
    </div>
  );

  return (
    <div className="space-y-6">
      {/* ── Active Provider Radio ───────────────────────────────── */}
      <Card title="Active Provider">
        <p className="mb-3 text-xs text-app-muted">
          Each provider bundles search + scrape (same API key for both). Default <span className="font-medium text-app-text">Tavily</span>. If active key is missing, analysis falls back to the other provider with a warning.
        </p>
        <div className="flex gap-4">
          <label className={`flex flex-1 cursor-pointer items-center gap-2 rounded-md border px-3 py-2 ${activeProvider === "tavily" ? "border-app-text bg-app-surface" : "border-app-border bg-white"}`}>
            <input type="radio" name="active-provider" value="tavily" checked={activeProvider === "tavily"} onChange={() => pickProvider("tavily")} className="h-4 w-4 accent-app-text" />
            <span className="text-sm font-medium text-app-text">Tavily</span>
            <span className="text-xs text-app-muted">(Recommended)</span>
          </label>
          <label className={`flex flex-1 cursor-pointer items-center gap-2 rounded-md border px-3 py-2 ${activeProvider === "scrapingdog" ? "border-app-text bg-app-surface" : "border-app-border bg-white"}`}>
            <input type="radio" name="active-provider" value="scrapingdog" checked={activeProvider === "scrapingdog"} onChange={() => pickProvider("scrapingdog")} className="h-4 w-4 accent-app-text" />
            <span className="text-sm font-medium text-app-text">ScrapingDog</span>
          </label>
        </div>
        <p className="mt-2 text-xs text-app-muted">Applies to both search and page extract.</p>
      </Card>

      {/* ── Tavily (bundled) ───────────────────────────────────── */}
      <Card title="Tavily — Search & Scrape">
        <p className="mb-4 text-xs text-app-muted">
          Primary toolkit — same key for search and page extract.
        </p>
        <div className="space-y-4">
          <KeyField
            label="Tavily API key"
            stateKey="search.tavily_key"
            value={tavily}
            onChange={setTavily}
            onRemove={() => { setTavily(""); deleteSecret("search.tavily_key").catch(() => {}); setSaved((m) => ({ ...m, ["search.tavily_key"]: false })); }}
            placeholder="tvly-…"
          />
          <Field label="Tavily Search URL">
            <input
              className={inputCls}
              value={tavilySearchUrl}
              placeholder="https://api.tavily.com/search"
              onChange={(e) => setTavilySearchUrl(e.target.value)}
            />
            <p className="mt-1 text-xs text-app-muted">Default: https://api.tavily.com/search</p>
          </Field>
          <Field label="Tavily Extract URL">
            <input
              className={inputCls}
              value={tavilyExtractUrl}
              placeholder="https://api.tavily.com/extract"
              onChange={(e) => setTavilyExtractUrl(e.target.value)}
            />
            <p className="mt-1 text-xs text-app-muted">Default: https://api.tavily.com/extract</p>
          </Field>
        </div>
      </Card>

      {/* ── ScrapingDog (bundled) ─────────────────────────────── */}
      <Card title="ScrapingDog — Search & Scrape">
        <p className="mb-4 text-xs text-app-muted">
          Secondary provider — same key for search and page extract. Uses a focused query to return a summarized extract.
        </p>
        <div className="space-y-4">
          <KeyField
            label="ScrapingDog API key"
            stateKey="search.scrapingdog_key"
            value={scrapingdog}
            onChange={setScrapingdog}
            onRemove={() => { setScrapingdog(""); deleteSecret("search.scrapingdog_key").catch(() => {}); setSaved((m) => ({ ...m, ["search.scrapingdog_key"]: false })); }}
            placeholder="69344a…"
          />
          <Field label="ScrapingDog Search URL">
            <input
              className={inputCls}
              value={dogSearchUrl}
              placeholder="https://api.scrapingdog.com/google"
              onChange={(e) => setDogSearchUrl(e.target.value)}
            />
            <p className="mt-1 text-xs text-app-muted">Default: https://api.scrapingdog.com/google</p>
          </Field>
          <Field label="ScrapingDog Scrape URL">
            <input
              className={inputCls}
              value={dogScrapeUrl}
              placeholder="https://api.scrapingdog.com/scrape"
              onChange={(e) => setDogScrapeUrl(e.target.value)}
            />
            <p className="mt-1 text-xs text-app-muted">Default: https://api.scrapingdog.com/scrape</p>
          </Field>
        </div>
      </Card>

      <div className="flex items-center gap-3">
        <Button onClick={save} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </Button>
        {ok && <span className="text-sm text-app-positive">Saved ✓</span>}
        {err && <span className="text-sm text-app-negative">{err}</span>}
      </div>
    </div>
  );
}

type CountryEdit = {
  _id: string;
  isNew: boolean;
  country_code: string;
  country_name: string;
  currency_code: string;
  currency_symbol: string;
  expected_market_return: string;
  risk_free_rate: string;
  corporate_tax_rate: string;
  nominal_gdp_growth: string;
  yahoo_ticker_suffix: string;
  reference_urls: string | null;
};

function CountriesTab() {
  const [rows, setRows] = useState<CountryEdit[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [savingId, setSavingId] = useState<string | null>(null);
  const [baselines, setBaselines] = useState<Record<string, CountryEdit>>({});
  const [editingCell, setEditingCell] = useState<{ id: string; field: string } | null>(null);

  const blank = (): CountryEdit => ({
    _id: Math.random().toString(36).slice(2),
    isNew: true,
    country_code: "",
    country_name: "",
    currency_code: "",
    currency_symbol: "",
    expected_market_return: "",
    risk_free_rate: "",
    corporate_tax_rate: "",
    nominal_gdp_growth: "",
    yahoo_ticker_suffix: "",
    reference_urls: null,
  });

  const toEdit = (r: CountryRow): CountryEdit => ({
    _id: Math.random().toString(36).slice(2),
    isNew: false,
    country_code: r.country_code,
    country_name: r.country_name,
    currency_code: r.currency_code,
    currency_symbol: r.currency_symbol ?? "",
    expected_market_return: String(r.expected_market_return),
    risk_free_rate: String(r.risk_free_rate),
    corporate_tax_rate: String(r.corporate_tax_rate),
    nominal_gdp_growth: String(r.nominal_gdp_growth),
    yahoo_ticker_suffix: r.yahoo_ticker_suffix ?? "",
    reference_urls: r.reference_urls ?? null,
  });

  useEffect(() => {
    let cancel = false;
    countriesList()
      .then((list) => {
        if (cancel) return;
        const edits = list.map(toEdit);
        const base: Record<string, CountryEdit> = {};
        edits.forEach((e) => {
          base[e._id] = { ...e };
        });
        setBaselines(base);
        setRows(edits);
      })
      .catch(() => {
        if (!cancel) setErr("Could not load countries.");
      });
    return () => {
      cancel = true;
    };
  }, []);

  const isDirty = (row: CountryEdit): boolean => {
    const base = baselines[row._id];
    if (!base) return true;
    const fields: (keyof CountryEdit)[] = [
      "country_code",
      "country_name",
      "currency_code",
      "currency_symbol",
      "expected_market_return",
      "risk_free_rate",
      "corporate_tax_rate",
      "nominal_gdp_growth",
      "yahoo_ticker_suffix",
    ];
    return fields.some((k) => (row[k] ?? "") !== (base[k] ?? ""));
  };

  const handleAddCountry = () => {
    const n = blank();
    setBaselines((prev) => ({ ...prev, [n._id]: { ...n } }));
    setRows((prev) => (prev ? [...prev, n] : prev));
  };

  const patch = (id: string, field: string, value: string) => {
    setRows((prev) =>
      prev
        ? prev.map((r) => (r._id === id ? { ...r, [field]: value } : r))
        : prev,
    );
  };

  const toPayload = (e: CountryEdit): CountryPayload => {
    const num = (s: string): number => {
      const n = parseFloat(s);
      return Number.isFinite(n) ? n : 0;
    };
    return {
      country_code: e.country_code.trim().toUpperCase(),
      country_name: e.country_name.trim(),
      currency_code: e.currency_code.trim(),
      currency_symbol: e.currency_symbol.trim() ? e.currency_symbol.trim() : null,
      expected_market_return: num(e.expected_market_return),
      risk_free_rate: num(e.risk_free_rate),
      corporate_tax_rate: num(e.corporate_tax_rate),
      nominal_gdp_growth: num(e.nominal_gdp_growth),
      yahoo_ticker_suffix: e.yahoo_ticker_suffix.trim()
        ? e.yahoo_ticker_suffix.trim()
        : null,
    };
  };

  const saveRow = async (edit: CountryEdit) => {
    setErr(null);
    if (!edit.country_code.trim()) {
      setErr("Each row needs a country code before saving.");
      return;
    }
    setSavingId(edit._id);
    setEditingCell(null);
    try {
      const payload = toPayload(edit);
      await countryUpsert(payload);
      const fresh = await countriesList();
      setRows((prev) => {
        if (!prev) return prev;
        const serverMap = new Map(fresh.map((f) => [f.country_code, f]));
        return prev.map((r) => {
          const code = r._id === edit._id ? payload.country_code : r.country_code ? r.country_code.trim().toUpperCase() : "";
          const s = code ? serverMap.get(code) : undefined;
          if (s) {
            return { ...toEdit(s), _id: r._id, isNew: false };
          }
          return r._id === edit._id ? { ...r, isNew: false } : r;
        });
      });
      setBaselines((prev) => {
        const freshMap = new Map(fresh.map((f) => [f.country_code, f]));
        const s = freshMap.get(payload.country_code);
        if (s) {
          const updated = { ...toEdit(s), _id: edit._id, isNew: false };
          return { ...prev, [edit._id]: { ...updated } };
        }
        return { ...prev, [edit._id]: { ...edit, isNew: false } };
      });
    } catch {
      setErr("Save failed — run via `tauri dev`?");
    } finally {
      setSavingId(null);
    }
  };

  const discardRow = (id: string) => {
    setRows((prev) => (prev ? prev.filter((r) => r._id !== id) : prev));
    setBaselines((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
    if (editingCell?.id === id) setEditingCell(null);
  };

  const renderEditableCell = (
    row: CountryEdit,
    field: keyof CountryEdit,
    placeholder: string,
    opts?: { inputMode?: "decimal" | "text" },
  ) => {
    const isEditing = editingCell?.id === row._id && editingCell?.field === field;
    const value = (row[field] as string) ?? "";
    if (isEditing) {
      return (
        <input
          autoFocus
          type="text"
          inputMode={opts?.inputMode ?? "text"}
          className={inputCls}
          value={value}
          placeholder={placeholder}
          onChange={(e) => patch(row._id, field as string, e.target.value)}
          onBlur={() => setEditingCell(null)}
          onKeyDown={(e) => {
            if (e.key === "Enter") setEditingCell(null);
            if (e.key === "Escape") setEditingCell(null);
          }}
        />
      );
    }
    return (
      <div
        onDoubleClick={() => setEditingCell({ id: row._id, field: field as string })}
        title="Double-click to edit"
        className="min-h-[36px] cursor-pointer rounded-md px-3 py-2 text-sm text-app-text hover:bg-app-surface hover:ring-1 hover:ring-app-border"
      >
        {value ? (
          <span>{value}</span>
        ) : (
          <span className="text-app-muted/40">{placeholder}</span>
        )}
      </div>
    );
  };

  if (err && !rows) return <ErrorNote message={err} />;
  if (!rows) return <Loading label="Loading countries…" />;

  return (
    <div className="space-y-4">
      {err && <ErrorNote message={err} />}
      <div className="flex items-center justify-between">
        <p className="text-sm text-app-muted">
          {rows.filter((r) => !r.isNew).length} countries · double-click a cell to edit, then save the row.
        </p>
        <Button variant="ghost" onClick={handleAddCountry}>
          Add Country
        </Button>
      </div>
      <Table
        headers={[
          "Code",
          "Name",
          "Currency",
          "Sym",
          "Mkt return",
          "Risk-free",
          "Tax",
          "GDP growth",
          "Yahoo suffix",
          "",
        ]}
      >
        {rows.map((r) => {
          const dirty = isDirty(r);
          const saving = savingId === r._id;
          return (
            <tr key={r._id}>
              <td className="px-1 py-1">{renderEditableCell(r, "country_code", "US")}</td>
              <td className="px-1 py-1">{renderEditableCell(r, "country_name", "United States")}</td>
              <td className="px-1 py-1">{renderEditableCell(r, "currency_code", "USD")}</td>
              <td className="px-1 py-1">{renderEditableCell(r, "currency_symbol", "$")}</td>
              <td className="px-1 py-1">{renderEditableCell(r, "expected_market_return", "0.12", { inputMode: "decimal" })}</td>
              <td className="px-1 py-1">{renderEditableCell(r, "risk_free_rate", "0.05", { inputMode: "decimal" })}</td>
              <td className="px-1 py-1">{renderEditableCell(r, "corporate_tax_rate", "0.25", { inputMode: "decimal" })}</td>
              <td className="px-1 py-1">{renderEditableCell(r, "nominal_gdp_growth", "0.06", { inputMode: "decimal" })}</td>
              <td className="px-1 py-1">{renderEditableCell(r, "yahoo_ticker_suffix", ".NS")}</td>
              <td className="px-3 py-2">
                <div className="flex items-center gap-1">
                  <Button
                    onClick={() => saveRow(r)}
                    disabled={saving || !dirty}
                    className="whitespace-nowrap px-2 py-1 text-xs"
                  >
                    {saving ? "Saving…" : "Save"}
                  </Button>
                  {r.isNew && (
                    <button
                      type="button"
                      onClick={() => discardRow(r._id)}
                      className="whitespace-nowrap rounded-md px-2 py-1 text-xs text-app-muted hover:text-app-negative"
                    >
                      Discard
                    </button>
                  )}
                </div>
              </td>
            </tr>
          );
        })}
      </Table>
      {(() => {
        const taxUrls = new Set<string>();
        const rfrMap = new Map<string, string>();
        const gdpMap = new Map<string, string>();
        rows.forEach((r) => {
          if (r.isNew) return;
          const raw = r.reference_urls;
          if (!raw) return;
          try {
            const j = JSON.parse(raw) as {
              risk_free_rate?: Record<string, string>;
              tax_rate?: string;
              nominal_gdp_growth?: Record<string, string>;
            };
            if (j.risk_free_rate) {
              Object.entries(j.risk_free_rate).forEach(([k, v]) => {
                if (v) rfrMap.set(k, v);
              });
            }
            if (j.tax_rate) taxUrls.add(j.tax_rate);
            if (j.nominal_gdp_growth) {
              Object.entries(j.nominal_gdp_growth).forEach(([k, v]) => {
                if (v) gdpMap.set(k, v);
              });
            }
          } catch {
            // ignore malformed
          }
        });
        if (taxUrls.size === 0 && rfrMap.size === 0 && gdpMap.size === 0) return null;
        return (
          <Card title="References" className="mt-6">
            <p className="mb-4 text-xs text-app-muted">
              Source links stored in <code className="rounded bg-app-bg px-1 py-0.5">country_parameters.reference_urls</code> — used to refresh macro inputs.
            </p>
            <div className="grid gap-6 md:grid-cols-3">
              <div>
                <h4 className="mb-2 text-xs font-semibold uppercase tracking-wide text-app-muted">
                  Risk-free rate
                </h4>
                <p className="mb-2 text-xs text-app-muted">10-year government bond yield</p>
                <ul className="space-y-1">
                  {Array.from(rfrMap.entries())
                    .sort(([a], [b]) => a.localeCompare(b))
                    .map(([code, url]) => (
                      <li key={code}>
                        <a
                          href={url}
                          target="_blank"
                          rel="noreferrer"
                          className="text-xs text-app-text underline decoration-app-muted underline-offset-2 hover:text-app-positive"
                        >
                          {code} — {new URL(url).hostname.replace("www.", "")}
                        </a>
                      </li>
                    ))}
                  {rfrMap.size === 0 && (
                    <li className="text-xs text-app-muted/60">No links</li>
                  )}
                </ul>
              </div>
              <div>
                <h4 className="mb-2 text-xs font-semibold uppercase tracking-wide text-app-muted">
                  Corporate tax rate
                </h4>
                <p className="mb-2 text-xs text-app-muted">Statutory rate / country premium</p>
                <ul className="space-y-1">
                  {Array.from(taxUrls).map((url) => (
                    <li key={url}>
                      <a
                        href={url}
                        target="_blank"
                        rel="noreferrer"
                        className="text-xs text-app-text underline decoration-app-muted underline-offset-2 hover:text-app-positive"
                      >
                        {new URL(url).hostname.replace("www.", "")} — Damodaran
                      </a>
                    </li>
                  ))}
                  {taxUrls.size === 0 && (
                    <li className="text-xs text-app-muted/60">No links</li>
                  )}
                </ul>
              </div>
              <div>
                <h4 className="mb-2 text-xs font-semibold uppercase tracking-wide text-app-muted">
                  Nominal GDP growth
                </h4>
                <p className="mb-2 text-xs text-app-muted">CEIC nominal growth indicator</p>
                <ul className="space-y-1">
                  {Array.from(gdpMap.entries())
                    .sort(([a], [b]) => a.localeCompare(b))
                    .map(([code, url]) => (
                      <li key={code}>
                        <a
                          href={url}
                          target="_blank"
                          rel="noreferrer"
                          className="text-xs text-app-text underline decoration-app-muted underline-offset-2 hover:text-app-positive"
                        >
                          {code} — {new URL(url).hostname.replace("www.", "")}
                        </a>
                      </li>
                    ))}
                  {gdpMap.size === 0 && (
                    <li className="text-xs text-app-muted/60">No links</li>
                  )}
                </ul>
              </div>
            </div>
          </Card>
        );
      })()}
    </div>
  );
}

function PromptsTab() {
  const PROMPTS_SELECTED_KEY = "ui.prompts_selected_key";
  const [prompts, setPrompts] = useState<PromptRow[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [saving, setSaving] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [ok, setOk] = useState(false);

  useEffect(() => {
    let cancel = false;
    Promise.all([promptsList(), getSetting(PROMPTS_SELECTED_KEY).catch(() => null)])
      .then(([list, savedKey]) => {
        if (cancel) return;
        setPrompts(list);
        if (list.length > 0) {
          const restore = savedKey && list.some((p) => p.prompt_key === savedKey) ? savedKey : list[0].prompt_key;
          setSelected(restore);
          const match = list.find((p) => p.prompt_key === restore);
          setText(match?.prompt_text ?? list[0].prompt_text);
        }
      })
      .catch(() => {
        if (!cancel) setErr("Could not load prompts.");
      });
    return () => {
      cancel = true;
    };
  }, []);

  const current = prompts?.find((p) => p.prompt_key === selected) ?? null;

  const selectPrompt = (key: string) => {
    setSelected(key);
    setSetting(PROMPTS_SELECTED_KEY, key).catch(() => {});
    const p = prompts?.find((x) => x.prompt_key === key);
    setText(p?.prompt_text ?? "");
    setOk(false);
    setErr(null);
  };

  const save = async () => {
    if (!selected) return;
    setSaving(true);
    setErr(null);
    setOk(false);
    try {
      await promptUpdate(selected, text);
      // Refetch so version/updated_at are authoritative and edits survive tab switches
      const fresh = await promptsList();
      setPrompts(fresh);
      const updated = fresh.find((p) => p.prompt_key === selected);
      if (updated) setText(updated.prompt_text);
      setOk(true);
    } catch {
      setErr("Save failed — run via `tauri dev`?");
    } finally {
      setSaving(false);
    }
  };

  const restore = async () => {
    if (!selected || !current) return;
    if (!confirm(`Restore "${current.prompt_name}" to its default? This will overwrite your edits and reset version to 1.`)) return;
    setRestoring(true);
    setErr(null);
    setOk(false);
    try {
      await promptRestoreDefault(selected);
      const fresh = await promptsList();
      setPrompts(fresh);
      const updated = fresh.find((p) => p.prompt_key === selected);
      if (updated) setText(updated.prompt_text);
      setOk(true);
    } catch {
      setErr("Restore failed — run via `tauri dev`?");
    } finally {
      setRestoring(false);
    }
  };

  if (!prompts) return <Loading label="Loading prompts…" />;

  return (
    <div className="grid gap-6 lg:grid-cols-[220px_minmax(0,1fr)]">
      <div className="space-y-1">
        {prompts.map((p, idx) => (
          <button
            key={p.prompt_key}
            type="button"
            onClick={() => selectPrompt(p.prompt_key)}
            className={`block w-full rounded-md border px-3 py-2 text-left text-sm ${
              selected === p.prompt_key
                ? "border-app-border bg-app-surface"
                : "border-transparent text-app-muted hover:bg-app-surface"
            }`}
          >
            <span className="block font-medium text-app-text">{idx + 1}. {p.prompt_name}</span>
            <span className="block text-xs text-app-muted">
              v{p.version}
            </span>
          </button>
        ))}
      </div>
      <div className="min-w-0">
        {current ? (
          <Card title={current.prompt_name}>
            {current.description && (
              <p className="mb-2 text-xs text-app-muted">{current.description}</p>
            )}
            <p className="mb-3 text-xs text-app-muted">
              Version {current.version}
              {current.is_active ? " · active" : ""}
              {current.updated_at ? ` · updated ${current.updated_at}` : ""}
            </p>
            <textarea
              className={`${inputCls} min-h-[520px] w-full resize-y font-mono text-sm leading-relaxed`}
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={22}
            />
            <div className="mt-4 flex items-center gap-3">
              <Button onClick={save} disabled={saving || restoring}>
                {saving ? "Saving…" : "Save prompt"}
              </Button>
              <Button variant="ghost" onClick={restore} disabled={saving || restoring}>
                {restoring ? "Restoring…" : "Restore defaults"}
              </Button>
              {ok && <span className="text-sm text-app-positive">Saved.</span>}
              {err && <span className="text-sm text-app-negative">{err}</span>}
            </div>
          </Card>
        ) : (
          <p className="text-sm text-app-muted">Select a prompt to edit it.</p>
        )}
      </div>
    </div>
  );
}

const DEFAULT_MARKET_KEY = "general.default_market";
const PDF_FOLDER_KEY = "general.pdf_save_folder";

function AppearanceTab() {
  const [collapsed, setCollapsed] = useState(false);
  const [loaded, setLoaded] = useState(false);
  // Default market
  const [defaultMarket, setDefaultMarket] = useState("US");
  const [countries, setCountries] = useState<CountryRow[]>([]);
  const [marketSaving, setMarketSaving] = useState(false);
  const [marketOk, setMarketOk] = useState(false);
  const [marketErr, setMarketErr] = useState<string | null>(null);
  // PDF save folder
  const [pdfFolder, setPdfFolder] = useState("");
  const [pdfSaving, setPdfSaving] = useState(false);
  const [pdfOk, setPdfOk] = useState(false);
  const [pdfErr, setPdfErr] = useState<string | null>(null);
  const [downloadDirPath, setDownloadDirPath] = useState<string | null>(null);

  useEffect(() => {
    let cancel = false;
    Promise.all([
      getSetting(SIDEBAR_SETTING),
      getSetting(DEFAULT_MARKET_KEY),
      getSetting(PDF_FOLDER_KEY),
      countriesList().catch(() => [] as CountryRow[]),
      import("@tauri-apps/api/path").then((m) => m.downloadDir().catch(() => null)).catch(() => null),
    ])
      .then(([sidebar, market, folder, list, dl]) => {
        if (cancel) return;
        if (sidebar != null) setCollapsed(sidebar === "true");
        if (market && market.trim()) setDefaultMarket(market.trim().toUpperCase());
        if (folder && folder.trim()) setPdfFolder(folder.trim());
        else if (dl && typeof dl === "string" && dl.trim()) setPdfFolder(dl.trim());
        if (Array.isArray(list) && list.length) setCountries(list);
        if (typeof dl === "string" && dl) setDownloadDirPath(dl);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancel) setLoaded(true);
      });
    return () => {
      cancel = true;
    };
  }, []);

  const toggle = (v: boolean) => {
    setCollapsed(v);
    setSetting(SIDEBAR_SETTING, String(v)).catch(() => {});
  };

  const saveDefaultMarket = async () => {
    setMarketSaving(true);
    setMarketErr(null);
    setMarketOk(false);
    try {
      const v = defaultMarket.trim().toUpperCase();
      if (!v) throw new Error("Select a market");
      await setSetting(DEFAULT_MARKET_KEY, v);
      setMarketOk(true);
    } catch {
      setMarketErr("Save failed — run via `tauri dev`?");
    } finally {
      setMarketSaving(false);
    }
  };

  const savePdfFolder = async () => {
    setPdfSaving(true);
    setPdfErr(null);
    setPdfOk(false);
    try {
      const v = pdfFolder.trim();
      // empty = always ask (no default)
      await setSetting(PDF_FOLDER_KEY, v);
      setPdfOk(true);
    } catch {
      setPdfErr("Save failed — run via `tauri dev`?");
    } finally {
      setPdfSaving(false);
    }
  };

  const browsePdfFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const dir = await open({ directory: true, title: "Choose PDF save folder" });
      if (dir && typeof dir === "string") setPdfFolder(dir);
    } catch {
      setPdfErr("Browse failed — run via `tauri dev`?");
    }
  };

  const resetPdfFolder = async () => {
    try {
      const { downloadDir } = await import("@tauri-apps/api/path");
      const dl = await downloadDir();
      setPdfFolder(dl);
    } catch {
      if (downloadDirPath) setPdfFolder(downloadDirPath);
    }
  };

  if (!loaded) return <Loading label="Loading miscellaneous…" />;

  return (
    <div className="space-y-6">
      <Card title="Appearance">
        <p className="text-sm text-app-muted">
          Light theme only — no dark mode toggle.
        </p>
        <div className="mt-4 flex items-center gap-3 text-sm text-app-text">
          <input
            type="checkbox"
            checked={collapsed}
            onChange={(e) => toggle(e.target.checked)}
            className="h-4 w-4 accent-app-text"
          />
          Sidebar starts collapsed
        </div>
        <p className="mt-3 text-xs text-app-muted">
          Window size restores on next launch.
        </p>
      </Card>

      <Card title="Default market">
        <p className="mb-3 text-sm text-app-muted">
          Used to pre-select the <span className="font-medium text-app-text">Market</span> on the New Analysis page.
        </p>
        <Field label="Default market">
          <select
            className={inputCls}
            value={defaultMarket}
            onChange={(e) => setDefaultMarket(e.target.value)}
          >
            {countries.length ? countries.map((c) => (
              <option key={c.country_code} value={c.country_code}>
                {c.country_code} — {c.country_name}
              </option>
            )) : (
              <>
                <option value="US">US — United States</option>
                <option value="IN">IN — India</option>
                <option value="GB">GB — United Kingdom</option>
                <option value="AU">AU — Australia</option>
              </>
            )}
          </select>
          <p className="mt-1 text-xs text-app-muted">Dynamic list from Country Parameters — add countries there to see them here.</p>
        </Field>
        <div className="mt-4 flex items-center gap-3">
          <Button onClick={saveDefaultMarket} disabled={marketSaving}>{marketSaving ? "Saving…" : "Save"}</Button>
          {marketOk && <span className="text-sm text-app-positive">Saved ✓</span>}
          {marketErr && <span className="text-sm text-app-negative">{marketErr}</span>}
        </div>
      </Card>

      <Card title="PDF save folder">
        <p className="mb-3 text-sm text-app-muted">
          Defaults to your Downloads folder. Leave empty to be prompted each time.
        </p>
        <Field label="Save location">
          <div className="flex gap-2">
            <input className={`${inputCls} flex-1`} value={pdfFolder} onChange={(e) => setPdfFolder(e.target.value)} placeholder={downloadDirPath || "~/Downloads"} />
            <Button variant="ghost" onClick={browsePdfFolder}>Browse…</Button>
            <Button variant="ghost" onClick={resetPdfFolder}>Reset to Downloads</Button>
          </div>
          <p className="mt-1 text-xs text-app-muted">You’ll still choose the file name.</p>
        </Field>
        <div className="mt-4 flex items-center gap-3">
          <Button onClick={savePdfFolder} disabled={pdfSaving}>{pdfSaving ? "Saving…" : "Save"}</Button>
          {pdfOk && <span className="text-sm text-app-positive">Saved ✓</span>}
          {pdfErr && <span className="text-sm text-app-negative">{pdfErr}</span>}
        </div>
      </Card>
    </div>
  );
}

function AnalysisQueueTab() {
  const [maxParallel, setMaxParallel] = useState<number>(1);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [ok, setOk] = useState(false);

  useEffect(() => {
    let cancel = false;
    getSetting("analysis.max_parallel")
      .then((v) => {
        if (cancel) return;
        const n = v ? parseInt(v, 10) : 1;
        if (!Number.isNaN(n)) setMaxParallel(Math.max(1, Math.min(5, n)));
      })
      .catch(() => {})
      .finally(() => {
        if (!cancel) setLoaded(true);
      });
    return () => { cancel = true; };
  }, []);

  const save = async () => {
    setSaving(true);
    setErr(null);
    setOk(false);
    try {
      const v = Math.max(1, Math.min(5, maxParallel));
      await setSetting("analysis.max_parallel", String(v));
      // Also update in-memory queue semaphore via Tauri command
      await import("../../lib/api").then(m => m.setMaxParallel(v).catch(() => {}));
      setOk(true);
    } catch {
      setErr("Save failed — run via `tauri dev`?");
    } finally {
      setSaving(false);
    }
  };

  if (!loaded) return <Loading label="Loading queue settings…" />;

  return (
    <div className="space-y-6">
      <Card title="Queue & Parallelism">
        <p className="mb-4 text-sm text-app-muted">
          Choose how many analyses run at once. Queued jobs start in order as earlier ones finish.
        </p>
        <Field label="Max parallel analyses (1-5)">
          <select className={inputCls} value={String(maxParallel)} onChange={(e) => setMaxParallel(parseInt(e.target.value, 10))}>
            <option value="1">1 — sequential (default)</option>
            <option value="2">2 — dual</option>
            <option value="3">3 — concurrent</option>
            <option value="4">4 — high</option>
            <option value="5">5 — max parallel</option>
          </select>
          <p className="mt-1 text-xs text-app-muted">Higher values run more analyses in parallel. Limited by LLM rate limits (TPM/RPM) and AI-tool quotas. Change applies immediately to pending queue.</p>
        </Field>
        <div className="mt-4 flex items-center gap-3">
          <Button onClick={save} disabled={saving}>{saving ? "Saving…" : "Save"}</Button>
          {ok && <span className="text-sm text-app-positive">Saved ✓</span>}
          {err && <span className="text-sm text-app-negative">{err}</span>}
        </div>
      </Card>
      <Card title="How it works">
        <ul className="list-disc space-y-1 pl-5 text-sm text-app-muted">
          <li>Start multiple analyses from New Analysis — they stay visible while you navigate the app.</li>
          <li>Queued jobs wait in order; the next one starts when a slot frees.</li>
          <li>Finished jobs leave the queue automatically.</li>
          <li>If the same ticker and model already exists with no new financials, the request is skipped without calling the AI.</li>
        </ul>
      </Card>
      <Card title="Rate limits — what to watch">
        <p className="mb-3 text-xs text-app-muted">5 parallel = 5× TPM/RPS. Free tiers throttle first — paid tiers and local hardware scale higher.</p>
        <div className="grid gap-4 md:grid-cols-2">
          <div>
            <h4 className="text-xs font-semibold text-app-text">LLM providers</h4>
            <ul className="mt-1 list-disc space-y-0.5 pl-4 text-xs text-app-muted">
              <li>Anthropic / Kimi / DeepSeek free: 50–60 RPM — keep 1–2; use 3+ only after raising tier.</li>
              <li>OpenAI paid Tier 2+: 3 is safe; 4–5 only with higher TPM quota.</li>
              <li>Custom local (Ollama / LM Studio / MLX): 1 on 16 GB Mac, ≤2 on 32 GB+ GPU; large models may OOM → 507.</li>
            </ul>
          </div>
          <div>
            <h4 className="text-xs font-semibold text-app-text">AI tools</h4>
            <ul className="mt-1 list-disc space-y-0.5 pl-4 text-xs text-app-muted">
              <li>Tavily: free 1,000 searches/mo; ~2–5 searches + 0–3 extracts per report — 100 reports ≈ 300 searches.</li>
              <li>ScrapingDog: 100–200/mo free; paid scales to 600+ RPS — quota burns 5× faster at max parallel.</li>
            </ul>
          </div>
        </div>
        <p className="mt-3 text-xs text-app-muted">On 429 the job fails fast (no retry yet) — lower parallelism or raise tier if you see throttle errors.</p>
      </Card>
    </div>
  );
}

function DebugLogsPanel() {
  const [logs, setLogs] = useState<DebugLogMeta[] | null>(null);
  const [dir, setDir] = useState<string>("");
  void dir;
  const [selected, setSelected] = useState<string | null>(null);
  const [entries, setEntries] = useState<DebugLogRead | null>(null);
  const [filter, setFilter] = useState<string>("all");
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [clearOk, setClearOk] = useState<string | null>(null);

  const refresh = () => {
    setErr(null);
    setClearOk(null);
    Promise.all([debugLogDir(), debugLogsList()])
      .then(([d, list]) => {
        setDir(d);
        setLogs(list);
      })
      .catch(() => setErr("Could not load debug logs."));
  };

  useEffect(() => { refresh(); }, []);

  const openLog = async (runId: string) => {
    setSelected(runId);
    setEntries(null);
    setLoading(true);
    try {
      const data = await debugLogRead(runId, 500);
      setEntries(data);
    } catch (e) { setErr(typeof e === "string" ? e : "Failed to read log."); }
    finally { setLoading(false); }
  };

  const handleClear = async () => {
    if (!confirm(`Clear all ${logs?.length ?? 0} debug log files?`)) return;
    try {
      const n = await debugLogsClear();
      setClearOk(`Cleared ${n} files.`);
      setSelected(null);
      setEntries(null);
      refresh();
    } catch (e) { setErr(typeof e === "string" ? e : "Clear failed."); }
  };

  const kinds = entries ? Array.from(new Set(entries.entries.map((e) => e.kind))).sort() : [];
  const filtered = entries ? entries.entries.filter((e) => filter === "all" || e.kind === filter) : [];

  const kindColor: Record<string, string> = {
    inputs: "bg-app-positive/10 text-app-positive",
    llm_request: "bg-app-warning/10 text-app-warning",
    llm_response: "bg-app-positive/10 text-app-positive",
    thinking: "bg-app-muted/20 text-app-muted",
    tool_call: "bg-blue-500/10 text-blue-600",
    tool_result: "bg-blue-500/10 text-blue-700",
    engine_input: "bg-purple-500/10 text-purple-600",
    engine_output: "bg-purple-500/10 text-purple-700",
    run_start: "bg-app-surface text-app-text",
    parse_failed: "bg-app-negative/10 text-app-negative",
    abort: "bg-app-negative/10 text-app-negative",
  };

  return (
    <Card title="Debug Logs — AI agentic trace">
      <p className="mb-3 text-xs text-app-muted">
        Logs are saved per run in the app’s log folder. Open a run to view inputs, tool calls, and results. Secrets are redacted.
      </p>
      <div className="mb-3 flex items-center gap-2">
        <Button variant="ghost" onClick={refresh}>Refresh</Button>
        <Button variant="ghost" onClick={handleClear} disabled={!logs || logs.length === 0}>Clear all</Button>
        {clearOk && <span className="text-xs text-app-positive">{clearOk}</span>}
        {err && <span className="text-xs text-app-negative">{err}</span>}
        {logs && <span className="ml-auto text-xs text-app-muted">{logs.length} files</span>}
      </div>
      {!logs ? <Loading label="Loading debug logs…" /> : logs.length === 0 ? (
        <p className="rounded-md border border-dashed border-app-border bg-app-surface px-3 py-4 text-center text-xs text-app-muted">
          No debug logs yet — run an analysis from New Analysis to generate a trace.
        </p>
      ) : (
        <div className="grid gap-4 lg:grid-cols-[280px_minmax(0,1fr)]">
          <div className="max-h-96 overflow-auto rounded-md border border-app-border bg-white">
            {logs.map((l) => (
              <button
                key={l.run_id}
                onClick={() => openLog(l.run_id)}
                className={`block w-full border-b border-app-border px-3 py-2 text-left hover:bg-app-surface ${selected === l.run_id ? "bg-app-surface" : ""}`}
              >
                <div className="truncate text-xs font-medium text-app-text">{l.run_id}</div>
                <div className="text-[11px] text-app-muted">{l.modified ? new Date(l.modified).toLocaleString() : ""} · {(l.size_bytes / 1024).toFixed(1)} KB</div>
              </button>
            ))}
          </div>
          <div className="min-w-0">
            {!selected ? (
              <p className="py-8 text-center text-xs text-app-muted">Select a log on the left to inspect the full agentic trace.</p>
            ) : loading ? <Loading label="Reading log…" /> : !entries ? (
              <ErrorNote message="Could not read log." />
            ) : (
              <div className="space-y-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-xs text-app-muted">{entries.summary} · filter:</span>
                  <select className={inputCls + " h-8 py-1 text-xs"} value={filter} onChange={(e) => setFilter(e.target.value)}>
                    <option value="all">all ({entries.entries.length})</option>
                    {kinds.map((k) => (
                      <option key={k} value={k}>{k} ({entries.entries.filter((e) => e.kind === k).length})</option>
                    ))}
                  </select>
                  <span className="ml-auto font-mono text-[11px] text-app-muted">{selected}.jsonl</span>
                </div>
                <div className="max-h-[520px] space-y-2 overflow-auto rounded-md border border-app-border bg-app-surface p-2">
                  {filtered.map((e) => (
                    <details key={e.seq} className="rounded-md border border-app-border bg-white">
                      <summary className="flex cursor-pointer list-none items-center gap-2 px-2 py-1.5 text-xs">
                        <span className={`inline-flex rounded px-1.5 py-0.5 text-[11px] font-medium ${kindColor[e.kind] ?? "bg-app-bg text-app-muted"}`}>{e.kind}</span>
                        <span className="font-mono text-[11px] text-app-muted">#{e.seq} · +{e.elapsed_ms}ms</span>
                        <span className="ml-auto text-[11px] text-app-muted">{e.ts ? new Date(e.ts).toLocaleTimeString() : ""}</span>
                      </summary>
                      <pre className="max-h-64 overflow-auto whitespace-pre-wrap border-t border-app-border bg-app-bg px-2 py-1.5 font-mono text-[11px] text-app-text">{JSON.stringify(e.payload, null, 2)}</pre>
                    </details>
                  ))}
                  {filtered.length === 0 && <p className="py-4 text-center text-xs text-app-muted">No entries for filter “{filter}”.</p>}
                </div>
                <p className="text-[11px] text-app-muted">Secrets (API keys etc.) are redacted as ***REDACTED***. Large payloads truncated at ~20KB.</p>
              </div>
            )}
          </div>
        </div>
      )}
    </Card>
  );
}

function DataTab() {
  const [info, setInfo] = useState<DataInfo | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const refresh = () => {
    getDataInfo()
      .then((d) => setInfo(d))
      .catch(() => setErr("Could not load data info."));
  };
  useEffect(() => {
    refresh();
  }, []);

  const [backupBusy, setBackupBusy] = useState(false);
  const [backupMsg, setBackupMsg] = useState<string | null>(null);
  const [backupErr, setBackupErr] = useState<string | null>(null);
  const [restoreBusy, setRestoreBusy] = useState(false);
  const [restoreMsg, setRestoreMsg] = useState<string | null>(null);
  const [restoreErr, setRestoreErr] = useState<string | null>(null);

  const handleBackup = async () => {
    setBackupBusy(true);
    setBackupMsg(null);
    setBackupErr(null);
    try {
      const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      if (!isTauri) {
        const data = await backupExport();
        const blob = new Blob([new Uint8Array(data)], { type: "application/octet-stream" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = `stocksy-backup-${new Date().toISOString().slice(0, 10)}.sbk`;
        a.style.display = "none";
        document.body.appendChild(a);
        a.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window }));
        setTimeout(() => { document.body.removeChild(a); URL.revokeObjectURL(url); }, 2000);
        setBackupMsg(`Backup downloaded (${(data.length/1024).toFixed(1)} KB).`);
        return;
      }
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { downloadDir } = await import("@tauri-apps/api/path");
      let defDir: string | null = null;
      try { defDir = await downloadDir(); } catch { defDir = null; }
      const defaultPath = defDir ? `${defDir.replace(/\/$/, "")}/stocksy-backup-${new Date().toISOString().slice(0, 10)}.sbk` : `stocksy-backup-${new Date().toISOString().slice(0, 10)}.sbk`;
      const picked: string | null = (await save({ defaultPath, filters: [{ name: "Stocksy Backup", extensions: ["sbk"] }] }) as string | null);
      if (!picked) { setBackupBusy(false); return; }
      const data: number[] = await backupExport();
      await saveBackupFile(picked, data);
      setBackupMsg(`Backup saved to ${picked} (${(data.length/1024).toFixed(1)} KB).`);
    } catch (e) {
      setBackupErr(typeof e === "string" ? e : `Backup failed: ${String(e)}`);
    } finally {
      setBackupBusy(false);
    }
  };

  const handleRestore = async () => {
    setRestoreBusy(true);
    setRestoreMsg(null);
    setRestoreErr(null);
    try {
      const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      let bytes: number[] | null = null;
      if (!isTauri) {
        const input = document.createElement("input");
        input.type = "file";
        input.accept = ".sbk";
        const picked: File | null = await new Promise((resolve) => {
          input.onchange = () => resolve(input.files?.[0] ?? null);
          input.click();
        });
        if (!picked) { setRestoreBusy(false); return; }
        const buf = await picked.arrayBuffer();
        bytes = Array.from(new Uint8Array(buf));
      } else {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const picked = await open({ filters: [{ name: "Stocksy Backup", extensions: ["sbk"] }], multiple: false }) as string | string[] | null;
        const path = Array.isArray(picked) ? picked[0] : picked;
        if (!path || typeof path !== "string") { setRestoreBusy(false); return; }
        const { readFile } = await import("@tauri-apps/plugin-fs");
        const data = await readFile(path);
        bytes = Array.from(data as Uint8Array);
      }
      if (!bytes) { setRestoreBusy(false); return; }
      let meta: BackupMeta;
      try {
        meta = await backupValidate(bytes);
      } catch (e) {
        setRestoreErr(typeof e === "string" ? e : `Invalid backup file: ${String(e)}`);
        setRestoreBusy(false);
        return;
      }
      const summary = `Restore backup from ${new Date(meta.created_at).toLocaleString()} (app ${meta.app_version})?\n\n` +
        `This will REPLACE 7 tables:\n` +
        `• ${meta.counts.analysis_reports} reports\n` +
        `• ${meta.counts.financial_data_cache} cached financials\n` +
        `• ${meta.counts.watchlists} watchlists (${meta.counts.watchlist_items} items)\n` +
        `• ${meta.counts.country_parameters} countries, ${meta.counts.system_prompts} prompts, ${meta.counts.app_settings} settings\n\n` +
        `All current data in those tables will be overwritten. Continue?`;
      if (!confirm(summary)) { setRestoreBusy(false); return; }
      const restored = await backupRestore(bytes);
      setRestoreMsg(`Restore complete — ${restored.counts.analysis_reports} reports, ${restored.counts.watchlists} watchlists restored from ${new Date(restored.created_at).toLocaleDateString()}. Refreshing…`);
      refresh();
    } catch (e) {
      setRestoreErr(typeof e === "string" ? e : `Restore failed: ${String(e)}`);
    } finally {
      setRestoreBusy(false);
    }
  };

  if (err) return <ErrorNote message={err} />;
  if (!info) return <Loading label="Loading data info…" />;

  const mb = info.db_size_bytes / (1024 * 1024);
  const sizeLabel =
    mb >= 1 ? `${mb.toFixed(2)} MB` : `${(info.db_size_bytes / 1024).toFixed(1)} KB`;

  return (
    <div className="space-y-6">
      <Card title="Database">
        <p className="break-all text-sm text-app-text">{info.db_path}</p>
      </Card>
      <div className="grid grid-cols-2 gap-4">
        <StatCard
          label="Database size"
          value={sizeLabel}
          sub={`${fmtNum(info.db_size_bytes)} bytes`}
        />
        <StatCard label="Settings rows" value={String(info.settings)} />
        <StatCard label="Countries" value={String(info.countries)} />
        <StatCard label="Prompts" value={String(info.prompts)} />
        <StatCard label="Ticker cache" value={String(info.ticker_cache)} />
        <StatCard label="Financial cache" value={String(info.financial_cache)} />
      </div>
      <Card title="Backup & restore">
        <p className="mb-4 text-sm text-app-muted">
          Back up and restore your reports, watchlists, countries, prompts and settings (.sbk file). Backups don’t include API keys.
        </p>
        <div className="flex flex-wrap gap-3">
          <Button onClick={handleBackup} disabled={backupBusy || restoreBusy}>{backupBusy ? "Backing up…" : "Backup… (save .sbk)"}</Button>
          <Button variant="ghost" onClick={handleRestore} disabled={backupBusy || restoreBusy}>{restoreBusy ? "Restoring…" : "Restore… (open .sbk)"}</Button>
          <Button variant="ghost" onClick={refresh} disabled={backupBusy || restoreBusy}>Refresh</Button>
        </div>
        {backupMsg && <p className="mt-3 text-sm text-app-positive">{backupMsg}</p>}
        {backupErr && <p className="mt-3 text-sm text-app-negative">{backupErr}</p>}
        {restoreMsg && <p className="mt-3 text-sm text-app-positive">{restoreMsg}</p>}
        {restoreErr && <p className="mt-3 text-sm text-app-negative">{restoreErr}</p>}
      </Card>
      <DebugLogsPanel />
    </div>
  );
}

export default function Settings() {
  const [activeTab, setActiveTab] = useState<TabId | null>(null);
  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    let cancel = false;
    const params = new URLSearchParams(location.search);
    const urlTab = params.get("tab") as TabId | null;
    if (urlTab && TABS.some((t) => t.id === urlTab)) {
      setActiveTab(urlTab);
      setSetting(ACTIVE_TAB_SETTING, urlTab).catch(() => {});
      return () => {
        cancel = true;
      };
    }
    getSetting(ACTIVE_TAB_SETTING)
      .then((v) => {
        if (cancel) return;
        const found = TABS.find((t) => t.id === v);
        setActiveTab(found ? found.id : "llm");
      })
      .catch(() => {
        if (!cancel) setActiveTab("llm");
      });
    return () => {
      cancel = true;
    };
  }, [location.search]);

  const selectTab = (id: TabId) => {
    setActiveTab(id);
    setSetting(ACTIVE_TAB_SETTING, id).catch(() => {});
    navigate(`/settings?tab=${id}`, { replace: true });
  };

  if (!activeTab) {
    return <Loading label="Loading settings…" />;
  }

  return (
    <div className={`${activeTab === "prompts" ? "max-w-7xl" : "max-w-5xl"} space-y-6`}>
      <SettingsHeader />
      <div className="flex gap-6">
        <nav className="w-44 shrink-0 space-y-1" aria-label="Settings sections">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => selectTab(t.id)}
              className={`block w-full rounded-md px-3 py-2 text-left text-sm transition ${
                activeTab === t.id
                  ? "bg-app-surface font-medium text-app-text"
                  : "text-app-muted hover:bg-app-surface hover:text-app-text"
              }`}
            >
              {t.title}
            </button>
          ))}
        </nav>
        <div className="min-w-0 flex-1">
          {activeTab === "llm" && <LlmProviderTab />}
          {activeTab === "tools" && <AiAgentToolsTab />}
          {activeTab === "countries" && <CountriesTab />}
          {activeTab === "prompts" && <PromptsTab />}
          {activeTab === "appearance" && <AppearanceTab />}
          {activeTab === "data" && <DataTab />}
          {activeTab === "analysis" && <AnalysisQueueTab />}
        </div>
      </div>
    </div>
  );
}