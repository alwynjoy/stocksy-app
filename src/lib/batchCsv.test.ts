import { describe, it, expect } from "vitest";
import { parseBatchCsv, MAX_BATCH, SAMPLE_CSV } from "./batchCsv";

const COUNTRIES = ["US","IN","GB","AU"];
const PROVIDERS = ["anthropic","openai","deepseek","kimi","custom"];

describe("parseBatchCsv", () => {
  it("parses sample csv correctly", () => {
    const rows = parseBatchCsv(SAMPLE_CSV, COUNTRIES, PROVIDERS);
    expect(rows.length).toBe(4);
    expect(rows.every(r => r.valid)).toBe(true);
    expect(rows[0].ticker).toBe("AAPL");
    expect(rows[1].ticker).toBe("RELIANCE.NS");
    expect(rows[1].country).toBe("IN");
    expect(rows[1].provider).toBe("openai");
  });

  it("handles header in any order", () => {
    const csv = `provider,ticker,country
kimi,BATS.L,GB
`;
    const rows = parseBatchCsv(csv, COUNTRIES, PROVIDERS);
    expect(rows.length).toBe(1);
    expect(rows[0].ticker).toBe("BATS.L");
    expect(rows[0].country).toBe("GB");
    expect(rows[0].provider).toBe("kimi");
  });

  it("flags missing fields", () => {
    const csv = `ticker,country,provider
AAPL,US,
,IN,openai
RELIANCE.NS,,kimi
`;
    const rows = parseBatchCsv(csv, COUNTRIES, PROVIDERS);
    expect(rows[0].error).toBe("missing provider");
    expect(rows[1].error).toBe("missing ticker");
    expect(rows[2].error).toBe("missing country");
    expect(rows.every(r => !r.valid)).toBe(true);
  });

  it("flags unknown country/provider", () => {
    const csv = `ticker,country,provider
AAPL,ZZ,anthropic
RELIANCE.NS,IN,unknown
`;
    const rows = parseBatchCsv(csv, COUNTRIES, PROVIDERS);
    expect(rows[0].error).toMatch(/unknown country/);
    expect(rows[1].error).toMatch(/unknown provider/);
  });

  it("detects duplicates", () => {
    const csv = `ticker,country,provider
AAPL,US,anthropic
aapl,us,anthropic
AAPL,US,openai
`;
    const rows = parseBatchCsv(csv, COUNTRIES, PROVIDERS);
    expect(rows[0].valid).toBe(true);
    expect(rows[1].error).toBe("duplicate");
    expect(rows[2].valid).toBe(true); // different provider not duplicate
  });

  it("handles quoted fields and BOM", () => {
    const csv = `\uFEFFticker,country,provider
"RELIANCE.NS","IN","openai"
`;
    const rows = parseBatchCsv(csv, COUNTRIES, PROVIDERS);
    expect(rows[0].ticker).toBe("RELIANCE.NS");
    expect(rows[0].valid).toBe(true);
  });

  it("caps at MAX_BATCH", () => {
    let csv = "ticker,country,provider\n";
    for (let i=0;i<MAX_BATCH+10;i++) csv += `T${i},US,anthropic\n`;
    const rows = parseBatchCsv(csv, COUNTRIES, PROVIDERS);
    expect(rows.length).toBe(MAX_BATCH);
  });

  it("returns empty for empty input", () => {
    expect(parseBatchCsv("", COUNTRIES, PROVIDERS)).toEqual([]);
    expect(parseBatchCsv("  \n  ", COUNTRIES, PROVIDERS)).toEqual([]);
  });
});
