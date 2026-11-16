/**
 * THE COLLECTIONS A WARM LOAD RUNS, UNTRACED, over the bigstore scene's store at `SUZATARY_LINES_RATE` (default 3): each
 * warm visit loads the todo list (`boot`), moves to the stream, and loads the stream with every read of the box held.
 * Per phase: its longest task, each `page:draw` with its parts, and each collection V8 printed (`--trace-gc`, kept in
 * `SUZATARY_E2E_BROWSER_LOG`) by kind, pause, marking steps, heap before and after (MB) and reason, on the page's isolate
 * (the first its renderer prints) and the workers'. Run it under `stdbuf -oL`, so no line lands in a later phase.
 * `SUZATARY_COLLECTIONS_ALLOCATIONS=1` instead samples what the page's thread allocates (every 16 KiB, collected objects
 * included; no CPU sampling), by function. No gate runs it: `SUZATARY_COLLECTIONS=<file> SUZATARY_COLLECTIONS_VISITS=<n>
 * scripts/web-e2e.sh collections`.
 */

import { readFileSync, statSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"

import type { HeapNode } from "./cdp"
import { PHONE, REPLICA_ROWS, STEP_MS, snapshotKept, suite, trustThisDevice } from "./harness"
import { BULK, OBSERVE, settle } from "./longtasks"
import { mapped } from "./sourcemap"

const ALLOCATIONS = process.env["SUZATARY_COLLECTIONS_ALLOCATIONS"] === "1"
const LOG = process.env["SUZATARY_E2E_BROWSER_LOG"] ?? join(tmpdir(), `suzatary-collections-${process.pid}.log`)
process.env["SUZATARY_E2E_BROWSER_LOG"] = LOG
if (!ALLOCATIONS) process.env["SUZATARY_E2E_JS_FLAGS"] = `--trace-gc ${process.env["SUZATARY_E2E_JS_FLAGS"] ?? ""}`.trim()
writeFileSync(LOG, "")

const required = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-e2e.sh`)
  return v
}
const BASE = required("E2E_BASE_LINES")
const INSTANCE = required("E2E_LINES_INSTANCE")
const RATE = Number(process.env["SUZATARY_LINES_RATE"] ?? "3")
const VISITS = Number(process.env["SUZATARY_COLLECTIONS_VISITS"] ?? "3")
const app = await suite("collections", PHONE)

// One line of `--trace-gc`: `[pid:isolate] <ms> ms: <kind> <before> (<committed>) -> <after> (<committed>) MB, pooled: …,
// <pause> / <external> ms [(+ <marking> ms in <n> steps …)] (average mu …) <reason>`. `ms` is the pause that held the
// thread at once; an incremental marking's steps ran before it, each in a task of its own.
type Collection = {
  readonly pid: string
  readonly isolate: string
  readonly kind: string
  readonly before: number
  readonly after: number
  readonly ms: number
  readonly steps: number
  readonly reason: string
}
const LINE = /^\[(\d+):(0x[0-9a-f]+)\]\s+\d+ ms: ([A-Za-z -]+?(?: \(reduce\))?) ([\d.]+) \([\d.]+\) -> ([\d.]+) \([\d.]+\) MB, pooled: [\d.]+ MB, ([\d.]+) \/ [\d.]+ ms (?:\(\+ ([\d.]+) ms in \d+ steps[^)]*\) )?\(average mu[^)]*\) (.*)$/

const collections = (text: string): Collection[] =>
  text.split("\n").flatMap((l) => {
    const m = LINE.exec(l)
    if (m === null) return []
    const [, pid = "", isolate = "", kind = "", before = "0", after = "0", ms = "0", steps = "0", reason = ""] = m
    return [{ pid, isolate, kind, before: Number(before), after: Number(after), ms: Number(ms), steps: Number(steps), reason: reason.replace(/;\s*$/, "") }]
  })

const size = (): number => statSync(LOG).size
const between = (from: number, to: number): string => new TextDecoder().decode(readFileSync(LOG).subarray(from, to))

// The first isolate each renderer printed, in the log so far: its main thread's, which lives as long as the renderer,
// while a worker's lives as long as its document.
const mains = (): ReadonlyMap<string, string> => {
  const first = new Map<string, string>()
  for (const c of collections(readFileSync(LOG, "utf8"))) if (!first.has(c.pid)) first.set(c.pid, c.isolate)
  return first
}

type Kinds = Record<string, { n: number; ms: number[]; steps: number[]; before: number[]; after: number[]; reasons: string[] }>
const byKind = (cs: readonly Collection[]): Kinds => {
  const out: Kinds = {}
  for (const c of cs) {
    const k = (out[c.kind] ??= { n: 0, ms: [], steps: [], before: [], after: [], reasons: [] })
    k.n += 1
    k.ms.push(Math.round(c.ms * 10) / 10)
    k.steps.push(c.steps)
    k.before.push(c.before)
    k.after.push(c.after)
    if (!k.reasons.includes(c.reason)) k.reasons.push(c.reason)
  }
  return out
}

type Allocated = readonly (readonly [string, number])[]
const allocatedBy = async (head: HeapNode): Promise<Allocated> => {
  const sum = new Map<string, number>()
  const walk = async (n: HeapNode): Promise<void> => {
    if (n.selfSize > 0) {
      const f = n.callFrame
      const lookup = f.url.startsWith("http") ? await mapped(f.url) : null
      const at = lookup?.(f.lineNumber, f.columnNumber) ?? (f.url === "" ? "(native)" : f.url.replace(/^.*\//, ""))
      const key = `${at} ${f.functionName === "" ? "(anonymous)" : f.functionName}`
      sum.set(key, (sum.get(key) ?? 0) + n.selfSize)
    }
    for (const c of n.children) await walk(c)
  }
  await walk(head)
  return [...sum].sort((a, b) => b[1] - a[1]).slice(0, 25)
}

// Each `page:draw` the phase took: a page's first draw, its parts named in its detail.
type Drawn = { readonly ms: number; readonly detail: unknown }
const drawsSince = (since: number): string => `performance.getEntriesByName("page:draw").filter((m) => m.startTime >= ${since}).map((m) => ({ ms: Math.round(m.duration), detail: m.detail ?? null }))`

type Measured = { readonly phase: string; readonly longest: number; readonly draws: readonly Drawn[]; readonly page: Kinds; readonly workers: Kinds; readonly allocated?: Allocated; readonly bytes?: number }

const longestSince = (since: number): string => `window.__long.filter(([at]) => at >= ${since}).reduce((m, [, d]) => Math.max(m, d), 0)`

// From `begin` until `until` holds and the page is quiet: a new document's from its start, else from the call.
const measure = async (phase: string, document: "New" | "Same", begin: () => Promise<void>, until: string): Promise<Measured> => {
  const stop = ALLOCATIONS ? await app.page.allocations() : null
  const from = size()
  const since = document === "New" ? 0 : await app.page.evaluate<number>(`performance.now()`)
  await begin()
  await settle(app.page, until, STEP_MS)
  const longest = Math.round(await app.page.evaluate<number>(longestSince(since)))
  const draws = await app.page.evaluate<Drawn[]>(drawsSince(since))
  const cs = collections(between(from, size()))
  const main = mains()
  const page = byKind(cs.filter((c) => main.get(c.pid) === c.isolate))
  const workers = byKind(cs.filter((c) => main.get(c.pid) !== c.isolate))
  if (stop === null) return { phase, longest, draws, page, workers }
  const profile = await stop()
  const allocated = await allocatedBy(profile.head)
  return { phase, longest, draws, page, workers, allocated, bytes: allocated.reduce((n, [, b]) => n + b, 0) }
}

app.scene("the collections of a warm load and a warm stream", async () => {
  await trustThisDevice(app.page, BASE, INSTANCE)
  const unobserve = await app.page.beforeEveryDocument(OBSERVE)
  // Cold: the device takes the stores, and keeps the lines' snapshot as a page left open does.
  await app.page.navigate(`${BASE}/?cold#/secretary`)
  await app.page.waitFor(BULK, { timeout: STEP_MS })
  await app.page.waitFor(snapshotKept("lines"), { timeout: 120000 })
  console.log(`lines snapshot ${await app.page.evaluate<number>(`${REPLICA_ROWS}.then((rows) => (rows.find(([k]) => k === "lines/snapshot")?.[1].length ?? 0) / 2)`)} bytes`)
  const visits: Measured[][] = []
  try {
    await app.page.throttle(RATE)
    for (let n = 0; n < VISITS; n += 1) {
      const boot = await measure("boot, warm", "New", () => app.page.navigate(`${BASE}/?warm${n}#/`), `document.querySelectorAll(".tree .row").length > 0`)
      const stream = await measure("the stream, warm", "Same", () => app.page.evaluate(`void (location.hash = "#/secretary")`), BULK)
      const release = await app.page.holdRequests(["/api/"])
      const load = await (async () => {
        try {
          return await measure("a warm load of the stream", "New", () => app.page.reload(), BULK)
        } finally {
          await release()
        }
      })()
      visits.push([boot, stream, load])
      for (const m of [boot, stream, load]) console.log(JSON.stringify({ visit: n, rate: RATE, ...m }))
    }
  } finally {
    await app.page.throttle(1)
    await unobserve()
  }
  const out = process.env["SUZATARY_COLLECTIONS"]
  if (out !== undefined && out !== "") writeFileSync(out, `${JSON.stringify(visits, null, 2)}\n`)
})
