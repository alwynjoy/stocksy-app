import { useEffect, useMemo, useState, useRef } from "react";
import { useNavigate, useSearchParams, useLocation } from "react-router-dom";
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, Cell, ScatterChart, Scatter, RadarChart, PolarGrid, PolarAngleAxis, PolarRadiusAxis, Radar, Legend } from "recharts";
import { getScreenerRows, stubScreenerRows, getSetting, setSetting, watchlistsList, watchlistAddItem, watchlistCreate, type ScreenerRow, type WatchlistRow } from "../../lib/api";
import { Plus } from "lucide-react";
import { Card, ErrorNote, Loading, VerdictPill, Button, inputCls } from "../../components/ui";
import { fmtPct, fmtNum, gapCls } from "../../lib/format";
import { BUILTIN_PRESETS, DEFAULT_FILTERS, PRESETS_STORAGE_KEY, type ScreenerFilters, type ScreenerPreset } from "../../lib/screenerPresets";

type SortKey = "ticker" | "company" | "sector" | "market_cap_category" | "verdict" | "weighted_fair_value" | "gap_pct" | "confidence" | "risk_reward" | "revenue_growth";
type SortDir = "asc" | "desc";
type SortState = { key: SortKey; dir: SortDir }[];

const BAR_COLORS = ["#0f172a", "#334155", "#475569", "#64748b", "#94a3b8", "#94a3b8"];
const PAGE_SIZE = 50;
const RADAR_COLORS = ["#0f172a", "#10b981", "#f59e0b", "#ef4444", "#6366f1"];

function parsePresets(raw: string | null): ScreenerPreset[] {
  if (!raw) return [];
  try {
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr
      .filter((p) => p && typeof p.name === "string" && p.filters && typeof p.filters === "object")
      .filter((p) => !("gapMin" in p.filters) && !("confidenceMin" in p.filters))
      .filter((p) => p.filters.gapBucket != null || p.filters.confidenceBucket != null || p.filters.riskBucket != null || p.filters.growthBucket != null || p.filters.country != null)
      .map((p) => {
        const f = p.filters as Partial<ScreenerFilters>;
        return {
          name: p.name,
          filters: {
            country: f.country ?? "All",
            sector: f.sector ?? "All",
            marketCap: f.marketCap ?? "All",
            verdict: f.verdict ?? "All",
            gapBucket: (f.gapBucket as ScreenerFilters["gapBucket"]) ?? "all",
            confidenceBucket: (f.confidenceBucket as ScreenerFilters["confidenceBucket"]) ?? "all",
            riskBucket: (f.riskBucket as ScreenerFilters["riskBucket"]) ?? "all",
            growthBucket: (f.growthBucket as ScreenerFilters["growthBucket"]) ?? "all",
          },
        } as ScreenerPreset;
      });
  } catch {
    return [];
  }
}

function passesGap(pct: number, b: ScreenerFilters["gapBucket"]): boolean {
  if (b === "all") return true;
  if (b === "<=-50") return pct <= -50;
  if (b === "-50_0") return pct > -50 && pct < 0;
  if (b === "0_25") return pct >= 0 && pct < 25;
  if (b === "25_100") return pct >= 25 && pct < 100;
  if (b === ">=100") return pct >= 100;
  return true;
}
function passesConfidence(v: number, b: ScreenerFilters["confidenceBucket"]): boolean {
  if (b === "all") return true;
  if (b === "<50") return v < 50;
  if (b === ">=50") return v >= 50;
  return true;
}
function passesRisk(v: number, b: ScreenerFilters["riskBucket"]): boolean {
  if (b === "all") return true;
  if (b === "<0.5") return v < 0.5;
  if (b === "0.5_1") return v >= 0.5 && v < 1;
  if (b === "1_2") return v >= 1 && v < 2;
  if (b === ">=2") return v >= 2;
  return true;
}
function passesGrowth(pct: number, b: ScreenerFilters["growthBucket"]): boolean {
  if (b === "all") return true;
  if (b === "<=0") return pct <= 0;
  if (b === "0_25") return pct > 0 && pct < 25;
  if (b === "25_50") return pct >= 25 && pct < 50;
  if (b === "50_100") return pct >= 50 && pct < 100;
  if (b === ">=100") return pct >= 100;
  return true;
}

function extractRadarScores(row: ScreenerRow): Record<string, number> {
  const cd = row.confidence_details as unknown as { dimensions?: Record<string, { score: number; max_score: number }> } | null;
  if (cd && cd.dimensions) {
    const out: Record<string, number> = {};
    for (const [k, v] of Object.entries(cd.dimensions)) {
      const max = v.max_score || 1;
      out[k] = Math.round((v.score / max) * 100);
    }
    return out;
  }
  // fallback: distribute overall confidence evenly (approx) — but show 0
  return {};
}

export default function Screener() {
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();
  const [rows, setRows] = useState<ScreenerRow[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [filters, setFilters] = useState<ScreenerFilters>(() => {
    const get = (k: string) => searchParams.get(k);
    const gap = get("gapBucket") as ScreenerFilters["gapBucket"] | null;
    const conf = get("confidenceBucket") as ScreenerFilters["confidenceBucket"] | null;
    const risk = get("riskBucket") as ScreenerFilters["riskBucket"] | null;
    const growth = get("growthBucket") as ScreenerFilters["growthBucket"] | null;
    return {
      country: get("country") || DEFAULT_FILTERS.country,
      sector: get("sector") || DEFAULT_FILTERS.sector,
      marketCap: get("marketCap") || DEFAULT_FILTERS.marketCap,
      verdict: get("verdict") || DEFAULT_FILTERS.verdict,
      gapBucket: (gap && ["all","<=-50","-50_0","0_25","25_100",">=100"].includes(gap) ? gap : DEFAULT_FILTERS.gapBucket) as ScreenerFilters["gapBucket"],
      confidenceBucket: (conf && ["all","<50",">=50"].includes(conf) ? conf : DEFAULT_FILTERS.confidenceBucket) as ScreenerFilters["confidenceBucket"],
      riskBucket: (risk && ["all","<0.5","0.5_1","1_2",">=2"].includes(risk) ? risk : DEFAULT_FILTERS.riskBucket) as ScreenerFilters["riskBucket"],
      growthBucket: (growth && ["all","<=0","0_25","25_50","50_100",">=100"].includes(growth) ? growth : DEFAULT_FILTERS.growthBucket) as ScreenerFilters["growthBucket"],
    };
  });
  const [sort, setSort] = useState<SortState>(() => {
    const s = searchParams.get("sort");
    if (!s) return [];
    try { const parsed = JSON.parse(s); if (Array.isArray(parsed)) return parsed as SortState; } catch {}
    return [];
  });
  const [page, setPage] = useState<number>(() => {
    const p = searchParams.get("page");
    const n = p ? parseInt(p, 10) : 1;
    return Number.isFinite(n) && n > 0 ? n : 1;
  });
  const [customPresets, setCustomPresets] = useState<ScreenerPreset[]>([]);
  const [presetSelect, setPresetSelect] = useState("__none__");
  const isFirstRender = useRef(true);

  const [selectedRadar, setSelectedRadar] = useState<Set<string>>(new Set());
  const [watchlistPicker, setWatchlistPicker] = useState<{ ticker: string; country: string } | null>(null);
  const [watchlists, setWatchlists] = useState<WatchlistRow[]>([]);
  const [watchlistMsg, setWatchlistMsg] = useState<string | null>(null);
  const [newWatchlistName, setNewWatchlistName] = useState("");

  const load = () => {
    setErr(null);
    getScreenerRows()
      .then(setRows)
      .catch(() => {
        stubScreenerRows()
          .then(setRows)
          .catch(() => setErr("Could not load screener (run via `tauri dev`)."));
      });
  };

  useEffect(() => {
    load();
    getSetting(PRESETS_STORAGE_KEY)
      .then((v) => setCustomPresets(parsePresets(v)))
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false;
      return;
    }
    const params = new URLSearchParams();
    if (filters.country !== "All") params.set("country", filters.country);
    if (filters.sector !== "All") params.set("sector", filters.sector);
    if (filters.marketCap !== "All") params.set("marketCap", filters.marketCap);
    if (filters.verdict !== "All") params.set("verdict", filters.verdict);
    if (filters.gapBucket !== "all") params.set("gapBucket", filters.gapBucket);
    if (filters.confidenceBucket !== "all") params.set("confidenceBucket", filters.confidenceBucket);
    if (filters.riskBucket !== "all") params.set("riskBucket", filters.riskBucket);
    if (filters.growthBucket !== "all") params.set("growthBucket", filters.growthBucket);
    if (sort.length > 0) params.set("sort", JSON.stringify(sort));
    if (page > 1) params.set("page", String(page));
    setSearchParams(params, { replace: true });
  }, [filters, sort, page, setSearchParams]);

  const countries = useMemo(() => {
    if (!rows) return ["All"];
    return ["All", ...Array.from(new Set(rows.map((r) => r.country)))];
  }, [rows]);

  const sectors = useMemo(() => {
    if (!rows) return ["All"];
    return ["All", ...Array.from(new Set(rows.map((r) => r.sector).filter(Boolean)))];
  }, [rows]);

  const caps = useMemo(() => {
    if (!rows) return ["All"];
    return ["All", ...Array.from(new Set(rows.map((r) => r.market_cap_category).filter(Boolean)))];
  }, [rows]);

  const normVerdict = (v: string) => v.toLowerCase().replace(/_/g, " ").trim();
  const filtered = useMemo(() => {
    if (!rows) return null;
    return rows.filter((r) => {
      if (filters.country !== "All" && r.country !== filters.country) return false;
      if (filters.sector !== "All" && r.sector !== filters.sector) return false;
      if (filters.marketCap !== "All" && r.market_cap_category !== filters.marketCap) return false;
      if (filters.verdict !== "All" && normVerdict(r.verdict) !== normVerdict(filters.verdict)) return false;
      const gapPct = r.gap_pct * 100;
      if (!passesGap(gapPct, filters.gapBucket)) return false;
      if (!passesConfidence(r.confidence, filters.confidenceBucket)) return false;
      if (!passesRisk(r.risk_reward, filters.riskBucket)) return false;
      const growthPct = (r.revenue_growth ?? 0) * 100;
      if (!passesGrowth(growthPct, filters.growthBucket)) return false;
      return true;
    });
  }, [rows, filters]);

  const sorted = useMemo(() => {
    if (!filtered) return null;
    if (sort.length === 0) return filtered;
    const arr = [...filtered];
    arr.sort((a, b) => {
      for (const s of sort) {
        let av: string | number = (a as unknown as Record<string, string | number>)[s.key] as string | number;
        let bv: string | number = (b as unknown as Record<string, string | number>)[s.key] as string | number;
        if (typeof av === "string") av = av.toLowerCase();
        if (typeof bv === "string") bv = bv.toLowerCase();
        if (av === bv) continue;
        if (av == null) return 1;
        if (bv == null) return -1;
        const cmp = av < bv ? -1 : 1;
        return s.dir === "asc" ? cmp : -cmp;
      }
      return 0;
    });
    return arr;
  }, [filtered, sort]);

  const paginated = useMemo(() => {
    if (!sorted) return null;
    return sorted.slice(0, page * PAGE_SIZE);
  }, [sorted, page]);

  useEffect(() => setPage(1), [filters, sort]);

  const gapHist = useMemo(() => {
    if (!filtered) return [];
    const defs: Array<{ label: string; bucket: ScreenerFilters["gapBucket"]; lo: number | null; hi: number | null }> = [
      { label: "≤-50%", bucket: "<=-50", lo: null, hi: -50 },
      { label: "-50~0%", bucket: "-50_0", lo: -50, hi: 0 },
      { label: "0~25%", bucket: "0_25", lo: 0, hi: 25 },
      { label: "25~100%", bucket: "25_100", lo: 25, hi: 100 },
      { label: "≥100%", bucket: ">=100", lo: 100, hi: null },
    ];
    return defs.map((d) => ({
      label: d.label,
      bucket: d.bucket,
      count: filtered.filter((r) => passesGap(r.gap_pct * 100, d.bucket)).length,
    }));
  }, [filtered]);

  const sectorData = useMemo(() => {
    if (!filtered) return [];
    const m = new Map<string, number>();
    for (const r of filtered) m.set(r.sector || "Unknown", (m.get(r.sector || "Unknown") || 0) + 1);
    return Array.from(m.entries()).map(([sector, count]) => ({ sector, count }));
  }, [filtered]);

  const scatterData = useMemo(() => {
    if (!filtered) return [];
    return filtered.map((r) => ({ x: r.gap_pct * 100, y: r.confidence, ticker: r.ticker, verdict: r.verdict }));
  }, [filtered]);

  const xDomain = useMemo<[number, number]>(() => {
    if (!scatterData.length) return [-50, 100];
    const xs = scatterData.map((d) => d.x);
    let min = Math.min(...xs);
    let max = Math.max(...xs);
    const pad = Math.max(5, (max - min) * 0.12) || 10;
    min = Math.floor((min - pad) / 10) * 10;
    max = Math.ceil((max + pad) / 10) * 10;
    if (max - min < 20) {
      min -= 10;
      max += 10;
    }
    return [min, max];
  }, [scatterData]);

  const radarData = useMemo(() => {
    const dims = ["cash_flow_predictability", "competitive_position", "financial_health", "industry_visibility", "management_quality"];
    const labels: Record<string, string> = {
      cash_flow_predictability: "Cash flow",
      competitive_position: "Competitive",
      financial_health: "Health",
      industry_visibility: "Visibility",
      management_quality: "Mgmt",
    };
    const selRows = (filtered || []).filter((r) => selectedRadar.has(r.ticker));
    if (selRows.length === 0) return [];
    return dims.map((d) => {
      const row: Record<string, unknown> = { dimension: labels[d] };
      for (const r of selRows) {
        const scores = extractRadarScores(r);
        row[r.ticker] = scores[d] ?? (Math.round((r.confidence / 100) * 100 * (d === "cash_flow_predictability" ? 0.3 : d === "competitive_position" ? 0.3 : d === "financial_health" ? 0.2 : d === "industry_visibility" ? 0.15 : 0.05) * 5) || 0);
      }
      return row;
    });
  }, [filtered, selectedRadar]);

  const toggleRadar = (ticker: string) => {
    setSelectedRadar((prev) => {
      const next = new Set(prev);
      if (next.has(ticker)) next.delete(ticker);
      else {
        if (next.size >= 5) return next;
        next.add(ticker);
      }
      return next;
    });
  };

  const openWatchlistPicker = async (ticker: string, country: string) => {
    setWatchlistPicker({ ticker, country });
    setWatchlistMsg(null);
    setNewWatchlistName("");
    try {
      const ls = await watchlistsList();
      setWatchlists(ls);
    } catch {
      setWatchlists([]);
    }
  };

  const handleAddToWatchlist = async (watchlistId: string) => {
    if (!watchlistPicker) return;
    try {
      await watchlistAddItem(watchlistId, watchlistPicker.ticker, watchlistPicker.country);
      setWatchlistMsg("Added");
      setTimeout(() => setWatchlistPicker(null), 600);
    } catch (e) {
      setWatchlistMsg(typeof e === "string" ? e : "Failed");
    }
  };

  const handleCreateAndAdd = async () => {
    if (!watchlistPicker) return;
    const n = newWatchlistName.trim();
    if (!n) return;
    try {
      const res = await watchlistCreate(n, null);
      await watchlistAddItem(res.id, watchlistPicker.ticker, watchlistPicker.country);
      setWatchlistMsg("Created & added");
      setTimeout(() => setWatchlistPicker(null), 600);
    } catch (e) {
      setWatchlistMsg(typeof e === "string" ? e : "Failed");
    }
  };

  const onHeaderClick = (key: SortKey, e: React.MouseEvent) => {
    const shift = e.shiftKey;
    setSort((prev) => {
      const idx = prev.findIndex((s) => s.key === key);
      let next: SortState;
      if (idx === -1) {
        const entry = { key, dir: "asc" as SortDir };
        next = shift ? [...prev, entry] : [entry];
      } else {
        const cur = prev[idx];
        if (cur.dir === "asc") next = prev.map((s, i) => (i === idx ? { ...s, dir: "desc" } : s));
        else next = prev.filter((_, i) => i !== idx);
        if (!shift && next.length > 1 && idx === -1) next = [next[next.length - 1]];
      }
      return next;
    });
  };

  const sortIndicator = (key: SortKey) => {
    const idx = sort.findIndex((s) => s.key === key);
    if (idx === -1) return null;
    return <span className="ml-1 text-[10px] leading-none text-app-text">{sort[idx].dir === "asc" ? "▲" : "▼"}{sort.length > 1 ? ` ${idx + 1}` : ""}</span>;
  };

  const applyPreset = (name: string) => {
    if (name === "__none__") return;
    const found = [...BUILTIN_PRESETS, ...customPresets].find((p) => p.name === name);
    if (found) setFilters({ ...found.filters });
  };

  const saveCustom = async () => {
    const name = window.prompt("Preset name:");
    if (!name || !name.trim()) return;
    const next: ScreenerPreset[] = [...customPresets, { name: name.trim(), filters: { ...filters } }];
    await setSetting(PRESETS_STORAGE_KEY, JSON.stringify(next)).catch(() => {});
    setCustomPresets(next);
    setPresetSelect(name.trim());
  };

  const deleteCustom = async () => {
    if (presetSelect === "__none__" || BUILTIN_PRESETS.some((p) => p.name === presetSelect)) {
      window.alert("Select a custom preset to delete.");
      return;
    }
    const next = customPresets.filter((p) => p.name !== presetSelect);
    await setSetting(PRESETS_STORAGE_KEY, JSON.stringify(next)).catch(() => {});
    setCustomPresets(next);
    setPresetSelect("__none__");
  };

  if (err) return <ErrorNote message={err} />;
  if (!rows || !filtered || !sorted || !paginated) return <Loading />;

  const empty = rows.length === 0;

  return (
    <div className="space-y-4">
      {empty ? (
        <Card>
          <div className="py-10 text-center">
            <p className="text-sm text-app-muted">No completed primary reports yet.</p>
            <p className="mt-1 text-xs text-app-muted">Run an analysis from New Analysis — once completed it appears here.</p>
          </div>
        </Card>
      ) : (
        <>
          {/* Filters */}
          <Card>
            <div className="space-y-3">
              <div className="flex flex-wrap items-end gap-3">
                <label className="w-44">
                  <span className="mb-1 block text-xs font-medium text-app-muted">Country</span>
                  <select className={`${inputCls}`} value={filters.country} onChange={(e) => setFilters((f) => ({ ...f, country: e.target.value }))}>
                    {countries.map((c) => (
                      <option key={c} value={c}>{c === "All" ? "All countries" : c}</option>
                    ))}
                  </select>
                </label>
                <label className="w-44">
                  <span className="mb-1 block text-xs font-medium text-app-muted">Sector</span>
                  <select className={`${inputCls}`} value={filters.sector} onChange={(e) => setFilters((f) => ({ ...f, sector: e.target.value }))}>
                    {sectors.map((s) => (
                      <option key={s} value={s}>{s === "All" ? "All sectors" : s}</option>
                    ))}
                  </select>
                </label>
                <label className="w-44">
                  <span className="mb-1 block text-xs font-medium text-app-muted">Market cap</span>
                  <select className={`${inputCls}`} value={filters.marketCap} onChange={(e) => setFilters((f) => ({ ...f, marketCap: e.target.value }))}>
                    {caps.map((c) => (
                      <option key={c} value={c}>{c === "All" ? "All caps" : c}</option>
                    ))}
                  </select>
                </label>
                <label className="w-44">
                  <span className="mb-1 block text-xs font-medium text-app-muted">Verdict</span>
                  <select className={`${inputCls}`} value={filters.verdict} onChange={(e) => setFilters((f) => ({ ...f, verdict: e.target.value }))}>
                    <option value="All">All verdicts</option>
                    <option>Undervalued</option>
                    <option>Fairly Valued</option>
                    <option>Overvalued</option>
                  </select>
                </label>
                <span className="ml-auto text-xs text-app-muted">{filtered.length} of {rows.length} tickers</span>
              </div>

              <div className="grid grid-cols-4 gap-3">
                <label>
                  <span className="mb-1 block text-xs font-medium text-app-muted">Gap %</span>
                  <select className={inputCls} value={filters.gapBucket} onChange={(e) => setFilters((f) => ({ ...f, gapBucket: e.target.value as ScreenerFilters["gapBucket"] }))}>
                    <option value="all">All</option>
                    <option value="<=-50">Less than or equal to -50%</option>
                    <option value="-50_0">Between -50% and 0%</option>
                    <option value="0_25">Between 0% and 25%</option>
                    <option value="25_100">Between 25% and 100%</option>
                    <option value=">=100">Greater than or equal to 100%</option>
                  </select>
                </label>
                <label>
                  <span className="mb-1 block text-xs font-medium text-app-muted">Confidence</span>
                  <select className={inputCls} value={filters.confidenceBucket} onChange={(e) => setFilters((f) => ({ ...f, confidenceBucket: e.target.value as ScreenerFilters["confidenceBucket"] }))}>
                    <option value="all">All</option>
                    <option value="<50">Less than 50</option>
                    <option value=">=50">Greater than or equal to 50</option>
                  </select>
                </label>
                <label>
                  <span className="mb-1 block text-xs font-medium text-app-muted">Risk / reward</span>
                  <select className={inputCls} value={filters.riskBucket} onChange={(e) => setFilters((f) => ({ ...f, riskBucket: e.target.value as ScreenerFilters["riskBucket"] }))}>
                    <option value="all">All</option>
                    <option value="<0.5">Less than 0.5</option>
                    <option value="0.5_1">Between 0.5 and 1</option>
                    <option value="1_2">Between 1 and 2</option>
                    <option value=">=2">Greater than or equal to 2</option>
                  </select>
                </label>
                <label>
                  <span className="mb-1 block text-xs font-medium text-app-muted">Revenue growth</span>
                  <select className={inputCls} value={filters.growthBucket} onChange={(e) => setFilters((f) => ({ ...f, growthBucket: e.target.value as ScreenerFilters["growthBucket"] }))}>
                    <option value="all">All</option>
                    <option value="<=0">Less than or equal to 0%</option>
                    <option value="0_25">Between 0% and 25%</option>
                    <option value="25_50">Between 25% and 50%</option>
                    <option value="50_100">Between 50% and 100%</option>
                    <option value=">=100">Greater than or equal to 100%</option>
                  </select>
                </label>
              </div>

              <div className="flex flex-wrap items-center gap-2 border-t border-app-border pt-3">
                <select className={`${inputCls} w-56`} value={presetSelect} onChange={(e) => { setPresetSelect(e.target.value); applyPreset(e.target.value); }}>
                  <option value="__none__">Presets…</option>
                  {BUILTIN_PRESETS.map((p) => (
                    <option key={p.name} value={p.name}>{p.name}</option>
                  ))}
                  {customPresets.length > 0 && <option disabled>— Custom —</option>}
                  {customPresets.map((p) => (
                    <option key={`c-${p.name}`} value={p.name}>{p.name} (custom)</option>
                  ))}
                </select>
                <Button variant="ghost" onClick={saveCustom}>Save as custom</Button>
                <Button variant="ghost" onClick={deleteCustom}>Delete custom</Button>
                <Button
                  variant="ghost"
                  onClick={() => {
                    setFilters({ ...DEFAULT_FILTERS });
                    setPresetSelect("__none__");
                    setSort([]);
                  }}
                >
                  Reset filters
                </Button>
                <span className="ml-auto text-xs text-app-muted">Click header to sort (Shift for multi-col). Charts drill into filters/report.</span>
              </div>
            </div>
          </Card>

          {/* Charts — 3 cols + radar */}
          <div className="grid grid-cols-3 gap-4">
            <Card title="Valuation gap histogram" className="overflow-hidden">
              <div style={{ width: "100%", height: 180 }}>
                <ResponsiveContainer>
                  <BarChart data={gapHist}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" vertical={false} />
                    <XAxis dataKey="label" stroke="#64748b" fontSize={11} tick={{ fill: "#64748b" }} />
                    <YAxis allowDecimals={false} stroke="#64748b" fontSize={11} />
                    <Tooltip contentStyle={{ borderRadius: 8, border: "1px solid #e2e8f0" }} />
                    {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
                    <Bar dataKey="count" radius={[8, 8, 0, 0]} barSize={22} onClick={(d: any) => d?.payload?.bucket && setFilters((f) => ({ ...f, gapBucket: d.payload.bucket }))}>
                      {gapHist.map((_e, i) => (
                        <Cell key={i} fill={BAR_COLORS[i % BAR_COLORS.length]} className="cursor-pointer" />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
              <p className="mt-2 text-center text-xs text-app-muted">Click a bar to filter gap.</p>
            </Card>

            <Card title="Sector breakdown">
              <div style={{ width: "100%", height: 180 }}>
                <ResponsiveContainer>
                  <BarChart data={sectorData} layout="vertical" margin={{ left: 20, right: 16 }}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" />
                    <XAxis type="number" allowDecimals={false} stroke="#64748b" fontSize={11} />
                    <YAxis type="category" dataKey="sector" width={120} stroke="#64748b" fontSize={11} tick={{ fill: "#64748b" }} />
                    <Tooltip contentStyle={{ borderRadius: 8, border: "1px solid #e2e8f0" }} />
                    {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
                    <Bar dataKey="count" radius={[0, 8, 8, 0]} barSize={16} onClick={(d: any) => d?.payload?.sector && setFilters((f) => ({ ...f, sector: d.payload.sector }))}>
                      {sectorData.map((_e, i) => (
                        <Cell key={i} fill={BAR_COLORS[i % BAR_COLORS.length]} className="cursor-pointer" />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
              <p className="mt-2 text-center text-xs text-app-muted">Click a bar to filter sector.</p>
            </Card>

            <Card title="Gap vs Confidence">
              <div style={{ width: "100%", height: 180 }}>
                <ResponsiveContainer>
                  <ScatterChart margin={{ top: 8, right: 16, left: 0, bottom: 8 }}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" />
                    <XAxis type="number" dataKey="x" name="Gap %" stroke="#64748b" fontSize={11} domain={xDomain} tickFormatter={(v: number) => `${Math.round(v)}%`} />
                    <YAxis type="number" dataKey="y" name="Confidence" stroke="#64748b" fontSize={11} domain={[0, 100]} tickFormatter={(v: number) => `${Math.round(v)}`} />
                    <Tooltip
                      content={({ active, payload }) => {
                        if (active && payload && payload[0] && (payload[0].payload as { ticker: string; verdict: string })) {
                          const p = payload[0].payload as { ticker: string; verdict: string };
                          return (
                            <div className="rounded-md border border-app-border bg-white px-2 py-1 text-xs shadow">
                              <span className="font-medium text-app-text">{p.ticker}</span>
                              <span className="ml-1 text-app-muted">· {p.verdict}</span>
                            </div>
                          );
                        }
                        return null;
                      }}
                    />
                    {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
                    <Scatter
                      data={scatterData}
                      fill="#0f172a"
                      onClick={(d: any) => d?.payload?.ticker && navigate(`/report/${d.payload.ticker}?from=${encodeURIComponent(location.pathname + location.search)}`)}
                      style={{ cursor: "pointer" }}
                    />
                  </ScatterChart>
                </ResponsiveContainer>
              </div>
              <p className="mt-2 text-center text-xs text-app-muted">Click a dot to open its report.</p>
            </Card>
          </div>

          {/* Radar — confidence 5 dims */}
          <Card title={`Confidence radar ${selectedRadar.size > 0 ? `(${selectedRadar.size}/5)` : "(select up to 5 tickers below)"}`} action={selectedRadar.size > 0 ? <Button variant="ghost" onClick={() => setSelectedRadar(new Set())}>Clear</Button> : undefined}>
            {selectedRadar.size === 0 ? (
              <p className="py-6 text-center text-sm text-app-muted">Tick checkboxes in the table to compare 5 confidence dimensions.</p>
            ) : (
              <div style={{ width: "100%", height: 280 }}>
                <ResponsiveContainer>
                  <RadarChart data={radarData}>
                    <PolarGrid stroke="#e2e8f0" />
                    <PolarAngleAxis dataKey="dimension" tick={{ fontSize: 11, fill: "#64748b" }} />
                    <PolarRadiusAxis domain={[0, 100]} tick={{ fontSize: 10, fill: "#94a3b8" }} axisLine={false} />
                    {Array.from(selectedRadar).map((ticker, i) => (
                      <Radar key={ticker} name={ticker} dataKey={ticker} stroke={RADAR_COLORS[i % RADAR_COLORS.length]} fill={RADAR_COLORS[i % RADAR_COLORS.length]} fillOpacity={0.15} />
                    ))}
                    <Legend wrapperStyle={{ fontSize: 11, paddingTop: 12 }} />
                    <Tooltip contentStyle={{ borderRadius: 8, border: "1px solid #e2e8f0" }} />
                  </RadarChart>
                </ResponsiveContainer>
              </div>
            )}
          </Card>

          {/* Table */}
          <Card>
            <div className="overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b border-app-border text-xs uppercase tracking-wide text-app-muted">
                    <th className="px-2 py-2 font-medium">✓</th>
                    {[
                      { k: "ticker" as SortKey, label: "Ticker" },
                      { k: "company" as SortKey, label: "Company" },
                      { k: "sector" as SortKey, label: "Sector" },
                      { k: "market_cap_category" as SortKey, label: "Market cap" },
                      { k: "verdict" as SortKey, label: "Verdict" },
                      { k: "weighted_fair_value" as SortKey, label: "Fair value" },
                      { k: "gap_pct" as SortKey, label: "Gap" },
                      { k: "revenue_growth" as SortKey, label: "Growth (rev)" },
                      { k: "risk_reward" as SortKey, label: "Risk / reward" },
                    ].map((h) => (
                      <th key={h.k} className="cursor-pointer select-none px-3 py-2 font-medium hover:text-app-text" onClick={(e) => onHeaderClick(h.k, e)}>
                        <span className="inline-flex items-center">{h.label}{sortIndicator(h.k)}</span>
                      </th>
                    ))}
                    <th className="px-3 py-2 font-medium">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-app-border">
                  {paginated.map((r) => (
                    <tr key={`${r.ticker}-${r.country}`} className="hover:bg-app-sidebar">
                      <td className="px-2 py-2">
                        <input type="checkbox" checked={selectedRadar.has(r.ticker)} onChange={() => toggleRadar(r.ticker)} disabled={!selectedRadar.has(r.ticker) && selectedRadar.size >= 5} className="h-4 w-4 rounded border-app-border" title={selectedRadar.has(r.ticker) ? "Remove from radar" : selectedRadar.size >= 5 ? "Max 5" : "Add to radar"} />
                      </td>
                      <td className="px-3 py-2.5 font-medium text-app-text cursor-pointer" onClick={() => navigate(`/report/${r.ticker}?from=${encodeURIComponent(location.pathname + location.search)}`)}>{r.ticker}</td>
                      <td className="px-3 py-2.5 text-app-muted cursor-pointer" onClick={() => navigate(`/report/${r.ticker}?from=${encodeURIComponent(location.pathname + location.search)}`)}>{r.company}</td>
                      <td className="px-3 py-2.5 text-app-muted">{r.sector}</td>
                      <td className="px-3 py-2.5 text-xs text-app-muted">{r.market_cap_category}</td>
                      <td className="px-3 py-2.5">
                        <VerdictPill verdict={r.verdict} />
                      </td>
                      <td className="px-3 py-2.5 tabular-nums text-app-text">{fmtNum(r.weighted_fair_value)}</td>
                      <td className={`px-3 py-2.5 tabular-nums ${gapCls(r.gap_pct * 100)}`}>{fmtPct(r.gap_pct * 100)}</td>
                      <td className={`px-3 py-2.5 tabular-nums ${r.revenue_growth >= 0.1 ? "text-app-positive" : "text-app-muted"}`}>
                        {fmtPct(r.revenue_growth * 100)}
                      </td>
                      <td className="px-3 py-2.5 tabular-nums text-app-muted">{r.risk_reward.toFixed(1)}</td>
                      <td className="px-3 py-2.5">
                        <div className="flex gap-1">
                          <Button variant="ghost" onClick={() => navigate(`/report/${r.ticker}?from=${encodeURIComponent(location.pathname + location.search)}`)}>View</Button>
                          <Button variant="ghost" onClick={() => openWatchlistPicker(r.ticker, r.country)}><span className="inline-flex items-center gap-1"><Plus className="h-3.5 w-3.5" /> Watchlist</span></Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                  {paginated.length === 0 && (
                    <tr>
                      <td colSpan={11} className="px-3 py-6 text-center text-sm text-app-muted">
                        No tickers match the selected filters.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            {paginated.length < (sorted?.length ?? 0) && (
              <div className="flex justify-center border-t border-app-border py-3">
                <Button variant="ghost" onClick={() => setPage((p) => p + 1)}>Show 50 more ({(sorted?.length ?? 0) - paginated.length} remaining)</Button>
              </div>
            )}
            <div className="flex items-center justify-between border-t border-app-border px-3 py-2 text-xs text-app-muted">
              <span>
                Showing {paginated.length} of {sorted.length} (filtered from {rows.length})
              </span>
              <button type="button" onClick={load} className="font-medium text-app-muted hover:text-app-text">
                Refresh
              </button>
            </div>
          </Card>
        </>
      )}

      {/* Watchlist picker for screener */}
      {watchlistPicker && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
          <div className="w-full max-w-md rounded-lg border border-app-border bg-app-surface p-4 shadow-xl">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold text-app-text">Add {watchlistPicker.ticker} to watchlist</h3>
              <button type="button" onClick={() => setWatchlistPicker(null)} className="rounded p-1 hover:bg-app-sidebar">✕</button>
            </div>
            {watchlistMsg && <div className="mt-2 rounded bg-app-positive/10 px-2 py-1 text-xs text-app-positive">{watchlistMsg}</div>}
            {watchlists.length === 0 ? (
              <p className="mt-3 text-sm text-app-muted">No watchlists — create one below.</p>
            ) : (
              <div className="mt-3 max-h-48 overflow-auto space-y-1">
                {watchlists.map((wl) => (
                  <button key={wl.id} type="button" onClick={() => handleAddToWatchlist(wl.id)} className="flex w-full items-center justify-between rounded border border-app-border px-3 py-2 text-left text-sm hover:bg-app-sidebar">
                    <span>
                      <span className="font-medium">{wl.name}</span>
                      {wl.description && <span className="ml-2 text-xs text-app-muted">{wl.description}</span>}
                    </span>
                    <span className="ml-2 inline-flex h-6 w-6 items-center justify-center rounded-full bg-app-text text-app-surface"><Plus className="h-3.5 w-3.5" /></span>
                  </button>
                ))}
              </div>
            )}
            <div className="mt-4 border-t border-app-border pt-3">
              <div className="text-xs font-medium text-app-muted">Create new watchlist</div>
              <div className="mt-2 flex gap-2">
                <input className={`${inputCls} flex-1`} value={newWatchlistName} onChange={(e) => setNewWatchlistName(e.target.value)} placeholder="Name e.g. My picks" />
                <Button onClick={handleCreateAndAdd} disabled={!newWatchlistName.trim()}><span className="inline-flex items-center gap-1"><Plus className="h-3.5 w-3.5" /> Create & add</span></Button>
              </div>
            </div>
            <div className="mt-3 flex justify-end">
              <Button variant="ghost" onClick={() => setWatchlistPicker(null)}>Close</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
