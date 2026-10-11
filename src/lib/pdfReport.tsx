import { Document, Page, Text, View, StyleSheet, Image, Link } from "@react-pdf/renderer";
import type { ReportDetail } from "./api";

const styles = StyleSheet.create({
  page: { padding: 24, fontSize: 9, fontFamily: "Helvetica", color: "#0f172a", backgroundColor: "#ffffff" },
  header: { flexDirection: "row", justifyContent: "space-between", alignItems: "center", borderBottomWidth: 1, borderBottomColor: "#e2e8f0", paddingBottom: 8, marginBottom: 12 },
  logo: { width: 32, height: 32 },
  title: { fontSize: 16, fontWeight: 700, color: "#0f172a" },
  subtitle: { fontSize: 8, color: "#64748b", marginTop: 2 },
  sectionTitle: { fontSize: 11, fontWeight: 700, color: "#0f172a", marginTop: 12, marginBottom: 6, borderBottomWidth: 1, borderBottomColor: "#e2e8f0", paddingBottom: 4 },
  row: { flexDirection: "row", gap: 8 },
  col: { flex: 1 },
  card: { borderWidth: 1, borderColor: "#e2e8f0", borderRadius: 4, padding: 6, backgroundColor: "#f8fafc" },
  metricLabel: { fontSize: 7, color: "#64748b", textTransform: "uppercase" },
  metricValue: { fontSize: 11, fontWeight: 700, marginTop: 2 },
  badge: { fontSize: 8, color: "#fff", backgroundColor: "#0f172a", paddingHorizontal: 6, paddingVertical: 2, borderRadius: 4, alignSelf: "flex-start" },
  text: { fontSize: 8, lineHeight: 1.4, color: "#334155" },
  bullet: { flexDirection: "row", gap: 4, marginBottom: 2 },
  bulletDot: { width: 4, height: 4, backgroundColor: "#0f172a", borderRadius: 2, marginTop: 4 },
  table: { borderWidth: 1, borderColor: "#e2e8f0", borderRadius: 4, overflow: "hidden" },
  tableHeader: { flexDirection: "row", backgroundColor: "#0f172a", paddingVertical: 4, paddingHorizontal: 4 },
  tableHeaderCell: { color: "#fff", fontSize: 7, fontWeight: 700, flex: 1, textAlign: "center" },
  tableRow: { flexDirection: "row", borderTopWidth: 1, borderTopColor: "#e2e8f0", paddingVertical: 4, paddingHorizontal: 4 },
  tableCell: { fontSize: 7, flex: 1, textAlign: "center", color: "#0f172a" },
  tableCellLabel: { fontSize: 7, flex: 1, textAlign: "left", color: "#334155" },
  footer: { marginTop: 12, borderTopWidth: 1, borderTopColor: "#e2e8f0", paddingTop: 6, flexDirection: "row", justifyContent: "space-between" },
  link: { fontSize: 7, color: "#0f172a", textDecoration: "underline" },
});

function safeText(v: unknown): string {
  if (v == null) return "";
  if (typeof v === "string") return v;
  return String(v);
}

function fmt(v: number | null | undefined, suffix = ""): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return v.toFixed(2) + suffix;
}

function getCurrency(detail: ReportDetail): string {
  return (detail.currency_code || (detail as unknown as { currency?: string }).currency || "USD").toUpperCase();
}

function formatCurrency(v: number | null | undefined, currency: string): string {
  if (v == null || !Number.isFinite(v)) return "—";
  try {
    return new Intl.NumberFormat("en-US", { style: "currency", currency, maximumFractionDigits: 2 }).format(v);
  } catch {
    const map: Record<string, string> = { INR: "₹", USD: "$", GBP: "£", AUD: "A$" };
    const sym = map[currency] || currency + " ";
    return sym + Number(v).toFixed(2);
  }
}

function extractBusinessDescription(detail: ReportDetail): string {
  const raw = detail.ai_business_context || (detail as unknown as { narrative?: string }).narrative || "";
  if (!raw) return "";
  if (typeof raw === "string" && raw.trim().startsWith("{")) {
    try {
      const p = JSON.parse(raw);
      if (p && typeof p === "object" && "business_description" in p) return String((p as Record<string, unknown>).business_description || "");
    } catch {}
  }
  return safeText(raw);
}

function cleanBulletLine(s: string): string {
  // Remove leading bullet markers like "•", "-", "*", "·", "–", numbers "1.", "1)"
  return s.replace(/^[\s•\-\*·–—]+/, "").replace(/^\d+[\.\)]\s+/, "").trim();
}

function normalizeHistorical(text: unknown): string[] {
  if (!text) return [];
  const s = safeText(text);
  // Handle both newline-separated and "•" inline bullets saved in DB
  const lines = s.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
  const out: string[] = [];
  for (const line of lines) {
    if (line.includes("•")) {
      const parts = line.split("•").map((p) => cleanBulletLine(p)).filter(Boolean);
      for (const p of parts) out.push(p);
    } else {
      out.push(cleanBulletLine(line));
    }
  }
  return out.filter(Boolean).slice(0, 8);
}

function extractInvestmentVerdict(detail: ReportDetail): string {
  const raw = detail.ai_investment_perspective;
  if (raw == null) return "";
  let text = "";
  if (typeof raw === "string") {
    const trimmed = raw.trim();
    if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
      try {
        const obj = JSON.parse(trimmed);
        if (obj && typeof obj === "object" && !Array.isArray(obj) && "verdict" in obj) {
          return String((obj as Record<string, unknown>).verdict || "").trim();
        }
        // If array or other, fallback to raw
      } catch {}
    }
    text = trimmed;
  } else if (typeof raw === "object") {
    const obj = raw as Record<string, unknown>;
    if (obj && "verdict" in obj) return String(obj.verdict || "").trim();
    text = safeText(raw);
  }
  // If text still looks like JSON dump with verdict inside, try to extract verdict field via regex
  if (text.includes('"verdict"')) {
    try {
      const obj = JSON.parse(text);
      if (obj && typeof obj === "object" && "verdict" in obj) return String((obj as Record<string, unknown>).verdict || "").trim();
    } catch {}
    const m = text.match(/"verdict"\s*:\s*"([^"]+)"/);
    if (m) return m[1].trim();
  }
  return text;
}

export function ReportPdfDoc({ detail, historicalData, fcfData, forecastData, logoSrc }: {
  detail: ReportDetail;
  historicalData: Array<{ year: string; revenue: number | null; opIncome: number | null; netIncome: number | null }>;
  fcfData: Array<Record<string, unknown>>;
  forecastData: Array<Record<string, unknown>>;
  logoSrc?: string | null;
}) {
  const scenarios = (() => {
    const src = detail.scenarios as unknown;
    if (Array.isArray(src) && src.length === 3 && typeof (src[0] as { scenario_name?: string })?.scenario_name === "string") {
      return src as Array<{ scenario_name: string; fair_value: number; narrative?: string; probability?: number }>;
    }
    if (src && typeof src === "object" && "bear" in (src as Record<string, unknown>)) {
      const o = src as { bear: { fair_value: number }; base: { fair_value: number }; bull: { fair_value: number } };
      return [
        { scenario_name: "pessimistic", fair_value: o.bear.fair_value },
        { scenario_name: "expected", fair_value: o.base.fair_value },
        { scenario_name: "optimistic", fair_value: o.bull.fair_value },
      ];
    }
    return [];
  })();

  const wfv = detail.weighted_fair_value;
  const baseFV = detail.base_case_fair_value;
  const bullFV = detail.bull_case_fair_value;
  const bearFV = detail.bear_case_fair_value;
  const gap = detail.valuation_gap_pct != null ? detail.valuation_gap_pct * 100 : detail.gap_pct != null ? (detail.gap_pct as number) * (Math.abs(detail.gap_pct as number) < 2 ? 100 : 1) : null;
  const verdict = detail.valuation_verdict || detail.verdict || "—";
  const currency = getCurrency(detail);

  const confidenceDims = (() => {
    const cd = detail.confidence_details as unknown as { dimensions?: Record<string, { score: number; max_score: number; reasoning?: string }> } | null;
    const labelMap: Record<string, string> = {
      cash_flow_predictability: "Cash Flow Predictability",
      competitive_position: "Competitive Position",
      financial_health: "Financial Health",
      industry_visibility: "Industry Visibility",
      management_quality: "Management Quality",
    };
    const toTitleCase = (s: string) => {
      const mapped = labelMap[s.toLowerCase()];
      if (mapped) return mapped;
      return s
        .replace(/[_-]+/g, " ")
        .split(/\s+/)
        .filter(Boolean)
        .map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
        .join(" ");
    };
    if (cd?.dimensions) return Object.entries(cd.dimensions).map(([k, v]) => ({ label: toTitleCase(k), score: v.score, max: v.max_score, reasoning: v.reasoning }));
    return [];
  })();

  const dcf = detail.dcf_assumptions as unknown as Record<string, unknown> | null;
  const rawDcf = (dcf?.raw as Record<string, unknown> | undefined) ?? dcf;
  const orData = detail.ai_opportunities_risks as unknown as { opportunities?: Array<{ catalyst_name?: string; description?: string; potential_impact?: string }>; risks?: Array<{ risk_category?: string; description?: string; potential_impact?: string; likelihood?: string }> } | null;

  const searchSources = Array.isArray(detail.search_sources) ? (detail.search_sources as Array<{ source_name?: string; source_url?: string; url?: string; data_obtained?: string }>) : [];

  return (
    <Document>
      <Page size="A4" style={styles.page}>
        {/* Header with logo */}
        <View style={styles.header}>
          <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
            {logoSrc ? <Image src={logoSrc} style={styles.logo} /> : <View style={[styles.logo, { backgroundColor: "#0f172a", borderRadius: 4 }]} />}
            <View>
              <Text style={styles.title}>Stocksy Desktop</Text>
              <Text style={styles.subtitle}>{detail.company_name || detail.company} ({detail.ticker}) · {detail.exchange || "—"} · {detail.country_code || detail.country} · {detail.price_date}</Text>
              <Text style={styles.subtitle}>{detail.llm_provider}/{detail.llm_model} · {detail.analysis_date?.slice(0, 10) || ""}</Text>
            </View>
          </View>
          <View style={{ alignItems: "flex-end" }}>
            <Text style={styles.badge}>{verdict}</Text>
          </View>
        </View>

        {/* Valuation summary */}
        <Text style={styles.sectionTitle}>Valuation Summary</Text>
        <View style={styles.row}>
          <View style={[styles.card, styles.col]}>
            <Text style={styles.metricLabel}>Weighted Fair Value</Text>
            <Text style={styles.metricValue}>{formatCurrency(wfv, currency)}</Text>
            <Text style={styles.text}>Gap {gap != null ? `${gap.toFixed(1)}%` : "—"} · {String(verdict)}</Text>
          </View>
          <View style={[styles.card, styles.col]}>
            <Text style={styles.metricLabel}>Base</Text>
            <Text style={styles.metricValue}>{formatCurrency(baseFV, currency)}</Text>
          </View>
          <View style={[styles.card, styles.col]}>
            <Text style={styles.metricLabel}>Optimistic</Text>
            <Text style={styles.metricValue}>{formatCurrency(bullFV, currency)}</Text>
          </View>
          <View style={[styles.card, styles.col]}>
            <Text style={styles.metricLabel}>Pessimistic</Text>
            <Text style={styles.metricValue}>{formatCurrency(bearFV, currency)}</Text>
          </View>
        </View>
        <View style={styles.row}>
          <View style={[styles.card, styles.col, { marginTop: 6 }]}>
            <Text style={styles.metricLabel}>Current Price (CMP)</Text>
            <Text style={styles.metricValue}>{formatCurrency(detail.current_price, currency)}</Text>
          </View>
          <View style={[styles.card, styles.col, { marginTop: 6 }]}>
            <Text style={styles.metricLabel}>Risk / Reward</Text>
            <Text style={styles.metricValue}>{detail.risk_reward_ratio != null ? Number(detail.risk_reward_ratio).toFixed(2) : "—"} · {detail.risk_reward_label || ""}</Text>
            <Text style={styles.text}>WACC {detail.wacc != null ? `${(detail.wacc * 100).toFixed(2)}%` : "—"} · g {detail.terminal_growth_rate != null ? `${(detail.terminal_growth_rate * 100).toFixed(2)}%` : "—"}</Text>
          </View>
        </View>

        {/* Business description */}
        <Text style={styles.sectionTitle}>Business Description</Text>
        <Text style={styles.text}>{extractBusinessDescription(detail).slice(0, 2000) || "No business description."}</Text>

        {/* Scenario narratives */}
        <Text style={styles.sectionTitle}>Scenario Narratives</Text>
        {scenarios.length === 3 ? (
          <View style={styles.row}>
            {scenarios.map((s) => (
              <View key={s.scenario_name} style={[styles.card, styles.col]}>
                <Text style={{ fontSize: 8, fontWeight: 700, textTransform: "uppercase", color: "#0f172a" }}>{s.scenario_name}</Text>
                <Text style={styles.metricValue}>{formatCurrency(s.fair_value, currency)}</Text>
                <Text style={styles.text}>{(s.narrative || "").slice(0, 400) || "No narrative."}</Text>
              </View>
            ))}
          </View>
        ) : (
          <Text style={styles.text}>No scenarios.</Text>
        )}

        {/* Historical insights + chart side by side */}
        <Text style={styles.sectionTitle}>Historical Financial Insights</Text>
        <View style={styles.row}>
          <View style={styles.col}>
            {normalizeHistorical(detail.ai_historical_performance).length ? (
              normalizeHistorical(detail.ai_historical_performance).map((b, i) => (
                <View key={i} style={styles.bullet}>
                  <View style={styles.bulletDot} />
                  <Text style={[styles.text, { flex: 1 }]}>{b.slice(0, 300)}</Text>
                </View>
              ))
            ) : (
              <Text style={styles.text}>No insights.</Text>
            )}
          </View>
          <View style={[styles.col, styles.table]}>
            <View style={styles.tableHeader}>
              <Text style={[styles.tableHeaderCell, { textAlign: "left" }]}>Year</Text>
              <Text style={styles.tableHeaderCell}>Rev (M)</Text>
              <Text style={styles.tableHeaderCell}>Op Inc (M)</Text>
              <Text style={styles.tableHeaderCell}>Net Inc (M)</Text>
            </View>
            {historicalData.filter((r) => r.year !== "TTM").length ? historicalData.filter((r) => r.year !== "TTM").slice(-5).map((r) => (
              <View key={r.year} style={styles.tableRow}>
                <Text style={styles.tableCellLabel}>{r.year}</Text>
                <Text style={styles.tableCell}>{r.revenue != null ? r.revenue.toFixed(1) : "—"}</Text>
                <Text style={styles.tableCell}>{r.opIncome != null ? r.opIncome.toFixed(1) : "—"}</Text>
                <Text style={styles.tableCell}>{r.netIncome != null ? r.netIncome.toFixed(1) : "—"}</Text>
              </View>
            )) : (
              <View style={styles.tableRow}><Text style={styles.tableCell}>No historical data</Text></View>
            )}
          </View>
        </View>

        {/* Confidence narratives — starts on new page after Historical Insights */}
        <Text style={styles.sectionTitle} break>Confidence — {detail.confidence_level || ""} ({detail.confidence_score != null ? Math.round(detail.confidence_score) : "—"}/100)</Text>
        {confidenceDims.length ? (
          <View style={{ gap: 4 }}>
            {confidenceDims.map((d) => (
              <View key={d.label} style={[styles.card, { flexDirection: "row", justifyContent: "space-between" }]}>
                <View style={{ flex: 1 }}>
                  <Text style={{ fontSize: 8, fontWeight: 700 }}>{d.label} ({d.score}/{d.max})</Text>
                  <Text style={styles.text}>{(d.reasoning || "").slice(0, 250) || "—"}</Text>
                </View>
                <Text style={{ fontSize: 8, color: "#0f172a", fontWeight: 700 }}>{Math.round((d.score / d.max) * 100)}%</Text>
              </View>
            ))}
          </View>
        ) : (
          <Text style={styles.text}>No confidence details.</Text>
        )}

        {/* DCF assumptions table */}
        <Text style={styles.sectionTitle}>DCF Assumptions</Text>
        <View style={styles.table}>
          <View style={styles.tableHeader}>
            <Text style={[styles.tableHeaderCell, { flex: 1.5, textAlign: "left" }]}>Parameter</Text>
            <Text style={styles.tableHeaderCell}>Y1</Text>
            <Text style={styles.tableHeaderCell}>Y2</Text>
            <Text style={styles.tableHeaderCell}>Y3</Text>
            <Text style={styles.tableHeaderCell}>Y4</Text>
            <Text style={styles.tableHeaderCell}>Y5</Text>
          </View>
          {(() => {
            const rev = (rawDcf?.revenue_growth_rates as number[] | undefined) ?? [];
            const mar = (rawDcf?.operating_margins as number[] | undefined) ?? [];
            return (
              <>
                <View style={styles.tableRow}>
                  <Text style={styles.tableCellLabel}>Revenue growth %</Text>
                  {Array.from({ length: 5 }).map((_, i) => (
                    <Text key={i} style={styles.tableCell}>{rev[i] != null ? (rev[i] * 100).toFixed(1) + "%" : "—"}</Text>
                  ))}
                </View>
                <View style={styles.tableRow}>
                  <Text style={styles.tableCellLabel}>Operating margin %</Text>
                  {Array.from({ length: 5 }).map((_, i) => (
                    <Text key={i} style={styles.tableCell}>{mar[i] != null ? (mar[i] * 100).toFixed(1) + "%" : "—"}</Text>
                  ))}
                </View>
              </>
            );
          })()}
          <View style={styles.tableRow}>
            <Text style={styles.tableCellLabel}>Terminal margin / growth / FCF factor</Text>
            <Text style={styles.tableCell}>{rawDcf?.terminal_margin != null ? fmt(rawDcf.terminal_margin as number * 100, "%") : "—"}</Text>
            <Text style={styles.tableCell}>{rawDcf?.terminal_growth_rate != null ? fmt(rawDcf.terminal_growth_rate as number * 100, "%") : "—"}</Text>
            <Text style={styles.tableCell}>{rawDcf?.fcf_conversion_factor != null ? fmt(rawDcf.fcf_conversion_factor as number) : "—"}</Text>
            <Text style={styles.tableCell}>{detail.wacc != null ? fmt(detail.wacc * 100, "%") : "—"}</Text>
            <Text style={styles.tableCell}>{rawDcf?.cyclicality_adjustment_bps != null ? String(rawDcf.cyclicality_adjustment_bps) + " bps" : "—"}</Text>
          </View>
        </View>

        {/* 2 charts side by side — forecast and FCF — with 1-2Y historical FY24 + 5Y forecast */}
        <Text style={styles.sectionTitle}>Forecast & FCF Projection</Text>
        <View style={styles.row}>
          <View style={[styles.col, styles.table]}>
            <Text style={{ fontSize: 8, fontWeight: 700, padding: 4, backgroundColor: "#f1f5f9", textAlign: "center" }}>Revenue Growth vs Operating Margin (Hist FY + 5Y)</Text>
            <View style={styles.tableHeader}>
              <Text style={styles.tableHeaderCell}>Year</Text>
              <Text style={styles.tableHeaderCell}>Hist Growth</Text>
              <Text style={styles.tableHeaderCell}>Fcst Growth</Text>
              <Text style={styles.tableHeaderCell}>Hist Margin</Text>
              <Text style={styles.tableHeaderCell}>Fcst Margin</Text>
            </View>
            {forecastData.length ? (forecastData as unknown as Array<{ year: string; histGrowth?: number | null; fcstGrowth?: number | null; histMargin?: number | null; fcstMargin?: number | null; growth?: number; margin?: number }>).map((r) => {
              const isHist = (r as unknown as { histGrowth?: number | null }).histGrowth !== undefined;
              if (isHist) {
                const hg = (r as unknown as { histGrowth: number | null }).histGrowth;
                const fg = (r as unknown as { fcstGrowth: number | null }).fcstGrowth;
                const hm = (r as unknown as { histMargin: number | null }).histMargin;
                const fm = (r as unknown as { fcstMargin: number | null }).fcstMargin;
                return (
                  <View key={r.year} style={styles.tableRow}>
                    <Text style={styles.tableCellLabel}>{r.year}</Text>
                    <Text style={styles.tableCell}>{hg != null ? hg.toFixed(1) + "%" : "—"}</Text>
                    <Text style={styles.tableCell}>{fg != null ? fg.toFixed(1) + "%" : "—"}</Text>
                    <Text style={styles.tableCell}>{hm != null ? hm.toFixed(1) + "%" : "—"}</Text>
                    <Text style={styles.tableCell}>{fm != null ? fm.toFixed(1) + "%" : "—"}</Text>
                  </View>
                );
              }
              return (
                <View key={r.year} style={styles.tableRow}>
                  <Text style={styles.tableCellLabel}>{r.year}</Text>
                  <Text style={styles.tableCell}>—</Text>
                  <Text style={styles.tableCell}>{(r as unknown as { growth: number }).growth.toFixed(1)}%</Text>
                  <Text style={styles.tableCell}>—</Text>
                  <Text style={styles.tableCell}>{(r as unknown as { margin: number }).margin.toFixed(1)}%</Text>
                </View>
              );
            }) : <View style={styles.tableRow}><Text style={styles.tableCell}>No forecast</Text></View>}
          </View>
          <View style={[styles.col, styles.table]}>
            <Text style={{ fontSize: 8, fontWeight: 700, padding: 4, backgroundColor: "#f1f5f9", textAlign: "center" }}>FCF Projection (Hist FY + 5Y — M)</Text>
            <View style={styles.tableHeader}>
              <Text style={styles.tableHeaderCell}>Year</Text>
              <Text style={styles.tableHeaderCell}>Hist FCF</Text>
              <Text style={styles.tableHeaderCell}>Fcst FCF</Text>
            </View>
            {fcfData.length ? (fcfData as unknown as Array<{ year: string; histFcf?: number | null; fcstFcf?: number | null; fcf?: number | null }>).map((r) => {
              const isHist = (r as unknown as { histFcf?: number | null }).histFcf !== undefined;
              if (isHist) {
                const hf = (r as unknown as { histFcf: number | null }).histFcf;
                const ff = (r as unknown as { fcstFcf: number | null }).fcstFcf;
                return (
                  <View key={r.year} style={styles.tableRow}>
                    <Text style={styles.tableCellLabel}>{r.year}</Text>
                    <Text style={styles.tableCell}>{hf != null ? hf.toFixed(1) : "—"}</Text>
                    <Text style={styles.tableCell}>{ff != null ? ff.toFixed(1) : "—"}</Text>
                  </View>
                );
              }
              return (
                <View key={r.year} style={styles.tableRow}>
                  <Text style={styles.tableCellLabel}>{r.year}</Text>
                  <Text style={styles.tableCell}>—</Text>
                  <Text style={styles.tableCell}>{(r as unknown as { fcf: number | null }).fcf != null ? (r as unknown as { fcf: number }).fcf.toFixed(1) : "—"}</Text>
                </View>
              );
            }) : <View style={styles.tableRow}><Text style={styles.tableCell}>No FCF data</Text></View>}
          </View>
        </View>

        {/* Opportunities & Risks side by side */}
        <Text style={styles.sectionTitle}>Opportunities & Risks</Text>
        <View style={styles.row}>
          <View style={styles.col}>
            <Text style={{ fontSize: 8, fontWeight: 700, color: "#10b981", marginBottom: 4 }}>Opportunities</Text>
            {orData?.opportunities?.length ? orData.opportunities.slice(0, 4).map((o, i) => (
              <View key={i} style={[styles.card, { marginBottom: 4 }]}>
                <Text style={{ fontSize: 8, fontWeight: 700 }}>{o.catalyst_name || `Opportunity ${i + 1}`}</Text>
                <Text style={styles.text}>{(o.description || "").slice(0, 300)}</Text>
                {o.potential_impact && <Text style={[styles.text, { color: "#0f172a" }]}>Impact: {(o.potential_impact as string).slice(0, 150)}</Text>}
              </View>
            )) : <Text style={styles.text}>No opportunities.</Text>}
          </View>
          <View style={styles.col}>
            <Text style={{ fontSize: 8, fontWeight: 700, color: "#ef4444", marginBottom: 4 }}>Risks</Text>
            {orData?.risks?.length ? orData.risks.slice(0, 4).map((r, i) => (
              <View key={i} style={[styles.card, { marginBottom: 4 }]}>
                <Text style={{ fontSize: 8, fontWeight: 700 }}>{r.risk_category || `Risk ${i + 1}`}</Text>
                <Text style={styles.text}>{(r.description || "").slice(0, 300)}</Text>
                {r.potential_impact && <Text style={[styles.text, { color: "#0f172a" }]}>Impact: {(r.potential_impact as string).slice(0, 150)}</Text>}
                {r.likelihood && <Text style={[styles.text, { color: "#64748b" }]}>Likelihood: {r.likelihood as string}</Text>}
              </View>
            )) : <Text style={styles.text}>No risks.</Text>}
          </View>
        </View>

        {/* Verdict narrative — only verdict text, starts on new page after Opportunities & Risks */}
        <Text style={styles.sectionTitle} break>Investment Perspective — Verdict</Text>
        <Text style={styles.text}>{extractInvestmentVerdict(detail).slice(0, 2000) || "No perspective."}</Text>

        {/* Search sources — clickable links */}
        <Text style={styles.sectionTitle}>References — Search Sources</Text>
        {searchSources.length ? searchSources.slice(0, 12).map((s, i) => {
          const url = (s.source_url || s.url || "").toString().trim();
          const label = (s.data_obtained || (s as unknown as { title?: string }).title || "").toString().slice(0, 200).trim();
          const sourceName = (s.source_name || "Source").toString();
          return (
            <View key={i} style={{ marginBottom: 4, flexDirection: "row", flexWrap: "wrap" }}>
              <Text style={styles.text}>{i + 1}. {label ? label + " " : ""}[{sourceName}] </Text>
              {url ? (
                <Link src={url} style={[styles.text, styles.link]}>{url}</Link>
              ) : (
                <Text style={styles.text}>—</Text>
              )}
            </View>
          );
        }) : <Text style={styles.text}>No sources.</Text>}

        <View style={styles.footer}>
          <Text style={{ fontSize: 7, color: "#94a3b8" }}>Generated by Stocksy Desktop — {new Date().toISOString().slice(0, 10)}</Text>
          <Text style={{ fontSize: 7, color: "#94a3b8" }}>{detail.ticker} · {detail.llm_model}</Text>
        </View>
      </Page>
    </Document>
  );
}
