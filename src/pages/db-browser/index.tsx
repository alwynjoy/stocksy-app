import { useCallback, useEffect, useState } from "react";
import {
  getDbTable,
  listDbTables,
  type DbTable,
  type DbTableData,
} from "../../lib/api";

const LIMITS = [25, 100, 500];

interface CellRef {
  table: string;
  column: string;
  row: number;
  value: unknown;
}

function CellValue({ v }: { v: unknown }) {
  if (v === null || v === undefined) {
    return <span className="italic text-app-muted">NULL</span>;
  }
  return (
    <span className="block max-w-[24rem] overflow-hidden text-ellipsis whitespace-nowrap">
      {String(v)}
    </span>
  );
}

function FullTextModal({
  cell,
  onClose,
}: {
  cell: CellRef;
  onClose: () => void;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-8"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex max-h-[80vh] w-full max-w-3xl flex-col overflow-hidden rounded-lg border border-app-border bg-app-bg shadow-xl">
        <div className="flex items-center justify-between border-b border-app-border px-4 py-2 text-sm">
          <span className="truncate font-medium">
            {cell.table} · {cell.column}{" "}
            <span className="tabular-nums text-app-muted">#{cell.row}</span>
          </span>
          <button
            onClick={onClose}
            className="ml-3 shrink-0 rounded-md border border-app-border px-2 py-0.5 text-xs text-app-muted hover:bg-app-surface hover:text-app-text"
          >
            esc
          </button>
        </div>
        <div className="overflow-auto whitespace-pre-wrap break-words p-4 text-sm leading-relaxed">
          {String(cell.value)}
        </div>
      </div>
    </div>
  );
}

export default function DbBrowser() {
  const [tables, setTables] = useState<DbTable[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [limit, setLimit] = useState(100);
  const [page, setPage] = useState(0);
  const [data, setData] = useState<DbTableData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [cell, setCell] = useState<CellRef | null>(null);

  const refresh = useCallback(() => {
    setSelected(null);
    setData(null);
    setError(null);
    setPage(0);
    listDbTables()
      .then(setTables)
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const totalPages = data ? Math.max(1, Math.ceil(data.total / limit)) : 1;
  const offset = page * limit;
  const rangeStart = data ? (data.total === 0 ? 0 : offset + 1) : 0;
  const rangeEnd = data ? Math.min(offset + data.returned, data.total) : 0;

  // Clamp page when total shrinks (limit change or table switch)
  useEffect(() => {
    if (data && page >= totalPages) setPage(Math.max(0, totalPages - 1));
  }, [data, page, totalPages]);

  useEffect(() => {
    if (!selected) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    getDbTable(selected, limit, offset)
      .then((d) => {
        if (!cancelled) setData(d);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selected, limit, offset]);

  return (
    <div className="flex h-[calc(100vh-3rem)] overflow-hidden rounded-lg border border-app-border">
      <aside className="flex w-72 shrink-0 flex-col border-r border-app-border bg-app-surface">
        <div className="flex items-center justify-between border-b border-app-border px-3 py-2">
          <span className="text-xs font-semibold uppercase tracking-wide text-app-muted">
            Tables
          </span>
          <button
            onClick={refresh}
            className="rounded-md border border-app-border px-2 py-0.5 text-xs text-app-muted hover:bg-app-bg hover:text-app-text"
          >
            refresh
          </button>
        </div>
        <nav className="flex-1 overflow-y-auto p-2">
          {tables.map((t) => {
            const active = t.name === selected;
            const shadow = t.name.startsWith("sqlite_") || t.name.includes("_fts");
            return (
              <button
                key={t.name}
                onClick={() => {
                  setSelected(t.name);
                  setPage(0);
                }}
                className={`flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 text-left text-xs ${
                  active
                    ? "bg-app-text text-app-bg"
                    : "text-app-text hover:bg-app-bg"
                }`}
              >
                <span className={`truncate ${shadow ? "text-app-muted" : ""}`}>
                  {t.name}
                  {t.name.includes("_fts") && <span className="ml-1 text-[10px]">fts</span>}
                </span>
                <span className={active ? "" : "tabular-nums text-app-muted"}>
                  {t.rows}
                </span>
              </button>
            );
          })}
        </nav>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex flex-wrap items-center gap-3 border-b border-app-border px-4 py-2 text-xs">
          <span className="font-medium">{selected ?? "— no table selected —"}</span>
          {data && (
            <span className="tabular-nums text-app-muted">
              {rangeStart}–{rangeEnd} of {data.total} · page {page + 1}/{totalPages}
            </span>
          )}
          <span className="text-app-muted">· double-click a cell to view full text</span>
          <div className="ml-auto flex items-center gap-2 text-app-muted">
            <span>rows/page</span>
            <select
              value={limit}
              onChange={(e) => {
                setLimit(Number(e.target.value));
                setPage(0);
              }}
              className="rounded-md border border-app-border bg-app-bg px-1.5 py-0.5 text-xs text-app-text"
            >
              {LIMITS.map((l) => (
                <option key={l} value={l}>
                  {l}
                </option>
              ))}
            </select>
            {data && data.total > limit && (
              <span className="flex items-center gap-1">
                <button
                  onClick={() => setPage((p) => Math.max(0, p - 1))}
                  disabled={page === 0 || loading}
                  className="rounded border border-app-border px-1.5 py-0.5 text-xs disabled:opacity-40 hover:bg-app-bg"
                >
                  ‹ Prev
                </button>
                <span className="tabular-nums">
                  {page + 1}/{totalPages}
                </span>
                <button
                  onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
                  disabled={page >= totalPages - 1 || loading}
                  className="rounded border border-app-border px-1.5 py-0.5 text-xs disabled:opacity-40 hover:bg-app-bg"
                >
                  Next ›
                </button>
              </span>
            )}
          </div>
        </div>

        {error && (
          <div className="border-b border-app-border px-4 py-2 text-xs text-app-danger">
            {error}
          </div>
        )}

        <div className="flex-1 overflow-auto">
          {loading && <div className="p-4 text-xs text-app-muted">loading…</div>}
          {!loading && !error && data && data.rows.length === 0 && (
            <div className="p-4 text-xs text-app-muted">(no rows)</div>
          )}
          {!loading && !error && data && data.rows.length > 0 && (
            <table className="min-w-full text-xs">
              <thead className="sticky top-0 z-10 bg-app-surface">
                <tr>
                  <th className="border-b border-app-border px-3 py-1.5 text-right font-medium text-app-muted">
                    #
                  </th>
                  {data.columns.map((c) => (
                    <th
                      key={c}
                      className="whitespace-nowrap border-b border-app-border px-3 py-1.5 text-left font-medium"
                    >
                      {c}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {data.rows.map((row, ri) => (
                  <tr key={ri} className="hover:bg-app-surface">
                    <td className="border-b border-app-border px-3 py-1 text-right tabular-nums text-app-muted">
                      {offset + ri + 1}
                    </td>
                    {row.map((cellValue, ci) => (
                      <td
                        key={ci}
                        className="max-w-[24rem] cursor-default border-b border-app-border px-3 py-1"
                        onDoubleClick={() =>
                          setCell({
                            table: data.name,
                            column: data.columns[ci],
                            row: offset + ri + 1,
                            value: cellValue,
                          })
                        }
                        title={cellValue === null || cellValue === undefined ? "NULL" : String(cellValue)}
                      >
                        <CellValue v={cellValue} />
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
            )}
          </div>
          {data && data.total > limit && (
            <div className="flex items-center justify-between border-t border-app-border bg-app-surface px-4 py-2 text-xs">
              <span className="tabular-nums text-app-muted">
                {rangeStart}–{rangeEnd} of {data.total} rows
              </span>
              <span className="flex items-center gap-1">
                <button
                  onClick={() => setPage(0)}
                  disabled={page === 0 || loading}
                  className="rounded border border-app-border px-2 py-1 disabled:opacity-40 hover:bg-app-bg"
                >
                  « First
                </button>
                <button
                  onClick={() => setPage((p) => Math.max(0, p - 1))}
                  disabled={page === 0 || loading}
                  className="rounded border border-app-border px-2 py-1 disabled:opacity-40 hover:bg-app-bg"
                >
                  ‹ Prev
                </button>
                <span className="tabular-nums px-2">
                  Page {page + 1} / {totalPages}
                </span>
                <button
                  onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
                  disabled={page >= totalPages - 1 || loading}
                  className="rounded border border-app-border px-2 py-1 disabled:opacity-40 hover:bg-app-bg"
                >
                  Next ›
                </button>
                <button
                  onClick={() => setPage(totalPages - 1)}
                  disabled={page >= totalPages - 1 || loading}
                  className="rounded border border-app-border px-2 py-1 disabled:opacity-40 hover:bg-app-bg"
                >
                  Last »
                </button>
              </span>
              <span className="text-app-muted">{limit} / page</span>
            </div>
          )}
        </section>

      {cell && <FullTextModal cell={cell} onClose={() => setCell(null)} />}
    </div>
  );
}