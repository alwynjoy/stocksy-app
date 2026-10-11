import type { ReactNode } from "react";

export function Card({
  title,
  children,
  action,
  className = "",
}: {
  title?: string;
  children: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <section className={`rounded-lg border border-app-border bg-app-surface ${className}`}>
      {(title || action) && (
        <header className="flex items-center justify-between border-b border-app-border px-4 py-3">
          {title && <h2 className="text-sm font-semibold text-app-text">{title}</h2>}
          {action}
        </header>
      )}
      <div className="p-4">{children}</div>
    </section>
  );
}

export function StatCard({
  label,
  value,
  sub,
  accent = "text-app-text",
}: {
  label: string;
  value: string;
  sub?: string;
  accent?: string;
}) {
  return (
    <div className="rounded-lg border border-app-border bg-app-surface p-4">
      <div className="text-xs uppercase tracking-wide text-app-muted">{label}</div>
      <div className={`mt-1 text-2xl font-semibold tabular-nums ${accent}`}>{value}</div>
      {sub && <div className="mt-1 text-xs text-app-muted">{sub}</div>}
    </div>
  );
}

export function VerdictPill({ verdict }: { verdict: string }) {
  const norm = verdict.toLowerCase().replace(/_/g, " ").trim();
  const bg =
    norm === "undervalued"
      ? "bg-app-positive"
      : norm === "overvalued"
        ? "bg-app-negative"
        : "bg-app-warning";
  const label =
    norm === "undervalued"
      ? "Undervalued"
      : norm === "overvalued"
        ? "Overvalued"
        : norm === "fairly valued"
          ? "Fairly Valued"
          : verdict || "—";
  return (
    <span className={`inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium ${bg}`} style={{ color: "white" }}>
      {label}
    </span>
  );
}

export function Loading({ label = "Loading…" }: { label?: string }) {
  return <div className="py-8 text-center text-sm text-app-muted">{label}</div>;
}

export function ErrorNote({ message }: { message: string }) {
  return (
    <div className="rounded-md border border-app-negative bg-app-negative/10 px-3 py-2 text-sm text-app-negative">
      {message}
    </div>
  );
}

export function Table({
  headers,
  children,
}: {
  headers: string[];
  children: ReactNode;
}) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-left text-sm">
        <thead>
          <tr className="border-b border-app-border text-xs uppercase tracking-wide text-app-muted">
            {headers.map((h, i) => (
              <th key={`${h}-${i}`} className="px-3 py-2 font-medium">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-app-border">{children}</tbody>
      </table>
    </div>
  );
}

export function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs font-medium text-app-muted">{label}</span>
      {children}
    </label>
  );
}

export const inputCls =
  "w-full rounded-md border border-app-border bg-app-surface px-3 py-2 text-sm text-app-text focus:outline-none focus:ring-2 focus:ring-app-text/20";

export function Button({
  children,
  onClick,
  variant = "primary",
  type = "button",
  disabled = false,
  className = "",
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "ghost";
  type?: "button" | "submit";
  disabled?: boolean;
  className?: string;
}) {
  const base =
    variant === "primary"
      ? "bg-app-text text-app-surface hover:opacity-90"
      : "border border-app-border text-app-text hover:bg-app-sidebar";
  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled}
      className={`rounded-md px-3 py-2 text-sm font-medium transition disabled:cursor-not-allowed disabled:opacity-50 ${base} ${className}`}
    >
      {children}
    </button>
  );
}