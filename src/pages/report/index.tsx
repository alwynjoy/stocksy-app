import { useEffect, useMemo, useState } from "react";
import { useParams, useSearchParams, useNavigate, useLocation } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  ResponsiveContainer,
  LineChart,
  Line,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  LabelList,
} from "recharts";
import { Star, RefreshCw, Download, Loader2, ChevronLeft, ChevronRight, Scale, ArrowLeft, Trash2, Eye, Plus } from "lucide-react";
import bearImg from "../../../images/bear.jpg";
import bullImg from "../../../images/bull.jpg";
import logoAsset from "../../../images/logo.jpg";
import {
  getReport,
  getReportVersions,
  toggleFavorite,
  deleteReports,
  updatePrice,
  recomputeLocal,
  getCachedFinancialData,
  getSetting,
  watchlistsList,
  watchlistCreate,
  watchlistAddItem,
  type ReportDetail,
  type ReportVersion,
  type WatchlistRow,
} from "../../lib/api";
import { Card, ErrorNote, Loading, VerdictPill, Button, inputCls } from "../../components/ui";
import { fmtMoney, fmtPct, gapCls, fmtNum } from "../../lib/format";

function ConfidenceBar({ label, value, max, reasoning }: { label: string; value: number; max: number; reasoning?: string }) {
  const pct = Math.max(0, Math.min(100, (value / max) * 100));
  const color = value >= max * 0.7 ? "bg-app-positive" : value >= max * 0.4 ? "bg-app-warning" : "bg-app-negative";
  return (
    <div title={reasoning || ""}>
      <div className="mb-1 flex items-center justify-between text-xs">
        <span className="text-app-muted">{label} <span className="tabular-nums">({value}/{max})</span></span>
        <span className="tabular-nums text-app-text">{Math.round(pct)}%</span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-app-sidebar">
        <div className={`h-full rounded-full ${color}`} style={{ width: `${pct}%` }} />
      </div>
      {reasoning && <div className="mt-1 text-[11px] leading-tight text-app-muted">{reasoning}</div>}
    </div>
  );
}

function MarkdownPanel({ title, content, emptyText }: { title: string; content: unknown; emptyText?: string }) {
  const text = typeof content === "string" ? content : content ? String(content) : "";
  if (!text || text.trim().length === 0) {
    return (
      <Card title={title}>
        <p className="text-sm text-app-muted">{emptyText || "No data."}</p>
      </Card>
    );
  }
  return (
    <Card title={title}>
      <div className="prose prose-sm max-w-none text-sm leading-relaxed text-app-text prose-p:my-1 prose-headings:text-app-text prose-strong:text-app-text">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
      </div>
    </Card>
  );
}

function clampNum(v: unknown, fallback = 0): number {
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (typeof v === "string") { const n = Number(v); if (Number.isFinite(n)) return n; }
  return fallback;
}

function extractBusinessDescription(content: unknown): string {
  if (content == null) return "";
  // If it's a string that is JSON, try parse
  if (typeof content === "string") {
    const trimmed = content.trim();
    if (trimmed.startsWith("{")) {
      try {
        const parsed = JSON.parse(trimmed);
        if (parsed && typeof parsed === "object" && "business_description" in parsed) {
          return String((parsed as Record<string, unknown>).business_description || "");
        }
      } catch { /* fallback to raw string */ }
    }
    return trimmed;
  }
  if (typeof content === "object") {
    const obj = content as Record<string, unknown>;
    if ("business_description" in obj && obj.business_description != null) {
      return String(obj.business_description);
    }
    // Sometimes the whole narrative object is stringified inside
    // e.g. { narrative: { business_description: ... } }
    if ("narrative" in obj && typeof obj.narrative === "object" && obj.narrative != null) {
      const inner = obj.narrative as Record<string, unknown>;
      if ("business_description" in inner) return String(inner.business_description || "");
    }
  }
  return String(content);
}

function normalizeHistoricalBullets(content: unknown): string[] {
  if (content == null) return [];
  const text = typeof content === "string" ? content : String(content);
  if (!text.trim()) return [];
  // Split by newline first
  const lines = text.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
  const bullets: string[] = [];
  for (const line of lines) {
    // Handle multiple bullets in one line separated by •
    if (line.includes("•")) {
      const parts = line.split("•").map((p) => p.trim()).filter(Boolean);
      for (const p of parts) bullets.push(p);
    } else if (line.startsWith("-") || line.startsWith("*")) {
      bullets.push(line.replace(/^[-*]\s*/, "").trim());
    } else {
      bullets.push(line.replace(/^•\s*/, "").trim());
    }
  }
  return bullets.filter(Boolean);
}

function extractDcfRates(dcfAssumptions: unknown): { rev: number[] | null; mar: number[] | null } {
  if (dcfAssumptions == null || typeof dcfAssumptions !== "object") return { rev: null, mar: null };
  const obj = dcfAssumptions as Record<string, unknown>;
  const raw = (obj.raw as Record<string, unknown> | undefined) ?? obj;
  const rev = (raw.revenue_growth_rates as number[] | undefined) ?? (obj.revenue_growth_rates as number[] | undefined) ?? null;
  const mar = (raw.operating_margins as number[] | undefined) ?? (obj.operating_margins as number[] | undefined) ?? null;
  return { rev: Array.isArray(rev) ? rev : null, mar: Array.isArray(mar) ? mar : null };
}

function fmtFY24(fye: string): string {
  if (!fye || typeof fye !== "string") return fye || "—";
  const parts = fye.split("/");
  if (parts.length === 2) {
    const y = parts[1];
    if (/^\d{4}$/.test(y)) return `FY${y.slice(-2)}`;
    if (/^\d{2}$/.test(y)) return `FY${y}`;
  }
  const m = fye.match(/(\d{4})/);
  if (m) return `FY${m[1].slice(-2)}`;
  return fye;
}

function parseFYYear(fye: string): number | null {
  if (!fye) return null;
  const parts = fye.split("/");
  if (parts.length === 2 && /^\d{4}$/.test(parts[1])) return parseInt(parts[1], 10);
  const m = fye.match(/(\d{4})/);
  if (m) return parseInt(m[1], 10);
  return null;
}

export default function Report() {
  const { ticker = "" } = useParams<{ ticker: string }>();
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const location = useLocation();
  const versionId = searchParams.get("version");
  const fromParam = searchParams.get("from");
  const fromState = (location.state as { from?: string })?.from;
  const backToRaw = fromParam || fromState || null;
  const backTo = backToRaw ? (() => { try { return decodeURIComponent(backToRaw); } catch { return backToRaw; } })() : null;
  const handleBack = () => {
    if (backTo && backTo.startsWith("/")) {
      navigate(backTo);
    } else if (backTo === "dashboard" || backTo === "/") {
      navigate("/");
    } else {
      navigate("/reports");
    }
  };
  const [detail, setDetail] = useState<ReportDetail | null>(null);
  const [versions, setVersions] = useState<ReportVersion[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [livePrice, setLivePrice] = useState<number | null>(null);
  const [liveUpdating, setLiveUpdating] = useState(false);
  const [liveMsg, setLiveMsg] = useState<string | null>(null);
  const [recomputed, setRecomputed] = useState<unknown | null>(null);
  const [whatIfDirty, setWhatIfDirty] = useState(false);
  const [overrides, setOverrides] = useState<{ revenue_growth_rates?: [number,number,number,number,number]; operating_margins?: [number,number,number,number,number]; terminal_margin?: number; terminal_growth_rate?: number; fcf_conversion_factor?: number; wacc?: number }>({});
  const [editing, setEditing] = useState<Record<string, string>>({});
  const [cachedHist, setCachedHist] = useState<unknown | null>(null);
  const [recomputeLoading, setRecomputeLoading] = useState(false);
  const [recomputeError, setRecomputeError] = useState<string | null>(null);
  const [orIdx, setOrIdx] = useState(0);
  const [orTouchStart, setOrTouchStart] = useState<number | null>(null);
  const [showComparePicker, setShowComparePicker] = useState(false);
  const [selectedCompareIds, setSelectedCompareIds] = useState<string[]>([]);
  const [showDeletePicker, setShowDeletePicker] = useState(false);
  const [selectedDeleteIds, setSelectedDeleteIds] = useState<string[]>([]);
  const [showDeleteFinalConfirm, setShowDeleteFinalConfirm] = useState(false);
  const [pendingDeleteIds, setPendingDeleteIds] = useState<string[]>([]);
  const [showWatchlistPicker, setShowWatchlistPicker] = useState(false);
  const [watchlists, setWatchlists] = useState<WatchlistRow[]>([]);
  const [watchlistMsg, setWatchlistMsg] = useState<string | null>(null);
  const [newWatchlistName, setNewWatchlistName] = useState("");
  const [pdfLoading, setPdfLoading] = useState(false);

  useEffect(() => {
    setLoading(true);
    setErr(null);
    setDetail(null);
    setRecomputed(null);
    setWhatIfDirty(false);
    setOverrides({});
    setEditing({});
    setRecomputeError(null);
    setLivePrice(null);
    setLiveMsg(null);
    getReport(ticker, versionId || null)
      .then((d) => {
        // Handle legacy stub shape: map to new fields if needed
        setDetail(d as ReportDetail);
        return getReportVersions(ticker).then(setVersions).catch(() => { /* no versions */ });
      })
      .catch(() => setErr("Could not load report (run via `tauri dev`)."))
      .finally(() => setLoading(false));
  }, [ticker, versionId]);

  // Load cached financials for historical charts
  useEffect(() => {
    if (!detail) return;
    const cc = detail.country_code || detail.country || "";
    if (!cc) return;
    getCachedFinancialData(detail.ticker, cc)
      .then((v) => setCachedHist(v))
      .catch(() => setCachedHist(null));
  }, [detail]);

  const currency = detail?.currency_code || detail?.currency || "USD";
  const currencySymbol = (() => {
    try {
      const parts = new Intl.NumberFormat("en-US", { style: "currency", currency: currency || "USD" }).formatToParts(0);
      return parts.find((p) => p.type === "currency")?.value || currency || "$";
    } catch {
      const map: Record<string, string> = { INR: "₹", USD: "$", GBP: "£", AUD: "A$" };
      return map[currency] || currency || "$";
    }
  })();
  const chartCurrencySuffix = `${currencySymbol}M`;
  const fmtMoneyWithCurrency = (v: number | null | undefined) => {
    if (v == null || !Number.isFinite(v)) return "—";
    try {
      return new Intl.NumberFormat("en-US", { style: "currency", currency: currency || "USD", maximumFractionDigits: 2 }).format(v);
    } catch { return fmtMoney(v); }
  };

  const currentPriceDisplay = livePrice ?? detail?.current_price ?? null;
  const baseFV = (recomputed as { base_case_fair_value?: number } | null)?.base_case_fair_value ?? detail?.base_case_fair_value ?? null;
  const weightedFV = (recomputed as { weighted_fair_value?: number } | null)?.weighted_fair_value ?? detail?.weighted_fair_value ?? null;
  const gapPct = useMemo(() => {
    // Signed Option A: (weighted - CMP)/CMP *100 ; positive = undervalued
    if (currentPriceDisplay != null && weightedFV != null && Math.abs(currentPriceDisplay) > 1e-9) {
      return ((weightedFV as number) - currentPriceDisplay) / currentPriceDisplay * 100;
    }
    const g = detail?.valuation_gap_pct ?? detail?.gap_pct ?? null;
    if (g == null) return null;
    // Fallback for error reports without FV/CMP: stored decimal 0.12 = 12%
    if (Math.abs(g) < 2) return g * 100;
    return g;
  }, [currentPriceDisplay, weightedFV, detail]);

  const verdictRaw = detail?.valuation_verdict || detail?.verdict || "";
  const verdictDisplay = useMemo(() => {
    const v = verdictRaw.toLowerCase();
    if (v === "undervalued") return "Undervalued";
    if (v === "overvalued") return "Overvalued";
    if (v === "fairly_valued" || v === "fairly valued") return "Fairly Valued";
    if (!v) return "—";
    return v.charAt(0).toUpperCase() + v.slice(1);
  }, [verdictRaw]);

  // Scenarios normalized
  const scenariosArr = useMemo(() => {
    const out = recomputed as { scenarios?: Array<{ scenario_name: string; fair_value: number; probability: number; narrative?: string; vs_current_price_pct?: number; revenue_cagr?: number; terminal_margin?: number; wacc?: number; terminal_growth?: number }> } | null;
    if (out?.scenarios && Array.isArray(out.scenarios) && out.scenarios.length === 3) {
      return out.scenarios.map((s) => ({
        name: s.scenario_name === "optimistic" ? "Optimistic" : s.scenario_name === "pessimistic" ? "Pessimistic" : s.scenario_name === "expected" ? "Base" : s.scenario_name,
        fair_value: s.fair_value,
        probability: s.probability,
        narrative: s.narrative,
        vs: s.vs_current_price_pct,
        cagr: s.revenue_cagr,
        margin: s.terminal_margin,
        wacc: s.wacc,
        growth: s.terminal_growth,
      }));
    }
    const src = detail?.scenarios as unknown;
    if (!src) return [];
    if (Array.isArray(src) && src.length === 3 && typeof (src[0] as { scenario_name?: string })?.scenario_name === "string") {
      const arr = src as Array<{ scenario_name: string; fair_value: number; probability: number; narrative?: string; vs_current_price_pct?: number; revenue_cagr?: number; terminal_margin?: number; wacc?: number; terminal_growth?: number }>;
      return arr.map((s) => ({
        name: s.scenario_name === "optimistic" ? "Optimistic" : s.scenario_name === "pessimistic" ? "Pessimistic" : s.scenario_name === "expected" ? "Base" : s.scenario_name,
        fair_value: s.fair_value,
        probability: s.probability,
        narrative: s.narrative,
        vs: s.vs_current_price_pct,
        cagr: s.revenue_cagr,
        margin: s.terminal_margin,
        wacc: s.wacc,
        growth: s.terminal_growth,
      }));
    }
    if (typeof src === "object" && src !== null && "bear" in (src as Record<string, unknown>)) {
      const o = src as { bear: { fair_value: number; weight: number }; base: { fair_value: number; weight: number }; bull: { fair_value: number; weight: number } };
      return [
        { name: "Pessimistic", fair_value: o.bear.fair_value, probability: o.bear.weight, narrative: undefined, vs: undefined, cagr: undefined, margin: undefined, wacc: undefined, growth: undefined },
        { name: "Base", fair_value: o.base.fair_value, probability: o.base.weight, narrative: undefined, vs: undefined, cagr: undefined, margin: undefined, wacc: undefined, growth: undefined },
        { name: "Optimistic", fair_value: o.bull.fair_value, probability: o.bull.weight, narrative: undefined, vs: undefined, cagr: undefined, margin: undefined, wacc: undefined, growth: undefined },
      ];
    }
    return [];
  }, [detail, recomputed]);

  const sensitivityCells = useMemo(() => {
    const src = (recomputed as { sensitivity_matrix?: unknown } | null)?.sensitivity_matrix ?? detail?.sensitivity_matrix;
    if (Array.isArray(src)) return src as Array<{ wacc_level: number; growth_level: number; wacc_value: number; growth_value: number; fair_value: number; pct_change_from_base: number | null; is_base_case: boolean; is_model_stable: boolean }>;
    return [];
  }, [detail, recomputed]);

  const confidenceDims = useMemo(() => {
    const cd = detail?.confidence_details as unknown as { overall_score?: number; overall_level?: string; dimensions?: Record<string, { score: number; max_score: number; reasoning: string }> } | null;
    if (cd && cd.dimensions && typeof cd.dimensions === "object") {
      const dims = cd.dimensions;
      const entries = Object.entries(dims);
      // Map keys to label
      const labels: Record<string, string> = {
        cash_flow_predictability: "Cash flow predictability",
        competitive_position: "Competitive position",
        financial_health: "Financial health",
        industry_visibility: "Industry visibility",
        management_quality: "Management quality",
      };
      const maxMap: Record<string, number> = { cash_flow_predictability: 30, competitive_position: 30, financial_health: 20, industry_visibility: 15, management_quality: 5 };
      return entries.map(([k, v]) => ({ key: k, label: labels[k] || k.replace(/_/g, " "), score: v.score, max: v.max_score || maxMap[k] || 10, reasoning: v.reasoning }));
    }
    // legacy: object of numbers
    if (detail?.confidence_details && typeof detail.confidence_details === "object") {
      const legacy = detail.confidence_details as Record<string, number>;
      const maxMap: Record<string, number> = { cash_flow_predictability: 30, competitive_position: 30, financial_health: 20, industry_visibility: 15, management_quality: 5 };
      return Object.entries(legacy).map(([k, v]) => ({ key: k, label: k.replace(/_/g, " "), score: v, max: maxMap[k] || 100, reasoning: undefined }));
    }
    return [];
  }, [detail]);

  const orData = useMemo(() => {
    const v = detail?.ai_opportunities_risks as unknown as { opportunities?: unknown; risks?: unknown } | null;
    if (!v || typeof v !== "object") return { opportunities: [] as Array<Record<string, unknown>>, risks: [] as Array<Record<string, unknown>> };
    const oppsRaw = (v as { opportunities?: unknown }).opportunities;
    const risksRaw = (v as { risks?: unknown }).risks;
    const opps = Array.isArray(oppsRaw) ? (oppsRaw as Array<Record<string, unknown>>) : [];
    const risks = Array.isArray(risksRaw) ? (risksRaw as Array<Record<string, unknown>>) : [];
    // Fallback for legacy string[] : wrap as object with description
    const normOpps = opps.map((o) => (typeof o === "string" ? { catalyst_name: o, description: o, potential_impact: "", timeline: null, source_url: null } as Record<string, unknown> : o));
    const normRisks = risks.map((r) => (typeof r === "string" ? { risk_category: r, description: r, potential_impact: "", likelihood: "moderate", source_url: null } as Record<string, unknown> : r));
    return { opportunities: normOpps, risks: normRisks };
  }, [detail]);

  const orSlides = useMemo(() => {
    const slides: Array<{ kind: "opportunity" | "risk"; data: Record<string, unknown>; idx: number }> = [];
    orData.opportunities.forEach((o, i) => slides.push({ kind: "opportunity", data: o, idx: i }));
    orData.risks.forEach((r, i) => slides.push({ kind: "risk", data: r, idx: i + orData.opportunities.length }));
    return slides;
  }, [orData]);

  // Reset carousel when report changes
  useEffect(() => { setOrIdx(0); }, [detail?.id]);

  const handleUpdateCmp = async () => {
    setLiveUpdating(true);
    setLiveMsg(null);
    try {
      const res = await updatePrice(detail!.ticker);
      if (res.current_price != null) {
        setLivePrice(res.current_price);
        setLiveMsg(`Live ${res.price_date} — CMP updated`);
      } else {
        setLiveMsg("Yahoo returned no price");
      }
    } catch (e) {
      setLiveMsg(typeof e === "string" ? e : "Update CMP failed");
    } finally {
      setLiveUpdating(false);
    }
  };

  const handleToggleFav = async () => {
    if (!detail) return;
    if (detail.is_primary) return;
    try {
      const updated = await toggleFavorite(detail.id);
      setDetail(updated as ReportDetail);
      try {
        const vers = await getReportVersions(detail.ticker);
        setVersions(vers);
      } catch {
        setVersions((prev) => prev.map((v) => v.id === updated.id ? { ...v, is_favorite: (updated as ReportDetail).is_favorite, is_primary: (updated as ReportDetail).is_primary } : { ...v, is_primary: v.id === updated.id ? (updated as ReportDetail).is_primary : false }));
      }
    } catch { /* ignore */ }
  };

  const openWatchlistPicker = async () => {
    setWatchlistMsg(null);
    setNewWatchlistName("");
    try {
      const ls = await watchlistsList();
      setWatchlists(ls);
    } catch {
      setWatchlists([]);
    }
    setShowWatchlistPicker(true);
  };

  const handleAddToWatchlist = async (watchlistId: string) => {
    if (!detail) return;
    try {
      await watchlistAddItem(watchlistId, detail.ticker, detail.country_code || detail.country || "US");
      setWatchlistMsg("Added to watchlist");
      setTimeout(() => setShowWatchlistPicker(false), 800);
    } catch (e) {
      setWatchlistMsg(typeof e === "string" ? e : "Failed to add");
    }
  };

  const handleCreateAndAdd = async () => {
    const n = newWatchlistName.trim();
    if (!n) return;
    try {
      const res = await watchlistCreate(n, null);
      await handleAddToWatchlist(res.id);
      const ls = await watchlistsList();
      setWatchlists(ls);
    } catch (e) {
      setWatchlistMsg(typeof e === "string" ? e : "Create failed");
    }
  };

  const handlePdf = async () => {
    if (!detail) return;
    setPdfLoading(true);
    try {
      const fileName = `${detail.ticker}_${detail.price_date}_${detail.llm_model}.pdf`.replace(/[^a-zA-Z0-9._-]/g, "_");
      const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      let targetPath: string | null = null;
      if (isTauri) {
        try {
          const { save } = await import("@tauri-apps/plugin-dialog");
          let folder: string | null = null;
          try {
            folder = await getSetting("general.pdf_save_folder");
            if (!folder || !folder.trim()) {
              const { downloadDir } = await import("@tauri-apps/api/path");
              folder = await downloadDir().catch(() => null);
            }
          } catch {}
          const defaultPath = folder && folder.trim() ? `${folder.trim().replace(/\/$/, "")}/${fileName}` : fileName;
          const path = await save({ defaultPath, filters: [{ name: "PDF", extensions: ["pdf"] }] });
          if (!path) return;
          targetPath = path as string;
        } catch (tauriErr) {
          console.warn("Tauri save dialog failed, will fallback after generating PDF", tauriErr);
        }
      }
      const { pdf } = await import("@react-pdf/renderer");
      const { ReportPdfDoc } = await import("../../lib/pdfReport");
      let logoSrc: string | null = null;
      try {
        const resp = await fetch(logoAsset);
        const blob = await resp.blob();
        logoSrc = await new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(reader.result as string);
          reader.onerror = reject;
          reader.readAsDataURL(blob);
        });
      } catch {
        logoSrc = null;
      }
      const doc = ReportPdfDoc({ detail, historicalData, fcfData: combinedFcfData, forecastData: combinedForecastData, logoSrc });
      const blob = await pdf(doc).toBlob();
      if (targetPath) {
        const buf = await blob.arrayBuffer();
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("save_pdf_file", { path: targetPath, data: Array.from(new Uint8Array(buf)) });
        alert(`PDF saved to:\n${targetPath}`);
        return;
      }
      // browser fallback or Tauri without dialog
      if (!isTauri) {
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = fileName;
        a.style.display = "none";
        document.body.appendChild(a);
        a.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window }));
        setTimeout(() => {
          URL.revokeObjectURL(url);
          a.remove();
        }, 2000);
        alert(`PDF downloaded as "${fileName}" to your browser's Downloads folder.\nIf no file appeared, check your Downloads or run via "npm run tauri dev" for native save dialog.`);
      } else {
        // Tauri but save not used (e.g., dialog failed) — try fallback to Downloads via invoke
        try {
          const { invoke } = await import("@tauri-apps/api/core");
          const { downloadDir } = await import("@tauri-apps/api/path");
          const dl = await downloadDir().catch(() => null);
          const fallbackPath = dl ? `${dl.replace(/\/$/, "")}/${fileName}` : fileName;
          const buf = await blob.arrayBuffer();
          await invoke("save_pdf_file", { path: fallbackPath, data: Array.from(new Uint8Array(buf)) });
          alert(`PDF saved to:\n${fallbackPath}\n(save dialog was not available)`);
        } catch {
          const url = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = url;
          a.download = fileName;
          a.style.display = "none";
          document.body.appendChild(a);
          a.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window }));
          setTimeout(() => URL.revokeObjectURL(url), 2000);
          alert(`PDF generated — if not downloaded, check Downloads.`);
        }
      }
    } catch (e) {
      console.error("PDF export failed", e);
      alert(typeof e === "string" ? e : e instanceof Error ? e.message : "PDF export failed — see console for details");
    } finally {
      setPdfLoading(false);
    }
  };

  const handleDeleteClick = () => {
    if (!detail) return;
    if (versions.length <= 1) {
      setPendingDeleteIds([detail.id]);
      setShowDeleteFinalConfirm(true);
    } else {
      setSelectedDeleteIds([]);
      setShowDeletePicker(true);
    }
  };

  const handleDeleteConfirm = () => {
    if (selectedDeleteIds.length === 0) return;
    setPendingDeleteIds([...selectedDeleteIds]);
    setShowDeletePicker(false);
    setShowDeleteFinalConfirm(true);
  };

  const handleDeleteFinal = async () => {
    const ids = pendingDeleteIds.length > 0 ? pendingDeleteIds : selectedDeleteIds;
    if (ids.length === 0) return;
    try {
      await deleteReports(ids);
      setShowDeleteFinalConfirm(false);
      setPendingDeleteIds([]);
      if (ids.includes(detail!.id)) {
        navigate("/reports");
      } else {
        const vers = await getReportVersions(detail!.ticker);
        setVersions(vers);
        try {
          const refreshed = await getReport(detail!.ticker, detail!.id);
          setDetail(refreshed as ReportDetail);
        } catch {
          navigate("/reports");
        }
      }
    } catch (e) {
      alert(typeof e === "string" ? e : "Delete failed");
    }
  };

  const doRecompute = async (nextOverrides: typeof overrides) => {
    if (!detail) return;
    setRecomputeLoading(true);
    setRecomputeError(null);
    try {
      const out = await recomputeLocal(detail.id, nextOverrides as Record<string, unknown>);
      setRecomputed(out);
      setWhatIfDirty(true);
    } catch (e) {
      setRecomputeError(typeof e === "string" ? e : e instanceof Error ? e.message : "Recompute failed");
    } finally {
      setRecomputeLoading(false);
    }
  };

  const resetWhatIf = () => {
    setOverrides({});
    setEditing({});
    setRecomputed(null);
    setWhatIfDirty(false);
    setRecomputeError(null);
  };

  // Slider bounds
  const sliderBounds = useMemo(() => {
    if (scenariosArr.length !== 3) return null;
    const vals = scenariosArr.map((s) => s.fair_value).filter((v) => Number.isFinite(v));
    if (vals.length < 2) return null;
    const min = Math.min(...vals, currentPriceDisplay ?? Infinity);
    const max = Math.max(...vals, currentPriceDisplay ?? -Infinity);
    const pad = (max - min) * 0.12 || max * 0.1 || 10;
    return { min: min - pad, max: max + pad, vals };
  }, [scenariosArr, currentPriceDisplay]);

  // Forecast data: dcf_assumptions (supports both top-level and raw.raw) + sensitivity_base
  const forecastData = useMemo(() => {
    const { rev: dcfRev, mar: dcfMar } = extractDcfRates(detail?.dcf_assumptions);
    const rev = overrides.revenue_growth_rates ?? (dcfRev as [number,number,number,number,number] | undefined) ?? null;
    const mar = overrides.operating_margins ?? (dcfMar as [number,number,number,number,number] | undefined) ?? null;
    if (!rev || !mar) return [];
    return rev.map((g, i) => ({ year: `Y${i+1}`, growth: g * 100, margin: mar[i] * 100 }));
  }, [detail, overrides]);

  const fcfData = useMemo(() => {
    // Use recomputed fcf_year_5 if available else stored sensitivity_base — display in millions
    const base = detail?.sensitivity_base_parameters as unknown as { fcf_year_5?: number } | null;
    const recomputedFcf = (recomputed as { fcf_year_5?: number } | null)?.fcf_year_5;
    const fcf5 = recomputedFcf ?? base?.fcf_year_5;
    if (fcf5 == null || !Number.isFinite(fcf5)) return [];
    const { rev: dcfRev } = extractDcfRates(detail?.dcf_assumptions);
    const rev = overrides.revenue_growth_rates ?? (dcfRev as number[] | undefined) ?? [0.1,0.1,0.1,0.1,0.1];
    let cum = 1;
    const growthFactors: number[] = [];
    for (let i = 4; i >=0; i--) {
      growthFactors[i] = cum;
      cum = cum / (1 + rev[i]);
    }
    const baseFactor = growthFactors[4];
    return growthFactors.map((f, i) => ({ year: `Y${i+1}`, fcf: (fcf5 * f) / baseFactor / 1e6 }));
  }, [detail, recomputed, overrides]);

  const historicalData = useMemo(() => {
    const parsed = cachedHist as unknown as { financials?: { historical_data?: Array<{ fiscal_year_end: string; revenue: number | null; operating_income: number | null; net_income: number | null; free_cash_flow: number | null }> } } | null;
    const hist = parsed?.financials?.historical_data;
    if (!hist || !Array.isArray(hist)) return [];
    const rev = [...hist].reverse();
    return rev.map((r) => ({
      year: r.fiscal_year_end,
      revenue: r.revenue != null ? r.revenue / 1e6 : null,
      opIncome: r.operating_income != null ? r.operating_income / 1e6 : null,
      netIncome: r.net_income != null ? r.net_income / 1e6 : null,
      fcf: (r as unknown as { free_cash_flow?: number | null }).free_cash_flow != null ? (r as unknown as { free_cash_flow: number }).free_cash_flow / 1e6 : null,
      _rawRevenue: r.revenue,
      _rawOpIncome: r.operating_income,
    }));
  }, [cachedHist]);

  const combinedForecastData = useMemo(() => {
    const now = new Date();
    const nowFY = `${String(now.getMonth() + 1).padStart(2, "0")}/${now.getFullYear()}`;
    const annuals = historicalData.filter((r) => r.year !== nowFY && r.year !== "TTM");
    const lastN = annuals.slice(-2);
    // show 1 if only 1 available, else 2
    const histRows = lastN.map((r) => {
      const absIdx = annuals.indexOf(r);
      const prev = absIdx > 0 ? annuals[absIdx - 1] : null;
      const rev = (r as unknown as { _rawRevenue: number | null })._rawRevenue;
      const prevRev = prev ? (prev as unknown as { _rawRevenue: number | null })._rawRevenue : null;
      const growth = rev != null && prevRev != null && prevRev !== 0 ? ((rev - prevRev) / Math.abs(prevRev)) * 100 : null;
      const margin = (r.revenue != null && r.opIncome != null && r.revenue !== 0) ? (r.opIncome / r.revenue) * 100 : null;
      return { year: fmtFY24(r.year), growth, margin, isHist: true };
    });
    const { rev: dcfRev, mar: dcfMar } = extractDcfRates(detail?.dcf_assumptions);
    const rev = overrides.revenue_growth_rates ?? (dcfRev as [number, number, number, number, number] | undefined) ?? null;
    const mar = overrides.operating_margins ?? (dcfMar as [number, number, number, number, number] | undefined) ?? null;
    if (!rev || !mar) return histRows.map((h) => ({ year: h.year, histGrowth: h.growth, histMargin: h.margin, fcstGrowth: null, fcstMargin: null }));
    const lastYearNum = lastN.length ? parseFYYear(lastN[lastN.length - 1].year) : null;
    const fcstRows = rev.map((g, i) => {
      const fy = lastYearNum != null ? `FY${String((lastYearNum + 1 + i) % 100).padStart(2, "0")}` : `Y${i + 1}`;
      return { year: fy, histGrowth: null, histMargin: null, fcstGrowth: g * 100, fcstMargin: mar[i] * 100 };
    });
    const histCombined = histRows.map((h) => ({ year: h.year, histGrowth: h.growth, histMargin: h.margin, fcstGrowth: null, fcstMargin: null }));
    return [...histCombined, ...fcstRows];
  }, [historicalData, detail, overrides]);

  const combinedFcfData = useMemo(() => {
    const now = new Date();
    const nowFY = `${String(now.getMonth() + 1).padStart(2, "0")}/${now.getFullYear()}`;
    const ttmEntry = historicalData.find((r) => r.year === "TTM");
    const annuals = historicalData.filter((r) => r.year !== nowFY && r.year !== "TTM");
    const lastN = annuals.slice(-2);
    const annualRows = lastN.map((r) => ({ year: fmtFY24(r.year), histFcf: r.fcf, fcstFcf: null as number | null, na: null as number | null }));
    const histRows: Array<{ year: string; histFcf: number | null; fcstFcf: number | null; na: number | null }> = [];
    histRows.push(...annualRows);
    if (ttmEntry) histRows.push({ year: "TTM", histFcf: ttmEntry.fcf, fcstFcf: null, na: ttmEntry.fcf == null ? 0 : null });
    const base = detail?.sensitivity_base_parameters as unknown as { fcf_year_5?: number } | null;
    const recomputedFcf = (recomputed as { fcf_year_5?: number } | null)?.fcf_year_5;
    const fcf5 = recomputedFcf ?? base?.fcf_year_5;
    if (fcf5 == null || !Number.isFinite(fcf5)) return histRows.map((h) => ({ ...h, fcstFcf: null }));
    const { rev: dcfRev } = extractDcfRates(detail?.dcf_assumptions);
    const rev = overrides.revenue_growth_rates ?? (dcfRev as number[] | undefined) ?? [0.1, 0.1, 0.1, 0.1, 0.1];
    let cum = 1;
    const growthFactors: number[] = [];
    for (let i = 4; i >= 0; i--) {
      growthFactors[i] = cum;
      cum = cum / (1 + rev[i]);
    }
    const baseFactor = growthFactors[4];
    const fcstVals = growthFactors.map((f) => (fcf5 * f) / baseFactor / 1e6);
    const lastYearNum = lastN.length ? parseFYYear(lastN[lastN.length - 1].year) : null;
    const fcstRows = fcstVals.map((v, i) => {
      const fy = lastYearNum != null ? `FY${String((lastYearNum + 1 + i) % 100).padStart(2, "0")}` : `Y${i + 1}`;
      return { year: fy, histFcf: null, fcstFcf: v };
    });
    const histCombined = histRows.map((h) => ({ year: h.year, histFcf: h.histFcf, fcstFcf: null, na: (h as any).na ?? null }));
    return [...histCombined, ...fcstRows.map((r) => ({ ...r, na: null as number | null }))];
  }, [historicalData, detail, recomputed, overrides]);

  // keep original for typecheck - now derived via combined
  void forecastData;
  void fcfData;

  if (loading) return <Loading label="Loading report…" />;
  if (err) return <ErrorNote message={err} />;
  if (!detail) return <ErrorNote message="Report not found." />;

  const waccVal = (recomputed as { wacc?: number } | null)?.wacc ?? detail.wacc;
  const costEq = (recomputed as { cost_of_equity?: number } | null)?.cost_of_equity ?? detail.cost_of_equity;
  const costDebt = (recomputed as { cost_of_debt_after_tax?: number } | null)?.cost_of_debt_after_tax ?? detail.cost_of_debt_after_tax;

  return (
    <div className="space-y-6">
      <button type="button" onClick={handleBack} className="inline-flex items-center gap-1 rounded-md border border-app-border px-3 py-2 text-sm hover:bg-app-sidebar">
        <ArrowLeft className="h-4 w-4" /> Back
      </button>
      {/* Header */}
      <Card>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold text-app-text flex items-center gap-2">
              {detail.company_name || detail.company} <span className="text-sm font-normal text-app-muted">({detail.ticker})</span>
              {detail.is_primary && <Star className="h-4 w-4 fill-amber-400 text-amber-400" aria-label="Primary" />}
            </h1>
            <p className="text-sm text-app-muted">
              {detail.exchange || "—"} · {detail.country_code || detail.country} · {detail.sector || "—"} · {detail.industry || "—"} · {currency} · as of {detail.price_date} · <span className="rounded bg-app-sidebar px-1.5 py-0.5 text-xs">{detail.llm_provider}/{detail.llm_model}</span>
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => {
                const params = new URLSearchParams({
                  ticker: detail.ticker,
                  country: detail.country_code || detail.country || "US",
                  provider: detail.llm_provider || "deepseek",
                  model: detail.llm_model || "",
                  company: detail.company_name || detail.company || "",
                });
                navigate(`/new-analysis?${params.toString()}`);
              }}
              className="rounded-md bg-app-text px-3 py-2 text-sm font-medium text-app-surface hover:opacity-90 inline-flex items-center gap-1"
            >
              <RefreshCw className="h-4 w-4" /> Reanalyse
            </button>
            <button
              type="button"
              onClick={() => {
                if (versions.length < 2) return;
                const initial = [detail.id, ...versions.filter((v) => v.id !== detail.id).slice(0, 3).map((v) => v.id)].slice(0, 4);
                setSelectedCompareIds(initial);
                setShowComparePicker(true);
              }}
              disabled={versions.length < 2}
              title={versions.length < 2 ? "Need at least 2 reports to compare" : "Compare reports (max 4)"}
              className="rounded-md border border-app-border px-3 py-2 text-sm hover:bg-app-sidebar disabled:opacity-50 disabled:cursor-not-allowed inline-flex items-center gap-1"
            >
              <Scale className="h-4 w-4" /> Compare
            </button>
            <button
              type="button"
              onClick={handleToggleFav}
              disabled={detail.is_primary}
              title={detail.is_primary ? "Primary report is already the default" : detail.is_favorite ? "Unmark favorite" : "Mark favorite — will also become primary"}
              className={`rounded-md border p-2 disabled:opacity-50 disabled:cursor-not-allowed ${detail.is_favorite ? "bg-app-warning text-white border-app-warning" : "border-app-border text-app-text hover:bg-app-sidebar"}`}
            >
              <Star className="h-4 w-4" fill={detail.is_favorite ? "currentColor" : "none"} />
            </button>
            <button
              type="button"
              onClick={openWatchlistPicker}
              title="Add to watchlist"
              className="rounded-md border border-app-border p-2 text-app-text hover:bg-app-sidebar"
            >
              <Eye className="h-4 w-4" />
            </button>
            <button
              type="button"
              onClick={handleDeleteClick}
              title="Delete report(s)"
              className="rounded-md border border-app-border p-2 text-app-negative hover:bg-red-50"
            >
              <Trash2 className="h-4 w-4" />
            </button>
            <button type="button" onClick={handlePdf} disabled={pdfLoading} className="rounded-md border border-app-border px-3 py-2 text-sm text-app-text hover:bg-app-sidebar inline-flex items-center gap-1 disabled:opacity-50">
              {pdfLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />} {pdfLoading ? "Generating…" : "PDF"}
            </button>
          </div>
        </div>
        {/* Version pills */}
        {versions.length > 1 && (
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <span className="text-xs text-app-muted">Versions:</span>
            {versions.map((v) => {
              const active = v.id === detail.id;
              return (
                <button
                  key={v.id}
                  type="button"
                  onClick={() => navigate(`/report/${detail.ticker}?version=${encodeURIComponent(v.id)}`)}
                  className={`rounded-full border px-2.5 py-1 text-xs ${active ? "bg-app-text text-app-surface border-app-text" : "border-app-border text-app-muted hover:bg-app-sidebar"}`}
                >
                  {v.llm_model} · {v.price_date} {v.is_primary ? "★" : ""} {v.is_favorite && !v.is_primary ? "♥" : ""}
                </button>
              );
            })}
          </div>
        )}
      </Card>

      {/* Valuation Summary */}
      <Card>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-sm font-semibold text-app-text">Valuation Summary</h2>
          <VerdictPill verdict={verdictDisplay} />
        </div>
        <div className="mt-4 grid grid-cols-2 gap-4 md:grid-cols-4">
          <div className="rounded-md bg-app-sidebar p-3">
            <div className="text-xs text-app-muted">Current price (CMP)</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-app-text">{currentPriceDisplay != null ? fmtMoneyWithCurrency(currentPriceDisplay) : "—"}</div>
            {livePrice != null && <div className="text-xs text-app-positive">Live</div>}
          </div>
          <div className="rounded-md bg-app-sidebar p-3">
            <div className="text-xs text-app-muted">Weighted fair value</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-app-text">{weightedFV != null ? fmtMoneyWithCurrency(weightedFV) : "—"}</div>
            <div className="text-xs text-app-muted">Base {baseFV != null ? fmtMoneyWithCurrency(baseFV) : "—"} · Bull {detail.bull_case_fair_value != null ? fmtMoneyWithCurrency(detail.bull_case_fair_value) : "—"} · Bear {detail.bear_case_fair_value != null ? fmtMoneyWithCurrency(detail.bear_case_fair_value) : "—"}</div>
          </div>
          <div className="rounded-md bg-app-sidebar p-3">
            <div className="text-xs text-app-muted">Valuation gap</div>
            <div className={`mt-1 text-lg font-semibold tabular-nums ${gapPct != null ? gapCls(gapPct) : ""}`}>{gapPct != null ? fmtPct(gapPct) : "—"}</div>
            <div className="text-xs text-app-muted">{detail.valuation_verdict || "—"}</div>
          </div>
          <div className="rounded-md bg-app-sidebar p-3">
            <div className="text-xs text-app-muted">Risk / reward</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-app-text">{detail.risk_reward_ratio != null ? Number(detail.risk_reward_ratio).toFixed(2) : "—"}</div>
            <div className="text-xs text-app-muted">{detail.risk_reward_label || "—"} {detail.upside_pct != null ? `↑${fmtPct(detail.upside_pct * 100)}` : ""} {detail.downside_pct != null ? `↓${fmtPct(detail.downside_pct * 100)}` : ""}</div>
          </div>
        </div>
      </Card>

      {/* Price Scenario Slider */}
      <Card
        title="Price Scenario Slider"
        action={
          <Button onClick={handleUpdateCmp} disabled={liveUpdating}>
            {liveUpdating ? (
              <span className="inline-flex items-center gap-1"><Loader2 className="h-4 w-4 animate-spin" /> Updating…</span>
            ) : (
              <span className="inline-flex items-center gap-1"><RefreshCw className="h-4 w-4" /> Update CMP</span>
            )}
          </Button>
        }
      >
        {scenariosArr.length === 3 && sliderBounds ? (
          <div className="space-y-2">
            <div className="flex items-center gap-2 md:gap-3">
              <img src={bearImg} alt="Bear" className="h-12 w-12 shrink-0 object-contain md:h-14 md:w-14" />
              <div className="relative flex-1">
                {/* Labels above */}
                <div className="relative h-5">
                  {scenariosArr.map((s) => {
                    const pct = ((s.fair_value - sliderBounds.min) / (sliderBounds.max - sliderBounds.min)) * 100;
                    return (
                      <div
                        key={`label-${s.name}`}
                        className="absolute bottom-0 whitespace-nowrap text-center text-[11px] font-medium leading-none text-app-text"
                        style={{ left: `${pct}%`, transform: "translateX(-50%)" }}
                      >
                        {s.name}
                      </div>
                    );
                  })}
                </div>
                {/* Track with circles */}
                <div className="relative mx-1 h-8">
                  <div className="absolute left-0 right-0 top-1/2 h-1.5 -translate-y-1/2 rounded-full bg-app-border" />
                  {scenariosArr.map((s) => {
                    const pct = ((s.fair_value - sliderBounds.min) / (sliderBounds.max - sliderBounds.min)) * 100;
                    const bg =
                      s.name === "Optimistic"
                        ? "bg-app-positive"
                        : s.name === "Pessimistic"
                          ? "bg-app-negative"
                          : "bg-app-warning";
                    return (
                      <div
                        key={s.name}
                        className="absolute top-1/2 -translate-y-1/2"
                        style={{ left: `${pct}%`, transform: "translate(-50%, -50%)" }}
                      >
                        <div className={`h-4 w-4 rounded-full border-2 border-white shadow ${bg}`} />
                      </div>
                    );
                  })}
                  {currentPriceDisplay != null && (
                    <div
                      className="absolute top-1/2 -translate-y-1/2"
                      style={{ left: `${((currentPriceDisplay - sliderBounds.min) / (sliderBounds.max - sliderBounds.min)) * 100}%`, transform: "translate(-50%, -50%) rotate(45deg)" }}
                    >
                      <div className="h-3.5 w-3.5 rounded-sm border border-white bg-app-text shadow" />
                    </div>
                  )}
                </div>
                {/* Values below */}
                <div className="relative h-5">
                  {scenariosArr.map((s) => {
                    const pct = ((s.fair_value - sliderBounds.min) / (sliderBounds.max - sliderBounds.min)) * 100;
                    return (
                      <div
                        key={`val-${s.name}`}
                        className="absolute top-0 whitespace-nowrap text-center text-[11px] tabular-nums leading-none text-app-muted"
                        style={{ left: `${pct}%`, transform: "translateX(-50%)" }}
                      >
                        {fmtMoneyWithCurrency(s.fair_value)}
                      </div>
                    );
                  })}
                </div>
              </div>
              <img src={bullImg} alt="Bull" className="h-12 w-12 shrink-0 object-contain md:h-14 md:w-14" />
            </div>
            {liveMsg && <div className="text-xs text-app-muted">{liveMsg}</div>}
          </div>
        ) : (
          <p className="text-sm text-app-muted">No scenario data.</p>
        )}
      </Card>

      {/* Scenario cards */}
      <div className="grid gap-4 md:grid-cols-3">
        {scenariosArr.map((s) => (
          <Card key={s.name} title={s.name}>
            <div className="space-y-2 text-sm">
              <div className="text-lg font-semibold tabular-nums">{fmtMoneyWithCurrency(s.fair_value)}</div>
              {s.cagr != null && <div className="text-xs text-app-muted">Revenue CAGR {fmtPct(s.cagr * 100)} · Terminal margin {s.margin != null ? fmtPct(s.margin * 100) : "—"}</div>}
              {s.wacc != null && <div className="text-xs text-app-muted">WACC {fmtPct(s.wacc * 100)} · Growth {s.growth != null ? fmtPct(s.growth * 100) : "—"}</div>}
              {s.narrative && <p className="mt-2 text-xs leading-relaxed text-app-text whitespace-pre-line">{s.narrative}</p>}
            </div>
          </Card>
        ))}
        {scenariosArr.length === 0 && <Card><p className="text-sm text-app-muted">No scenarios.</p></Card>}
      </div>

      {/* Sensitivity Heatmap 3x3 */}
      <Card title="Sensitivity Heatmap (WACC × Growth ±50bps)">
        {sensitivityCells.length === 9 ? (
          <div className="overflow-auto">
            <div className="grid grid-cols-4 gap-1 text-xs">
              <div className="p-2 text-app-muted" />
              {[-1, 0, 1].map((gl) => (
                <div key={gl} className="p-2 text-center font-medium text-app-text">
                  g {gl === -1 ? "−0.5%" : gl === 1 ? "+0.5%" : "base"}
                </div>
              ))}
              {[-1, 0, 1].map((wl) => (
                <div key={wl} className="contents">
                  <div className="p-2 text-center font-medium text-app-text">WACC {wl === -1 ? "−0.5%" : wl === 1 ? "+0.5%" : "base"}</div>
                  {[-1, 0, 1].map((gl) => {
                    const cell = sensitivityCells.find((c) => c.wacc_level === wl && c.growth_level === gl);
                    if (!cell) return <div key={gl} className="rounded p-2 text-center text-app-muted">—</div>;
                    const pct = cell.pct_change_from_base ?? 0;
                    const intensity = Math.min(1, Math.abs(pct) / 20);
                    const bg = !cell.is_model_stable
                      ? "bg-gray-200 text-gray-500 striped"
                      : cell.is_base_case
                        ? "bg-app-text text-app-surface ring-2 ring-app-text"
                        : pct > 0
                          ? `bg-app-positive text-white`
                          : `bg-app-negative text-white`;
                    const style = !cell.is_model_stable || cell.is_base_case ? {} : { opacity: 0.4 + intensity * 0.6 } as React.CSSProperties;
                    return (
                      <div key={gl} className={`rounded p-2 text-center tabular-nums ${bg}`} style={style} title={`WACC ${fmtPct(cell.wacc_value * 100)} g ${fmtPct(cell.growth_value * 100)} → ${fmtMoney(cell.fair_value)} ${pct != null ? `(${fmtPct(pct)})` : ""} ${!cell.is_model_stable ? "unstable" : ""}`}>
                        <div className="font-semibold">{fmtMoney(cell.fair_value)}</div>
                        <div className="text-[11px]">{cell.pct_change_from_base != null ? fmtPct(cell.pct_change_from_base) : "—"}</div>
                      </div>
                    );
                  })}
                </div>
              ))}
            </div>
          </div>
        ) : (
          <p className="text-sm text-app-muted">No sensitivity matrix.</p>
        )}
      </Card>

      {/* Confidence */}
      <Card title={`Confidence — ${detail.confidence_level || ""} (${detail.confidence_score != null ? Math.round(detail.confidence_score as number) : "—"} / 100)`}>
        {confidenceDims.length > 0 ? (
          <div className="grid gap-4 md:grid-cols-2">
            {confidenceDims.map((d) => (
              <ConfidenceBar key={d.key} label={d.label} value={d.score} max={d.max} reasoning={d.reasoning} />
            ))}
          </div>
        ) : (
          <p className="text-sm text-app-muted">No confidence details.</p>
        )}
      </Card>

      {/* Narratives - Business (50%) + Historical (50%) side by side */}
      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Business Description">
          {(() => {
            const bd = extractBusinessDescription(detail.ai_business_context || detail.narrative);
            if (!bd.trim()) return <p className="text-sm text-app-muted">No business context.</p>;
            return (
              <div className="prose prose-sm max-w-none text-sm leading-relaxed text-app-text prose-p:my-1 prose-headings:text-app-text prose-strong:text-app-text">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{bd}</ReactMarkdown>
              </div>
            );
          })()}
        </Card>
        <Card title="Historical Financial Insights">
          {(() => {
            const bullets = normalizeHistoricalBullets(detail.ai_historical_performance);
            if (bullets.length === 0) return <p className="text-sm text-app-muted">No data.</p>;
            return (
              <ul className="list-disc space-y-2 pl-5 text-sm text-app-text">
                {bullets.map((b, i) => (
                  <li key={i} className="leading-relaxed">{b}</li>
                ))}
              </ul>
            );
          })()}
        </Card>
      </div>

      {/* WACC Breakdown - moved after business description */}
      <Card title="WACC Breakdown">
        <div className="grid grid-cols-2 gap-3 text-sm md:grid-cols-3">
          <div className="rounded-md bg-app-sidebar p-3"><div className="text-xs text-app-muted">WACC</div><div className="font-semibold tabular-nums">{waccVal != null ? fmtPct(waccVal * 100) : "—"}</div></div>
          <div className="rounded-md bg-app-sidebar p-3"><div className="text-xs text-app-muted">Cost of equity</div><div className="font-semibold tabular-nums">{costEq != null ? fmtPct(costEq * 100) : "—"}</div></div>
          <div className="rounded-md bg-app-sidebar p-3"><div className="text-xs text-app-muted">Cost of debt (after tax)</div><div className="font-semibold tabular-nums">{costDebt != null ? fmtPct(costDebt * 100) : "—"}</div></div>
          <div className="rounded-md bg-app-sidebar p-3"><div className="text-xs text-app-muted">Beta (raw / adj)</div><div className="font-semibold tabular-nums">{detail.raw_beta != null ? detail.raw_beta.toFixed(2) : "—"} / {detail.blume_adjusted_beta != null ? detail.blume_adjusted_beta.toFixed(2) : "—"}</div></div>
          <div className="rounded-md bg-app-sidebar p-3"><div className="text-xs text-app-muted">Terminal growth</div><div className="font-semibold tabular-nums">{detail.terminal_growth_rate != null ? fmtPct(detail.terminal_growth_rate * 100) : "—"}</div></div>
          <div className="rounded-md bg-app-sidebar p-3"><div className="text-xs text-app-muted">Terminal margin</div><div className="font-semibold tabular-nums">{detail.terminal_margin != null ? fmtPct(detail.terminal_margin * 100) : "—"}</div></div>
        </div>
      </Card>

      {/* Growth + Opportunities side by side 50% */}
      <div className="grid gap-4 md:grid-cols-2">
        <MarkdownPanel title="Growth Projection Rationale" content={detail.ai_growth_projections} />
        <Card title="Opportunities & Risks">
          {(() => {
            if (orSlides.length === 0) return <p className="text-sm text-app-muted">No data.</p>;
            const total = orSlides.length;
            const safeIdx = ((orIdx % total) + total) % total;
            const slide = orSlides[safeIdx];
            const isOpp = slide.kind === "opportunity";
            const d = slide.data;
            const catalyst = (d.catalyst_name as string) || "";
            const riskCat = (d.risk_category as string) || "";
            const desc = (d.description as string) || "";
            const impact = (d.potential_impact as string) || "";
            const timeline = (d.timeline as string) || null;
            const likelihood = (d.likelihood as string) || null;
            const url = (d.source_url as string) || null;
            const sourceName = isOpp ? catalyst : riskCat;
            const titleLabel = isOpp ? "Opportunity" : "Risk";
            const titleColor = isOpp ? "text-app-positive" : "text-app-negative";
            const go = (dir: number) => setOrIdx((prev) => (prev + dir + total) % total);
            return (
              <div
                className="space-y-3"
                role="region"
                aria-roledescription="carousel"
                aria-label="Opportunities and Risks"
                onTouchStart={(e) => setOrTouchStart(e.touches[0].clientX)}
                onTouchEnd={(e) => {
                  if (orTouchStart == null) return;
                  const diff = e.changedTouches[0].clientX - orTouchStart;
                  if (Math.abs(diff) > 50) go(diff > 0 ? -1 : 1);
                  setOrTouchStart(null);
                }}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className={`rounded-full px-2 py-0.5 text-xs font-semibold ${isOpp ? "bg-app-positive/10 text-app-positive" : "bg-app-negative/10 text-app-negative"}`}>
                    {titleLabel} {orData.opportunities.length > 0 && orData.risks.length > 0 ? `${safeIdx + 1} / ${total}` : `${(isOpp ? orSlides.filter((s) => s.kind === "opportunity").findIndex((s) => s.idx === slide.idx) + 1 : orSlides.filter((s) => s.kind === "risk").findIndex((s) => s.idx === slide.idx) + 1)} / ${isOpp ? orData.opportunities.length : orData.risks.length}`}
                  </span>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      aria-label="Previous"
                      onClick={() => go(-1)}
                      className="rounded border border-app-border p-1 hover:bg-app-sidebar"
                    >
                      <ChevronLeft className="h-4 w-4" />
                    </button>
                    <button
                      type="button"
                      aria-label="Next"
                      onClick={() => go(1)}
                      className="rounded border border-app-border p-1 hover:bg-app-sidebar"
                    >
                      <ChevronRight className="h-4 w-4" />
                    </button>
                  </div>
                </div>

                <div className="min-h-[180px] rounded-md border border-app-border bg-app-surface p-3">
                  <h4 className={`text-sm font-semibold ${titleColor}`}>{sourceName || titleLabel}</h4>
                  {desc && <p className="mt-2 text-sm leading-relaxed text-app-text">{desc}</p>}
                  {impact && (
                    <div className="mt-3 rounded bg-app-sidebar p-2">
                      <div className="text-[11px] font-semibold uppercase tracking-wide text-app-muted">Potential Impact</div>
                      <div className="mt-1 text-sm text-app-text">{impact}</div>
                    </div>
                  )}
                  <div className="mt-2 flex flex-wrap gap-2">
                    {timeline && (
                      <span className="rounded-full bg-app-sidebar px-2 py-0.5 text-xs text-app-muted">
                        Timeline: {timeline}
                      </span>
                    )}
                    {likelihood && (
                      <span
                        className={`rounded-full px-2 py-0.5 text-xs font-medium ${
                          likelihood.toLowerCase() === "high"
                            ? "bg-red-100 text-red-700"
                            : likelihood.toLowerCase() === "moderate"
                              ? "bg-amber-100 text-amber-800"
                              : "bg-gray-100 text-gray-700"
                        }`}
                      >
                        Likelihood: {likelihood}
                      </span>
                    )}
                    {url ? (
                      <a href={url} target="_blank" rel="noreferrer" className="rounded bg-app-text px-2 py-0.5 text-xs font-medium text-app-surface hover:opacity-90">
                        [{sourceName || (isOpp ? "Source" : "Source")}]
                      </a>
                    ) : null}
                  </div>
                  {url && (
                    <a href={url} target="_blank" rel="noreferrer" className="mt-2 block break-all text-xs text-app-muted underline hover:text-app-text">
                      {url}
                    </a>
                  )}
                </div>

                <div className="flex justify-center gap-1.5" role="tablist" aria-label="Slides">
                  {orSlides.map((_, i) => (
                    <button
                      key={i}
                      type="button"
                      role="tab"
                      aria-selected={i === safeIdx}
                      aria-label={`Slide ${i + 1}`}
                      onClick={() => setOrIdx(i)}
                      className={`h-1.5 rounded-full transition-colors ${i === safeIdx ? "w-4 bg-app-text" : "w-1.5 bg-app-border"}`}
                    />
                  ))}
                </div>
              </div>
            );
          })()}
        </Card>
      </div>

      {/* Historical + FCF — side by side before Investment row */}
      <div className="grid gap-4 md:grid-cols-2">
        <Card title={`Historical Financials — Revenue & Income (${chartCurrencySuffix})`}>
          {historicalData.length > 0 ? (
            <ResponsiveContainer width="100%" height={300}>
              <LineChart data={historicalData} margin={{ top: 8, right: 16, left: 0, bottom: 0 }}>
                <CartesianGrid stroke="#e2e8f0" strokeDasharray="3 3" vertical={false} opacity={0.6} />
                <XAxis dataKey="year" tick={{ fontSize: 11, fill: "#64748b" }} axisLine={false} tickLine={false} />
                <YAxis tick={{ fontSize: 11, fill: "#64748b" }} axisLine={false} tickLine={false} tickFormatter={(v) => `${fmtNum(Number(v))}M`} width={70} />
                <Tooltip
                  contentStyle={{ borderRadius: 8, border: "1px solid #e2e8f0", boxShadow: "0 4px 12px rgba(0,0,0,0.08)" }}
                  formatter={(v: any, n: any) => [`${fmtNum(Number(v))}M`, n]}
                />
                <Legend wrapperStyle={{ fontSize: 11, paddingTop: 8 }} iconType="circle" />
                <Line type="monotone" dataKey="revenue" name="Revenue (M)" stroke="#0f172a" strokeWidth={2.5} dot={{ r: 3.5, fill: "#0f172a", stroke: "#fff", strokeWidth: 2 }} activeDot={{ r: 5 }} />
                <Line type="monotone" dataKey="opIncome" name="Operating income (M)" stroke="#f59e0b" strokeWidth={2.5} dot={{ r: 3.5, fill: "#f59e0b", stroke: "#fff", strokeWidth: 2 }} activeDot={{ r: 5 }} />
                <Line type="monotone" dataKey="netIncome" name="Net income (M)" stroke="#10b981" strokeWidth={2.5} dot={{ r: 3.5, fill: "#10b981", stroke: "#fff", strokeWidth: 2 }} activeDot={{ r: 5 }} />
              </LineChart>
            </ResponsiveContainer>
          ) : (
            <p className="text-sm text-app-muted">No historical data — run analysis or ensure financial_data_cache populated.</p>
          )}
        </Card>
        <Card title={`FCF Projection (Hist FY + 5Y — ${chartCurrencySuffix})`}>
          {combinedFcfData.length > 0 ? (
            <ResponsiveContainer width="100%" height={300}>
              <BarChart data={combinedFcfData} margin={{ top: 8, right: 16, left: 0, bottom: 0 }} barCategoryGap="22%" barGap={4}>
                <defs>
                  <linearGradient id="fcfGradient" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="#0f172a" stopOpacity={0.95} />
                    <stop offset="100%" stopColor="#334155" stopOpacity={0.85} />
                  </linearGradient>
                  <pattern id="fcfDotted" patternUnits="userSpaceOnUse" width="6" height="6" patternTransform="rotate(45)">
                    <line x1="0" y1="0" x2="0" y2="6" stroke="#0f172a" strokeWidth="1.5" strokeDasharray="2 2" />
                  </pattern>
                </defs>
                <CartesianGrid stroke="#e2e8f0" strokeDasharray="3 3" vertical={false} opacity={0.6} />
                <XAxis dataKey="year" tick={{ fontSize: 11, fill: "#64748b" }} axisLine={false} tickLine={false} />
                <YAxis tick={{ fontSize: 11, fill: "#64748b" }} axisLine={false} tickLine={false} tickFormatter={(v) => `${fmtNum(Number(v))}M`} width={65} />
                <Tooltip
                  cursor={{ fill: "rgba(15,23,42,0.04)" }}
                  content={({ active, payload, label }: any) => {
                    if (!active || !payload?.length) return null;
                    const year = label ?? payload[0]?.payload?.year;
                    const isTtmNa = year === "TTM" && combinedFcfData.find((d) => d.year === "TTM")?.histFcf == null;
                    if (isTtmNa) {
                      return (
                        <div style={{ background: "#fff", border: "1px solid #e2e8f0", borderRadius: 8, padding: 8, fontSize: 11, boxShadow: "0 4px 12px rgba(0,0,0,0.08)" }}>
                          <div style={{ fontWeight: 600, marginBottom: 2 }}>TTM</div>
                          <div>TTM FCF: Not reported in interim quarters</div>
                        </div>
                      );
                    }
                    return (
                      <div style={{ background: "#fff", border: "1px solid #e2e8f0", borderRadius: 8, padding: 8, fontSize: 11, boxShadow: "0 4px 12px rgba(0,0,0,0.08)" }}>
                        <div style={{ fontWeight: 600, marginBottom: 4 }}>{year}</div>
                        {payload.map((p: any) => {
                          const v = p.value;
                          const name = p.name || p.dataKey;
                          const labelName = name === "histFcf" ? "Hist FCF" : name === "fcstFcf" ? "Forecast FCF" : String(name);
                          const display = v == null ? "—" : `${fmtNum(Number(v))}M`;
                          return (
                            <div key={String(p.dataKey)} style={{ display: "flex", gap: 8, alignItems: "center" }}>
                              <span style={{ width: 8, height: 8, background: p.color || p.fill || "#64748b", display: "inline-block", borderRadius: 2 }} />
                              <span>{labelName}:</span>
                              <span style={{ fontWeight: 600 }}>{display}</span>
                            </div>
                          );
                        })}
                      </div>
                    );
                  }}
                />
                <Legend wrapperStyle={{ fontSize: 11, paddingTop: 8 }} iconType="square" />
                <Bar dataKey="histFcf" name="Hist FCF (M)" fill="#64748b" radius={[8, 8, 0, 0]} barSize={26} />
                <Bar dataKey="fcstFcf" name="Forecast FCF (M)" fill="#0f172a" fillOpacity={0.45} stroke="#0f172a" strokeWidth={1.5} strokeDasharray="5 4" radius={[8, 8, 0, 0]} barSize={26} />
                {/* Dummy bar for N/A label — renders exactly at TTM's category, ensures correct x */}
                <Bar dataKey="na" fill="transparent" isAnimationActive={false} barSize={26}>
                  <LabelList
                    dataKey="na"
                    content={(props: any) => {
                      const year = props.payload?.year;
                      if (year !== "TTM") return null;
                      const entry = combinedFcfData.find((d) => d.year === "TTM");
                      if (!entry || entry.histFcf != null) return null;
                      const { x, width } = props;
                      const cx = typeof x === "number" && typeof width === "number" ? x + width / 2 : Number(x) || 0;
                      return (
                        <text x={cx} y={18} textAnchor="middle" fontSize={11} fill="#64748b" fontWeight={600}>
                          N/A
                        </text>
                      );
                    }}
                  />
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          ) : (
            <p className="text-sm text-app-muted">No FCF data.</p>
          )}
        </Card>
      </div>

      {/* Investment Perspective + Search Sources side by side 50% */}
      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Investment Perspective">
          {(() => {
            const raw = detail.ai_investment_perspective;
            let verdict = "";
            let body: string | null = null;
            let metrics: Array<{ metric_name?: string; target_value?: string; red_flag_condition?: string; valuation_impact?: string }> | null = null;
            // Try to parse raw as object
            let obj: Record<string, unknown> | null = null;
            if (typeof raw === "string") {
              try { obj = JSON.parse(raw); } catch { obj = null; body = raw; }
            } else if (raw && typeof raw === "object") {
              obj = raw as Record<string, unknown>;
            }
            if (obj) {
              if (typeof obj.verdict === "string") verdict = obj.verdict as string;
              if (Array.isArray(obj.monitoring_metrics)) metrics = obj.monitoring_metrics as unknown as typeof metrics;
              else if (Array.isArray(obj.key_monitoring_metrics)) metrics = obj.key_monitoring_metrics as unknown as typeof metrics;
              // Use verdict + remaining text as body if present
              if (!body) {
                const textParts: string[] = [];
                if (typeof obj.verdict === "string") textParts.push(obj.verdict as string);
                // If object has other string fields, use them
                for (const [k, v] of Object.entries(obj)) {
                  if (k === "verdict" || k === "monitoring_metrics" || k === "key_monitoring_metrics") continue;
                  if (typeof v === "string" && v.trim()) textParts.push(v);
                }
                if (textParts.length > 0) body = textParts.join("\n\n");
                else if (obj && Object.keys(obj).length === 1 && verdict) body = verdict;
              }
            }
            if (!verdict && !body) {
              // Fallback: raw is plain string verdict
              if (typeof raw === "string" && raw.trim()) { verdict = raw.trim(); body = raw.trim(); }
            }
            return (
              <div className="space-y-3 text-sm">
                {verdict && (
                  <div className="rounded-md bg-app-sidebar p-3">
                    <div className="text-xs font-semibold uppercase tracking-wide text-app-muted">Verdict</div>
                    <div className="mt-1 leading-relaxed text-app-text">{verdict}</div>
                  </div>
                )}
                {body && body !== verdict && (
                  <div className="prose prose-sm max-w-none text-sm leading-relaxed text-app-text prose-p:my-1">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>{body}</ReactMarkdown>
                  </div>
                )}
                {metrics && (metrics as Array<{ metric_name?: string; target_value?: string; red_flag_condition?: string }>).length > 0 && (
                  <div>
                    <div className="text-xs font-semibold text-app-muted">Key Monitoring Metrics</div>
                    <ul className="mt-1 list-disc space-y-1 pl-5 text-xs text-app-text">
                      {(metrics as Array<{ metric_name?: string; target_value?: string; red_flag_condition?: string }>).slice(0, 5).map((m: { metric_name?: string; target_value?: string; red_flag_condition?: string }, i: number) => (
                        <li key={i}>
                          <span className="font-medium">{m.metric_name || `Metric ${i + 1}`}</span>
                          {m.target_value ? ` — target ${m.target_value}` : ""}
                          {m.red_flag_condition ? ` (red flag: ${m.red_flag_condition})` : ""}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
                {!verdict && !body && !metrics && <p className="text-sm text-app-muted">No data.</p>}
              </div>
            );
          })()}
        </Card>
        <Card title="Search Sources">
          {(() => {
            const src = detail.search_sources;
            if (!Array.isArray(src) || src.length === 0) return <p className="text-sm text-app-muted">No sources.</p>;
            // Omit resources already shown in Opportunities & Risks cards
            const oppRiskUrls = new Set(
              [...orData.opportunities, ...orData.risks]
                .map((r) => String((r.source_url as string) || "").trim())
                .filter(Boolean)
                .map((u) => u.toLowerCase().replace(/\/$/, "")),
            );
            const filtered = (src as Array<Record<string, unknown>>).filter((s) => {
              const url = String((s.source_url as string) || (s.url as string) || "").trim().toLowerCase().replace(/\/$/, "");
              return !url || !oppRiskUrls.has(url);
            });
            if (filtered.length === 0) return <p className="text-sm text-app-muted">No additional sources — all referenced in Opportunities & Risks.</p>;
            const list = filtered.slice(0, 10);
            return (
              <ul className="space-y-3 text-sm">
                {list.map((s, i) => {
                  const dataObtained = (s.data_obtained as string) || (s.data as string) || (s.description as string) || (s.title as string) || "";
                  const sourceName = (s.source_name as string) || (s.source as string) || (s.name as string) || "Source";
                  const url = (s.source_url as string) || (s.url as string) || "";
                  return (
                    <li key={i} className="leading-relaxed text-app-text">
                      {dataObtained && <span>{dataObtained} </span>}
                      {url ? (
                        <a href={url} target="_blank" rel="noreferrer" className="font-medium text-app-text underline hover:text-app-positive">
                          [{sourceName}]
                        </a>
                      ) : (
                        <span>[{sourceName}]</span>
                      )}
                    </li>
                  );
                })}
              </ul>
            );
          })()}
        </Card>
      </div>

      {/* Forecast + What-if 50-50 */}
      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Forecast — Revenue Growth vs Operating Margin (Hist FY + 5Y)">
          {combinedForecastData.length > 0 ? (
            <ResponsiveContainer width="100%" height={280}>
              <BarChart data={combinedForecastData} margin={{ top: 8, right: 16, left: 0, bottom: 0 }} barCategoryGap="18%" barGap={3}>
                <CartesianGrid stroke="#e2e8f0" strokeDasharray="3 3" vertical={false} opacity={0.6} />
                <XAxis dataKey="year" tick={{ fontSize: 11, fill: "#64748b" }} axisLine={false} tickLine={false} />
                <YAxis tick={{ fontSize: 11, fill: "#64748b" }} axisLine={false} tickLine={false} tickFormatter={(v: any) => `${v}%`} width={45} />
                <Tooltip contentStyle={{ borderRadius: 8, border: "1px solid #e2e8f0", boxShadow: "0 4px 12px rgba(0,0,0,0.08)" }} formatter={(v: any, n: any) => [`${Number(v).toFixed(1)}%`, n]} cursor={{ fill: "rgba(15,23,42,0.04)" }} />
                <Legend wrapperStyle={{ fontSize: 11, paddingTop: 8 }} iconType="square" />
                <Bar dataKey="histGrowth" name="Hist Growth %" fill="#64748b" radius={[6, 6, 0, 0]} barSize={10} />
                <Bar dataKey="fcstGrowth" name="Fcst Growth %" fill="#0f172a" fillOpacity={0.45} stroke="#0f172a" strokeWidth={1.2} strokeDasharray="5 3" radius={[6, 6, 0, 0]} barSize={10} />
                <Bar dataKey="histMargin" name="Hist Margin %" fill="#10b981" radius={[6, 6, 0, 0]} barSize={10} />
                <Bar dataKey="fcstMargin" name="Fcst Margin %" fill="#059669" fillOpacity={0.45} stroke="#059669" strokeWidth={1.2} strokeDasharray="5 3" radius={[6, 6, 0, 0]} barSize={10} />
              </BarChart>
            </ResponsiveContainer>
          ) : (
            <p className="text-sm text-app-muted">No forecast data (dcf_assumptions missing).</p>
          )}
        </Card>
        <Card
          title="Interactive What If Analysis"
          action={
            whatIfDirty ? (
              <button type="button" onClick={resetWhatIf} className="rounded border border-app-border px-2 py-1 text-xs hover:bg-app-sidebar">Reset</button>
            ) : undefined
          }
        >
          <div className="space-y-4">
            {(() => {
              const { rev: dcfRev, mar: dcfMar } = extractDcfRates(detail.dcf_assumptions);
              const dcfObj = detail.dcf_assumptions as unknown as Record<string, unknown> | null;
              const rawObj = (dcfObj?.raw as Record<string, unknown> | undefined) ?? null;
              const initRev = dcfRev || [0.12, 0.11, 0.10, 0.09, 0.08];
              const initMar = dcfMar || [0.20, 0.20, 0.20, 0.20, 0.20];
              const rev = overrides.revenue_growth_rates || initRev as [number,number,number,number,number];
              const mar = overrides.operating_margins || initMar as [number,number,number,number,number];
              const tMargin = overrides.terminal_margin ?? (rawObj?.terminal_margin as number | undefined) ?? (dcfObj?.terminal_margin as number | undefined) ?? 0.20;
              const tGrowth = overrides.terminal_growth_rate ?? (rawObj?.terminal_growth_rate as number | undefined) ?? (dcfObj?.terminal_growth_rate as number | undefined) ?? 0.04;
              const fcf = overrides.fcf_conversion_factor ?? (rawObj?.fcf_conversion_factor as number | undefined) ?? (dcfObj?.fcf_conversion_factor as number | undefined) ?? (dcfObj?.fcf_factor_used as number | undefined) ?? 0.65;
              const waccControl = overrides.wacc ?? waccVal ?? 0.12;
              const setEdit = (k: string, v: string) => setEditing((p) => ({ ...p, [k]: v }));
              const clearEdit = (k: string) => setEditing((p) => { const n = { ...p }; delete n[k]; return n; });
              const commitRev = (i: number) => {
                const raw = editing[`rev_${i}`];
                if (raw === undefined) return;
                const next = [...rev] as [number,number,number,number,number];
                next[i] = clampNum(Number(raw), 0.08) / 100;
                setOverrides((prev) => ({ ...prev, revenue_growth_rates: next }));
                setWhatIfDirty(true);
                clearEdit(`rev_${i}`);
              };
              const commitMar = (i: number) => {
                const raw = editing[`mar_${i}`];
                if (raw === undefined) return;
                const next = [...mar] as [number,number,number,number,number];
                next[i] = clampNum(Number(raw), 20) / 100;
                setOverrides((prev) => ({ ...prev, operating_margins: next }));
                setWhatIfDirty(true);
                clearEdit(`mar_${i}`);
              };
              const commitSimple = (key: keyof typeof overrides) => {
                const raw = editing[key as string];
                if (raw === undefined) return;
                const num = Number(raw);
                const isPct = key === "terminal_margin" || key === "terminal_growth_rate" || key === "wacc";
                const v = isPct ? clampNum(num, key === "terminal_margin" ? 20 : key === "terminal_growth_rate" ? 4 : 12) / 100 : clampNum(num, 0.65);
                setOverrides((prev) => ({ ...prev, [key]: v } as typeof prev));
                setWhatIfDirty(true);
                clearEdit(key as string);
              };
              const handleRecompute = () => {
                // Build full overrides from current UI values to avoid stale closure
                const nextOverrides: typeof overrides = {
                  revenue_growth_rates: [...rev] as [number,number,number,number,number],
                  operating_margins: [...mar] as [number,number,number,number,number],
                  terminal_margin: tMargin,
                  terminal_growth_rate: tGrowth,
                  fcf_conversion_factor: fcf,
                  wacc: waccControl,
                };
                // Only send if changed from original? Always send full set so engine sees all
                doRecompute(nextOverrides);
              };
              // computed figures for first row: bear / base / bull with fixed WACC + gap pct
              const bearFV = (recomputed as { bear_case_fair_value?: number; scenarios?: Array<{ scenario_name: string; fair_value: number }> } | null)?.bear_case_fair_value
                ?? (recomputed as { scenarios?: Array<{ scenario_name: string; fair_value: number }> } | null)?.scenarios?.find((s) => s.scenario_name === "pessimistic")?.fair_value
                ?? detail.bear_case_fair_value;
              const bullFV = (recomputed as { bull_case_fair_value?: number; scenarios?: Array<{ scenario_name: string; fair_value: number }> } | null)?.bull_case_fair_value
                ?? (recomputed as { scenarios?: Array<{ scenario_name: string; fair_value: number }> } | null)?.scenarios?.find((s) => s.scenario_name === "optimistic")?.fair_value
                ?? detail.bull_case_fair_value;
              const baseFVDisplay = (recomputed as { base_case_fair_value?: number } | null)?.base_case_fair_value ?? detail.base_case_fair_value;
              const gapDisplay = gapPct;
              return (
                <>
                  {/* First row: computed figures + Recompute button */}
                  <div className="flex flex-wrap items-center gap-3 rounded-md bg-app-sidebar p-3">
                    <div className="grid flex-1 grid-cols-2 gap-3 text-xs md:grid-cols-4">
                      <div><div className="text-app-muted">Bear</div><div className="font-semibold tabular-nums">{bearFV != null ? fmtMoneyWithCurrency(bearFV) : "—"}</div></div>
                      <div><div className="text-app-muted">Base</div><div className="font-semibold tabular-nums">{baseFVDisplay != null ? fmtMoneyWithCurrency(baseFVDisplay) : "—"}</div></div>
                      <div><div className="text-app-muted">Bull</div><div className="font-semibold tabular-nums">{bullFV != null ? fmtMoneyWithCurrency(bullFV) : "—"}</div></div>
                      <div><div className="text-app-muted">Gap %</div><div className={`font-semibold tabular-nums ${gapDisplay != null ? gapCls(gapDisplay) : ""}`}>{gapDisplay != null ? fmtPct(gapDisplay) : "—"}</div></div>
                    </div>
                    <Button onClick={handleRecompute} disabled={recomputeLoading} className="shrink-0">
                      {recomputeLoading ? <span className="inline-flex items-center gap-1"><Loader2 className="h-4 w-4 animate-spin" /> Recomputing…</span> : "Recompute"}
                    </Button>
                  </div>
                  {recomputeError && <div className="rounded bg-app-negative/10 px-2 py-1 text-xs text-app-negative">{recomputeError}</div>}
                  <div>
                    <div className="text-xs font-medium text-app-text">Revenue growth (5Y %)</div>
                    <div className="mt-1 grid grid-cols-5 gap-2">
                      {rev.map((r, i) => (
                        <input key={i} type="number" step={0.5} className="w-full rounded border border-app-border px-2 py-1 text-xs tabular-nums" value={editing[`rev_${i}`] ?? (r * 100).toFixed(1)} onChange={(e) => setEdit(`rev_${i}`, e.target.value)} onBlur={() => commitRev(i)} onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} />
                      ))}
                    </div>
                  </div>
                  <div>
                    <div className="text-xs font-medium text-app-text">Operating margins (5Y %)</div>
                    <div className="mt-1 grid grid-cols-5 gap-2">
                      {mar.map((m, i) => (
                        <input key={i} type="number" step={0.5} className="w-full rounded border border-app-border px-2 py-1 text-xs tabular-nums" value={editing[`mar_${i}`] ?? (m * 100).toFixed(1)} onChange={(e) => setEdit(`mar_${i}`, e.target.value)} onBlur={() => commitMar(i)} onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} />
                      ))}
                    </div>
                  </div>
                  <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                    <label className="text-xs">Terminal margin %<input type="number" step={0.5} className="mt-1 w-full rounded border border-app-border px-2 py-1 tabular-nums" value={editing["terminal_margin"] ?? (tMargin * 100).toFixed(1)} onChange={(e) => setEdit("terminal_margin", e.target.value)} onBlur={() => commitSimple("terminal_margin")} onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} /></label>
                    <label className="text-xs">Terminal growth %<input type="number" step={0.5} className="mt-1 w-full rounded border border-app-border px-2 py-1 tabular-nums" value={editing["terminal_growth_rate"] ?? (tGrowth * 100).toFixed(1)} onChange={(e) => setEdit("terminal_growth_rate", e.target.value)} onBlur={() => commitSimple("terminal_growth_rate")} onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} /></label>
                    <label className="text-xs">FCF factor<input type="number" step={0.05} className="mt-1 w-full rounded border border-app-border px-2 py-1 tabular-nums" value={editing["fcf_conversion_factor"] ?? fcf.toFixed(2)} onChange={(e) => setEdit("fcf_conversion_factor", e.target.value)} onBlur={() => commitSimple("fcf_conversion_factor")} onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} /></label>
                    <label className="text-xs">WACC %<input type="number" step={0.5} className="mt-1 w-full rounded border border-app-border px-2 py-1 tabular-nums" value={editing["wacc"] ?? (waccControl * 100).toFixed(2)} onChange={(e) => setEdit("wacc", e.target.value)} onBlur={() => commitSimple("wacc")} onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} /></label>
                  </div>
                  <p className="text-[11px] text-app-muted">Edit controls then press Recompute — recomputes locally only, never persisted. Original report remains unchanged. Reset to restore.</p>
                </>
              );
            })()}
          </div>
        </Card>
      </div>

      {/* Compare picker popup — max 4 */}
      {showComparePicker && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
          <div className="w-full max-w-lg rounded-lg border border-app-border bg-app-surface shadow-xl">
            <div className="flex items-center justify-between border-b border-app-border px-4 py-3">
              <h3 className="text-sm font-semibold text-app-text">Select reports to compare (max 4)</h3>
              <button type="button" onClick={() => setShowComparePicker(false)} className="rounded p-1 text-app-muted hover:text-app-text">✕</button>
            </div>
            <div className="max-h-[50vh] overflow-auto p-2">
              {versions
                .slice()
                .sort((a, b) => b.analysis_date.localeCompare(a.analysis_date))
                .map((v) => {
                  const checked = selectedCompareIds.includes(v.id);
                  const disabled = !checked && selectedCompareIds.length >= 4;
                  return (
                    <label key={v.id} className={`flex items-center gap-3 rounded-md px-3 py-2 hover:bg-app-sidebar ${disabled ? "opacity-50" : "cursor-pointer"}`}>
                      <input
                        type="checkbox"
                        checked={checked}
                        disabled={disabled}
                        onChange={(e) => {
                          if (e.target.checked) {
                            if (selectedCompareIds.length < 4) setSelectedCompareIds([...selectedCompareIds, v.id]);
                          } else {
                            setSelectedCompareIds(selectedCompareIds.filter((id) => id !== v.id));
                          }
                        }}
                        className="h-4 w-4 rounded border-app-border"
                      />
                      <span className="min-w-0 flex-1 text-sm">
                        <span className="font-medium text-app-text">{v.llm_model}</span>
                        <span className="ml-2 text-xs text-app-muted">{v.price_date} · {v.analysis_date.slice(0, 10)} {v.is_primary ? "★" : ""} {v.is_favorite && !v.is_primary ? "♥" : ""}</span>
                        {v.id === detail.id && <span className="ml-2 text-xs text-app-positive">(current)</span>}
                      </span>
                    </label>
                  );
                })}
            </div>
            <div className="flex items-center justify-between border-t border-app-border px-4 py-3">
              <span className="text-xs text-app-muted">{selectedCompareIds.length} selected {selectedCompareIds.length >= 4 ? "(max 4)" : ""}</span>
              <div className="flex gap-2">
                <button type="button" onClick={() => setShowComparePicker(false)} className="rounded-md border border-app-border px-3 py-1.5 text-sm hover:bg-app-sidebar">Cancel</button>
                <button
                  type="button"
                  disabled={selectedCompareIds.length < 2}
                  onClick={() => {
                    const ids = selectedCompareIds.slice(0, 4).join(",");
                    setShowComparePicker(false);
                    navigate(`/compare/${detail.ticker}?ids=${encodeURIComponent(ids)}&from=${encodeURIComponent(detail.id)}`);
                  }}
                  className="rounded-md bg-app-text px-3 py-1.5 text-sm font-medium text-app-surface hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Compare
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Delete picker popup — select models to delete */}
      {showDeletePicker && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
          <div className="w-full max-w-lg rounded-lg border border-app-border bg-app-surface shadow-xl">
            <div className="flex items-center justify-between border-b border-app-border px-4 py-3">
              <h3 className="text-sm font-semibold text-app-text">Select reports to delete</h3>
              <button type="button" onClick={() => setShowDeletePicker(false)} className="rounded p-1 text-app-muted hover:text-app-text">✕</button>
            </div>
            <div className="max-h-[50vh] overflow-auto p-2">
              {versions
                .slice()
                .sort((a, b) => b.analysis_date.localeCompare(a.analysis_date))
                .map((v) => {
                  const checked = selectedDeleteIds.includes(v.id);
                  return (
                    <label key={v.id} className="flex items-center gap-3 rounded-md px-3 py-2 hover:bg-app-sidebar cursor-pointer">
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={(e) => {
                          if (e.target.checked) {
                            setSelectedDeleteIds([...selectedDeleteIds, v.id]);
                          } else {
                            setSelectedDeleteIds(selectedDeleteIds.filter((id) => id !== v.id));
                          }
                        }}
                        className="h-4 w-4 rounded border-app-border"
                      />
                      <span className="min-w-0 flex-1 text-sm">
                        <span className="font-medium text-app-text">{v.llm_model}</span>
                        <span className="ml-2 text-xs text-app-muted">{v.price_date} {v.is_primary ? "★" : ""} {v.is_favorite && !v.is_primary ? "♥" : ""}</span>
                      </span>
                    </label>
                  );
                })}
            </div>
            <div className="flex items-center justify-between border-t border-app-border px-4 py-3">
              <span className="text-xs text-app-muted">{selectedDeleteIds.length} selected</span>
              <div className="flex gap-2">
                <button type="button" onClick={() => setShowDeletePicker(false)} className="rounded-md border border-app-border px-3 py-1.5 text-sm hover:bg-app-sidebar">Cancel</button>
                <button
                  type="button"
                  disabled={selectedDeleteIds.length === 0}
                  onClick={handleDeleteConfirm}
                  className="rounded-md bg-app-negative px-3 py-1.5 text-sm font-medium text-white hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  OK
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Final confirm popup — Are you sure? */}
      {showDeleteFinalConfirm && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4">
          <div className="w-full max-w-sm rounded-lg border border-app-border bg-app-surface p-4 shadow-xl">
            <h3 className="text-sm font-semibold text-app-text">Are you sure?</h3>
            <p className="mt-2 text-sm text-app-muted">This will permanently delete the selected report(s). This action cannot be undone.</p>
            <div className="mt-4 flex justify-end gap-2">
              <button type="button" onClick={() => { setShowDeleteFinalConfirm(false); setPendingDeleteIds([]); }} className="rounded-md border border-app-border px-3 py-1.5 text-sm hover:bg-app-sidebar">Cancel</button>
              <button type="button" onClick={handleDeleteFinal} className="rounded-md bg-app-negative px-3 py-1.5 text-sm font-medium text-white hover:opacity-90">Delete</button>
            </div>
          </div>
        </div>
      )}

      {/* Watchlist picker */}
      {showWatchlistPicker && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
          <div className="w-full max-w-md rounded-lg border border-app-border bg-app-surface p-4 shadow-xl">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold text-app-text">Add to watchlist — {detail.ticker}</h3>
              <button type="button" onClick={() => setShowWatchlistPicker(false)} className="rounded p-1 hover:bg-app-sidebar">✕</button>
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
              <Button variant="ghost" onClick={() => setShowWatchlistPicker(false)}>Close</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
