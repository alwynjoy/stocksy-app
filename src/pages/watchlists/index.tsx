import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Plus, Pencil, Trash2, X } from "lucide-react";
import {
  watchlistsList,
  watchlistCreate,
  watchlistUpdate,
  watchlistDelete,
  watchlistItems,
  watchlistRemoveItem,
  watchlistUpdateItem,
  type WatchlistRow,
  type WatchlistItemEnriched,
} from "../../lib/api";
import { Button, Card, ErrorNote, Loading, VerdictPill } from "../../components/ui";
import { fmtPct, fmtMoneyCurrency, gapCls, inputCls } from "../../lib/format";

export default function Watchlists() {
  const navigate = useNavigate();
  const [lists, setLists] = useState<WatchlistRow[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [items, setItems] = useState<WatchlistItemEnriched[] | null>(null);
  const [itemsErr, setItemsErr] = useState<string | null>(null);

  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newDesc, setNewDesc] = useState("");
  const [createErr, setCreateErr] = useState<string | null>(null);

  const [editing, setEditing] = useState<WatchlistRow | null>(null);
  const [editName, setEditName] = useState("");
  const [editDesc, setEditDesc] = useState("");

  const [editingItem, setEditingItem] = useState<WatchlistItemEnriched | null>(null);
  const [editNotes, setEditNotes] = useState("");
  const [editTarget, setEditTarget] = useState("");

  const loadLists = () => {
    setErr(null);
    watchlistsList()
      .then((ls) => {
        setLists(ls);
        if (ls.length > 0 && !selectedId) setSelectedId(ls[0].id);
        else if (ls.length === 0) setSelectedId(null);
      })
      .catch(() => setErr("Could not load watchlists (run via `tauri dev`)."));
  };

  useEffect(() => {
    loadLists();
  }, []);

  useEffect(() => {
    if (!selectedId) {
      setItems(null);
      return;
    }
    setItems(null);
    setItemsErr(null);
    watchlistItems(selectedId)
      .then(setItems)
      .catch(() => setItemsErr("Could not load items."));
  }, [selectedId]);

  const handleCreate = async () => {
    setCreateErr(null);
    const n = newName.trim();
    if (!n) {
      setCreateErr("Name required");
      return;
    }
    try {
      const res = await watchlistCreate(n, newDesc.trim() || null);
      setShowCreate(false);
      setNewName("");
      setNewDesc("");
      const ls = await watchlistsList();
      setLists(ls);
      setSelectedId(res.id);
    } catch (e) {
      setCreateErr(typeof e === "string" ? e : "Create failed");
    }
  };

  const handleUpdate = async () => {
    if (!editing) return;
    try {
      await watchlistUpdate(editing.id, editName.trim(), editDesc.trim() || null);
      setEditing(null);
      loadLists();
    } catch (e) {
      alert(typeof e === "string" ? e : "Update failed");
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm("Delete this watchlist? Items will be removed.")) return;
    try {
      await watchlistDelete(id);
      const ls = await watchlistsList();
      setLists(ls);
      setSelectedId(ls.length ? ls[0].id : null);
    } catch (e) {
      alert(typeof e === "string" ? e : "Delete failed");
    }
  };

  const handleRemoveItem = async (itemId: string) => {
    try {
      await watchlistRemoveItem(itemId);
      if (selectedId) setItems(await watchlistItems(selectedId));
    } catch (e) {
      alert(typeof e === "string" ? e : "Remove failed");
    }
  };

  const handleUpdateItem = async () => {
    if (!editingItem) return;
    const tp = editTarget.trim() === "" ? null : Number(editTarget);
    if (editTarget.trim() !== "" && !Number.isFinite(tp as number)) {
      alert("Target price must be a number");
      return;
    }
    try {
      await watchlistUpdateItem(editingItem.id, editNotes.trim() || null, tp);
      setEditingItem(null);
      if (selectedId) setItems(await watchlistItems(selectedId));
    } catch (e) {
      alert(typeof e === "string" ? e : "Update failed");
    }
  };

  if (err) return <ErrorNote message={err} />;
  if (!lists) return <Loading />;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-app-text">Watchlists</h1>
        <Button onClick={() => setShowCreate(true)}>
          <Plus className="mr-1 inline h-4 w-4" />
          New watchlist
        </Button>
      </div>

      {lists.length === 0 ? (
        <Card>
          <p className="py-6 text-center text-sm text-app-muted">No watchlists yet. Create one to start tracking.</p>
        </Card>
      ) : (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-[280px_1fr]">
          {/* Left: watchlist list */}
          <Card title="My watchlists">
            <div className="space-y-2">
              {lists.map((wl) => (
                <div
                  key={wl.id}
                  onClick={() => setSelectedId(wl.id)}
                  className={`flex cursor-pointer items-center justify-between rounded-md border px-3 py-2 ${selectedId === wl.id ? "border-app-text bg-app-sidebar" : "border-app-border hover:bg-app-sidebar"}`}
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-app-text">{wl.name}</div>
                    {wl.description && <div className="truncate text-xs text-app-muted">{wl.description}</div>}
                  </div>
                  <div className="ml-2 flex shrink-0 items-center gap-1">
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        setEditing(wl);
                        setEditName(wl.name);
                        setEditDesc(wl.description || "");
                      }}
                      className="rounded p-1 text-app-muted hover:text-app-text"
                      title="Edit"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDelete(wl.id);
                      }}
                      className="rounded p-1 text-app-negative hover:bg-red-50"
                      title="Delete"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </Card>

          {/* Right: items */}
          <Card
            title={lists.find((l) => l.id === selectedId)?.name || "Watchlist"}
            action={
              <span className="text-xs text-app-muted">
                {items ? `${items.length} tickers` : "—"}
              </span>
            }
          >
            {itemsErr && <ErrorNote message={itemsErr} />}
            {!items ? (
              <Loading label="Loading items…" />
            ) : items.length === 0 ? (
              <div className="py-8 text-center">
                <p className="text-sm text-app-muted">Empty watchlist.</p>
                <p className="mt-1 text-xs text-app-muted">Add stocks from a Report (lens icon) or the Screener row action.</p>
              </div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr className="border-b border-app-border text-xs uppercase tracking-wide text-app-muted">
                      <th className="px-3 py-2 font-medium">Ticker</th>
                      <th className="px-3 py-2 font-medium">Company</th>
                      <th className="px-3 py-2 font-medium">Price</th>
                      <th className="px-3 py-2 font-medium">Fair value</th>
                      <th className="px-3 py-2 font-medium">Gap</th>
                      <th className="px-3 py-2 font-medium">Verdict</th>
                      <th className="px-3 py-2 font-medium">Target</th>
                      <th className="px-3 py-2 font-medium">Notes</th>
                      <th className="px-3 py-2 font-medium"></th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-app-border">
                    {items.map((it) => {
                      const gap = it.valuation_gap_pct != null ? it.valuation_gap_pct * 100 : null;
                      return (
                        <tr
                          key={it.id}
                          onClick={() => navigate(`/report/${it.ticker}`)}
                          className="cursor-pointer transition hover:bg-app-sidebar"
                        >
                          <td className="px-3 py-2.5 font-medium text-app-text">
                            {it.ticker}
                            <span className="ml-1 text-xs text-app-muted">· {it.country_code}</span>
                          </td>
                          <td className="px-3 py-2.5 text-app-muted">{it.company_name || "—"}</td>
                          <td className="px-3 py-2.5 tabular-nums text-app-text">{it.current_price != null ? fmtMoneyCurrency(it.current_price, it.currency_code || "USD") : "—"}</td>
                          <td className="px-3 py-2.5 tabular-nums text-app-text">{it.weighted_fair_value != null ? fmtMoneyCurrency(it.weighted_fair_value, it.currency_code || "USD") : "—"}</td>
                          <td className={`px-3 py-2.5 tabular-nums ${gap != null ? gapCls(gap) : ""}`}>{gap != null ? fmtPct(gap) : "—"}</td>
                          <td className="px-3 py-2.5">{it.valuation_verdict ? <VerdictPill verdict={it.valuation_verdict} /> : <span className="text-xs text-app-muted">—</span>}</td>
                          <td className="px-3 py-2.5 tabular-nums text-app-text">{it.target_price != null ? fmtMoneyCurrency(it.target_price, it.currency_code || "USD") : "—"}</td>
                          <td className="max-w-[160px] truncate px-3 py-2.5 text-xs italic text-app-muted">{it.notes || "—"}</td>
                          <td className="px-3 py-2.5">
                            <div className="flex gap-1" onClick={(e) => e.stopPropagation()}>
                              <button
                                type="button"
                                onClick={() => {
                                  setEditingItem(it);
                                  setEditNotes(it.notes || "");
                                  setEditTarget(it.target_price != null ? String(it.target_price) : "");
                                }}
                                className="rounded border border-app-border px-2 py-1 text-xs hover:bg-app-sidebar"
                              >
                                Edit
                              </button>
                              <button
                                type="button"
                                onClick={() => handleRemoveItem(it.id)}
                                className="rounded border border-app-border px-2 py-1 text-xs text-app-negative hover:bg-red-50"
                              >
                                Remove
                              </button>
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </Card>
        </div>
      )}

      {/* Create dialog */}
      {showCreate && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
          <div className="w-full max-w-md rounded-lg border border-app-border bg-app-surface p-4 shadow-xl">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold">New watchlist</h3>
              <button type="button" onClick={() => setShowCreate(false)} className="rounded p-1 hover:bg-app-sidebar">
                <X className="h-4 w-4" />
              </button>
            </div>
            {createErr && <div className="mt-3 rounded bg-app-negative/10 px-2 py-1 text-xs text-app-negative">{createErr}</div>}
            <label className="mt-3 block text-xs font-medium text-app-muted">
              Name
              <input className={`${inputCls} mt-1`} value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="e.g. Tech leaders" />
            </label>
            <label className="mt-3 block text-xs font-medium text-app-muted">
              Description (optional)
              <input className={`${inputCls} mt-1`} value={newDesc} onChange={(e) => setNewDesc(e.target.value)} placeholder="Optional" />
            </label>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setShowCreate(false)}>Cancel</Button>
              <Button onClick={handleCreate}>Create</Button>
            </div>
          </div>
        </div>
      )}

      {/* Edit watchlist dialog */}
      {editing && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
          <div className="w-full max-w-md rounded-lg border border-app-border bg-app-surface p-4 shadow-xl">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold">Edit watchlist</h3>
              <button type="button" onClick={() => setEditing(null)} className="rounded p-1 hover:bg-app-sidebar">
                <X className="h-4 w-4" />
              </button>
            </div>
            <label className="mt-3 block text-xs font-medium text-app-muted">
              Name
              <input className={`${inputCls} mt-1`} value={editName} onChange={(e) => setEditName(e.target.value)} />
            </label>
            <label className="mt-3 block text-xs font-medium text-app-muted">
              Description
              <input className={`${inputCls} mt-1`} value={editDesc} onChange={(e) => setEditDesc(e.target.value)} />
            </label>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setEditing(null)}>Cancel</Button>
              <Button onClick={handleUpdate}>Save</Button>
            </div>
          </div>
        </div>
      )}

      {/* Edit item dialog */}
      {editingItem && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
          <div className="w-full max-w-md rounded-lg border border-app-border bg-app-surface p-4 shadow-xl">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold">Edit item — {editingItem.ticker}</h3>
              <button type="button" onClick={() => setEditingItem(null)} className="rounded p-1 hover:bg-app-sidebar">
                <X className="h-4 w-4" />
              </button>
            </div>
            <label className="mt-3 block text-xs font-medium text-app-muted">
              Target price (optional)
              <input className={`${inputCls} mt-1`} value={editTarget} onChange={(e) => setEditTarget(e.target.value)} placeholder="e.g. 1200" type="number" />
            </label>
            <label className="mt-3 block text-xs font-medium text-app-muted">
              Notes (optional)
              <textarea className={`${inputCls} mt-1`} value={editNotes} onChange={(e) => setEditNotes(e.target.value)} rows={3} placeholder="Thesis, reminders…" />
            </label>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setEditingItem(null)}>Cancel</Button>
              <Button onClick={handleUpdateItem}>Save</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
