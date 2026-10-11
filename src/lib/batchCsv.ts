export const MAX_BATCH = 100;
export const SAMPLE_CSV = `ticker,country,provider
AAPL,US,anthropic
RELIANCE.NS,IN,openai
BATS.L,GB,kimi
CBA.AX,AU,deepseek
`;

export type BatchRow = {
  line: number;
  ticker: string;
  country: string;
  provider: string;
  rawTicker: string;
  rawCountry: string;
  rawProvider: string;
  valid: boolean;
  error?: string;
};

function splitRow(line: string): string[] {
  const out: string[] = [];
  let cur = "";
  let inQuote = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '"') { inQuote = !inQuote; continue; }
    if (c === "," && !inQuote) { out.push(cur.trim().replace(/^"|"$/g, "").trim()); cur = ""; }
    else cur += c;
  }
  out.push(cur.trim().replace(/^"|"$/g, "").trim());
  return out;
}

export function parseBatchCsv(
  text: string,
  marketCodes: string[],
  providerIds: string[],
  maxBatch: number = MAX_BATCH,
): BatchRow[] {
  const cleaned = text.replace(/^\uFEFF/, "").trim();
  if (!cleaned) return [];
  const lines = cleaned.split(/\r?\n/).map((l) => l.trim()).filter((l) => l.length > 0);
  if (lines.length === 0) return [];
  const validCountries = new Set(marketCodes.map((c) => c.toUpperCase()));
  const validProviders = new Set(providerIds.map((p) => p.toLowerCase()));
  let startIdx = 0;
  let idxTicker = 0; let idxCountry = 1; let idxProvider = 2;
  const first = splitRow(lines[0]).map((s) => s.toLowerCase());
  const isHeader = first.some((v) => v === "ticker" || v === "country" || v === "provider");
  if (isHeader) {
    startIdx = 1;
    idxTicker = first.indexOf("ticker");
    idxCountry = first.indexOf("country");
    idxProvider = first.indexOf("provider");
    if (idxTicker === -1) idxTicker = 0;
    if (idxCountry === -1) idxCountry = 1;
    if (idxProvider === -1) idxProvider = 2;
  }
  const seen = new Set<string>();
  const rows: BatchRow[] = [];
  for (let i = startIdx; i < lines.length && rows.length < maxBatch; i++) {
    const cols = splitRow(lines[i]);
    const lineNo = i + 1;
    const rawTicker = (cols[idxTicker] ?? "").trim();
    const rawCountry = (cols[idxCountry] ?? "").trim();
    const rawProvider = (cols[idxProvider] ?? "").trim().toLowerCase();
    const tickerNorm = rawTicker.toUpperCase();
    const countryNorm = rawCountry.toUpperCase();
    const providerNorm = rawProvider.toLowerCase();
    let err: string | undefined;
    if (!rawTicker) err = "missing ticker";
    else if (!rawCountry) err = "missing country";
    else if (!rawProvider) err = "missing provider";
    else if (!validCountries.has(countryNorm)) err = `unknown country '${rawCountry}'`;
    else if (!validProviders.has(providerNorm)) err = `unknown provider '${rawProvider}'`;
    const dupKey = `${tickerNorm}::${countryNorm}::${providerNorm}`;
    if (!err && seen.has(dupKey)) err = "duplicate";
    if (!err) seen.add(dupKey);
    rows.push({
      line: lineNo, ticker: tickerNorm, country: countryNorm, provider: providerNorm || "anthropic",
      rawTicker, rawCountry, rawProvider, valid: !err, error: err,
    });
  }
  return rows;
}
