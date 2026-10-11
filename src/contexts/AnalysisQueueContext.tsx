/* eslint-disable @typescript-eslint/no-explicit-any */
import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface QueueJob {
  id: string;
  ticker: string;
  country_code: string;
  provider: string;
  model: string;
  status: "queued" | "running" | "completed" | "failed" | "already_exists";
  enqueued_at: string;
  started_at?: string | null;
  finished_at?: string | null;
  report_id?: string | null;
  message?: string | null;
  thinking: string;
  steps: { id: string; label: string; status: string; message?: string | null }[];
  debug_log_id?: string | null;
}

interface QueueSnapshot {
  pending: QueueJob[];
  active: QueueJob[];
  recent: QueueJob[];
  max_parallel: number;
}

interface Ctx {
  pending: QueueJob[];
  active: QueueJob[];
  recent: QueueJob[];
  jobs: QueueJob[]; // pending + active + recent (display stacks)
  maxParallel: number;
  enqueue: (ticker: string, countryCode: string, provider: string) => Promise<string>;
  setMaxParallel: (v: number) => Promise<void>;
  refresh: () => Promise<void>;
}

const AnalysisQueueContext = createContext<Ctx | null>(null);

export function AnalysisQueueProvider({ children }: { children: React.ReactNode }) {
  const [pending, setPending] = useState<QueueJob[]>([]);
  const [active, setActive] = useState<QueueJob[]>([]);
  const [recent, setRecent] = useState<QueueJob[]>([]);
  const [maxParallel, setMaxParallel] = useState(1);

  const refresh = useCallback(async () => {
    try {
      const snap = await invoke<QueueSnapshot>("list_queue");
      setPending(snap.pending);
      setActive(snap.active);
      setRecent(snap.recent);
      setMaxParallel(snap.max_parallel);
    } catch {
      // ignore
    }
  }, []);

  const enqueue = useCallback(async (ticker: string, countryCode: string, provider: string) => {
    const jobId = await invoke<string>("enqueue_analysis", { ticker, countryCode, provider });
    // Optimistic refresh
    await refresh();
    return jobId;
  }, [refresh]);

  const setMaxParallelInner = useCallback(async (v: number) => {
    const clamped = Math.max(1, Math.min(5, v));
    await invoke("set_max_parallel", { value: clamped });
    setMaxParallel(clamped);
    await refresh();
  }, [refresh]);

  useEffect(() => {
    refresh();
    // StrictMode-safe: avoid double listeners when React mounts/unmounts quickly in dev.
    // Each listen() is async; if cancelled before it resolves, immediately unlisten.
    let cancelled = false;
    const unlistens: Array<() => void> = [];
    const add = (p: Promise<() => void>) => {
      p.then(fn => {
        if (cancelled) fn();
        else unlistens.push(fn);
      });
    };
    add(listen("queue://update", (e: any) => {
      const p = e.payload as QueueSnapshot;
      if (p.pending) setPending(p.pending);
      if (p.active) setActive(p.active);
      if (p.recent) setRecent(p.recent);
      if (typeof p.max_parallel === "number") setMaxParallel(p.max_parallel);
    }));

    // Per-job step updates: payload {jobId, idx, status}
    add(listen("pipeline://step", (e: any) => {
      const { jobId, idx, status, message } = e.payload as { jobId?: string; idx: number; status: string; message?: string };
      if (!jobId) return; // global legacy emit without jobId — ignore for queue
      const update = (list: QueueJob[]) => list.map(j => {
        if (j.id !== jobId) return j;
        const next = [...j.steps];
        if (next[idx]) next[idx] = { ...next[idx], status, message: message ?? null };
        return { ...j, steps: next, status: status === "failed" ? "failed" as const : j.status };
      });
      setActive(prev => update(prev));
      setPending(prev => update(prev));
      setRecent(prev => update(prev));
    }));

    // Single global token stream — frontend stays per-token immediate; log aggregates separately.
    add(listen("llm://token", (e: any) => {
      const { delta, jobId } = e.payload as { delta: string; jobId?: string };
      if (!jobId || !delta) return;
      const append = (list: QueueJob[]) => list.map(j => j.id === jobId ? { ...j, thinking: j.thinking + delta } : j);
      setActive(prev => append(prev));
      setPending(prev => append(prev));
    }));

    add(listen("pipeline://tool", () => {
      void 0; // placeholder for future tool progress
    }));

    add(listen("debug://log_path", (e: any) => {
      const { jobId, run_id } = e.payload as { jobId?: string; run_id: string };
      if (!jobId) return;
      const upd = (list: QueueJob[]) => list.map(j => j.id === jobId ? { ...j, debug_log_id: run_id } : j);
      setActive(prev => upd(prev));
      setPending(prev => upd(prev));
    }));

    return () => {
      cancelled = true;
      unlistens.forEach(fn => { try { fn(); } catch (_e) { void _e; } });
    };
  }, [refresh]);

  const jobs = [...active, ...pending, ...recent].sort((a,b) => b.enqueued_at.localeCompare(a.enqueued_at));

  return (
    <AnalysisQueueContext.Provider value={{ pending, active, recent, jobs, maxParallel, enqueue, setMaxParallel: setMaxParallelInner, refresh }}>
      {children}
    </AnalysisQueueContext.Provider>
  );
}

export function useAnalysisQueue() {
  const ctx = useContext(AnalysisQueueContext);
  if (!ctx) throw new Error("useAnalysisQueue must be inside AnalysisQueueProvider");
  return ctx;
}
