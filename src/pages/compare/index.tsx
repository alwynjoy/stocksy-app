import { useEffect, useState } from "react";
import { useParams, useSearchParams, useNavigate } from "react-router-dom";
import { ArrowLeft } from "lucide-react";
import { getReportsByIds, type ReportDetail } from "../../lib/api";
import { Card, Loading, ErrorNote, VerdictPill } from "../../components/ui";
import { fmtMoney, fmtPct } from "../../lib/format";

function fmtVal(v: number | null | undefined, currency: string): string {
  if (v == null || !Number.isFinite(v)) return "—";
  try {
    return new Intl.NumberFormat("en-US", { style: "currency", currency: currency || "USD", maximumFractionDigits: 2 }).format(v);
  } catch { return fmtMoney(v); }
}

function extractDcf(reports: ReportDetail[]) {
  return reports.map((r) => {
    const obj = r.dcf_assumptions as unknown as Record<string, unknown> | null;
    const raw = (obj?.raw as Record<string, unknown> | undefined) ?? obj;
    const rev = (raw?.revenue_growth_rates as number[] | undefined) ?? (obj?.revenue_growth_rates as number[] | undefined) ?? null;
    const mar = (raw?.operating_margins as number[] | undefined) ?? (obj?.operating_margins as number[] | undefined) ?? null;
    const tMargin = (raw?.terminal_margin as number | undefined) ?? (obj?.terminal_margin as number | undefined) ?? r.terminal_margin;
    const tGrowth = (raw?.terminal_growth_rate as number | undefined) ?? (obj?.terminal_growth_rate as number | undefined) ?? r.terminal_growth_rate;
    const fcf = (raw?.fcf_conversion_factor as number | undefined) ?? (obj?.fcf_conversion_factor as number | undefined) ?? (obj?.fcf_factor_used as number | undefined) ?? null;
    return { rev, mar, tMargin, tGrowth, fcf };
  });
}

export default function Compare() {
  const { ticker = "" } = useParams<{ ticker: string }>();
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const idsParam = searchParams.get("ids") || "";
  const from = searchParams.get("from");
  const ids = idsParam.split(",").map((s) => s.trim()).filter(Boolean).slice(0, 4);

  const [reports, setReports] = useState<ReportDetail[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (ids.length < 2) {
      setErr("Select at least 2 reports to compare (max 4).");
      setLoading(false);
      return;
    }
    setLoading(true);
    setErr(null);
    getReportsByIds(ids)
      .then((rows) => {
        if (rows.length < 2) setErr("Could not load selected reports.");
        else setReports(rows);
      })
      .catch((e) => setErr(typeof e === "string" ? e : "Failed to load reports"))
      .finally(() => setLoading(false));
  }, [idsParam]);

  const handleBack = () => {
    if (from) {
      navigate(`/report/${ticker}?version=${encodeURIComponent(from)}`);
    } else if (ids.length > 0) {
      navigate(`/report/${ticker}?version=${encodeURIComponent(ids[0])}`);
    } else {
      navigate(`/report/${ticker}`);
    }
  };

  if (loading) return <Loading label="Loading comparison…" />;
  if (err) return (
    <div className="space-y-4">
      <button type="button" onClick={handleBack} className="inline-flex items-center gap-1 rounded-md border border-app-border px-3 py-2 text-sm hover:bg-app-sidebar">
        <ArrowLeft className="h-4 w-4" /> Back
      </button>
      <ErrorNote message={err} />
    </div>
  );
  if (!reports || reports.length < 2) return (
    <div className="space-y-4">
      <button type="button" onClick={handleBack} className="inline-flex items-center gap-1 rounded-md border border-app-border px-3 py-2 text-sm hover:bg-app-sidebar">
        <ArrowLeft className="h-4 w-4" /> Back
      </button>
      <ErrorNote message="Not enough reports to compare." />
    </div>
  );

  const dcfInfos = extractDcf(reports);
  const currencies = reports.map((r) => r.currency_code || r.currency || "USD");

  const rows: Array<{ label: string; get: (r: ReportDetail, idx: number) => string | React.ReactNode }> = [
    { label: "Model", get: (r) => `${r.llm_provider}/${r.llm_model}` },
    { label: "Price Date", get: (r) => r.price_date || "—" },
    { label: "Analysis Date", get: (r) => (r.analysis_date || "").slice(0, 10) },
    { label: "Current Price", get: (r, i) => fmtVal(r.current_price, currencies[i]) },
    { label: "Weighted FV", get: (r, i) => <span className="font-semibold">{fmtVal(r.weighted_fair_value, currencies[i])}</span> },
    { label: "Base FV", get: (r, i) => fmtVal(r.base_case_fair_value, currencies[i]) },
    { label: "Optimistic FV", get: (r, i) => fmtVal(r.bull_case_fair_value ?? (r.scenarios as unknown as Array<{ scenario_name: string; fair_value: number }> | null)?.find(s => s.scenario_name === "optimistic")?.fair_value ?? null, currencies[i]) },
    { label: "Pessimistic FV", get: (r, i) => fmtVal(r.bear_case_fair_value ?? (r.scenarios as unknown as Array<{ scenario_name: string; fair_value: number }> | null)?.find(s => s.scenario_name === "pessimistic")?.fair_value ?? null, currencies[i]) },
    { label: "Gap %", get: (r) => {
      const g = r.valuation_gap_pct ?? r.gap_pct;
      if (g == null) return "—";
      const pct = Math.abs(g) < 2 ? g * 100 : g;
      return fmtPct(pct);
    } },
    { label: "Verdict", get: (r) => <VerdictPill verdict={r.valuation_verdict || r.verdict || ""} /> },
    { label: "WACC", get: (r) => r.wacc != null ? fmtPct(r.wacc * 100) : "—" },
    { label: "Terminal Growth", get: (r) => r.terminal_growth_rate != null ? fmtPct(r.terminal_growth_rate * 100) : "—" },
    { label: "Terminal Margin", get: (r) => r.terminal_margin != null ? fmtPct(r.terminal_margin * 100) : "—" },
    { label: "FCF Factor", get: (_r, i) => dcfInfos[i].fcf != null ? Number(dcfInfos[i].fcf).toFixed(2) : "—" },
    { label: "Revenue Y1", get: (_r, i) => dcfInfos[i].rev?.[0] != null ? fmtPct(dcfInfos[i].rev![0] * 100) : "—" },
    { label: "Revenue Y2", get: (_r, i) => dcfInfos[i].rev?.[1] != null ? fmtPct(dcfInfos[i].rev![1] * 100) : "—" },
    { label: "Revenue Y3", get: (_r, i) => dcfInfos[i].rev?.[2] != null ? fmtPct(dcfInfos[i].rev![2] * 100) : "—" },
    { label: "Revenue Y4", get: (_r, i) => dcfInfos[i].rev?.[3] != null ? fmtPct(dcfInfos[i].rev![3] * 100) : "—" },
    { label: "Revenue Y5", get: (_r, i) => dcfInfos[i].rev?.[4] != null ? fmtPct(dcfInfos[i].rev![4] * 100) : "—" },
    { label: "OPM Y1", get: (_r, i) => dcfInfos[i].mar?.[0] != null ? fmtPct(dcfInfos[i].mar![0] * 100) : "—" },
    { label: "OPM Y2", get: (_r, i) => dcfInfos[i].mar?.[1] != null ? fmtPct(dcfInfos[i].mar![1] * 100) : "—" },
    { label: "OPM Y3", get: (_r, i) => dcfInfos[i].mar?.[2] != null ? fmtPct(dcfInfos[i].mar![2] * 100) : "—" },
    { label: "OPM Y4", get: (_r, i) => dcfInfos[i].mar?.[3] != null ? fmtPct(dcfInfos[i].mar![3] * 100) : "—" },
    { label: "OPM Y5", get: (_r, i) => dcfInfos[i].mar?.[4] != null ? fmtPct(dcfInfos[i].mar![4] * 100) : "—" },
    { label: "Confidence", get: (r) => r.confidence_score != null ? `${Math.round(r.confidence_score as number)} (${r.confidence_level || ""})` : "—" },
  ];

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <button type="button" onClick={handleBack} className="inline-flex items-center gap-1 rounded-md border border-app-border px-3 py-2 text-sm hover:bg-app-sidebar">
          <ArrowLeft className="h-4 w-4" /> Back to Report
        </button>
        <h1 className="text-xl font-semibold text-app-text">{ticker} — Compare Reports</h1>
        <span className="text-sm text-app-muted">({reports.length} selected)</span>
      </div>

      <Card>
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b border-app-border text-xs uppercase tracking-wide text-app-muted">
                <th className="px-3 py-2 font-medium bg-app-sidebar sticky left-0">Parameter</th>
                {reports.map((r) => (
                  <th key={r.id} className="px-3 py-2 font-medium whitespace-nowrap">
                    <div className="flex flex-col">
                      <span>{r.llm_model}</span>
                      <span className="text-[11px] font-normal text-app-muted">{r.price_date} · {r.analysis_date.slice(0, 10)}</span>
                    </div>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-app-border">
              {rows.map((row) => {
                const highlight: Record<string, string> = {
                  "Weighted FV": "bg-sky-50",
                  "Base FV": "bg-indigo-50/70",
                  "Optimistic FV": "bg-emerald-50",
                  "Pessimistic FV": "bg-rose-50",
                  "WACC": "bg-amber-50",
                  "Terminal Growth": "bg-teal-50",
                  "Revenue Y1": "bg-violet-50",
                  "OPM Y1": "bg-orange-50",
                };
                const rowBg = highlight[row.label] || "bg-white";
                const isHighlighted = row.label in highlight;
                return (
                  <tr key={row.label} className={`${rowBg} hover:brightness-95`}>
                    <td className={`px-3 py-2.5 text-xs font-medium sticky left-0 whitespace-nowrap border-r border-app-border ${isHighlighted ? rowBg : "bg-app-sidebar"} ${isHighlighted ? "text-app-text" : "text-app-muted"}`}>{row.label}</td>
                    {reports.map((r, idx) => (
                      <td key={r.id} className={`px-3 py-2.5 tabular-nums whitespace-nowrap ${rowBg} ${isHighlighted ? "font-medium" : ""}`}>
                        {row.get(r, idx) as React.ReactNode}
                      </td>
                    ))}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </Card>

      <div className="text-xs text-app-muted">
        Comparison shows weighted/base/optimistic/pessimistic values, DCF assumption parameters (terminal, FCF factor), and revenue/OPM forecast (Y1-Y5) per model/date.
      </div>
    </div>
  );
}
