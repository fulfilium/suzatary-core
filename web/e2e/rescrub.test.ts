/**
 * A STORE SEALED AGAIN IS READ FROM ITS RESEALS: the rescrub server holds a generated store whose lines were sealed
 * before lines recorded their rules and were never scrubbed (`suzatary-dev fixture-chain --lines … --sealed
 * unversioned`; 500 lines over 30 conversations by default, `SUZATARY_E2E_RESCRUB_LINES` and
 * `SUZATARY_E2E_RESCRUB_CONVERSATIONS` to change it). A tab loads it cold and warm, the box runs `secretary
 * migrate-scrub --apply` while the tab is closed, which seals every slot again and destroys the keys it replaced, and
 * the tab opens it again, hearing of the reseals in one pull.
 *
 * After that load the device holds the box's keys and no other: its own sweep destroyed each key a reseal replaced,
 * which it reads from the reseals' nodes. A device that took the reseals in an earlier pull and still holds a key they
 * replaced, as one whose purge was cut short does, drops it again on its next load, from the nodes it holds, while the
 * pull it makes lacks nothing and brings no key's name. A last warm load then scrubs nothing in the core's worker, which
 * the warm load before the migration did, and draws the stream within `DRAW_MS`. The count is the core module's own
 * (`scrubs`), read in the worker's scope from the module the worker loaded, so it holds whatever the fixture's size.
 *
 * As a measurement at the live shape: `SUZATARY_E2E_RESCRUB_LINES=6500 SUZATARY_E2E_RESCRUB_CONVERSATIONS=100
 * SUZATARY_LINES_RATE=6 SUZATARY_E2E_SCENE_MS=1800000 SUZATARY_LINESBENCH=<file> scripts/web-e2e.sh rescrub` writes
 * each warm load's draw time.
 */

import assert from "node:assert/strict"
import { readdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"

import { LANDED_ROW, PHONE, REPLICA_ROWS, STEP_MS, atTheTerminal, suite, trustThisDevice, withCore } from "./harness"

const required = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-e2e.sh`)
  return v
}

const BASE = required("E2E_BASE_RESCRUB")
const INSTANCE = required("E2E_RESCRUB_INSTANCE")
const RATE = Number(process.env["SUZATARY_LINES_RATE"] ?? "1")
// The longest a warm load may take to draw the stream, from the document's start, at each CPU rate the scene runs at.
const DRAW_MS = RATE >= 6 ? 30_000 : 10_000

// A round of the generated conversations, drawn in the stream.
const BULK = `document.querySelectorAll('.streamround .streammeta a[href*="bulk-"]').length > 0`

// The names of the keys the device's lines hold, as the core reads the device's snapshot and frames back.
const DEVICE_KEYS = `${LANDED_ROW}.then((row) => [...row.keys].sort())`

// The keys the box's lines name, `{<name>: <hex>}`, as its skeleton answers this login.
const BOX_KEYS = `fetch("/api/stores/lines/skeleton").then((r) => r.json()).then((a) => a.keys)`

// The key `name` holding `hex` written on the device again, as a closed tab's late frame: a landed row of the box's
// epoch holding that key and no node, printed by the core and appended after the device's log.
const plant = (name: string, hex: string): string =>
  withCore(`
    const box = await fetch("/api/stores/lines/skeleton").then((r) => r.json())
    const lines = new core.Lines()
    const nothing = { heads: { are: "these", heads: [] }, geneses: [], epoch: box.epoch, nodes: [], since: "Known", frontier: { digest: "0".repeat(64), count: 0 }, keys: {}, rest: 0, size: 0, weight: 0 }
    lines.piece(JSON.stringify(nothing), JSON.stringify({ ${JSON.stringify(name)}: ${JSON.stringify(hex)} }), 0)
    const landed = lines.land()
    const frame = landed.delta
    landed.free()
    lines.free()
    await new Promise((done, fail) => {
      const r = indexedDB.open("suzatary")
      r.onerror = () => fail(String(r.error))
      r.onsuccess = () => {
        const tx = r.result.transaction("replica", "readwrite")
        tx.objectStore("replica").put(frame, "lines/log/999999999999")
        tx.oncomplete = () => (r.result.close(), done(true))
        tx.onerror = () => fail(String(tx.error))
      }
    })
  `)

// The skeleton the box answers to the frontier the device's lines stand at.
const KNOWN_PULL = `${LANDED_ROW}.then((row) =>
  fetch("/api/stores/lines/skeleton?since=" + row.frontier.digest).then((a) => a.text()).then((text) => ({ bytes: text.length, answer: JSON.parse(text) })))`

// The names of the keys the box keeps, one file each beside its lines.
const boxKeys = (): string[] => readdirSync(join(dirname(INSTANCE), "secretary-lines-keys")).sort()

// How many values the core's worker has scrubbed, asked of the glue module it loaded beside its own script.
const SCRUBS = `import(new URL("prodrome.js", self.location.href).href).then((m) => m.scrubs())`

const app = await suite("rescrub", PHONE)

// The stream drawn, timed from the document's start until its first round is drawn, and what the core's worker
// scrubbed until then.
const drawn = async (): Promise<{ readonly ms: number; readonly scrubs: number }> => {
  await app.page.waitFor(BULK, { timeout: STEP_MS })
  const ms = Math.round(await app.page.evaluate<number>(`performance.now()`))
  return { ms, scrubs: await app.page.inWorker<number>("prodromework", SCRUBS) }
}
const load = async (): Promise<{ readonly ms: number; readonly scrubs: number }> => {
  await app.page.reload()
  return drawn()
}

app.scene("after migrate-scrub the device holds the box's keys and a warm load scrubs nothing and draws within its bound", async () => {
  await trustThisDevice(app.page, BASE, INSTANCE)
  await app.page.throttle(RATE)
  try {
    await app.page.navigate(`${BASE}/#/secretary`)
    await app.page.waitFor(BULK, { timeout: STEP_MS })
    const stale = await load()
    const held = new Map(Object.entries(await app.page.evaluate<Record<string, string>>(BOX_KEYS)))
    // The tab is closed while the box runs the migration, so it hears of the reseals in one pull when it opens again.
    await app.page.navigate("about:blank")
    const before = boxKeys()
    // The migration's own time, logged: it runs within one step's bound (`STEP_MS`), and costs about 7 ms a stale slot
    // and 2 s of census on a fast machine, several times that on the box.
    const migrating = Date.now()
    const said = atTheTerminal(["secretary", "migrate-scrub", "--apply"], INSTANCE)
    const migrated = Date.now() - migrating
    assert.ok(/sealed [1-9][0-9]* slot\(s\) again/.test(said), said)
    const after = boxKeys()
    assert.ok(after.some((k) => !before.includes(k)) && before.some((k) => !after.includes(k)), "the box replaced its keys")
    await app.page.navigate(`${BASE}/#/secretary`)
    const resealed = await drawn()
    // The device's sweep runs once the content naming the reseals is held, after the reading it waited for.
    const swept = Date.now()
    const deadline = swept + STEP_MS
    let device = await app.page.evaluate<string[]>(DEVICE_KEYS)
    while (JSON.stringify(device) !== JSON.stringify(after) && Date.now() < deadline) {
      await new Promise((done) => setTimeout(done, 500))
      device = await app.page.evaluate<string[]>(DEVICE_KEYS)
    }
    assert.deepEqual(device, after, "the device holds the box's keys and no other")
    const converged = Date.now() - swept
    // A key the reseals replaced, held again by a device that took them in an earlier pull.
    const replaced = before.find((k) => !after.includes(k) && held.has(k))
    assert.ok(replaced !== undefined, "a key the device held was replaced")
    await app.page.evaluate(plant(replaced, held.get(replaced)!))
    await app.page.reload()
    await app.page.waitFor(BULK, { timeout: STEP_MS })
    const again = Date.now() + STEP_MS
    device = await app.page.evaluate<string[]>(DEVICE_KEYS)
    while (JSON.stringify(device) !== JSON.stringify(after) && Date.now() < again) {
      await new Promise((done) => setTimeout(done, 500))
      device = await app.page.evaluate<string[]>(DEVICE_KEYS)
    }
    assert.deepEqual(device, after, "the device drops the replaced key again, from the nodes it holds")
    assert.equal(
      await app.page.evaluate<boolean>(`${REPLICA_ROWS}.then((rows) => rows.some(([, v]) => v.includes(${JSON.stringify(held.get(replaced))})))`),
      false,
      "and no snapshot or frame holds its bytes",
    )
    const known = await app.page.evaluate<{ bytes: number; answer: { nodes: unknown[]; keys: Record<string, string>; since: string } }>(KNOWN_PULL)
    assert.equal(known.answer.since, "Known")
    assert.deepEqual([known.answer.nodes.length, Object.keys(known.answer.keys).length], [0, 0], "a pull that lacks nothing brings no node and no key")
    assert.ok(known.bytes < 1024, `a pull that lacks nothing is ${known.bytes} bytes`)
    const warm = await load()
    const measured = { lines: Number(process.env["SUZATARY_E2E_RESCRUB_LINES"] ?? "500"), rate: RATE, migrated, stale, resealed, converged, warm }
    console.log(JSON.stringify(measured))
    const out = process.env["SUZATARY_LINESBENCH"]
    if (out !== undefined && out !== "") writeFileSync(out, `${JSON.stringify(measured, null, 2)}\n`)
    assert.ok(stale.scrubs > 0, "the warm load before the migration scrubbed the stale slots as it opened them")
    assert.equal(warm.scrubs, 0, "the warm load after the migration scrubs nothing")
    assert.ok(warm.ms <= DRAW_MS, `the warm load drew the stream ${warm.ms} ms after the document started (bound ${DRAW_MS})`)
  } finally {
    await app.page.throttle(1)
  }
})
