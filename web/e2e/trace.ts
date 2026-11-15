/**
 * A Chromium trace (`Page.trace`) as the scenes and benches read it: the page's thread, its events as complete ones,
 * and the layouts it ran with the objects each one laid out.
 */

import type { TraceEvent } from "./cdp";

/** The thread an event ran on, as `pid:tid`. */
export const threadKey = (e: TraceEvent): string => `${e.pid}:${e.tid}`;

/** The page's thread (the renderer's main thread that ran the most), and its events as complete ones: a B/E pair is
 * one X with its `dur`. */
export const pageThread = (events: readonly TraceEvent[]): { readonly main: string; readonly complete: readonly TraceEvent[] } => {
  const mains = new Set(events.filter((e) => e.ph === "M" && e.name === "thread_name" && e.args?.name === "CrRendererMain").map(threadKey));
  const sum = new Map<string, number>();
  for (const e of events) if (e.ph === "X" && e.name === "RunTask") sum.set(threadKey(e), (sum.get(threadKey(e)) ?? 0) + (e.dur ?? 0));
  const candidates = [...sum.keys()].filter((k) => mains.size === 0 || mains.has(k)).sort((a, b) => (sum.get(b) ?? 0) - (sum.get(a) ?? 0));
  const main = candidates[0] ?? "";
  const complete: TraceEvent[] = [];
  const open: TraceEvent[] = [];
  for (const e of events.filter((e) => threadKey(e) === main).sort((a, b) => a.ts - b.ts)) {
    if (e.ph === "X") complete.push(e);
    else if (e.ph === "B") open.push(e);
    else if (e.ph === "E") {
      const b = open.pop();
      if (b !== undefined) complete.push({ ...b, ph: "X", dur: e.ts - b.ts });
    }
  }
  return { main, complete };
};

/** One layout of the page's thread: when (µs, the trace's clock), how long (µs, and on the processor), and how many
 * layout objects it found dirty of all the tree held. An object a `content-visibility` element skips is in neither. */
export type Layout = { readonly at: number; readonly us: number; readonly cpuUs: number; readonly dirty: number; readonly total: number };

const counted = (e: TraceEvent, name: string): number => {
  const n = e.args?.beginData?.[name];
  if (typeof n !== "number") throw new Error(`a Layout event without ${name}: ${JSON.stringify(e.args)}`);
  return n;
};

export const layoutsOf = (complete: readonly TraceEvent[]): readonly Layout[] =>
  complete
    .filter((e) => e.name === "Layout")
    .map((e) => ({ at: e.ts, us: e.dur ?? 0, cpuUs: e.tdur ?? 0, dirty: counted(e, "dirtyObjects"), total: counted(e, "totalObjects") }));
