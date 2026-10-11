import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  name: string;
  version: string;
  db_path: string;
  schema_ok: boolean;
  tables: number;
}

export interface DbHealth {
  migrated: boolean;
  tables: number;
  countries: number;
  prompts: number;
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

export function getDbHealth(): Promise<DbHealth> {
  return invoke<DbHealth>("db_health");
}

export interface DbTable {
  name: string;
  rows: number;
}

export interface DbTableData {
  name: string;
  columns: string[];
  rows: unknown[][];
  total: number;
  returned: number;
}

export function listDbTables(): Promise<DbTable[]> {
  return invoke<DbTable[]>("db_browser_tables");
}

export function getDbTable(table: string, limit: number, offset = 0): Promise<DbTableData> {
  return invoke<DbTableData>("db_browser_query", { table, limit, offset });
}

export function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>("get_setting", { key });
}

export function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>("set_setting", { key, value });
}

export interface ReportRow {
  id: string;
  ticker: string;
  company: string; // alias of company_name
  company_name: string;
  country: string; // alias
  country_code: string;
  sector: string | null;
  price_date: string;
  model: string; // alias llm_model
  llm_model: string;
  llm_provider: string;
  verdict: string; // alias valuation_verdict
  valuation_verdict: string | null;
  weighted_fair_value: number | null;
  gap_pct: number | null; // alias valuation_gap_pct
  valuation_gap_pct: number | null;
  confidence: number | null; // alias confidence_score
  confidence_score: number | null;
  confidence_level: string | null;
  favorite: boolean; // alias is_favorite
  is_favorite: boolean;
  is_primary: boolean;
  analysis_date: string;
  current_price: number | null;
}

export interface DashboardStats {
  total_reports: number;
  reports_per_model: { model: string; count: number }[];
  reports_per_country: { country: string; count: number }[];
  recent_analyses: ReportRow[];
  watchlist_summary: {
    name: string;
    tickers: number;
    gainers: number;
    latest_gap_pct: number;
  }[];
  provider_stats: { provider: string; count: number; input_tokens: number; output_tokens: number; total_tokens: number; avg_duration_ms: number; estimated_usd: number }[];
  total_input_tokens: number;
  total_output_tokens: number;
  total_tokens: number;
  total_estimated_usd: number;
}

export interface ReportDetail {
  id: string;
  ticker: string;
  company: string; // alias
  company_name: string;
  exchange: string | null;
  country: string; // alias
  country_code: string;
  sector: string | null;
  industry: string | null;
  market_cap_category: string | null;
  currency: string | null; // alias currency_code
  currency_code: string | null;
  price_date: string;
  analysis_date: string;
  is_primary: boolean;
  status: string;
  error_message: string | null;
  current_price: number | null;
  market_cap: number | null;
  base_case_fair_value: number | null;
  bull_case_fair_value: number | null;
  bear_case_fair_value: number | null;
  weighted_fair_value: number | null;
  valuation_gap_pct: number | null;
  gap_pct: number | null;
  verdict: string; // alias
  valuation_verdict: string | null;
  wacc: number | null;
  cost_of_equity: number | null;
  cost_of_debt_after_tax: number | null;
  raw_beta: number | null;
  blume_adjusted_beta: number | null;
  terminal_growth_rate: number | null;
  terminal_margin: number | null;
  upside_pct: number | null;
  downside_pct: number | null;
  risk_reward_ratio: number | null;
  risk_reward_label: string | null;
  confidence: number | null; // alias
  confidence_score: number | null;
  confidence_level: string | null;
  confidence_details: Record<string, unknown> | null; // {overall_score, overall_level, dimensions:{score,max_score,reasoning}}
  scenarios: unknown; // array of ScenarioOutput
  sensitivity_matrix: unknown; // array of SensitivityCell
  sensitivity_base_parameters: unknown;
  // legacy stub shape falls back
  scenarios_legacy?: { bear: { fair_value: number; weight: number }; base: { fair_value: number; weight: number }; bull: { fair_value: number; weight: number }; };
  confidence_details_legacy?: Record<string, number>;
  narrative: string | null; // stub
  ai_business_context: string | null;
  ai_historical_performance: string | null;
  ai_growth_projections: string | null;
  ai_opportunities_risks: unknown | null; // {opportunities, risks}
  // Typed helpers for carousel
  ai_opportunities_risks_typed?: OpportunitiesRisks | null;
  ai_investment_perspective: string | null;
  dcf_assumptions: unknown | null;
  country_params_snapshot: unknown | null;
  search_sources: unknown | null;
  llm_provider: string;
  llm_model: string;
  model: string; // alias
  llm_thinking_tokens: number | null;
  llm_output_tokens: number | null;
  llm_duration_ms: number | null;
  is_favorite: boolean;
  favorite: boolean;
  user_notes: string | null;
  tags: unknown | null;
}

export type Opportunity = {
  catalyst_name: string;
  description: string;
  potential_impact: string;
  timeline: string | null;
  source_url: string | null;
};

export type Risk = {
  risk_category: string;
  description: string;
  potential_impact: string;
  likelihood: "low" | "moderate" | "high" | string;
  source_url: string | null;
};

export type OpportunitiesRisks = {
  opportunities: Opportunity[];
  risks: Risk[];
};

export interface ReportVersion {
  id: string;
  llm_provider: string;
  llm_model: string;
  price_date: string;
  analysis_date: string;
  is_primary: boolean;
  is_favorite: boolean;
  status: string;
}

export interface ScreenerRow {
  ticker: string;
  company: string;
  sector: string;
  industry: string;
  market_cap_category: string;
  country: string;
  verdict: string;
  weighted_fair_value: number;
  gap_pct: number;
  confidence: number;
  risk_reward: number;
  revenue_growth: number;
  confidence_details?: unknown;
}

export interface WatchlistItem {
  ticker: string;
  company: string;
  country: string;
  price: number;
  weighted_fair_value: number;
  gap_pct: number;
  verdict: string;
  note: string;
}

export interface WatchlistRow {
  id: string;
  name: string;
  description: string | null;
  created_at: string | null;
  updated_at: string | null;
}

export interface WatchlistItemEnriched {
  id: string;
  ticker: string;
  country_code: string;
  notes: string | null;
  target_price: number | null;
  added_at: string | null;
  company_name: string | null;
  sector: string | null;
  industry: string | null;
  current_price: number | null;
  weighted_fair_value: number | null;
  valuation_gap_pct: number | null;
  valuation_verdict: string | null;
  confidence_score: number | null;
  currency_code: string | null;
}

export interface SettingRow {
  key: string;
  value: string;
  category: string | null;
}

export function listSettings(prefix?: string | null): Promise<SettingRow[]> {
  return invoke<SettingRow[]>("list_settings", { prefix: prefix ?? null });
}

export function getSecret(key: string): Promise<string | null> {
  return invoke<string | null>("get_secret", { key });
}

export function setSecret(key: string, value: string): Promise<void> {
  return invoke<void>("set_secret", { key, value });
}

export function deleteSecret(key: string): Promise<void> {
  return invoke<void>("delete_secret", { key });
}

export interface SecretStatus {
  key: string;
  is_saved: boolean;
}

export function secretStatus(): Promise<SecretStatus[]> {
  return invoke<SecretStatus[]>("secret_status");
}

export interface CountryPayload {
  country_code: string;
  country_name: string;
  currency_code: string;
  currency_symbol?: string | null;
  expected_market_return: number;
  risk_free_rate: number;
  corporate_tax_rate: number;
  nominal_gdp_growth: number;
  yahoo_ticker_suffix?: string | null;
}

export interface CountryRow extends CountryPayload {
  currency_symbol: string | null;
  yahoo_ticker_suffix: string | null;
  equity_risk_premium: number;
  reference_urls: string | null;
}

export function countriesList(): Promise<CountryRow[]> {
  return invoke<CountryRow[]>("countries_list");
}

export function countryUpsert(country: CountryPayload): Promise<void> {
  return invoke<void>("country_upsert", { country });
}

export interface PromptRow {
  prompt_key: string;
  prompt_name: string;
  description: string | null;
  prompt_text: string;
  is_active: number;
  version: number;
  updated_at: string | null;
}

export function promptsList(): Promise<PromptRow[]> {
  return invoke<PromptRow[]>("prompts_list");
}

export function promptUpdate(key: string, text: string): Promise<void> {
  return invoke<void>("prompt_update", { key, text });
}

export function promptRestoreDefault(key: string): Promise<void> {
  return invoke<void>("prompt_restore_default", { key });
}

export function promptRestoreAll(): Promise<number> {
  return invoke<number>("prompt_restore_all");
}

export interface DcfDefault {
  name: string;
  value: string;
  note: string;
}

export function dcfDefaults(): Promise<DcfDefault[]> {
  return invoke<DcfDefault[]>("dcf_defaults");
}

export function llmFetchModels(
  provider: string,
  baseUrl: string,
  apiKey?: string,
): Promise<string[]> {
  return invoke<string[]>("llm_fetch_models", {
    provider,
    baseUrl,
    apiKey: apiKey || null,
  });
}

export interface LlmTestResult {
  ok: boolean;
  message: string;
  latency_ms: number;
}

export function llmTestConnection(
  provider: string,
  baseUrl: string,
  model: string,
  apiKey?: string,
): Promise<LlmTestResult> {
  return invoke<LlmTestResult>("llm_test_connection", {
    provider,
    baseUrl,
    model,
    apiKey: apiKey || null,
  });
}

export interface DataInfo {
  db_path: string;
  db_size_bytes: number;
  settings: number;
  countries: number;
  prompts: number;
  ticker_cache: number;
  financial_cache: number;
}

export function getDataInfo(): Promise<DataInfo> {
  return invoke<DataInfo>("data_info");
}

export interface Watchlist {
  id: string;
  name: string;
  items: WatchlistItem[];
}

export interface AnalysisPreview {
  ticker: string;
  country: string;
  status: string;
  stages: string[];
}

export interface TickerHit {
  ticker: string;
  company_name: string | null;
  exchange: string | null;
  country_code: string;
}

export function tickerSearch(
  query: string,
  country?: string | null,
  limit = 8,
): Promise<TickerHit[]> {
  return invoke<TickerHit[]>("ticker_search", {
    query,
    country: country ?? null,
    limit,
  });
}

export function tickerYahooSearch(
  query: string,
  country?: string | null,
): Promise<TickerHit[]> {
  return invoke<TickerHit[]>("ticker_yahoo_search", {
    query,
    country: country ?? null,
  });
}

export function stubDashboardStats(): Promise<DashboardStats> {
  return invoke<DashboardStats>("stub_dashboard_stats");
}

export function listReports(): Promise<ReportDetail[]> {
  return invoke<ReportDetail[]>("list_reports");
}

export function getReport(ticker: string, versionId?: string | null): Promise<ReportDetail> {
  return invoke<ReportDetail>("get_report", { ticker, versionId: versionId ?? null });
}

export function getReportById(id: string): Promise<ReportDetail> {
  return invoke<ReportDetail>("get_report_by_id", { id });
}

export function getReportVersions(ticker: string): Promise<ReportVersion[]> {
  return invoke<ReportVersion[]>("get_report_versions", { ticker });
}

export function getReportsByIds(ids: string[]): Promise<ReportDetail[]> {
  return invoke<ReportDetail[]>("get_reports_by_ids", { ids });
}

export function toggleFavorite(id: string): Promise<ReportDetail> {
  return invoke<ReportDetail>("toggle_favorite", { id });
}

export function deleteReports(ids: string[]): Promise<number> {
  return invoke<number>("delete_reports", { ids });
}

export function updateCmp(ticker: string): Promise<{ ticker: string; country_code: string; current_price: number | null; price_date: string; market_cap: number | null; status: string }> {
  return invoke("update_cmp", { ticker });
}

export function updatePrice(ticker: string): Promise<{ ticker: string; country_code: string; current_price: number | null; price_date: string; currency: string | null; status: string }> {
  return invoke("update_price", { ticker });
}

export function recomputeLocal(id: string, overrides: { revenue_growth_rates?: [number, number, number, number, number]; operating_margins?: [number, number, number, number, number]; terminal_margin?: number; terminal_growth_rate?: number; fcf_conversion_factor?: number; financial_services_ni_factor?: number; cyclicality_adjustment_bps?: number }): Promise<unknown> {
  return invoke("recompute_local", { id, overrides });
}

export function getDashboardStats(): Promise<DashboardStats> {
  return invoke<DashboardStats>("dashboard_stats");
}

export function getScreenerRows(): Promise<ScreenerRow[]> {
  return invoke<ScreenerRow[]>("screener_rows");
}

export function stubScreenerRows(): Promise<ScreenerRow[]> {
  return invoke<ScreenerRow[]>("stub_screener_rows");
}

export function watchlistsList(): Promise<WatchlistRow[]> {
  return invoke<WatchlistRow[]>("watchlists_list");
}
export function watchlistCreate(name: string, description?: string | null): Promise<{ id: string; name: string; description: string | null }> {
  return invoke("watchlist_create", { name, description: description ?? null });
}
export function watchlistUpdate(id: string, name: string, description?: string | null): Promise<void> {
  return invoke("watchlist_update", { id, name, description: description ?? null });
}
export function watchlistDelete(id: string): Promise<void> {
  return invoke("watchlist_delete", { id });
}
export function watchlistItems(watchlistId: string): Promise<WatchlistItemEnriched[]> {
  return invoke<WatchlistItemEnriched[]>("watchlist_items", { watchlistId });
}
export function watchlistAddItem(watchlistId: string, ticker: string, countryCode: string, notes?: string | null, targetPrice?: number | null): Promise<{ id: string; ticker: string; country_code: string }> {
  return invoke("watchlist_add_item", { watchlistId, ticker, countryCode, notes: notes ?? null, targetPrice: targetPrice ?? null });
}
export function watchlistRemoveItem(itemId: string): Promise<void> {
  return invoke("watchlist_remove_item", { itemId });
}
export function watchlistUpdateItem(itemId: string, notes?: string | null, targetPrice?: number | null): Promise<void> {
  return invoke("watchlist_update_item", { itemId, notes: notes ?? null, targetPrice: targetPrice ?? null });
}

export interface FetchFinancialResult {
  status: string;
  error_message: string | null;
  company_info: {
    company_name: string;
    ticker: string;
    exchange: string;
    country: string;
    sector: string;
    industry: string;
    currency_code: string;
    is_financial_services: boolean;
  };
  current_market_data: {
    current_price: number | null;
    market_cap: number | null;
    market_cap_category: string | null;
    price_date: string | null;
    shares_outstanding: number | null;
  };
  market_parameters: {
    risk_free_rate: number;
    market_risk_premium: number;
    corporate_tax_rate: number;
    nominal_gdp_growth: number;
  };
  financials: {
    beta: number;
    historical_data: Array<{
      fiscal_year_end: string;
      revenue: number | null;
      operating_income: number | null;
      ebitda: number | null;
      net_income: number | null;
      total_debt: number | null;
      total_equity: number | null;
      cash_equivalents: number | null;
      free_cash_flow: number | null;
    }>;
  };
  cached: boolean;
}

export function fetchFinancialData(
  ticker: string,
  country_code: string,
  force_refresh = false,
): Promise<FetchFinancialResult> {
  return invoke<FetchFinancialResult>("fetch_financial_data", {
    ticker,
    countryCode: country_code,
    forceRefresh: force_refresh,
  });
}

export function getCachedFinancialData(
  ticker: string,
  country_code: string,
): Promise<unknown | null> {
  return invoke<unknown | null>("get_cached_financial_data", {
    ticker,
    countryCode: country_code,
  });
}

export function checkFinancialStaleness(
  ticker: string,
  country_code: string,
): Promise<{ is_stale: boolean; reason: string; fetch_date: string | null; price_date: string | null }> {
  return invoke("check_financial_staleness", { ticker, countryCode: country_code });
}

export interface RunAnalysisResult {
  report_id: string;
  ticker: string;
  country_code: string;
  status: string;
  message: string;
  weighted_fair_value: number | null;
  base_case_fair_value: number | null;
}

export function runAnalysis(ticker: string, country_code: string, provider?: string): Promise<RunAnalysisResult> {
  return invoke<RunAnalysisResult>("run_analysis", { ticker, countryCode: country_code, provider: provider ?? null });
}

export interface DebugLogMeta {
  run_id: string;
  path: string;
  size_bytes: number;
  modified: string;
}

export interface DebugLogRead {
  summary: string;
  run_id: string;
  entries: Array<{ ts: string; elapsed_ms: number; seq: number; run_id: string; kind: string; payload: unknown }>;
}

export function debugLogDir(): Promise<string> {
  return invoke<string>("debug_log_dir");
}
export function debugLogsList(): Promise<DebugLogMeta[]> {
  return invoke<DebugLogMeta[]>("debug_logs_list");
}
export function debugLogRead(runId: string, limit?: number): Promise<DebugLogRead> {
  return invoke<DebugLogRead>("debug_log_read", { runId, limit: limit ?? null });
}
export function debugLogsClear(): Promise<number> {
  return invoke<number>("debug_logs_clear");
}

export function getMaxParallel(): Promise<number> {
  return invoke<number>("get_max_parallel");
}
export function setMaxParallel(value: number): Promise<number> {
  return invoke<number>("set_max_parallel", { value });
}
export function enqueueAnalysis(ticker: string, countryCode: string, provider: string): Promise<string> {
  return invoke<string>("enqueue_analysis", { ticker, countryCode, provider });
}
export function isHistoricalUnchanged(ticker: string, countryCode: string, provider: string): Promise<boolean> {
  return invoke<boolean>("is_historical_unchanged", { ticker, countryCode, provider });
}
export function listQueue(): Promise<{ pending: unknown[]; active: unknown[]; recent: unknown[]; max_parallel: number }> {
  return invoke<unknown>("list_queue") as Promise<{ pending: unknown[]; active: unknown[]; recent: unknown[]; max_parallel: number }>;
}

export interface BackupCounts {
  country_parameters: number;
  system_prompts: number;
  app_settings: number;
  financial_data_cache: number;
  watchlists: number;
  watchlist_items: number;
  analysis_reports: number;
}
export interface BackupMeta {
  version: number;
  app_version: string;
  schema_version: number;
  created_at: string;
  counts: BackupCounts;
}

export function backupExport(): Promise<number[]> {
  return invoke<number[]>("backup_export");
}
export function backupValidate(data: number[]): Promise<BackupMeta> {
  return invoke<BackupMeta>("backup_validate", { data });
}
export function backupRestore(data: number[]): Promise<BackupMeta> {
  return invoke<BackupMeta>("backup_restore", { data });
}
export function saveBackupFile(path: string, data: number[]): Promise<void> {
  return invoke<void>("save_backup_file", { path, data });
}