/**
 * WHAT A WARM LOAD PAYS FOR A SNAPSHOT (docs/LAZYSYNC-PLAN-2026-10-21.md §16, step 1): over the lines' server at the
 * live shape, the cold load done, a page of the same origin that runs no app (the box's `/api/about`, so nothing else
 * reads the device meanwhile) builds one core from the lines' rows as a restore hands them over, prints its snapshot and
 * keeps it in the lines' store under a name no row of the restore reads. That page is loaded again three times, and each
 * time the scene times one IndexedDB `get` of that value, one read of the head and the key names a warm load reads beside
 * it, and one wasm call that loads a core from it, at whatever CPU `SUZATARY_E2E_BROWSER_CPU` gives the browser.
 *
 * `SUZATARY_E2E_BROWSER_CPU=0.67 SUZATARY_E2E_SCENE_MS=1800000 scripts/web-e2e.sh snapshotmeasure`
 */

import { PHONE, suite, trustThisDevice } from "./harness"

const required = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-e2e.sh`)
  return v
}
const LINES_BASE = required("E2E_BASE_LINES")
const LINES_INSTANCE = required("E2E_LINES_INSTANCE")
const BULK = `document.querySelectorAll('.streamround .streammeta a[href*="bulk-"]').length > 0`
const KEY = "measure/snapshot"

const app = await suite("snapshotmeasure", PHONE)

const open = `new Promise((done, fail) => { const r = indexedDB.open("suzatary"); r.onsuccess = () => done(r.result); r.onerror = () => fail(r.error) })`
const req = (r: string): string => `new Promise((done, fail) => { const q = ${r}; q.onsuccess = () => done(q.result); q.onerror = () => fail(q.error) })`

// The core's module of the build the app loaded, by the URL its bundle came from, loaded in a page that runs no app.
const BUILT = `[...performance.getEntriesByType("resource")].map((e) => e.name).find((n) => n.endsWith("/app.js")).replace(/app\.js$/, "prodrome.js")`
let core = ""
const CORE = (): string => `(async () => { const m = await import(${JSON.stringify(core)}); await m.default(); return m })()`

// A core made from the rows as a restore hands them over, its snapshot kept.
const BUILD = (): string => `(async () => {
  const m = await ${CORE()}
  const db = await ${open}
  const s = db.transaction("lines").objectStore("lines")
  const nodes = await ${req(`s.getAll(IDBKeyRange.bound("node/", "node/\\uffff"))`)}
  const names = await ${req(`s.getAllKeys(IDBKeyRange.bound("key/", "key/\\uffff"))`)}
  const hexes = await ${req(`s.getAll(IDBKeyRange.bound("key/", "key/\\uffff"))`)}
  const keys = Object.fromEntries(names.map((n, i) => [n.slice(4), hexes[i]]))
  const lines = new m.Lines()
  lines.nodes(JSON.stringify(nodes), "[]")
  lines.keys(JSON.stringify(keys), "[]")
  const t0 = performance.now()
  const bytes = lines.snapshot()
  const printed = performance.now() - t0
  lines.free()
  await ${req(`db.transaction("lines", "readwrite").objectStore("lines").put({ key: "measure", bytes }, ${JSON.stringify(KEY)})`)}
  db.close()
  return { nodes: nodes.length, keys: names.length, bytes: bytes.length, printMs: Math.round(printed) }
})()`

// One get of the snapshot, one read of the head and the key names a warm load reads beside it, and one load.
const LOAD = (): string => `(async () => {
  const m = await ${CORE()}
  const db = await ${open}
  const t0 = performance.now()
  const tx = db.transaction("lines").objectStore("lines")
  const row = await ${req(`tx.get(${JSON.stringify(KEY)})`)}
  const t1 = performance.now()
  const tx2 = db.transaction("lines").objectStore("lines")
  const head = ${req(`tx2.get("head")`)}
  const names = await ${req(`tx2.getAllKeys(IDBKeyRange.bound("key/", "key/\\uffff"))`)}
  await head
  const t2 = performance.now()
  const lines = new m.Lines()
  lines.load(row.bytes)
  const t3 = performance.now()
  lines.free()
  db.close()
  return { getMs: Math.round(t1 - t0), headAndKeyNamesMs: Math.round(t2 - t1), keyNames: names.length, loadMs: Math.round(t3 - t2) }
})()`

app.scene("one get of the lines' snapshot and one load of the core from it, timed", async () => {
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  await app.page.navigate(`${LINES_BASE}/#/secretary`)
  await app.page.waitFor(BULK, { timeout: 1200000 })
  const journal = await app.page.journal()
  const deadline = Date.now() + 1200000
  while (journal.inFlight().some((r) => r.includes(" /api/") && !r.endsWith("/events")) && Date.now() < deadline) await new Promise((done) => setTimeout(done, 200))
  core = await app.page.evaluate<string>(BUILT)
  await app.page.navigate(`${LINES_BASE}/api/about`)
  const built = await app.page.evaluate<Record<string, number>>(BUILD())
  console.log(JSON.stringify({ built, cpu: process.env["SUZATARY_E2E_BROWSER_CPU"] ?? "all" }))
  for (let n = 0; n < 3; n += 1) {
    await app.page.reload()
    console.log(JSON.stringify({ load: n + 1, ...(await app.page.evaluate<Record<string, number>>(LOAD())), cpu: process.env["SUZATARY_E2E_BROWSER_CPU"] ?? "all" }))
  }
})
