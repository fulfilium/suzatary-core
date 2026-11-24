/**
 * A NEW DEVICE'S FIRST LOAD OF THE CONVERSATIONS, MEASURED: a browser that holds nothing opens `#/secretary` on the
 * `lines` server's generated store (the bigstore shape, `scripts/web-e2e.sh`) over a phone's network, and the bench
 * says where the time goes until the stream's first round is drawn and until the cold load ends (no read of the box's
 * API left open). It records every request the page and its workers make (`Page.wire`), with its bytes on the wire and
 * when it was sent, answered and ended; the worker's own measures (`http:decode`, `lines:piece`, `lines:kept`,
 * `content:admit`, `content:kept`) and marks; and, from the box's whole skeleton read after the load, how many of the
 * pulled bytes the conversations the first screen drew rest on.
 *
 * The network is Lighthouse's Fast 4G (`FAST_4G`: 165 ms a request, about 1.0 MB/s down and 170 KB/s up), what a phone
 * on a tailnet relay gets on a good day; `SUZATARY_COLDSYNC_NET=none` runs it at the box's own speed. Each pass is a
 * new profile's first visit. No gate runs it: `SUZATARY_COLDSYNC=<file> SUZATARY_COLDSYNC_PASSES=3 scripts/web-e2e.sh
 * coldsync` does, by name, and writes each pass's record and its table to `<file>`.
 */

import assert from "node:assert/strict"
import { readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"

import type { Conditions, Wired } from "./cdp"
import { PHONE, STEP_MS, WORKER_MARKS, suite, trustThisDevice } from "./harness"
import { BULK } from "./longtasks"

const LINES_BASE = process.env["E2E_BASE_LINES"] ?? ""
const LINES_INSTANCE = process.env["E2E_LINES_INSTANCE"] ?? ""
const OUT = process.env["SUZATARY_COLDSYNC"] ?? ""
const PASSES = Number(process.env["SUZATARY_COLDSYNC_PASSES"] ?? "1")
const FAST_4G: Conditions = { offline: false, latency: 60 * 2.75, downloadThroughput: (9_000_000 / 8) * 0.9, uploadThroughput: (1_500_000 / 8) * 0.9 }
const NET: Conditions | null = process.env["SUZATARY_COLDSYNC_NET"] === "none" ? null : FAST_4G

const app = await suite("coldsync", PHONE)

// The lines server's log (`scripts/e2e-servers.sh`'s `launch`), beside its instance's directory: each skeleton piece the
// box answered logs its parts' times (`suzatary::skeleton`).
const BOX_LOG = join(dirname(dirname(LINES_INSTANCE)), "lines.log")
const logged = (): string => readFileSync(BOX_LOG, "utf8")
const piecesSince = (from: number): string[] =>
  logged()
    .slice(from)
    .split("\n")
    .filter((l) => l.includes("skeleton piece"))
    .map((l) => l.replace(/^.*?skeleton piece/, "skeleton piece").replace(/\u001b\[[0-9;]*m/g, ""))

// What a request is for, by its path.
const route = (url: string): string => {
  const path = url.split("?")[0] ?? url
  if (path.startsWith("/api/stores/lines/skeleton")) return "lines skeleton"
  if (path.startsWith("/api/stores/lines/content")) return "lines content"
  const store = /^\/api\/stores\/([^/]+)\/objects/.exec(path)
  if (store !== null) return `${store[1]} objects`
  if (/^\/api\/[^/]+\/objects/.test(path)) return "chain objects"
  if (path.startsWith("/api/")) return "other api"
  return "shell"
}

// Requests by route: how many, their bytes, the time from the first sent to the last ended, the sum of each one's own
// time, and the most out at once.
type Route = { readonly requests: number; readonly bytes: number; readonly decoded: number; readonly spanMs: number; readonly wireMs: number; readonly mostAtOnce: number; readonly beforeDraw: number; readonly bytesBeforeDraw: number }
const routes = (wired: readonly Wired[], drawnAt: number): Record<string, Route> => {
  const by = new Map<string, Wired[]>()
  for (const w of wired) by.set(route(w.url), [...(by.get(route(w.url)) ?? []), w])
  return Object.fromEntries(
    [...by].map(([name, ws]) => {
      const edges = ws.flatMap((w) => [[w.sent, 1] as const, [w.ended, -1] as const]).sort((a, b) => a[0] - b[0] || a[1] - b[1])
      let out = 0
      let most = 0
      for (const [, d] of edges) most = Math.max(most, (out += d))
      const before = ws.filter((w) => w.ended <= drawnAt)
      return [
        name,
        {
          requests: ws.length,
          bytes: ws.reduce((k, w) => k + w.bytes, 0),
          decoded: ws.reduce((k, w) => k + w.decoded, 0),
          spanMs: Math.round(Math.max(...ws.map((w) => w.ended)) - Math.min(...ws.map((w) => w.sent))),
          wireMs: Math.round(ws.reduce((k, w) => k + (w.ended - w.sent), 0)),
          mostAtOnce: most,
          beforeDraw: before.length,
          bytesBeforeDraw: before.reduce((k, w) => k + w.bytes, 0),
        },
      ]
    }),
  )
}

// In every worker before its own script: its measures summed by name as they are taken, since a tab keeps only the
// latest of each name (`perf.KEPT`); read back as \`self.__summed\`.
const SUMMED = `(() => {
  const by = (self.__summed = {})
  new PerformanceObserver((list) => {
    for (const m of list.getEntries()) {
      const b = (by[m.name] ??= { count: 0, ms: 0, bytes: 0 })
      b.count += 1
      b.ms += m.duration
      b.bytes += m.detail !== null && typeof m.detail === "object" && typeof m.detail.bytes === "number" ? m.detail.bytes : 0
    }
  }).observe({ type: "measure" })
})()`
const MEASURES = `({ origin: performance.timeOrigin, by: self.__summed })`
type Measures = { readonly origin: number; readonly by: Record<string, { count: number; ms: number; bytes: number }> }

// The conversations the stream drew rounds of, by their links.
const DRAWN = `[...new Set([...document.querySelectorAll(".streamround .streammeta a")].map((a) => decodeURIComponent(/secretary\\/([^/?#&@]+)/.exec(a.getAttribute("href"))?.[1] ?? "")))]`

// The box's whole skeleton, piece after piece, each node with its bytes as JSON and its content's size.
const SKELETON = `(async () => {
  const nodes = []
  for (let since = ""; ; ) {
    const a = await fetch("/api/stores/lines/skeleton" + (since === "" ? "" : "?since=" + since)).then((r) => r.json())
    for (const n of a.nodes) {
      const json = JSON.stringify(n)
      nodes.push({ conversation: n.header.conversation ?? "", json: json.length, names: (json.match(/[0-9a-f]{64}/g) ?? []).length * 66, size: n.size })
    }
    if (a.rest === 0) return nodes
    since = a.frontier.digest
  }
})()`
// A node's bytes as JSON, those of the names it holds (its own, its parents', its keys'), and its content's size.
type Node = { readonly conversation: string; readonly json: number; readonly names: number; readonly size: number }

// Written into every document before its scripts: the instant the stream's first round of the store is drawn.
const DRAWN_AT = `(() => {
  const tick = () => (${BULK} ? (window.__drawnAt = performance.now()) : requestAnimationFrame(tick))
  requestAnimationFrame(tick)
})()`

// The device's store deleted, from a file of the origin so no page of the app holds it open.
const forget = async (): Promise<void> => {
  await app.page.navigate(`${LINES_BASE}/manifest.webmanifest`)
  await app.page.evaluate(`new Promise((ok) => { const r = indexedDB.deleteDatabase("suzatary"); r.onsuccess = r.onerror = r.onblocked = () => ok(true) })`)
}

const QUIET_MS = 5000
const reads = (open: readonly string[]): string[] => open.filter((r) => r.includes(" /api/") && !r.endsWith("/events"))

type Pass = {
  readonly drawnMs: number
  readonly endedMs: number
  readonly routes: Record<string, Route>
  readonly worker: Measures["by"]
  readonly marks: Record<string, number>
  readonly protocols: readonly string[]
  readonly box: readonly string[]
  readonly waterfall: readonly (readonly [string, number, number, number])[]
  readonly firstScreen: { readonly conversations: number; readonly of: number; readonly skeletonBytes: number; readonly nameBytes: number; readonly skeletonOfDrawn: number; readonly contentBytes: number; readonly contentOfDrawn: number }
}

const pass = async (n: number): Promise<Pass> => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  await app.page.attachWorkers()
  await app.page.network(NET)
  const journal = await app.page.journal()
  const stop = await app.page.wire()
  const from = logged().length
  try {
    await app.page.navigate(`${LINES_BASE}/?cold${n}#/secretary`)
    await app.page.waitFor(`window.__drawnAt !== undefined`, { timeout: STEP_MS })
    const origin = await app.page.evaluate<number>(`performance.timeOrigin`)
    const drawnAt = origin + (await app.page.evaluate<number>(`window.__drawnAt`))
    // Ended once no read of the box's API is out for `QUIET_MS`: a prefetch sends its answers one after another.
    const deadline = Date.now() + STEP_MS
    for (let quietSince = Date.now(); Date.now() - quietSince < QUIET_MS && Date.now() < deadline; ) {
      if (reads(journal.inFlight()).length > 0) quietSince = Date.now()
      await new Promise((done) => setTimeout(done, 100))
    }
    assert.deepEqual(reads(journal.inFlight()), [], "the cold load ended")
    const wired = stop()
    const drawn = await app.page.evaluate<string[]>(DRAWN)
    const endedAt = Math.max(...wired.filter((w) => w.url.startsWith("/api/") && !w.url.endsWith("/events")).map((w) => w.ended))
    const worker = await app.page.inWorker<Measures>("prodromework", MEASURES)
    const marks = await app.page.inWorker<Record<string, number>>("prodromework", WORKER_MARKS)
    const box = piecesSince(from)
    await app.page.network(null)
    const nodes = await app.page.evaluate<Node[]>(SKELETON)
    const of = new Set(drawn)
    const ofDrawn = nodes.filter((x) => x.conversation === "" || of.has(x.conversation))
    return {
      drawnMs: Math.round(drawnAt - origin),
      endedMs: Math.round(endedAt - origin),
      routes: routes(wired, drawnAt),
      protocols: [...new Set(wired.map((w) => `${w.from} ${w.protocol}`))],
      box,
      waterfall: wired
        .filter((w) => w.url.startsWith("/api/"))
        .sort((a, b) => a.sent - b.sent)
        .map((w) => [w.url.slice(0, 60), Math.round(w.sent - origin), Math.round(w.ended - origin), w.bytes] as const),
      worker: worker.by,
      marks: Object.fromEntries(Object.entries(marks).map(([k, v]) => [k, Math.round(worker.origin + v - origin)])),
      firstScreen: {
        conversations: drawn.length,
        of: new Set(nodes.map((x) => x.conversation)).size - 1,
        skeletonBytes: nodes.reduce((k, x) => k + x.json, 0),
        nameBytes: nodes.reduce((k, x) => k + x.names, 0),
        skeletonOfDrawn: ofDrawn.reduce((k, x) => k + x.json, 0),
        contentBytes: nodes.reduce((k, x) => k + x.size, 0),
        contentOfDrawn: ofDrawn.reduce((k, x) => k + x.size, 0),
      },
    }
  } finally {
    await app.page.network(null)
  }
}

// One pass as the table's rows.
const table = (p: Pass): string => {
  const kb = (b: number): string => `${Math.round(b / 1000)} KB`
  const rows = Object.entries(p.routes)
    .sort((a, b) => b[1].decoded - a[1].decoded)
    .map(([name, r]) => `| ${name} | ${r.requests} (${r.beforeDraw} before the draw) | ${kb(r.bytes)} (${kb(r.bytesBeforeDraw)}) | ${kb(r.decoded)} | ${r.mostAtOnce} | ${r.spanMs} | ${r.wireMs} |`)
  const w = (name: string): string => {
    const m = p.worker[name]
    return m === undefined ? "none" : `${Math.round(m.ms)} ms over ${m.count}`
  }
  return [
    `first round drawn ${p.drawnMs} ms, cold load ended ${p.endedMs} ms after the document started`,
    "| route | requests | bytes on the wire (before the draw) | decoded | most at once | span ms | sum of each request's ms |",
    "|---|---|---|---|---|---|---|",
    ...rows,
    `protocols: ${p.protocols.join(", ")}`,
    ...p.box,
    `worker: decode ${w("http:decode")}; the core's piece ${w("lines:piece")}; the piece's frame kept ${w("lines:kept")}; content verified ${w("content:admit")}; content kept and held ${w("content:kept")}`,
    `first screen: ${p.firstScreen.conversations} of ${p.firstScreen.of} conversations drawn; the skeleton's nodes ${kb(p.firstScreen.skeletonBytes)} as JSON (names ${kb(p.firstScreen.nameBytes)} of it), ${kb(p.firstScreen.skeletonOfDrawn)} of it theirs and the structure; content ${kb(p.firstScreen.contentBytes)}, ${kb(p.firstScreen.contentOfDrawn)} of it theirs`,
  ].join("\n")
}

app.scene("a new device's first load of the conversations, measured", async () => {
  const each: Pass[] = []
  const unobserve = await app.page.beforeEveryDocument(DRAWN_AT)
  const unsum = await app.page.beforeEveryWorker(SUMMED)
  await app.page.cold(true)
  try {
    for (let n = 0; n < PASSES; n += 1) {
      await forget()
      const p = await pass(n)
      each.push(p)
      console.log(table(p))
      console.log(JSON.stringify(p))
    }
  } finally {
    await app.page.cold(false)
    await unobserve()
    await unsum()
  }
  if (OUT !== "") writeFileSync(OUT, `${JSON.stringify({ net: NET, passes: each }, null, 2)}\n${each.map(table).join("\n\n")}\n`)
})
