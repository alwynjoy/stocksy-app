export type GapBucket = "all" | "<=-50" | "-50_0" | "0_25" | "25_100" | ">=100";
export type ConfidenceBucket = "all" | "<50" | ">=50";
export type RiskBucket = "all" | "<0.5" | "0.5_1" | "1_2" | ">=2";
export type GrowthBucket = "all" | "<=0" | "0_25" | "25_50" | "50_100" | ">=100";

export interface ScreenerFilters {
  country: string; // All or IN/US/GB/AU
  sector: string; // All or sector name
  marketCap: string; // All or mega-cap/large-cap/mid-cap/small-cap/micro-cap
  verdict: string; // All or Undervalued/Fairly Valued/Overvalued
  gapBucket: GapBucket;
  confidenceBucket: ConfidenceBucket;
  riskBucket: RiskBucket;
  growthBucket: GrowthBucket;
}

export interface ScreenerPreset {
  name: string;
  filters: ScreenerFilters;
}

export const DEFAULT_FILTERS: ScreenerFilters = {
  country: "All",
  sector: "All",
  marketCap: "All",
  verdict: "All",
  gapBucket: "all",
  confidenceBucket: "all",
  riskBucket: "all",
  growthBucket: "all",
};

export const BUILTIN_PRESETS: ScreenerPreset[] = [
  {
    name: "Deep Value",
    filters: { ...DEFAULT_FILTERS, gapBucket: "0_25", confidenceBucket: ">=50", riskBucket: "<0.5" },
  },
  {
    name: "Quality at Fair Price",
    filters: { ...DEFAULT_FILTERS, verdict: "Fairly Valued", confidenceBucket: ">=50", riskBucket: "<0.5" },
  },
  {
    name: "Low Risk Value",
    filters: { ...DEFAULT_FILTERS, verdict: "Undervalued", gapBucket: "0_25", riskBucket: "<0.5" },
  },
  {
    name: "Growth Leaders",
    filters: { ...DEFAULT_FILTERS, growthBucket: "25_50", confidenceBucket: ">=50" },
  },
];

export const PRESETS_STORAGE_KEY = "screener.presets.custom";
