import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Search } from "lucide-react";
import { listReports, type ReportDetail } from "../../lib/api";
import {
  Card,
  ErrorNote,
  Loading,
  Table,
  VerdictPill,
} from "../../components/ui";
import { fmtPct, fmtNum, gapCls, inputCls } from "../../lib/format";

export default function MyReports() {
  const navigate = useNavigate();
  const [rows, setRows] = useState<ReportDetail[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const load = () => {
    setErr(null);
    listReports()
      .then(setRows)
      .catch(() => setErr("Could not load reports (run via `tauri dev`)."));
  };

  useEffect(() => { load(); }, []);

  const filtered = useMemo(() => {
    if (!rows) return rows;
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter(
      (r) =>
        r.ticker.toLowerCase().includes(q) ||
        (r.company_name || r.company || "").toLowerCase().includes(q) ||
        (r.sector || "").toLowerCase().includes(q) ||
        (r.country_code || r.country || "").toLowerCase().includes(q),
    );
  }, [rows, query]);

  if (err) return <ErrorNote message={err} />;
  if (!rows || !filtered) return <Loading />;

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <div className="relative w-72">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-app-muted" />
          <input
            className={`${inputCls} pl-9`}
            placeholder="Filter by ticker or company…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <span className="text-xs text-app-muted">
          {filtered.length} of {rows.length} reports
        </span>
      </div>

      {rows.length === 0 ? (
        <Card>
          <div className="py-10 text-center">
            <p className="text-sm text-app-muted">No reports yet.</p>
            <p className="mt-1 text-xs text-app-muted">Run an analysis from New Analysis to create your first report.</p>
          </div>
        </Card>
      ) : (
        <Card>
          <Table
            headers={["", "Company", "Sector", "Verdict", "Fair Value", "Gap", "Confidence", "Current Price", ""]}
          >
            {filtered.map((r) => {
              const company = r.company_name || r.company || r.ticker;
              const sector = r.sector || "—";
              const countryCode = r.country_code || r.country || "—";
              const verdict = r.valuation_verdict || r.verdict || "—";
              const fv = r.weighted_fair_value;
              const cp = r.current_price;
              // Prefer live recomputed gap (FV-CMP)/CMP; fallback to stored (handles legacy sign via verdict check)
              let gap: number | null = null;
              if (fv != null && cp != null && Math.abs(cp) > 1e-9) {
                gap = ((fv as number) - (cp as number)) / (cp as number) * 100;
              } else {
                const gapRaw = r.valuation_gap_pct ?? r.gap_pct;
                gap = gapRaw != null && Math.abs(gapRaw) < 2 ? gapRaw * 100 : gapRaw as number | null;
                // Legacy reports had inverted sign (CMP-FV)/FV negative for undervalued — fix via verdict
                if (gap != null && verdict.toLowerCase().replace(/_/g, " ").trim() === "undervalued" && gap < 0) gap = -gap;
                if (gap != null && verdict.toLowerCase().replace(/_/g, " ").trim() === "overvalued" && gap > 0) gap = -gap;
              }
              const conf = r.confidence_score ?? r.confidence;
              return (
                <tr
                  key={r.id}
                  onClick={() => navigate(`/report/${r.ticker}?version=${encodeURIComponent(r.id)}&from=${encodeURIComponent("/reports")}`)}
                  className="cursor-pointer transition hover:bg-app-sidebar"
                >
                  <td className="px-3 py-2.5 text-xs font-medium tabular-nums text-app-muted">{countryCode}</td>
                  <td className="px-3 py-2.5">
                    <div className="font-medium leading-tight text-app-text">{company}</div>
                    <div className="text-xs tabular-nums text-app-muted">{r.ticker}</div>
                  </td>
                  <td className="px-3 py-2.5 text-xs text-app-muted">{sector}</td>
                  <td className="px-3 py-2.5">
                    <VerdictPill verdict={verdict} />
                  </td>
                  <td className="px-3 py-2.5 tabular-nums text-app-text">
                    {fv != null ? fmtNum(fv) : "—"}
                  </td>
                  <td className={`px-3 py-2.5 tabular-nums ${gap != null ? gapCls(gap) : "text-app-muted"}`}>
                    {gap != null ? fmtPct(gap) : "—"}
                  </td>
                  <td className="px-3 py-2.5 tabular-nums text-app-muted">
                    {conf != null ? `${Math.round(conf)}%` : "—"}
                  </td>
                  <td className="px-3 py-2.5 tabular-nums text-app-text">
                    {cp != null ? fmtNum(cp) : "—"}
                  </td>
                  <td className="px-3 py-2.5">
                    <button
                      type="button"
                      className="rounded-md border border-app-border px-3 py-1 text-xs font-medium text-app-text hover:bg-app-sidebar"
                      onClick={(e) => {
                        e.stopPropagation();
                        navigate(`/report/${r.ticker}?version=${encodeURIComponent(r.id)}&from=${encodeURIComponent("/reports")}`);
                      }}
                    >
                      View
                    </button>
                  </td>
                </tr>
              );
            })}
            {filtered.length === 0 && (
              <tr>
                <td colSpan={9} className="px-3 py-6 text-center text-sm text-app-muted">
                  No reports match “{query}”.
                </td>
              </tr>
            )}
          </Table>
        </Card>
      )}
    </div>
  );
}
