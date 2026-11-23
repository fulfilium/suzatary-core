/**
 * LARGE STORES DO NOT HOLD THE PAGE'S THREAD: the lines' server holds generated stores of the live shape
 * (`suzatary-dev fixture-chain`, as `scripts/web-e2e.sh` sets it: about 14,500 line objects over 100 conversations
 * with reseals, 730 inbox objects, 900 topic objects and 600 todos), and the core's worker pulls, keeps and reads every store, so
 * the stream, a conversation and a new line are drawn a slice to a painter's step, with no task on the page's thread
 * over 100 ms of that thread's processor time, cold (a device holding nothing) and warm (the same device loaded again).
 *
 * What one task may do is the page's code's to decide: the stream and a conversation draw their rounds a slice per
 * step, each slice's words within `slices::WORDS` (`app/pages/src/slices.rs`), and the reading before them is decoded
 * in a step that draws nothing. So a phase is judged by two counts a busy machine does not move: the bytes of patches
 * its largest step applied (`painter:step`'s detail), which grow with a slice on any machine, and the processor time
 * its heaviest task took on the page's thread, read from the browser's trace (`trace.ts`'s `tasksOf`), which another
 * process on the same core stretches on the clock and not on the processor. Each phase also records its longest task
 * on the clock (what a person waits) and the long tasks the browser reported; neither decides.
 *
 * The store's words are outside ASCII and hold digits, the text the scrub is slowest on. A warm load of the stream
 * reads the lines the device holds, which scrubs nothing, and draws it well within a read's bound while every request
 * to the box's API is held, so nothing it draws waits on the box. Loaded again with the box answering, as it does live,
 * the shell asks its own questions of the core's worker beside the page's, and the worker answers none of them while
 * the page's first stream is owed (`prodrome/lanes.ts`): the scene reads that order in the worker itself, and reports
 * the first draw.
 *
 * A phase ends once what it waits for is drawn whole (a conversation is marked `marks.PLACED` when its window is
 * drawn and placed) and the page has run no long task and no painter's step for a second and a half since
 * (`longtasks.ts`), so a later slice or placing is counted in the phase it belongs to.
 *
 * As a measurement: `SUZATARY_E2E_LINES=6000 SUZATARY_E2E_CONVERSATIONS=1500 SUZATARY_LINES_RATE=6
 * SUZATARY_E2E_SCENE_MS=1800000 SUZATARY_LINESBENCH=<file> scripts/web-e2e.sh bigstore` runs it at a 6x CPU throttle and writes each phase's
 * record. The throttle pauses the page's main thread, which stretches its tasks on the clock and not on the processor.
 * The boot's (from the document's start until the todo list draws) is the app's own, and reported.
 * `SUZATARY_E2E_BROWSER_CPU=0.67` gives the whole browser two thirds
 * of a core, its worker and its storage included, and each warm load of the last scene reports the worker's own phases.
 */

import assert from "node:assert/strict"
import { writeFileSync } from "node:fs"

import wait from "../src/wire/wait"
import type { Recorded } from "./harness"
import type { TraceEvent } from "./cdp"
import { BULK, OBSERVE, placed, settle } from "./longtasks"
import { TASKS, type Task, layoutsOf, measured, pageThread, tasksOf, threadKey, workerThread } from "./trace"
import { PHONE, REPLICA_ROWS, STEP_MS, WORKER_MARKS, WORKER_QUESTIONS, askAtTheTerminal, snapshotKept, suite, trustThisDevice } from "./harness"

type Shape = { readonly lines: number; readonly conversations: number; readonly reseals: number; readonly proposals: number; readonly topics: number; readonly todos: number }
// The stores' shape as `scripts/web-e2e.sh` hands it to the fixture.
const SHAPE: Shape = {
  lines: Number(process.env["SUZATARY_E2E_LINES"] ?? "13000"),
  conversations: Number(process.env["SUZATARY_E2E_CONVERSATIONS"] ?? "100"),
  reseals: Number(process.env["SUZATARY_E2E_RESEALS"] ?? "1500"),
  proposals: Number(process.env["SUZATARY_E2E_PROPOSALS"] ?? "400"),
  topics: Number(process.env["SUZATARY_E2E_TOPICS"] ?? "900"),
  todos: Number(process.env["SUZATARY_E2E_TODOS"] ?? "600"),
}

const required = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-e2e.sh`)
  return v
}

const LINES_BASE = required("E2E_BASE_LINES")
const LINES_INSTANCE = required("E2E_LINES_INSTANCE")
const RATE = Number(process.env["SUZATARY_LINES_RATE"] ?? "1")
// The most bytes of patches one painter's step may apply while the stream, a conversation and a new line are drawn: a
// slice's words come to at most `slices::WORDS` (8 KiB), and with their markup the store's largest slice is 21 KiB.
const STEP_BYTES = 32 * 1024
// The most processor time one task may take on the page's thread meanwhile, whatever it runs.
const CPU_MS = 100
const app = await suite("bigstore", PHONE)

// A measure the page's own code took (`page:draw`, `painter:step`, `typst:draw`, …), as the browser kept it.
type Held = { readonly name: string; readonly ms: number; readonly detail: unknown }
// `ms`: until what the phase waits for was drawn. `cpuMs`: the processor time of the page thread's heaviest task, and
// `longest`: that thread's longest task on the clock, both in ms. `tasks`: the long tasks the browser reported.
// `steps`, `slices` and `stepBytes`: the painter's steps, those that drew a slice, and the bytes of the largest step's
// patches. `within`: the measures inside the task of `cpuMs`, so a log names the code that held the thread.
type Phase = {
  readonly ms: number
  readonly cpuMs: number
  readonly longest: number
  readonly tasks: number
  readonly steps: number
  readonly slices: number
  readonly stepBytes: number
  readonly within: readonly Held[]
}

// The page's measures that overlap the window `[at, at + ms]`, longest first.
const measuresIn = (at: number, ms: number): string => `performance.getEntriesByType("measure")
  .filter((m) => m.startTime < ${at + ms} && m.startTime + m.duration > ${at})
  .sort((a, b) => b.duration - a.duration)
  .slice(0, 8)
  .map((m) => ({ name: m.name, ms: Math.round(m.duration), detail: m.detail ?? null }))`

// The painter's steps since `since`: how many, how many drew a slice, and the bytes of the largest one's patches.
const stepsSince = (since: number): string => `(() => {
  const steps = performance.getEntriesByName("painter:step").filter((m) => m.startTime >= ${since})
  return [steps.length, steps.filter((m) => m.detail.step.includes('"Slice"')).length, Math.max(0, ...steps.map((m) => m.detail.bytes))]
})()`

// A phase is judged by the work of its largest step and its heaviest task, which a busy machine does not move.
const judge = (what: string, p: Phase): void => {
  assert.ok(p.stepBytes <= STEP_BYTES, `${what}: one painter's step applied ${p.stepBytes} bytes of patches (at most ${STEP_BYTES})`)
  assert.ok(p.cpuMs <= CPU_MS, `${what}: one task ran ${p.cpuMs} ms on the page's thread's processor (at most ${CPU_MS}; ${p.longest} ms on the clock); inside it: ${JSON.stringify(p.within)}`)
}

// The mark a phase ends on, which places the trace's tasks on the page's clock.
const ENDED = "bigstore:phase"

// `act` run, until `until` holds and the page is quiet, traced: its tasks counted from now, or from the start of the
// document `act` loads.
const phase = async (act: () => unknown, until: string, from: "now" | "document"): Promise<Phase> => {
  const stop = await app.page.trace(TASKS)
  const since = from === "now" ? await app.page.evaluate<number>(`performance.now()`) : 0
  await act()
  const drawn = await settle(app.page, until, STEP_MS)
  await app.page.evaluate(`void performance.mark(${JSON.stringify(ENDED)})`)
  const traced = tasksOf(await stop(), ENDED).filter((t) => t.at >= since)
  const heaviest = traced.reduce<Task | null>((a, t) => (a === null || t.cpuMs > a.cpuMs ? t : a), null)
  const long = await app.page.evaluate<[number, number][]>(`window.__long.filter(([at]) => at >= ${since})`)
  const [steps, slices, stepBytes] = await app.page.evaluate<[number, number, number]>(stepsSince(since))
  const within = heaviest === null ? [] : await app.page.evaluate<Held[]>(measuresIn(heaviest.at, heaviest.ms))
  return {
    ms: Math.round(drawn - since),
    cpuMs: Math.round(heaviest?.cpuMs ?? 0),
    longest: Math.round(Math.max(0, ...long.map(([, ms]) => ms))),
    tasks: long.length,
    steps,
    slices,
    stepBytes,
    within,
  }
}

// A load of the app on the todo list, then the stream of conversations and the largest conversation as pages it moves to.
const visit = async (load: string): Promise<{ readonly boot: Phase; readonly stream: Phase; readonly opened: Phase }> => {
  const boot = await phase(() => app.page.navigate(`${LINES_BASE}/?${load}#/`), `document.querySelectorAll(".tree .row").length > 0`, "document")
  const stream = await phase(() => app.page.evaluate(`void (location.hash = "#/secretary")`), BULK, "now")
  const was = await app.page.evaluate<number>(placed)
  const opened = await phase(() => app.page.evaluate(`void (location.hash = "#/secretary/bulk-0")`), `document.querySelectorAll(".turns .turn").length > 0 && ${placed} > ${was}`, "now")
  return { boot, stream, opened }
}

app.scene("a store of many lines is listed, opened and grown a slice to a step, with no task on the page's thread over 100 ms of its processor time", async () => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  const unobserve = await app.page.beforeEveryDocument(OBSERVE)
  await app.page.throttle(RATE)
  try {
    const cold = await visit("cold")
    const before = await app.page.evaluate<number>(`document.querySelectorAll(".turns .turn-model").length`)
    const was = await app.page.evaluate<number>(placed)
    let asked: Promise<unknown> = Promise.resolve()
    const line = await phase(() => void (asked = askAtTheTerminal("bulk-0", "one more question", LINES_INSTANCE)), `document.querySelectorAll(".turns .turn-model").length > ${before} && ${placed} > ${was}`, "now")
    await asked
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
    for (const [what, p] of phases) judge(what, p)
  } finally {
    await app.page.throttle(1)
    await unobserve()
  }
})

// The most layout objects one frame of the stream's slices lays out: a slice's rounds (their words within the page's
// `slices::WORDS`, or one round alone) and what inserting them dirties around them. One of the store's rounds is 58;
// two, the slice before words bounded it, were 91.
const SLICE_OBJECTS = 70
// The wasm functions the page's thread compiles on their first call while the stream draws, at most: the rehearsal
// worker compiles every one the stream's first readings reach (`web/src/pages/rehearse.ts`).
const COMPILED_ON_THE_PAGE = 0

app.scene("a frame of the stream lays out one slice, and the rehearsal compiled the stream's functions before it draws", async () => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  // The earlier scenes' pages are gone, and with them the views module they compiled and ran: this page compiles its
  // own, as a first visit does, so what the stream finds compiled is what the rehearsal compiled.
  await app.page.collectGarbage()
  const unobserve = await app.page.beforeEveryDocument(OBSERVE)
  try {
    const stop = await app.page.trace(["devtools.timeline", "disabled-by-default-devtools.timeline", "v8.wasm", "blink.user_timing"])
    await app.page.navigate(`${LINES_BASE}/?traced#/`)
    await settle(app.page, `document.querySelectorAll(".tree .row").length > 0`, STEP_MS)
    await app.page.evaluate(`void (location.hash = "#/secretary")`)
    await settle(app.page, BULK, STEP_MS)
    const events = await stop()
    const { main, complete } = pageThread(events)
    const lazy = (thread: string): readonly TraceEvent[] => events.filter((e) => e.name === "wasm.CompileLazy" && threadKey(e) === thread)
    const rehearsed = lazy(workerThread(events, "/pagework.js")).length
    const drawn = measured(events, "page:draw").find(([, , d]) => (JSON.parse(d) as { readonly page?: string } | null)?.page === "SecretaryPage")
    assert.ok(drawn !== undefined, "the stream's page was drawn")
    const slices = measured(events, "painter:step").filter(([, , d]) => (JSON.parse(d) as { readonly step?: string } | null)?.step === `Answered {"type":"Slice"}`)
    // What each slice's patches dirtied: the layouts of its frame after its step. A layout inside the step is a keeping
    // step's (`run.start`), which reads the place before its patches and so lays out what an earlier step left.
    const frames = slices.map(([, end]) => {
      const task = complete.find((t) => t.name === "RunTask" && t.ts <= end && end <= t.ts + (t.dur ?? 0))
      const after = complete.filter((e) => task !== undefined && e.ts >= end && e.ts + (e.dur ?? 0) <= task.ts + (task.dur ?? 0))
      return layoutsOf(after).reduce((n, l) => n + l.dirty, 0)
    })
    const compiled = lazy(main).filter((e) => e.ts >= drawn[0]).map((e) => e.args?.func_index ?? -1)
    console.log(JSON.stringify({ sliceFrames: frames, rehearsed, compiledOnThePage: compiled }))
    assert.ok(rehearsed > 0, "the rehearsal compiled nothing: the page was handed a views module an earlier page compiled and ran")
    // The first slice's frame also lays out what the page's own first step left, its topic bar among it.
    const later = frames.slice(1)
    assert.ok(later.length > 0, "the stream drew its rounds in slices")
    assert.ok(Math.max(...later) <= SLICE_OBJECTS, `a frame of the stream laid out ${Math.max(...later)} objects (at most ${SLICE_OBJECTS}): ${frames.join(", ")}`)
    assert.ok(compiled.length <= COMPILED_ON_THE_PAGE, `the page's thread compiled ${compiled.length} wasm functions as the stream drew (at most ${COMPILED_ON_THE_PAGE}): ${compiled.join(", ")}`)
  } finally {
    await unobserve()
  }
})

// A closed fold holds nothing in layout: under a closed <details>, no element but its summary has a box. Chromium keeps
// a closed fold's content laid out behind a lock and lays all of it out at once when the lock is lifted (it does under
// memory pressure), so the largest conversation's folds would cost one layout of every fold together.
const FOLDS = `(() => {
  const closed = [...document.querySelectorAll("details:not([open])")]
  const boxed = closed.flatMap((d) =>
    [...d.querySelectorAll(":scope > :not(summary), :scope > :not(summary) *")]
      .filter((e) => e.getClientRects().length > 0)
      .map((e) => e.tagName.toLowerCase() + (e.className === "" ? "" : "." + String(e.className).split(" ")[0])))
  return { closed: closed.length, boxed }
})()`

app.scene("a conversation's closed folds hold no layout box once it is opened", async () => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  await app.page.navigate(`${LINES_BASE}/?folds#/`)
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: STEP_MS })
  await app.page.evaluate(`void (location.hash = "#/secretary")`)
  await app.page.waitFor(BULK, { timeout: STEP_MS })
  const was = await app.page.evaluate<number>(placed)
  await app.page.evaluate(`void (location.hash = "#/secretary/bulk-0")`)
  await app.page.waitFor(`document.querySelectorAll(".turns .turn").length > 0 && ${placed} > ${was}`, { timeout: STEP_MS })
  const folds = await app.page.evaluate<{ readonly closed: number; readonly boxed: readonly string[] }>(FOLDS)
  assert.ok(folds.closed > 0, "the largest conversation draws closed folds")
  assert.equal(folds.boxed.length, 0, `elements under ${folds.closed} closed folds have a layout box: ${folds.boxed.slice(0, 8).join(", ")}`)
})

// The API reads open now, less the event streams, which stay open for a page's life; a build's own files are the shell's
// worker's to answer.
const reads = (open: readonly string[]): string[] => open.filter((r) => r.includes(" /api/") && !r.endsWith("/events"))

// The conversations the stream drew rounds of, by their links.
const DRAWN = `[...new Set([...document.querySelectorAll(".streamround .streammeta a")].map((a) => decodeURIComponent(/secretary\\/([^/?#&@]+)/.exec(a.getAttribute("href"))?.[1] ?? "")))]`
// In the core's worker: how many prints its lines decoded, hashed and opened, and how many objects the device's skeleton
// holds, in all and of the conversations `ids` and the structure every line rests on, which bounds what drawing their
// rounds needs.
// How many prints the core's worker decoded, hashed and opened, asked of the glue module it loaded.
const DECODED = `import(new URL("prodrome.js", self.location.href).href).then((m) => ({ decoded: m.decodes(), hashed: m.hashes(), opened: m.opens() }))`

// The box's whole skeleton, piece after piece: what the drawn rounds' conversations need is counted from it.
type Skeleton = { readonly nodes: readonly { readonly header: { readonly node: string; readonly conversation?: string } }[] }
const SKELETON = `(async () => {
  const nodes = []
  for (let since = ""; ; ) {
    const a = await fetch("/api/stores/lines/skeleton" + (since === "" ? "" : "?since=" + since)).then((r) => r.json())
    nodes.push(...a.nodes)
    if (a.rest === 0) return { nodes }
    since = a.frontier.digest
  }
})()`

type Cost = { decoded: number; hashed: number; opened: number; needed: number; all: number }

app.scene("a warm load of the conversations draws them from the device, with every read of the box held, a slice to a step and no task on the page's thread over 100 ms of its processor time", async () => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  const unobserve = await app.page.beforeEveryDocument(OBSERVE)
  await app.page.navigate(`${LINES_BASE}/#/secretary`)
  await app.page.waitFor(BULK, { timeout: STEP_MS })
  // The cold load done: the device holds the lines, and nothing it asked for is still coming.
  const journal = await app.page.journal()
  const deadline = Date.now() + STEP_MS
  while (reads(journal.inFlight()).length > 0 && Date.now() < deadline) await new Promise((done) => setTimeout(done, 200))
  assert.deepEqual(reads(journal.inFlight()), [], "the cold load ended")
  // As a page left open does: the lines are quiet, so the core's snapshot of them is kept.
  await app.page.waitFor(snapshotKept("lines"), { timeout: 120000 })
  const kept = await app.page.evaluate<number>(`${REPLICA_ROWS}.then((rows) => (rows.find(([k]) => k === "lines/snapshot")?.[1].length ?? 0) / 2)`)
  console.log(`lines snapshot ${kept} bytes for ${SHAPE.lines} lines`)
  const skeleton = await app.page.evaluate<Skeleton>(SKELETON)
  journal.mark("warm load")
  // Every request to the box's API is held until the stream has drawn, so what draws is drawn from the device.
  const release = await app.page.holdRequests(["/api/"])
  const drawn = await (async (): Promise<{ readonly draw: Phase; readonly held: readonly string[]; readonly cost: Cost }> => {
    try {
      await app.page.throttle(RATE)
      const draw = await phase(() => app.page.reload(), BULK, "document")
      const ids = await app.page.evaluate<string[]>(DRAWN)
      const counted = await app.page.inWorker<Pick<Cost, "decoded" | "hashed" | "opened">>("prodromework", DECODED)
      const of = new Set(ids)
      const needed = skeleton.nodes.filter((n) => n.header.node === "structure" || of.has(n.header.conversation ?? "")).length
      return { draw, held: app.page.heldUrls(), cost: { ...counted, needed, all: skeleton.nodes.length } }
    } finally {
      await app.page.throttle(1)
      await release()
      await unobserve()
    }
  })()
  console.log(JSON.stringify({ ...SHAPE, rate: RATE, warm: drawn.draw, ...drawn.cost }))
  assert.ok(drawn.draw.ms < wait.MS / 3, `the stream drew ${drawn.draw.ms} ms after the document started (bound ${wait.MS / 3})`)
  judge("the warm load with the box held", drawn.draw)
  assert.ok(drawn.cost.decoded <= drawn.cost.needed, `the warm load decoded ${drawn.cost.decoded} prints; its rounds need ${drawn.cost.needed} of ${drawn.cost.all}`)
  assert.equal(drawn.cost.hashed, 0, "every print the warm load read was admitted by its name, so none is hashed again")
  assert.ok(drawn.held.length > 0, `the box was asked and answered nothing before the stream drew:\n${journal.lines().join("\n")}`)
})

// The worker's marks in milliseconds from the first stream asked.
const fromStream = (marks: Readonly<Record<string, number>>): Readonly<Record<string, number>> =>
  Object.fromEntries(Object.entries(marks).map(([name, at]) => [name, Math.round(at - (marks["stream asked"] ?? 0))]))

// How many warm loads the scene reads, each reported.
const LOADS = 3

app.scene("a warm load of the conversations answers the page's first stream before any question of the shell asked beside it", async () => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  const unobserve = await app.page.beforeEveryDocument(OBSERVE)
  await app.page.navigate(`${LINES_BASE}/#/secretary`)
  await app.page.waitFor(BULK, { timeout: STEP_MS })
  const journal = await app.page.journal()
  const deadline = Date.now() + STEP_MS
  while (reads(journal.inFlight()).length > 0 && Date.now() < deadline) await new Promise((done) => setTimeout(done, 200))
  assert.deepEqual(reads(journal.inFlight()), [], "the cold load ended")
  // As a page left open does: the lines are quiet, so the core's snapshot of them is kept.
  await app.page.waitFor(snapshotKept("lines"), { timeout: 120000 })
  // The box answers as it does live, so the shell asks what it asks on every load.
  const unrecord = await app.page.beforeEveryWorker(WORKER_QUESTIONS)
  type Load = { readonly draw: Phase; readonly seen: readonly Recorded[]; readonly phases: Readonly<Record<string, number>> }
  const loads = await (async (): Promise<Load[]> => {
    const each: Load[] = []
    try {
      await app.page.throttle(RATE)
      for (let n = 0; n < LOADS; n += 1) {
        const draw = await phase(() => app.page.reload(), BULK, "document")
        each.push({ draw, seen: await app.page.inWorker<Recorded[]>("prodromework", "self.__questions"), phases: fromStream(await app.page.inWorker<Readonly<Record<string, number>>>("prodromework", WORKER_MARKS)) })
      }
      return each
    } finally {
      await app.page.throttle(1)
      await unrecord()
      await unobserve()
    }
  })()
  // The one number each stage of docs/ONEOWNER-PLAN-2026-11-02.md reports, printed before any assertion can stop the scene.
  const draws = loads.map((l) => l.draw.ms).sort((a, b) => a - b)
  console.log(`warm first draw ${draws[Math.floor(draws.length / 2)]} ms (median of ${LOADS} loads, cpu ${process.env["SUZATARY_E2E_BROWSER_CPU"] ?? "all"})`)
  for (const { draw, seen, phases } of loads) {
    const asked = new Map(seen.flatMap((s) => (s[0] === "asked" ? [[s[1], { lane: s[3], kind: s[4], at: s[2] }] as const] : [])))
    const at = (type: Recorded[0], id: number): number => seen.findIndex((s) => s[0] === type && s[1] === id)
    const stream = [...asked].find(([, q]) => q.kind === "Stream")?.[0]
    assert.ok(stream !== undefined, `the page asked for its stream:\n${JSON.stringify(seen)}`)
    const [from, to] = [at("asked", stream), at("answered", stream)]
    // Each question answered while the stream was owed, as `<lane> <kind> #<number>`.
    const between = seen.slice(from, to).flatMap((s) => (s[0] === "answered" ? [{ id: s[1], ...asked.get(s[1]) }] : []))
    const named = (qs: readonly { readonly id: number; readonly lane?: string; readonly kind?: string }[]): string[] => qs.map((q) => `${q.lane} ${q.kind} #${q.id}`)
    const answered = seen[to]?.[2] ?? -1
    // How many times the page asked for its stream before the first answer.
    const streams = seen.slice(0, to).filter((s) => s[0] === "asked" && s[4] === "Stream").length
    console.log(JSON.stringify({ ...SHAPE, rate: RATE, firstDraw: draw.ms, cpuMs: draw.cpuMs, longest: draw.longest, within: draw.within, streamAsked: asked.get(stream)?.at, streamAnswered: answered, streams, answeredWhileTheStreamWasOwed: named(between), cpu: process.env["SUZATARY_E2E_BROWSER_CPU"] ?? "all", worker: phases }))
    assert.deepEqual(named(between.filter((q) => q.lane === "Shell")), [], `the worker answered the shell while the page's stream was owed:\n${JSON.stringify(seen)}`)
    assert.ok(draw.ms < wait.MS / 3, `the stream drew ${draw.ms} ms after the document started (bound ${wait.MS / 3})`)
  }
})
