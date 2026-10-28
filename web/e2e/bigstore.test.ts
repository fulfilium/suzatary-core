/**
 * LARGE STORES DO NOT HOLD THE PAGE'S THREAD: the lines' server holds generated stores of the live shape
 * (`suzatary-dev fixture-chain`, as `scripts/web-e2e.sh` sets it: about 14,500 line objects over 100 conversations
 * with reseals, 730 inbox objects and 900 topic objects), and the core's worker pulls, keeps and reads every store, so
 * the stream, a conversation and a new line are drawn with no task on the page's thread over 100 ms. The scene records every long
 * task (over 50 ms) the browser reports, cold (a device holding nothing) and warm (the same device loaded again).
 *
 * The store's words are outside ASCII and hold digits, the text the scrub is slowest on. A warm load of the stream
 * reads the lines the device holds, which scrubs nothing, and draws it well within a read's bound while every request
 * to the box's API is held, so nothing it draws waits on the box.
 *
 * A phase ends once what it waits for is drawn whole (a conversation is marked `marks.PLACED` when its window is
 * drawn and placed) and the page has run no long task for a second and a half since, so a later slice or placing is
 * counted in the phase it belongs to.
 *
 * As a measurement: `SUZATARY_E2E_LINES=6000 SUZATARY_E2E_CONVERSATIONS=1500 SUZATARY_LINES_RATE=6
 * SUZATARY_E2E_SCENE_MS=1800000 SUZATARY_LINESBENCH=<file> scripts/web-e2e.sh bigstore` runs it at a 6x CPU throttle and writes each phase's longest
 * task and its time. The boot's (from the document's start until the todo list draws) is the app's own, and reported.
 */

import assert from "node:assert/strict"
import { writeFileSync } from "node:fs"

import marks from "../src/chat/marks"
import wait from "../src/wire/wait"
import { PHONE, SIGNED_POST, suite, trustThisDevice } from "./harness"

type Shape = { readonly lines: number; readonly conversations: number; readonly reseals: number; readonly proposals: number; readonly topics: number }
// The stores' shape as `scripts/web-e2e.sh` hands it to the fixture.
const SHAPE: Shape = {
  lines: Number(process.env["SUZATARY_E2E_LINES"] ?? "13000"),
  conversations: Number(process.env["SUZATARY_E2E_CONVERSATIONS"] ?? "100"),
  reseals: Number(process.env["SUZATARY_E2E_RESEALS"] ?? "1500"),
  proposals: Number(process.env["SUZATARY_E2E_PROPOSALS"] ?? "400"),
  topics: Number(process.env["SUZATARY_E2E_TOPICS"] ?? "900"),
}

const required = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-e2e.sh`)
  return v
}

const LINES_BASE = required("E2E_BASE_LINES")
const LINES_INSTANCE = required("E2E_LINES_INSTANCE")
const RATE = Number(process.env["SUZATARY_LINES_RATE"] ?? "1")
// The longest task the scene allows on the page's thread while the stream, a conversation and a new line are read.
const LONGEST_MS = 100
// How long the page runs no long task before a phase is over.
const QUIET_MS = 1500

const app = await suite("bigstore", PHONE)

// Every long task the page's thread ran since the document started, kept in the page.
const OBSERVE = `(() => {
  window.__long = []
  new PerformanceObserver((list) => { for (const e of list.getEntries()) window.__long.push([e.startTime, e.duration]) })
    .observe({ type: "longtask", buffered: true })
})()`

// When the latest conversation's window was placed, or -1 before any.
const placed = `(performance.getEntriesByName(${JSON.stringify(marks.PLACED)}).at(-1)?.startTime ?? -1)`
// Whether the page has run no long task for `QUIET_MS`, counted from `drawn` at the earliest.
const quiet = (drawn: number): string =>
  `(() => { const l = window.__long.at(-1); return performance.now() - Math.max(${drawn}, l === undefined ? 0 : l[0] + l[1]) > ${QUIET_MS} })()`

type Phase = { readonly ms: number; readonly longest: number; readonly tasks: number }

// The long tasks from `from` (now, when absent; the document's start is 0) until `until` holds and the page is quiet,
// and how long until `until` held.
const phase = async (until: string, from?: number, timeout = 600000): Promise<Phase> => {
  const since = from ?? (await app.page.evaluate<number>(`performance.now()`))
  await app.page.waitFor(until, { timeout })
  const drawn = await app.page.evaluate<number>(`performance.now()`)
  await app.page.waitFor(quiet(drawn), { timeout })
  const tasks = await app.page.evaluate<[number, number][]>(`window.__long.filter(([at]) => at >= ${since})`)
  return { ms: Math.round(drawn - since), longest: Math.round(Math.max(0, ...tasks.map(([, d]) => d))), tasks: tasks.length }
}

// A load of the app on the todo list, then the stream of conversations and the largest conversation as pages it moves to.
const visit = async (load: string): Promise<{ readonly boot: Phase; readonly stream: Phase; readonly opened: Phase }> => {
  await app.page.navigate(`${LINES_BASE}/?${load}#/`)
  const boot = await phase(`document.querySelectorAll(".tree .row").length > 0`, 0)
  await app.page.evaluate(`void (location.hash = "#/secretary")`)
  const stream = await phase(`document.querySelectorAll('.streamround .streammeta a[href*="bulk-"]').length > 0`)
  const was = await app.page.evaluate<number>(placed)
  await app.page.evaluate(`void (location.hash = "#/secretary/bulk-0")`)
  const opened = await phase(`document.querySelectorAll(".turns .turn").length > 0 && ${placed} > ${was}`)
  return { boot, stream, opened }
}

app.scene("a store of many lines is listed, opened and grown with no task on the page's thread over 100 ms", async () => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  const unobserve = await app.page.beforeEveryDocument(OBSERVE)
  await app.page.throttle(RATE)
  try {
    const cold = await visit("cold")
    const before = await app.page.evaluate<number>(`document.querySelectorAll(".turns .turn-model").length`)
    const was = await app.page.evaluate<number>(placed)
    const since = await app.page.evaluate<number>(`performance.now()`)
    const status = await app.page.evaluate<number>(
      `${SIGNED_POST}("/api/secretary/conversations/bulk-0/say", JSON.stringify({say: "one more question"})).then((r) => r.status)`,
    )
    assert.ok(status < 300, `the box took the line (${status})`)
    const line = await phase(`document.querySelectorAll(".turns .turn-model").length > ${before} && ${placed} > ${was}`, since)
    const warm = await visit("warm")
    const measured = {
      ...SHAPE,
      rate: RATE,
      cold,
      line,
      warm,
    }
    console.log(JSON.stringify(measured))
    const out = process.env["SUZATARY_LINESBENCH"]
    if (out !== undefined && out !== "") writeFileSync(out, `${JSON.stringify(measured, null, 2)}\n`)
    const phases = [
      ["the stream, cold", cold.stream],
      ["a conversation opening, cold", cold.opened],
      ["a new line", line],
      ["the stream, warm", warm.stream],
      ["a conversation opening, warm", warm.opened],
    ] as const
    for (const [what, p] of phases) assert.ok(p.longest <= LONGEST_MS, `${what} held the page's thread ${p.longest} ms at once (at most ${LONGEST_MS})`)
  } finally {
    await app.page.throttle(1)
    await unobserve()
  }
})

// The API reads open now, less the event streams, which stay open for a page's life; a build's own files are the shell's
// worker's to answer.
const reads = (open: readonly string[]): string[] => open.filter((r) => r.includes(" /api/") && !r.endsWith("/events"))

// A round of the generated conversations, drawn in the stream.
const BULK = `document.querySelectorAll('.streamround .streammeta a[href*="bulk-"]').length > 0`

// The conversations the stream drew rounds of, by their links.
const DRAWN = `[...new Set([...document.querySelectorAll(".streamround .streammeta a")].map((a) => decodeURIComponent(/secretary\\/([^/?#&@]+)/.exec(a.getAttribute("href"))?.[1] ?? "")))]`
// In the core's worker: how many prints its lines decoded, and how many objects the device's skeleton holds, in all and
// of the conversations `ids` and the structure every line rests on, which is what drawing their rounds needs.
const DECODED = (ids: readonly string[]): string => `(async () => {
  const m = await import(new URL("prodrome.js", self.location.href).href)
  const db = await new Promise((done, fail) => { const r = indexedDB.open("suzatary"); r.onsuccess = () => done(r.result); r.onerror = () => fail(r.error) })
  const nodes = await new Promise((done, fail) => {
    const r = db.transaction("lines").objectStore("lines").getAll(IDBKeyRange.bound("node/", "node/\uffff"))
    r.onsuccess = () => done(r.result); r.onerror = () => fail(r.error)
  })
  db.close()
  const ids = new Set(${JSON.stringify(ids)})
  const needed = nodes.filter((n) => n.header.node === "structure" || ids.has(n.header.conversation)).length
  return { decoded: m.decodes(), needed, all: nodes.length }
})()`

app.scene("a warm load of the conversations draws them from the device, with every read of the box held and no task on the page's thread over 100 ms", async () => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  const unobserve = await app.page.beforeEveryDocument(OBSERVE)
  await app.page.navigate(`${LINES_BASE}/#/secretary`)
  await app.page.waitFor(BULK, { timeout: 600000 })
  // The cold load done: the device holds the lines, and nothing it asked for is still coming.
  const journal = await app.page.journal()
  const deadline = Date.now() + 600000
  while (reads(journal.inFlight()).length > 0 && Date.now() < deadline) await new Promise((done) => setTimeout(done, 200))
  assert.deepEqual(reads(journal.inFlight()), [], "the cold load ended")
  journal.mark("warm load")
  // Every request to the box's API is held until the stream has drawn, so what draws is drawn from the device.
  const release = await app.page.holdRequests(["/api/"])
  const drawn = await (async (): Promise<{ readonly ms: number; readonly longest: number; readonly held: readonly string[]; readonly cost: { decoded: number; needed: number; all: number } }> => {
    try {
      await app.page.throttle(RATE)
      await app.page.reload()
      const draw = await phase(BULK, 0)
      const ids = await app.page.evaluate<string[]>(DRAWN)
      return { ms: draw.ms, longest: draw.longest, held: app.page.heldUrls(), cost: await app.page.inWorker<{ decoded: number; needed: number; all: number }>("prodromework", DECODED(ids)) }
    } finally {
      await app.page.throttle(1)
      await release()
      await unobserve()
    }
  })()
  console.log(JSON.stringify({ ...SHAPE, rate: RATE, warm: drawn.ms, longest: drawn.longest, ...drawn.cost }))
  assert.ok(drawn.ms < wait.MS / 3, `the stream drew ${Math.round(drawn.ms)} ms after the document started (bound ${wait.MS / 3})`)
  assert.ok(drawn.longest <= LONGEST_MS, `the warm load held the page's thread ${drawn.longest} ms at once (at most ${LONGEST_MS})`)
  assert.ok(drawn.cost.decoded <= drawn.cost.needed, `the warm load decoded ${drawn.cost.decoded} prints; its rounds need ${drawn.cost.needed} of ${drawn.cost.all}`)
  assert.ok(drawn.held.length > 0, `the box was asked and answered nothing before the stream drew:\n${journal.lines().join("\n")}`)
})
