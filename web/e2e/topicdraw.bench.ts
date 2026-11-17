/**
 * FROM THE TAP TO THE TOPIC DRAWN, MEASURED: on the lines' server (`bigstore`'s shape), over a phone-class network, the
 * time from the tap on the topic sheet's write to the sheet closed, and to the round's new chip drawn. Each is timed in
 * the page, from the click to the first frame that shows it, so the browser's own channel to this script costs nothing.
 * A measurement, not a test: no gate runs it.
 *
 *   scripts/web-e2e.sh topicdraw
 *
 * It prints the median and the slowest of `SUZATARY_TOPICDRAW_RUNS` taps (default 7) in milliseconds, and writes them as
 * JSON to `$SUZATARY_TOPICDRAW` when that is set, with each step the app measures of a write: its duration, and for a
 * step of the page `<step>@`, when it began after the tap. A second scene times a rename's Enter to its title drawn, and
 * writes it to `$SUZATARY_RENAMEDRAW`.
 */

import assert from "node:assert/strict"
import { writeFileSync } from "node:fs"

import { PHONE, suite, trustThisDevice } from "./harness"

const BULK = process.env["E2E_BASE_LINES"] ?? ""
const RUNS = Number(process.env["SUZATARY_TOPICDRAW_RUNS"] ?? "7")
// A slow phone connection: each request waits this long each way, at about 1 Mbit/s.
const LATENCY_MS = 400
const THROUGHPUT = 125_000
const app = await suite("topicdraw", PHONE)

// In the page: the write tapped, then the milliseconds until the sheet is gone and until a round shows `topic`. The
// topic is the run's own: a round holds three, and rounds still arrive at the first tap, so no position names the round.
const tapped = (topic: string): string => `new Promise((done) => {
  const start = performance.now()
  let closed = null
  window.e2eTapped = start
  document.querySelector(".topicsheet .act-topicwrite").click()
  const look = () => {
    const now = performance.now()
    if (closed === null && document.querySelector(".topicsheet") === null) closed = now - start
    const drawn = [...document.querySelectorAll(".streamrounds .roundtopic")].some((c) => c.textContent === ${JSON.stringify(topic)})
    if (closed !== null && drawn) return done({ closed, drawn: now - start })
    if (now - start > 60000) return done({ closed: closed ?? -1, drawn: -1 })
    requestAnimationFrame(look)
  }
  requestAnimationFrame(look)
})`

type Run = { readonly closed: number; readonly drawn: number }
type Steps = Readonly<Record<string, number>>

// The page's measures from the tap to the chip drawn, each name's last: when it began after the tap and how long it took.
const PAGE_STEPS = ["write:worker", "moved:batched", "ask:Stream", "read:Stream", "page:heard", "painter:step"]
const pageSteps = (drawn: number): string => `(() => {
  const from = window.e2eTapped, until = from + ${drawn}
  const steps = {}
  for (const m of performance.getEntriesByType("measure"))
    if (${JSON.stringify(PAGE_STEPS)}.includes(m.name) && m.startTime >= from && m.startTime + m.duration <= until) {
      steps[m.name] = m.duration
      steps[m.name + "@"] = m.startTime - from
    }
  return steps
})()`

// The worker's measures of its latest write, summed by name, and of the stream it read after.
const WORKER_STEPS = `(() => {
  const all = performance.getEntriesByType("measure").filter((m) => m.name.startsWith("write:") || m.name === "stream:read")
  const begun = all.filter((m) => m.name === "write:ready").at(-1)?.startTime ?? 0
  const steps = {}
  for (const m of all) if (m.startTime >= begun) steps[m.name] = m.name === "stream:read" ? m.duration : (steps[m.name] ?? 0) + m.duration
  return steps
})()`

const summary = (ms: readonly number[]): { readonly median: number; readonly slowest: number } => {
  const sorted = [...ms].sort((a, b) => a - b)
  return { median: Math.round(sorted[Math.floor(sorted.length / 2)] ?? 0), slowest: Math.round(sorted.at(-1) ?? 0) }
}

app.scene("measured: the tap on a topic to the sheet closed and the chip drawn, over a slow network", async () => {
  assert.notEqual(BULK, "", "E2E_BASE_LINES is unset — scripts/web-e2e.sh sets it")
  await trustThisDevice(app.page, BULK, process.env["E2E_LINES_INSTANCE"])
  await app.page.attachWorkers()
  await app.page.navigate(`${BULK}/#/secretary`)
  await app.page.waitFor(`document.querySelector(".streamround .roundtopicadd") !== null`, { timeout: 30 * 60_000 })
  await app.page.network({ offline: false, latency: LATENCY_MS, downloadThroughput: THROUGHPUT, uploadThroughput: THROUGHPUT })
  const runs: Run[] = []
  const steps: Steps[] = []
  for (let n = 0; n < RUNS; n++) {
    const topic = `measured-${Date.now()}-${n}`
    await app.page.click(".streamround .roundtopicadd")
    await app.page.waitFor(`document.activeElement?.classList.contains("topicfield") === true`, { timeout: 15000 })
    await app.page.evaluate(`(() => { const f = document.querySelector(".topicsheet .topicfield"); f.value = ${JSON.stringify(topic)}; f.dispatchEvent(new Event("input", { bubbles: true })); })()`)
    await app.page.waitFor(`document.querySelector(".topicsheet .act-topicwrite")?.disabled === false`, { timeout: 15000 })
    const run = await app.page.evaluate<Run>(tapped(topic), 90_000)
    assert.ok(run.drawn >= 0, `the chip ${topic} was drawn`)
    runs.push(run)
    steps.push({ ...(await app.page.evaluate<Steps>(pageSteps(run.drawn))), ...(await app.page.inWorker<Steps>("prodromework", WORKER_STEPS)) })
  }
  await app.page.network(null)
  const named = [...new Set(steps.flatMap(s => Object.keys(s)))].sort()
  const perStep = Object.fromEntries(named.map(n => [n, summary(steps.map(s => s[n] ?? 0))]))
  const report = { latency: LATENCY_MS, closed: summary(runs.map(r => r.closed)), drawn: summary(runs.map(r => r.drawn)), steps: perStep, runs }
  console.log(JSON.stringify(report, null, 2))
  const out = process.env["SUZATARY_TOPICDRAW"]
  if (out !== undefined && out !== "") writeFileSync(out, JSON.stringify(report, null, 2))
})

// In the page: the rename's Enter to the first frame whose page title is `name`. The start is the field's own keydown, so
// the channel from this script to the page costs nothing.
const renamed = (name: string): string => `new Promise((done) => {
  const field = document.querySelector(".renamefield")
  field.addEventListener("keydown", () => {
    const start = performance.now()
    const look = () => {
      const now = performance.now()
      if (document.querySelector(".pagetitle")?.textContent === ${JSON.stringify(name)}) return done(now - start)
      if (now - start > 60000) return done(-1)
      requestAnimationFrame(look)
    }
    requestAnimationFrame(look)
  }, { once: true, capture: true })
})`

app.scene("measured: a rename's Enter to the new title drawn, over a slow network", async () => {
  await app.page.navigate(`${BULK}/#/secretary`)
  await app.page.waitFor(`document.querySelector(".streamround .streammeta a") !== null`, { timeout: 30 * 60_000 })
  const href = await app.page.evaluate<string>(`document.querySelector(".streamround .streammeta a").getAttribute("href").replace(/\\?.*$/, "")`)
  await app.page.navigate(`${BULK}/${href}`)
  await app.page.waitFor(`document.querySelector(".act-rename") !== null`, { timeout: 60_000 })
  await app.page.network({ offline: false, latency: LATENCY_MS, downloadThroughput: THROUGHPUT, uploadThroughput: THROUGHPUT })
  const drawn: number[] = []
  for (let n = 0; n < RUNS; n++) {
    const name = `Renamed ${Date.now()} ${n}`
    await app.page.click(".act-rename")
    await app.page.waitFor(`document.activeElement?.classList.contains("renamefield") === true`, { timeout: 15000 })
    await app.page.evaluate(`document.querySelector(".renamefield").value = ${JSON.stringify(name)}`)
    // Set and not awaited: the promise ends only after the Enter below.
    await app.page.evaluate(`(window.e2eRename = ${renamed(name)}, true)`)
    await app.page.press("Enter")
    const ms = await app.page.evaluate<number>(`window.e2eRename`, 90_000)
    assert.ok(ms >= 0, `the title ${name} was drawn`)
    drawn.push(ms)
  }
  await app.page.network(null)
  const report = { latency: LATENCY_MS, renameDrawn: summary(drawn), runs: drawn }
  console.log(JSON.stringify(report, null, 2))
  const out = process.env["SUZATARY_RENAMEDRAW"]
  if (out !== undefined && out !== "") writeFileSync(out, JSON.stringify(report, null, 2))
})
