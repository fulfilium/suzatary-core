/**
 * A PHASE OF THE BIGSTORE SCENE, TRACED: what the page's thread runs inside the longest task of a conversation opening,
 * or of the stream of conversations drawn, over the bigstore scene's store. `SUZATARY_OPENPROF_PHASE=opening` (the
 * default) traces from the hash change to `#/secretary/bulk-0` until its window is placed; `stream` traces from the hash
 * change to `#/secretary` until the stream's rounds are drawn, each visit a new document (the first `cold`, the device
 * holding nothing). Either ends once the page has run no long task and no painter's step for 1.5 s. Each phase is traced (`Page.trace`: the browser's timeline events, V8's CPU samples and the page's
 * User Timing, on the trace's clock), and the longest task of the page's thread is broken down by the timeline events'
 * own time, by each layout's dirty objects, and by the sampled functions read through the bundle's source maps. Each
 * opening also prints its ORDER: every task of the page's thread from the hash change to the window placed, with the
 * painter's steps, animation frames and layouts inside it, and the window it drew (turns, rounds, how many in view).
 * For the opening's largest layout it prints what that layout laid out (its roots as `tag#id.class`) and every task
 * of the page's thread between the window's placing and that layout, each timer firing among them with its timeout
 * and the function it called. The page takes a census of `body`'s and `main#view`'s children (tag, id, class,
 * `display`, descendants) at every long task and at the end.
 * No gate runs it: `SUZATARY_OPENPROF=<file> SUZATARY_OPENPROF_VISITS=12 SUZATARY_OPENPROF_RATE=<n>
 * scripts/web-e2e.sh openprof` does, by name. `SUZATARY_OPENPROF_CATEGORIES` adds trace categories, comma-separated:
 * `disabled-by-default-devtools.timeline.invalidationTracking` (what dirtied a layout),
 * `disabled-by-default-devtools.timeline.stack` (the script stack of each layout, timer and style recalculation),
 * `loading` and `blink`. Each one costs the page's thread time and can move the order it measures.
 * `SUZATARY_OPENPROF_PRESSURE=1` sends a critical memory-pressure notice once an opening settles and traces 2 s more:
 * the browser then drops caches it can rebuild. `SUZATARY_OPENPROF_LATE_FONT=<fragment of a font's URL>` holds that
 * font's request, with the cache off, until the opening settles. `SUZATARY_OPENPROF_NEW_DOCUMENT=1` loads each warm
 * opening's visit as a new document. Every `document.fonts` event of a visit is logged. `SUZATARY_E2E_JS_FLAGS` hands
 * V8 its switches (`--no-wasm-lazy-compilation`, say). Each visit also prints the longest task's time by kind (`kinds`:
 * script, compilation, layout, style, garbage collection, HTML parsing, painting) and how many functions the page's
 * thread compiled from the phase's start on (`compiled`: wasm and JavaScript).
 */

import assert from "node:assert/strict"
import { writeFileSync } from "node:fs"

import marks from "../src/chat/marks"
import type { TraceEvent } from "./cdp"
import { PHONE, STEP_MS, suite, trustThisDevice } from "./harness"
import { BULK, OBSERVE as LONG, placed, settle as settled } from "./longtasks"
import { mapped } from "./sourcemap"
import { layoutsOf, measured, pageThread, threadKey } from "./trace"

const LINES_BASE = process.env["E2E_BASE_LINES"] ?? ""
const LINES_INSTANCE = process.env["E2E_LINES_INSTANCE"] ?? ""
const OUT = process.env["SUZATARY_OPENPROF"] ?? ""
const VISITS = Number(process.env["SUZATARY_OPENPROF_VISITS"] ?? "6")
const RATE = Number(process.env["SUZATARY_OPENPROF_RATE"] ?? "1")
type Phase = "opening" | "stream"
const PHASE: Phase = ((p: string): Phase => {
  if (p === "opening" || p === "stream") return p
  throw new Error(`SUZATARY_OPENPROF_PHASE is opening or stream, not ${p}`)
})(process.env["SUZATARY_OPENPROF_PHASE"] ?? "opening")
// Marked as the phase starts: the trace is read from it on.
const OPEN = "openprof:open"
// After an opening settles: a critical memory-pressure notice, and the trace kept for PRESSURE_MS more.
const PRESSURE = process.env["SUZATARY_OPENPROF_PRESSURE"] === "1"
const PRESSURE_MS = 2000
// A font file (a fragment of its URL) whose request is held, with the cache off, from each visit's load until its
// opening settles: the face then arrives after the window is drawn, as it does on a machine that needs it late.
const LATE_FONT = process.env["SUZATARY_OPENPROF_LATE_FONT"] ?? ""
// Each warm visit a new document (a query of its own), or, by default, the same query again, which changes only the
// fragment: then what one opening left in the document (a dropped font, say) meets the next opening.
const NEW_DOCUMENT = process.env["SUZATARY_OPENPROF_NEW_DOCUMENT"] === "1"

const app = await suite("openprof", PHONE)

// --- the trace ------------------------------------------------------------------

type Frame = { readonly functionName?: string; readonly url?: string; readonly lineNumber?: number; readonly columnNumber?: number; readonly codeType?: string }
type Node = { readonly id: number; readonly parent?: number; readonly callFrame: Frame }
type Ev = TraceEvent
const CATEGORIES = [
  "devtools.timeline",
  "disabled-by-default-devtools.timeline",
  "v8",
  "v8.wasm",
  "v8.execute",
  "disabled-by-default-v8.gc",
  "disabled-by-default-v8.compile",
  "disabled-by-default-v8.cpu_profiler",
  "blink.user_timing",
  ...(process.env["SUZATARY_OPENPROF_CATEGORIES"] ?? "").split(",").filter((c) => c !== ""),
]

const ms = (us: number): number => Math.round(us / 100) / 10

// What a timeline event's own time was spent on. `wasm` is V8 compiling a wasm function (lazily, on its first call, or
// tiering it up); `js` is V8 parsing or compiling JavaScript; `parse` is the HTML parser (typst's HTML through
// `rich.ts`); `script` is the rest of a call into JavaScript and the wasm it runs.
const KINDS = ["script", "wasm", "js", "layout", "style", "gc", "parse", "paint", "other"] as const
type Kind = (typeof KINDS)[number]
const kindOf = (name: string): Kind =>
  /^wasm\.|^v8\.wasm\./i.test(name) ? "wasm"
  : /Compile|Parse(?!HTML)|Deserialize|Ignition|Sparkplug|Maglev|Turbofan|Optimize/i.test(name) && !/^Parse(HTML|AuthorStyleSheet)/.test(name) ? "js"
  : name === "Layout" || name === "UpdateLayoutTree" ? (name === "Layout" ? "layout" : "style")
  : /GC|Scavenge|MarkCompact|Sweep/i.test(name) ? "gc"
  : name === "ParseHTML" || name === "ParseAuthorStyleSheet" ? "parse"
  : /Paint|Commit|Composite|Layerize|PrePaint|UpdateLayer|Raster|HitTest|IntersectionObserver/.test(name) ? "paint"
  : /FunctionCall|EvaluateScript|v8\.run|V8\.Execute|RunMicrotasks|FireAnimationFrame|EventDispatch|TimerFire|v8\.callFunction|TimerInstall|RequestAnimationFrame|v8\.evaluateModule/.test(name) || name.startsWith("FunctionCall") || name.startsWith("EvaluateScript") ? "script"
  : "other"

// Functions the page's thread compiled from the phase's start on: each wasm function's first compilation (a lazy one)
// and each JavaScript compilation, counted by its trace event, and the index of each wasm function compiled lazily
// (`lazy`), which a build of the module that keeps its name section names.
type Compiled = { readonly wasm: number; readonly wasmMs: number; readonly js: number; readonly jsMs: number; readonly names: readonly (readonly [string, number])[]; readonly lazy: readonly number[] }
const compiledOf = (events: readonly Ev[]): Compiled => {
  const from = openedAt(events)
  const all = pageThread(events).complete.filter((e) => e.ts >= from)
  const by = new Map<string, number>()
  let [wasm, wasmUs, js, jsUs] = [0, 0, 0, 0]
  for (const e of all) {
    const k = kindOf(e.name)
    if (k !== "wasm" && k !== "js") continue
    by.set(e.name, (by.get(e.name) ?? 0) + 1)
    if (k === "wasm") [wasm, wasmUs] = [wasm + 1, wasmUs + (e.dur ?? 0)]
    else [js, jsUs] = [js + 1, jsUs + (e.dur ?? 0)]
  }
  const lazy = all.filter((e) => e.name === "wasm.CompileLazy").map((e) => e.args?.func_index ?? -1)
  return { wasm, wasmMs: ms(wasmUs), js, jsMs: ms(jsUs), names: [...by].sort((a, b) => b[1] - a[1]).slice(0, 12), lazy }
}

const labelOf = (e: Ev): string => {
  const d = e.args?.data
  if (d === undefined) return e.name
  if (e.name === "FunctionCall") return `FunctionCall ${String(d["functionName"] ?? "")} ${String(d["url"] ?? "").replace(/^.*\//, "")}`.trim()
  if (e.name === "EventDispatch") return `EventDispatch ${String(d["type"] ?? "")}`
  if (e.name === "EvaluateScript" || e.name === "v8.compile") return `${e.name} ${String(d["url"] ?? "").replace(/^.*\//, "")}`.trim()
  return e.name
}

type Broken = {
  readonly taskMs: number
  // The task's time on the processor: far under `taskMs` when the thread waited for a core.
  readonly taskCpuMs?: number
  // Each layout inside the task: [ms, ms on the processor, dirty objects, all objects].
  readonly layouts?: readonly (readonly [number, number, number, number])[]
  // Each style recalculation inside it: [ms, elements].
  readonly styles?: readonly (readonly [number, number])[]
  readonly tasksOver50: readonly number[]
  // The task's events' own time by kind (`kindOf`), in ms.
  readonly kinds?: Readonly<Record<Kind, number>>
  readonly events: readonly (readonly [string, number])[]
  readonly sampledMs: number
  readonly self: readonly (readonly [string, number])[]
  readonly on: readonly (readonly [string, number])[]
}

// One task of the page's thread as the opening ran it: [ms since the opening began, ms, what ran inside it]. Inside: `rAF`
// (an animation frame's callbacks), `step <input> <bytes>` (a painter's step, the input it took and the size of its
// patches), `style <elements>`, `layout <dirty>/<all> <ms>`, `gc`, and `placed` (the window's mark).
type Ran = readonly [number, number, string]

const order = (events: readonly Ev[]): readonly Ran[] => {
  const thread = pageThread(events).complete
  const layouts = new Map(layoutsOf(thread).map((l) => [l.at, l]))
  const begin = openedAt(events)
  const steps = measured(events, "painter:step")
  const placedAt = events.filter((e) => e.name === marks.PLACED).map((e) => e.ts)
  const tasks = thread.filter((e) => e.name === "RunTask" && e.ts >= begin).sort((a, b) => a.ts - b.ts)
  return tasks.flatMap((t): Ran[] => {
    const [t0, t1] = [t.ts, t.ts + (t.dur ?? 0)]
    const inside = thread.filter((e) => e.ts >= t0 && e.ts + (e.dur ?? 0) <= t1 && e !== t).sort((a, b) => a.ts - b.ts)
    const said: string[] = []
    for (const e of inside) {
      if (e.name === "FireAnimationFrame") said.push("rAF")
      else if (e.name === "UpdateLayoutTree") said.push(`style ${e.args?.elementCount ?? "?"}`)
      else if (e.name === "Layout") said.push(`layout ${layouts.get(e.ts)?.dirty ?? "?"}/${layouts.get(e.ts)?.total ?? "?"} ${ms(e.dur ?? 0)}`)
      else if (e.name === "MajorGC" || e.name === "V8.GC_MARK_COMPACTOR") said.push("gc")
    }
    for (const [b, , detail] of steps) {
      if (b < t0 || b > t1) continue
      const d = JSON.parse(detail) as { readonly step?: string; readonly bytes?: number } | null
      said.push(d?.step === undefined ? `step raw ${detail.slice(0, 120)}` : `step ${d.step} ${d.bytes ?? "?"}`)
    }
    if (placedAt.some((at) => at >= t0 && at <= t1)) said.push("placed")
    const notable = said.some((w) => w.startsWith("step") || w.startsWith("layout") || w === "placed") || (t.dur ?? 0) > 10_000
    return notable ? [[ms(t0 - begin), ms(t.dur ?? 0), said.join(", ")]] : []
  })
}

// When the opening began, on the trace's clock.
const openedAt = (events: readonly Ev[]): number => {
  const mark = events.find((e) => e.name === OPEN)
  if (mark === undefined) throw new Error("the trace holds no mark of the opening")
  return mark.ts
}

// V8's samples of the thread `main` inside `windows` (µs, the trace's clock): each function's own time (`top`), the time
// on the stack of each of the app's functions and of the wasm modules (`on`), and the time sampled.
const sampledIn = async (
  events: readonly Ev[],
  main: string,
  windows: readonly (readonly [number, number])[],
): Promise<{ readonly top: ReadonlyMap<string, number>; readonly on: ReadonlyMap<string, number>; readonly sampled: number }> => {
  const profiles = events.filter((e) => e.name === "Profile" && threadKey(e) === main)
  const top = new Map<string, number>()
  const on = new Map<string, number>()
  let sampled = 0
  for (const profile of profiles) {
    const nodes = new Map<number, Node>()
    let at = Number(profile.args?.data?.["startTime"] ?? profile.ts)
    const chunks = events.filter((e) => e.name === "ProfileChunk" && e.id === profile.id && e.pid === profile.pid).sort((a, b) => a.ts - b.ts)
    for (const chunk of chunks) {
      const data = chunk.args?.data ?? {}
      const cpu = (data["cpuProfile"] ?? {}) as { nodes?: Node[]; samples?: number[] }
      for (const n of cpu.nodes ?? []) nodes.set(n.id, n)
      const deltas = (data["timeDeltas"] ?? []) as number[]
      const samples = cpu.samples ?? []
      for (let i = 0; i < samples.length; i += 1) {
        const delta = deltas[i] ?? 0
        at += delta
        if (!windows.some(([w0, w1]) => at >= w0 && at <= w1)) continue
        sampled += delta
        const named = async (frame: Frame): Promise<string> => {
          const fn = frame.functionName === undefined || frame.functionName === "" ? "(anonymous)" : frame.functionName
          const url = frame.url ?? ""
          if (frame.codeType === "wasm" || url.startsWith("wasm://")) return `wasm ${fn}`
          if (!url.startsWith("http")) return fn
          const lookup = await mapped(url)
          const line = frame.lineNumber ?? 0
          const column = frame.columnNumber ?? 0
          const where = lookup?.(line, column) ?? lookup?.(line - 1, column - 1) ?? null
          return `${where ?? url.replace(/^.*\//, "")} ${fn}`
        }
        const seen = new Set<string>()
        let first = true
        for (let id: number | undefined = samples[i]; id !== undefined; id = nodes.get(id)?.parent) {
          const node = nodes.get(id)
          if (node === undefined) break
          const label = await named(node.callFrame)
          if (first) top.set(label, (top.get(label) ?? 0) + delta)
          first = false
          const url = node.callFrame.url ?? ""
          const ours = url.startsWith("wasm://") || node.callFrame.codeType === "wasm" ? "core (wasm)" : url.startsWith("http") && !label.includes("node_modules") ? label : null
          if (ours !== null && !seen.has(ours)) {
            seen.add(ours)
            on.set(ours, (on.get(ours) ?? 0) + delta)
          }
        }
      }
    }
  }
  return { top, on, sampled }
}

const ranked = (m: ReadonlyMap<string, number>, n: number): (readonly [string, number])[] =>
  [...m]
    .sort((a, b) => b[1] - a[1])
    .slice(0, n)
    .map(([k, v]): readonly [string, number] => [k.slice(0, 110), ms(v)])

// The events inside `task`, and each label's own time in µs: its time less the events nested in it.
const selfOf = (complete: readonly Ev[], task: Ev): { readonly inside: readonly Ev[]; readonly self: ReadonlyMap<string, number> } => {
  const [t0, t1] = [task.ts, task.ts + (task.dur ?? 0)]
  const inside = complete.filter((e) => e.ts >= t0 && e.ts + (e.dur ?? 0) <= t1).sort((a, b) => a.ts - b.ts || (b.dur ?? 0) - (a.dur ?? 0))
  const self = new Map<string, number>()
  const stack: { readonly end: number; readonly label: string }[] = []
  for (const e of inside) {
    while (stack.length > 0 && (stack.at(-1)?.end ?? 0) <= e.ts) stack.pop()
    const label = labelOf(e)
    self.set(label, (self.get(label) ?? 0) + (e.dur ?? 0))
    const parent = stack.at(-1)
    if (parent !== undefined) self.set(parent.label, (self.get(parent.label) ?? 0) - (e.dur ?? 0))
    stack.push({ end: e.ts + (e.dur ?? 0), label })
  }
  return { inside, self }
}

const kindsOf = (self: ReadonlyMap<string, number>): Record<Kind, number> =>
  Object.fromEntries(KINDS.map((k) => [k, ms([...self].filter(([name]) => kindOf(name.split(" ")[0] ?? "") === k).reduce((a, [, us]) => a + us, 0))])) as Record<Kind, number>

// Each task of the phase that holds a painter's step: [ms since the phase began, ms, its kinds, its layouts' dirty
// objects, its style recalculations' elements, the steps' patch bytes].
type Stepped = readonly [number, number, Readonly<Record<Kind, number>>, readonly number[], readonly number[], readonly number[]]
// With the sampled functions summed over every one of those tasks (`self` and `on` as `Broken`'s).
type Steps = { readonly tasks: readonly Stepped[]; readonly self: readonly (readonly [string, number])[]; readonly on: readonly (readonly [string, number])[] }
const steppedOf = async (events: readonly Ev[]): Promise<Steps> => {
  const { main, complete } = pageThread(events)
  const begin = openedAt(events)
  const steps = measured(events, "painter:step")
  const holding = complete
    .filter((t) => t.name === "RunTask" && t.ts >= begin && steps.some(([b]) => b >= t.ts && b <= t.ts + (t.dur ?? 0)))
    .sort((a, b) => a.ts - b.ts)
  const tasks = holding.map((t): Stepped => {
    const { inside, self } = selfOf(complete, t)
    const bytes = steps.filter(([b]) => b >= t.ts && b <= t.ts + (t.dur ?? 0)).map(([, , d]) => (JSON.parse(d) as { readonly bytes?: number } | null)?.bytes ?? 0)
    return [ms(t.ts - begin), ms(t.dur ?? 0), kindsOf(self), layoutsOf(inside).map((l) => l.dirty), inside.filter((e) => e.name === "UpdateLayoutTree").map((e) => Number(e.args?.elementCount ?? -1)), bytes]
  })
  const { top, on } = await sampledIn(events, main, holding.map((t) => [t.ts, t.ts + (t.dur ?? 0)] as const))
  return { tasks, self: ranked(top, 24), on: ranked(on, 16) }
}

const analyse = async (events: readonly Ev[]): Promise<Broken> => {
  const { main, complete } = pageThread(events)
  const from = openedAt(events)
  const tasks = complete.filter((e) => e.name === "RunTask" && e.ts >= from).sort((a, b) => (b.dur ?? 0) - (a.dur ?? 0))
  const task = tasks[0]
  if (task === undefined) return { taskMs: 0, tasksOver50: [], events: [], sampledMs: 0, self: [], on: [] }
  const t0 = task.ts
  const t1 = task.ts + (task.dur ?? 0)
  const { inside, self } = selfOf(complete, task)
  const { top, on, sampled } = await sampledIn(events, main, [[t0, t1]])
  return {
    taskMs: ms(task.dur ?? 0),
    taskCpuMs: ms(task.tdur ?? -1000),
    layouts: layoutsOf(inside).map((l) => [ms(l.us), ms(l.cpuUs), l.dirty, l.total] as const),
    styles: inside.filter((e) => e.name === "UpdateLayoutTree").map((e) => [ms(e.dur ?? 0), Number(e.args?.elementCount ?? -1)] as const),
    tasksOver50: tasks.filter((t) => (t.dur ?? 0) > 50_000).map((t) => ms(t.dur ?? 0)),
    kinds: kindsOf(self),
    events: ranked(self, 14).filter(([, v]) => v >= 1),
    sampledMs: ms(sampled),
    self: ranked(top, 16).filter(([, v]) => v >= 1),
    on: ranked(on, 14).filter(([, v]) => v >= 1),
  }
}

// --- the scene ------------------------------------------------------------------

// Which face each text of the page uses: elements holding words, by the first family of their font and its style, with
// how many there are, how many of them are inside `.turns`, and the first classes among them.
const FACES = `(() => {
  const by = {}
  for (const e of document.querySelectorAll("body *")) {
    if (![...e.childNodes].some((n) => n.nodeType === Node.TEXT_NODE && n.data.trim() !== "")) continue
    const c = getComputedStyle(e)
    const face = c.fontFamily.split(",")[0].replace(/"/g, "").trim() + " " + c.fontStyle
    const at = (by[face] ??= { count: 0, inTurns: 0, classes: [] })
    at.count += 1
    if (e.closest(".turns") !== null) at.inTurns += 1
    const cls = e.tagName.toLowerCase() + (e.className === "" ? "" : "." + String(e.className).split(" ")[0])
    if (at.classes.length < 8 && !at.classes.includes(cls)) at.classes.push(cls)
  }
  return by
})()`

// The census: each child of `body`, of `main#view` and of `main#view`'s children as [parent, tag#id.class, display,
// descendants].
const OBSERVE = `(() => {
  window.__fonts = []
  for (const type of ["loading", "loadingdone", "loadingerror"])
    document.fonts.addEventListener(type, (e) =>
      window.__fonts.push([Math.round(performance.now()), type, e.fontfaces.map((f) => f.family + " " + f.style + " " + f.weight)]))
  window.__censuses = []
  window.__census = (at, ms) => {
    const named = (e) => e.tagName.toLowerCase() + (e.id === "" ? "" : "#" + e.id) + [...e.classList].map((c) => "." + c).join("")
    const rows = []
    const view = document.getElementById("view")
    const parents = [document.body, ...(view === null ? [] : [view, ...view.children])]
    for (const parent of parents)
      for (const e of parent.children) rows.push([named(parent), named(e), getComputedStyle(e).display, e.querySelectorAll("*").length])
    window.__censuses.push({ at: Math.round(at), ms: Math.round(ms), rows })
  }
  new PerformanceObserver((list) => { for (const e of list.getEntries()) window.__census(e.startTime, e.duration) })
    .observe({ type: "longtask", buffered: true })
})()`

// What the opening's largest layout laid out, and the tasks of the page's thread between the window's placing and it:
// [ms since the opening began, ms, what ran], a timer as `timer <id> (<timeout> ms, once|repeating) -> <function>`.
type Largest = {
  readonly at: number
  readonly ms: number
  readonly dirty: number
  readonly total: number
  readonly roots: readonly string[]
  readonly stack: readonly string[]
  readonly between: readonly Ran[]
  // With `disabled-by-default-devtools.timeline.invalidationTracking`: each invalidation event between the layout before
  // this one and this one, as [event, reason, count].
  readonly reasons: readonly (readonly [string, string, number])[]
}

type CallFrame = { readonly functionName?: string; readonly url?: string; readonly lineNumber?: number; readonly columnNumber?: number }
// A trace's frame through the bundle's source maps: its lines and columns count from 1.
const where = async (f: CallFrame): Promise<string> => {
  const url = f.url ?? ""
  const fn = f.functionName === undefined || f.functionName === "" ? "(anonymous)" : f.functionName
  if (!url.startsWith("http")) return fn
  const lookup = await mapped(url)
  const at = lookup?.((f.lineNumber ?? 1) - 1, (f.columnNumber ?? 1) - 1) ?? url.replace(/^.*\//, "")
  return `${at} ${fn}`
}

const largestOf = async (events: readonly Ev[]): Promise<Largest | null> => {
  const { complete } = pageThread(events)
  const begin = openedAt(events)
  const layout = complete.filter((e) => e.name === "Layout" && e.ts >= begin).reduce<Ev | null>((a, e) => {
    const dirty = (x: Ev): number => Number(x.args?.beginData?.["dirtyObjects"] ?? 0)
    return a === null || dirty(e) > dirty(a) ? e : a
  }, null)
  if (layout === null) return null
  const roots = ((layout.args?.endData?.["layoutRoots"] ?? []) as readonly { readonly nodeId?: number }[]).flatMap((r) => (r.nodeId === undefined ? [] : [r.nodeId]))
  const stack = await Promise.all(((layout.args?.beginData?.["stackTrace"] ?? []) as readonly CallFrame[]).slice(0, 6).map(where))
  const placedAt = Math.max(begin, ...events.filter((e) => e.name === marks.PLACED && e.ts <= layout.ts).map((e) => e.ts))
  const installs = new Map(events.filter((e) => e.name === "TimerInstall").map((e) => [String(e.args?.data?.["timerId"]), e.args?.data ?? {}]))
  const tasks = complete.filter((e) => e.name === "RunTask" && e.ts >= placedAt && e.ts <= layout.ts).sort((a, b) => a.ts - b.ts)
  const between = await Promise.all(
    tasks.map(async (t): Promise<Ran> => {
      const inside = complete.filter((e) => e !== t && e.ts >= t.ts && e.ts + (e.dur ?? 0) <= t.ts + (t.dur ?? 0)).sort((a, b) => a.ts - b.ts)
      const said: string[] = []
      for (const e of inside) {
        const d = e.args?.data ?? {}
        if (e.name === "TimerFire") {
          const id = String(d["timerId"])
          const installed = installs.get(id)
          const call = inside.find((c) => c.name === "FunctionCall" && c.ts >= e.ts && c.ts <= e.ts + (e.dur ?? 0))
          const fn = call === undefined ? "?" : await where((call.args?.data ?? {}) as CallFrame)
          said.push(`timer ${id} (${String(installed?.["timeout"] ?? "?")} ms, ${installed?.["singleShot"] === false ? "repeating" : "once"}) -> ${fn}`)
        } else if (e.name === "FireAnimationFrame") said.push("rAF")
        else if (e.name === "EventDispatch") said.push(`event ${String(d["type"] ?? "")}`)
        else if (e.name === "FunctionCall" && !inside.some((o) => o.name === "TimerFire" && o.ts <= e.ts && e.ts <= o.ts + (o.dur ?? 0))) said.push(`call ${await where(d as CallFrame)}`)
        else if (e.name === "UpdateLayoutTree") said.push(`style ${e.args?.elementCount ?? "?"}`)
        else if (e.name === "Layout") said.push(`layout ${String(e.args?.beginData?.["dirtyObjects"])}/${String(e.args?.beginData?.["totalObjects"])} ${ms(e.dur ?? 0)}`)
      }
      return [ms(t.ts - begin), ms(t.dur ?? 0), said.join(", ")]
    }),
  ).then((rs) => rs.filter(([, d, said]) => said !== "" || d >= 1))
  const before = Math.max(begin, ...complete.filter((e) => e.name === "Layout" && e.ts < layout.ts).map((e) => e.ts))
  const counted = new Map<string, number>()
  for (const e of events)
    if (e.name.endsWith("InvalidationTracking") && e.ts >= before && e.ts <= layout.ts) {
      const k = `${e.name}\u0000${String(e.args?.data?.["reason"] ?? e.args?.data?.["invalidationSet"] ?? "")}`
      counted.set(k, (counted.get(k) ?? 0) + 1)
    }
  const reasons = [...counted].map(([k, n]): readonly [string, string, number] => {
    const [name = "", reason = ""] = k.split("\u0000")
    return [name, reason, n]
  })
  return {
    reasons,
    at: ms(layout.ts - begin),
    ms: ms(layout.dur ?? 0),
    dirty: Number(layout.args?.beginData?.["dirtyObjects"] ?? 0),
    total: Number(layout.args?.beginData?.["totalObjects"] ?? 0),
    roots: await app.page.describeNodes(roots),
    stack,
    between,
  }
}
const settle = async (until: string): Promise<void> => void (await settled(app.page, until, STEP_MS))

// What a phase drew: an opening's turns and rounds, or the stream's rounds, how many of each meet the view, and the
// elements under their box.
type Shape = { readonly turns: number; readonly turnsInView: number; readonly rounds: number; readonly roundsInView: number; readonly elements: number; readonly scrollHeight: number; readonly clientHeight: number }
const SHAPE = ((box: string, round: string): string => `(() => {
  const box = document.scrollingElement
  const seen = (e) => { const r = e.getBoundingClientRect(); return r.bottom > 0 && r.top < innerHeight }
  const turns = [...document.querySelectorAll(".turns .turn")]
  const rounds = [...document.querySelectorAll(${JSON.stringify(round)})]
  return { turns: turns.length, turnsInView: turns.filter(seen).length, rounds: rounds.length, roundsInView: rounds.filter(seen).length,
    elements: document.querySelectorAll(${JSON.stringify(`${box} *`)}).length, scrollHeight: box.scrollHeight, clientHeight: box.clientHeight }
})()`)(PHASE === "stream" ? ".streamrounds" : ".turns", PHASE === "stream" ? ".streamround" : ".turns .round")

type Census = { readonly at: number; readonly ms: number; readonly rows: readonly (readonly [string, string, string, number])[] }
type Opening = {
  readonly load: string
  readonly longtaskMs: number
  readonly within: unknown
  readonly shape: Shape
  readonly order: readonly Ran[]
  readonly largest: Largest | null
  readonly censuses: readonly Census[]
  // Each `document.fonts` event from the opening's mark on: [ms since it, type, faces].
  readonly fonts: readonly unknown[]
  // Each face the document declares, with its status once the opening is over.
  readonly faces: readonly unknown[]
  // Which face each text of the page uses before the phase and once it is over (`FACES`).
  readonly uses: { readonly before: unknown; readonly after: unknown }
  readonly compiled: Compiled
  readonly stepped: Steps
} & Broken

const visit = async (load: string): Promise<Opening> => {
  if (LATE_FONT !== "") await app.page.cold(true)
  const release = LATE_FONT === "" ? null : await app.page.holdRequests([LATE_FONT])
  await app.page.navigate(`${LINES_BASE}/?${load}#/`)
  await settle(`document.querySelectorAll(".tree .row").length > 0`)
  const [to, drawn] = await (async (): Promise<readonly [string, string]> => {
    switch (PHASE) {
      case "stream":
        return ["#/secretary", BULK]
      case "opening": {
        await app.page.evaluate(`void (location.hash = "#/secretary")`)
        await settle(BULK)
        const was = await app.page.evaluate<number>(placed)
        return ["#/secretary/bulk-0", `document.querySelectorAll(".turns .turn").length > 0 && ${placed} > ${was}`]
      }
    }
  })()
  const before = await app.page.evaluate<unknown>(FACES)
  const stop = await app.page.trace(CATEGORIES)
  // The profiler starts in a long task of its own, which is the trace's and no phase's.
  await settle("true")
  const since = await app.page.evaluate<number>(`performance.now()`)
  await app.page.evaluate(`void (performance.mark(${JSON.stringify(OPEN)}), (location.hash = ${JSON.stringify(to)}))`)
  await settle(drawn)
  if (release !== null) {
    await release()
    await app.page.cold(false)
    await settle(`[...document.fonts].every((f) => f.status !== "loading")`)
  }
  if (PRESSURE) {
    await app.page.memoryPressure("critical")
    const at = await app.page.evaluate<number>(`performance.now()`)
    await settle(`performance.now() - ${at} > ${PRESSURE_MS}`)
  }
  const events = await stop()
  const fonts = await app.page.evaluate<unknown[]>(`window.__fonts.map(([at, type, faces]) => [at - ${since}, type, faces])`)
  const faces = await app.page.evaluate<unknown[]>(`[...document.fonts].map((f) => f.family + " " + f.style + " " + f.status)`)
  const uses = { before, after: await app.page.evaluate<unknown>(FACES) }
  const largest = await largestOf(events)
  await app.page.evaluate(`window.__census(performance.now(), 0)`)
  const censuses = await app.page.evaluate<Census[]>(`window.__censuses.filter((c) => c.at >= ${since})`)
  const tasks = await app.page.evaluate<[number, number][]>(`window.__long.filter(([at]) => at >= ${since})`)
  const longest = tasks.reduce<[number, number] | null>((a, t) => (a === null || t[1] > a[1] ? t : a), null)
  const within =
    longest === null
      ? []
      : await app.page.evaluate<unknown>(`performance.getEntriesByType("measure")
  .filter((m) => m.startTime < ${longest[0] + longest[1]} && m.startTime + m.duration > ${longest[0]})
  .sort((a, b) => b.duration - a.duration).slice(0, 6)
  .map((m) => [m.name, Math.round(m.duration)])`)
  const broken = await analyse(events)
  const shape = await app.page.evaluate<Shape>(SHAPE)
  const opening = { load, longtaskMs: Math.round(longest?.[1] ?? 0), within, shape, order: order(events), largest, censuses, fonts, faces, uses, compiled: compiledOf(events), stepped: await steppedOf(events), ...broken }
  console.log(`openprof: ${JSON.stringify(opening)}`)
  return opening
}

app.scene(`measured: what the page's thread runs inside the longest task of a phase (${PHASE})`, async () => {
  assert.notEqual(LINES_BASE, "", "E2E_BASE_LINES is unset — scripts/web-e2e.sh sets it")
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  const unobserve = await app.page.beforeEveryDocument(`${LONG}; ${OBSERVE}`)
  await app.page.throttle(RATE)
  try {
    const openings: Opening[] = [await visit("cold")]
    for (let i = 0; i < VISITS; i += 1) openings.push(await visit(NEW_DOCUMENT || PHASE === "stream" ? `warm${i}` : "warm"))
    if (OUT !== "") writeFileSync(OUT, `${JSON.stringify(openings, null, 2)}\n`)
  } finally {
    await app.page.throttle(1)
    await unobserve()
  }
})
