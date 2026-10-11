import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, Cell } from "recharts";
import { getDashboardStats, stubDashboardStats, type DashboardStats } from "../../lib/api";
import { Card, ErrorNote, Loading, StatCard, Table, VerdictPill } from "../../components/ui";
import { fmtPct, fmtNum, gapCls } from "../../lib/format";

const BAR_COLORS = ["#0f172a", "#334155", "#475569", "#64748b", "#94a3b8"];

export default function Dashboard() {
  const navigate = useNavigate();
  const [data, setData] = useState<DashboardStats | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    getDashboardStats()
      .then(setData)
      .catch(() => {
        // fallback to stub for non-Tauri dev
        stubDashboardStats()
          .then(setData)
          .catch(() => setErr("Could not load dashboard (run via `tauri dev`)."));
      });
  }, []);

  if (err) return <ErrorNote message={err} />;
  if (!data) return <Loading />;

  const normVerdict = (v: string) => v.toLowerCase().replace(/_/g, " ").trim();
  const undervalued = data.recent_analyses.filter((r) => normVerdict((r.valuation_verdict || r.verdict || "")) === "undervalued").length;

  const empty = data.total_reports === 0 && data.recent_analyses.length === 0;

  const providerStats = (data as DashboardStats & { provider_stats?: DashboardStats["provider_stats"] }).provider_stats || [];
  const totalTokens = (data as DashboardStats & { total_tokens?: number }).total_tokens ?? 0;
  const totalEst = (data as DashboardStats & { total_estimated_usd?: number }).total_estimated_usd ?? 0;
  const totalInput = (data as DashboardStats & { total_input_tokens?: number }).total_input_tokens ?? 0;
  const totalOutput = (data as DashboardStats & { total_output_tokens?: number }).total_output_tokens ?? 0;
  const fmtTokens = (n: number) => new Intl.NumberFormat("en-US").format(n);
  const fmtUsd = (n: number) => new Intl.NumberFormat("en-US", { style: "currency", currency: "USD", maximumFractionDigits: 2 }).format(n);

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-3 gap-4">
        <StatCard label="Total reports" value={String(data.total_reports)} sub="completed primary reports" />
        <StatCard label="Undervalued" value={String(undervalued)} sub="of recent analyses" accent="text-app-positive" />
        <StatCard label="Watchlists" value={String(data.watchlist_summary.length)} sub="tracked portfolios" />
      </div>
      {(totalTokens > 0 || providerStats.length > 0) && (
        <div className="grid grid-cols-3 gap-4">
          <StatCard label="Total tokens" value={fmtTokens(totalTokens)} sub={`${fmtTokens(totalInput)} in · ${fmtTokens(totalOutput)} out`} />
          <StatCard label="Est. cost" value={fmtUsd(totalEst)} sub="$ only · provider avg low" />
          <StatCard label="Avg per report" value={data.total_reports > 0 ? fmtTokens(Math.round(totalTokens / data.total_reports)) : "—"} sub="tokens / report" />
        </div>
      )}

      {empty && (
        <Card>
          <div className="py-10 text-center">
            <p className="text-sm text-app-muted">No reports yet.</p>
            <p className="mt-1 text-xs text-app-muted">Run an analysis from New Analysis to populate the dashboard.</p>
            <button
              type="button"
              onClick={() => navigate("/new-analysis")}
              className="mt-3 rounded-md bg-app-text px-4 py-2 text-xs font-medium text-app-surface hover:opacity-90"
            >
              New analysis
            </button>
          </div>
        </Card>
      )}

      {!empty && (
        <>
          {(data.reports_per_model.length > 0 || data.reports_per_country.length > 0 || providerStats.length > 0) && (
            <div className="grid grid-cols-3 gap-4">
              <Card title="Reports per model">
                {data.reports_per_model.length === 0 ? (
                  <p className="text-sm text-app-muted">No data.</p>
                ) : (
                  <div style={{ width: "100%", height: 180 }}>
                    <ResponsiveContainer>
                      <BarChart data={data.reports_per_model} layout="vertical" margin={{ left: 20, right: 16 }}>
                        <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" />
                        <XAxis type="number" allowDecimals={false} stroke="#64748b" fontSize={11} />
                        <YAxis type="category" dataKey="model" width={110} stroke="#64748b" fontSize={11} tick={{ fill: "#64748b" }} />
                        <Tooltip
                          contentStyle={{ borderRadius: 8, border: "1px solid #e2e8f0" }}
                          labelStyle={{ color: "#0f172a" }}
                        />
                        <Bar dataKey="count" radius={[0, 8, 8, 0]} barSize={18}>
                          {data.reports_per_model.map((_e, i) => (
                            <Cell key={i} fill={BAR_COLORS[i % BAR_COLORS.length]} />
                          ))}
                        </Bar>
                      </BarChart>
                    </ResponsiveContainer>
                  </div>
                )}
              </Card>
              <Card title="Reports per country">
                {data.reports_per_country.length === 0 ? (
                  <p className="text-sm text-app-muted">No data.</p>
                ) : (
                  <div style={{ width: "100%", height: 180 }}>
                    <ResponsiveContainer>
                      <BarChart data={data.reports_per_country} margin={{ top: 4, right: 16, left: 0, bottom: 0 }}>
                        <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" vertical={false} />
                        <XAxis dataKey="country" stroke="#64748b" fontSize={12} tick={{ fill: "#0f172a" }} />
                        <YAxis allowDecimals={false} stroke="#64748b" fontSize={11} />
                        <Tooltip contentStyle={{ borderRadius: 8, border: "1px solid #e2e8f0" }} />
                        <Bar dataKey="count" radius={[8, 8, 0, 0]} barSize={32}>
                          {data.reports_per_country.map((_e, i) => (
                            <Cell key={i} fill={BAR_COLORS[i % BAR_COLORS.length]} />
                          ))}
                        </Bar>
                      </BarChart>
                    </ResponsiveContainer>
                  </div>
                )}
              </Card>
              <Card title="Tokens & Est. cost per provider">
                {providerStats.length === 0 ? (
                  <p className="text-sm text-app-muted">No token data yet.</p>
                ) : (
                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-xs">
                      <thead>
                        <tr className="border-b border-app-border text-[11px] uppercase tracking-wide text-app-muted">
                          <th className="px-2 py-1.5 font-medium">Provider</th>
                          <th className="px-2 py-1.5 font-medium text-right">#</th>
                          <th className="px-2 py-1.5 font-medium text-right">Input</th>
                          <th className="px-2 py-1.5 font-medium text-right">Output</th>
                          <th className="px-2 py-1.5 font-medium text-right">Total</th>
                          <th className="px-2 py-1.5 font-medium text-right">Est. $</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-app-border">
                        {providerStats.map((p) => (
                          <tr key={p.provider} className="hover:bg-app-sidebar/50">
                            <td className="px-2 py-1.5 font-medium text-app-text capitalize">{p.provider}</td>
                            <td className="px-2 py-1.5 text-right tabular-nums">{p.count}</td>
                            <td className="px-2 py-1.5 text-right tabular-nums text-app-muted">{fmtTokens(p.input_tokens)}</td>
                            <td className="px-2 py-1.5 text-right tabular-nums text-app-muted">{fmtTokens(p.output_tokens)}</td>
                            <td className="px-2 py-1.5 text-right tabular-nums font-medium">{fmtTokens(p.total_tokens)}</td>
                            <td className="px-2 py-1.5 text-right tabular-nums font-medium">{fmtUsd(p.estimated_usd)}</td>
                          </tr>
                        ))}
                        <tr className="border-t-2 border-app-border font-semibold">
                          <td className="px-2 py-1.5">Total</td>
                          <td className="px-2 py-1.5 text-right">{data.total_reports}</td>
                          <td className="px-2 py-1.5 text-right tabular-nums">{fmtTokens(totalInput)}</td>
                          <td className="px-2 py-1.5 text-right tabular-nums">{fmtTokens(totalOutput)}</td>
                          <td className="px-2 py-1.5 text-right tabular-nums">{fmtTokens(totalTokens)}</td>
                          <td className="px-2 py-1.5 text-right tabular-nums">{fmtUsd(totalEst)}</td>
                        </tr>
                      </tbody>
                    </table>
                    <p className="mt-2 text-[11px] text-app-muted">$ only · provider avg low (anthropic 3/15, openai 2.5/10, deepseek 0.55/2.19, kimi 1/4, custom low)</p>
                  </div>
                )}
              </Card>
            </div>
          )}

          <Card
            title="Recent analyses"
            action={
              <button
                type="button"
                onClick={() => navigate("/reports")}
                className="text-xs font-medium text-app-muted hover:text-app-text"
              >
                View all
              </button>
            }
          >
            {data.recent_analyses.length === 0 ? (
              <p className="text-sm text-app-muted">No recent analyses.</p>
            ) : (
              <Table headers={["Ticker", "Company", "Price date", "Model", "Verdict", "Fair value", "Gap", "Confidence"]}>
                {data.recent_analyses.map((r) => {
                  const v = r.valuation_verdict || r.verdict || "—";
                  const fv = r.weighted_fair_value;
                  let gap: number | null = r.valuation_gap_pct ?? r.gap_pct ?? null;
                  if (gap != null && Math.abs(gap) < 2 && gap !== 0) {
                    // already decimal; keep
                  }
                  // gap stored as decimal (0.126). fmtPct expects percent value; multiply
                  const gapDisp = gap != null ? gap * 100 : null;
                  return (
                    <tr
                      key={r.id}
                      onClick={() => navigate(`/report/${r.ticker}?version=${encodeURIComponent(r.id)}&from=${encodeURIComponent("/")}`)}
                      className="cursor-pointer transition hover:bg-app-sidebar"
                    >
                      <td className="px-3 py-2.5 font-medium text-app-text">{r.ticker}</td>
                      <td className="px-3 py-2.5 text-app-muted">{r.company_name || r.company}</td>
                      <td className="px-3 py-2.5 text-app-muted">{r.price_date}</td>
                      <td className="px-3 py-2.5 text-xs text-app-muted">{r.llm_model || r.model}</td>
                      <td className="px-3 py-2.5">
                        <VerdictPill verdict={v} />
                      </td>
                      <td className="px-3 py-2.5 tabular-nums text-app-text">{fv != null ? fmtNum(fv) : "—"}</td>
                      <td className={`px-3 py-2.5 tabular-nums ${gapDisp != null ? gapCls(gapDisp) : ""}`}>
                        {gapDisp != null ? fmtPct(gapDisp) : "—"}
                      </td>
                      <td className="px-3 py-2.5 tabular-nums text-app-muted">
                        {r.confidence_score ?? r.confidence ?? "—"}
                        {r.confidence_score != null || r.confidence != null ? "%" : ""}
                      </td>
                    </tr>
                  );
                })}
              </Table>
            )}
          </Card>

          {data.watchlist_summary.length > 0 && (
            <div className="grid grid-cols-2 gap-4">
              {data.watchlist_summary.map((w) => (
                <Card key={w.name} title={w.name}>
                  <div className="grid grid-cols-3 gap-3 text-center">
                    <div>
                      <div className="text-sm text-app-muted">Tickers</div>
                      <div className="text-xl font-semibold tabular-nums text-app-text">{w.tickers}</div>
                    </div>
                    <div>
                      <div className="text-sm text-app-muted">Gainers</div>
                      <div className="text-xl font-semibold tabular-nums text-app-positive">{w.gainers}</div>
                    </div>
                    <div>
                      <div className="text-sm text-app-muted">Gap</div>
                      <div className={`text-xl font-semibold tabular-nums ${gapCls(w.latest_gap_pct)}`}>
                        {fmtPct(w.latest_gap_pct)}
                      </div>
                    </div>
                  </div>
                </Card>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
