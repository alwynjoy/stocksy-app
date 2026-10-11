export const inputCls =
  "w-full rounded-md border border-app-border bg-app-surface px-3 py-2 text-sm text-app-text focus:outline-none focus:ring-2 focus:ring-app-text/20";

export function fmtPct(v: number): string {
  const sign = v > 0 ? "+" : "";
  return `${sign}${v.toFixed(1)}%`;
}

export function fmtNum(v: number): string {
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(v);
}

export function fmtMoney(v: number): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 2,
  }).format(v);
}

export function fmtMoneyCurrency(v: number, currency: string): string {
  const cur = currency || "USD";
  try {
    return new Intl.NumberFormat("en-US", {
      style: "currency",
      currency: cur,
      maximumFractionDigits: 2,
    }).format(v);
  } catch {
    return fmtMoney(v);
  }
}

export function gapCls(v: number): string {
  if (v > 3) return "text-app-positive";
  if (v < -3) return "text-app-negative";
  return "text-app-muted";
}