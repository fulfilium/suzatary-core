/**
 * A Chromium trace (`Page.trace`) as the scenes and benches read it: the page's thread, its events as complete ones,
 * and the layouts it ran with the objects each one laid out.
 */

import type { TraceEvent } from "./cdp";

/** The thread an event ran on, as `pid:tid`. */
export const threadKey = (e: TraceEvent): string => `${e.pid}:${e.tid}`;

/** The thread of the dedicated worker whose script's URL ends with `script`, as `pid:tid`; "" when the trace saw none start. */
export const workerThread = (events: readonly TraceEvent[], script: string): string => {
  const started = events.find((e) => e.name === "TracingSessionIdForWorker" && String(e.args?.data?.["url"] ?? "").endsWith(script));
  return started === undefined ? "" : `${started.pid}:${String(started.args?.data?.["workerThreadId"])}`;
};

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

/** The categories a trace needs for `tasksOf`: each task of every thread with its processor time, and the page's marks. */
export const TASKS = ["disabled-by-default-devtools.timeline", "blink.user_timing"] as const;

/** One task of the page's thread: when it began, on the page's clock, and how long it held the thread: on the clock (`ms`),
 * on the processor as the trace says (`threadMs`, the time the thread itself ran, which another process running on its
 * core does not stretch) and as the judge reads it (`cpuMs`, the smaller of the two). A task cannot run longer on the
 * processor than it lasted on the clock, so a `threadMs` over `ms` is the runner's (a thread clock that jumped across
 * a descheduling of its virtual core), not the page's. */
export type Task = { readonly at: number; readonly ms: number; readonly threadMs: number; readonly cpuMs: number };

/** The task one `tdur` and `dur` (µs) of a trace event are of. A task the browser did not time on the processor (it
 * leaves out `tdur` for the shortest) counts its time on the clock, which bounds its processor time from above. */
export const taskOf = (at: number, dur: number, tdur: number | undefined): Task => {
  const ms = dur / 1000;
  const threadMs = (tdur ?? dur) / 1000;
  return { at, ms, threadMs, cpuMs: Math.min(threadMs, ms) };
};

/** The task that held the processor longest by `cpuMs`, or null without any. */
export const heaviestOf = (tasks: readonly Task[]): Task | null => tasks.reduce<Task | null>((a, t) => (a === null || t.cpuMs > a.cpuMs ? t : a), null);

/** The top-level tasks of the thread that made the mark `mark`, placed on its document's clock by the latest such mark
 * (its `startTime` against its trace time). */
export const tasksOf = (events: readonly TraceEvent[], mark: string): readonly Task[] => {
  const m = events.filter((e) => e.name === mark && e.cat.includes("user_timing")).at(-1);
  const start = m?.args?.data?.["startTime"];
  if (m === undefined || typeof start !== "number") throw new Error(`the trace holds no mark ${mark} with its startTime`);
  const offset = m.ts / 1000 - start;
  const runs = events.filter((e) => e.ph === "X" && e.name === "RunTask" && threadKey(e) === threadKey(m)).sort((a, b) => a.ts - b.ts);
  const tasks: Task[] = [];
  let end = -Infinity;
  for (const e of runs) {
    if (e.ts < end) continue;
    const dur = e.dur ?? 0;
    end = e.ts + dur;
    tasks.push(taskOf(e.ts / 1000 - offset, dur, e.tdur));
  }
  return tasks;
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

/** Each User Timing measure named `name` the page took: when it began and ended (µs, the trace's clock) and its detail
 * as JSON (`"null"` without one). */
export const measured = (events: readonly TraceEvent[], name: string): readonly (readonly [number, number, string])[] => {
  const begun = new Map<string, TraceEvent>();
  const out: (readonly [number, number, string])[] = [];
  for (const e of events) {
    if (e.name !== name || !e.cat.includes("user_timing")) continue;
    const id = `${e.id ?? ""}:${e.id2?.local ?? e.id2?.global ?? ""}`;
    if (e.ph === "b") begun.set(id, e);
    else if (e.ph === "e") {
      const b = begun.get(id);
      if (b !== undefined) out.push([b.ts, e.ts, b.args?.detail ?? "null"]);
    }
  }
  return out;
};
