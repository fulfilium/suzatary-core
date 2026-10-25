/**
 * A LARGE STORE OF LINES DOES NOT HOLD THE PAGE'S THREAD: the lines' server holds a generated store
 * (`suzatary-dev fixture-chain --lines`, 1,500 lines by default, `SUZATARY_E2E_LINES` to change it), and the
 * core's worker reads it, so a new line is read with no long task on the page's thread. The scene records every long
 * task (over 50 ms) the browser reports while the stream draws, a conversation opens and a new line arrives.
 *
 * As a measurement: `SUZATARY_E2E_LINES=6000 SUZATARY_LINES_RATE=6 SUZATARY_LINESBENCH=<file> scripts/web-e2e.sh
 * bigstore` runs it at a 6x CPU throttle and writes each phase's longest task and its time; the list's is from the
 * document's start until it draws.
 */

import assert from "node:assert/strict"
import { writeFileSync } from "node:fs"

import { PHONE, SIGNED_POST, suite, trustThisDevice } from "./harness"

const required = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-e2e.sh`)
  return v
}

const LINES_BASE = required("E2E_BASE_LINES")
const LINES_INSTANCE = required("E2E_LINES_INSTANCE")
const RATE = Number(process.env["SUZATARY_LINES_RATE"] ?? "1")
// The longest task the scene allows on the page's thread while a new line is read, scaled by the throttle.
const LONGEST_MS = 500 * RATE

const app = await suite("bigstore", PHONE)

// Every long task the page's thread ran since the document started, kept in the page.
const OBSERVE = `(() => {
  window.__long = []
  new PerformanceObserver((list) => { for (const e of list.getEntries()) window.__long.push([e.startTime, e.duration]) })
    .observe({ type: "longtask", buffered: true })
})()`

type Phase = { readonly ms: number; readonly longest: number; readonly tasks: number }

// The long tasks from `from` (now, when absent; the document's start is 0) until `until` holds, and how long that took.
const phase = async (until: string, from?: number, timeout = 600000): Promise<Phase> => {
  const since = from ?? (await app.page.evaluate<number>(`performance.now()`))
  await app.page.waitFor(until, { timeout })
  const now = await app.page.evaluate<number>(`performance.now()`)
  const tasks = await app.page.evaluate<[number, number][]>(`window.__long.filter(([at]) => at >= ${since})`)
  return { ms: Math.round(now - since), longest: Math.round(Math.max(0, ...tasks.map(([, d]) => d))), tasks: tasks.length }
}

app.scene("a new line in a store of many lines is read with the page's thread free", async () => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  const unobserve = await app.page.beforeEveryDocument(OBSERVE)
  await app.page.throttle(RATE)
  try {
    await app.page.navigate(`${LINES_BASE}/#/secretary`)
    const list = await phase(`document.querySelectorAll('.streamround .streammeta a[href*="bulk-"]').length > 0`, 0)
    await app.page.navigate(`${LINES_BASE}/#/secretary/bulk-0`)
    const opened = await phase(`document.querySelectorAll(".turns .turn").length > 0`)
    const before = await app.page.evaluate<number>(`document.querySelectorAll(".turns .turn-model").length`)
    const status = await app.page.evaluate<number>(
      `${SIGNED_POST}("/api/secretary/conversations/bulk-0/say", JSON.stringify({say: "one more question"})).then((r) => r.status)`,
    )
    assert.ok(status < 300, `the box took the line (${status})`)
    const line = await phase(`document.querySelectorAll(".turns .turn-model").length > ${before}`)
    const measured = { lines: Number(process.env["SUZATARY_E2E_LINES"] ?? "1500"), rate: RATE, list, opened, line }
    console.log(JSON.stringify(measured))
    const out = process.env["SUZATARY_LINESBENCH"]
    if (out !== undefined && out !== "") writeFileSync(out, `${JSON.stringify(measured, null, 2)}\n`)
    assert.ok(line.longest <= LONGEST_MS, `a new line held the page's thread ${line.longest} ms at once (at most ${LONGEST_MS})`)
  } finally {
    await app.page.throttle(1)
    await unobserve()
  }
})
