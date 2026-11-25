/**
 * A device that holds nothing opens `#/conversations` on the lines server's generated store over a phone's network:
 * each row is drawn from the skeleton before its words land and filled in place, a search typed meanwhile says how many
 * conversations it read and never "nothing matches" before it read them all, and rows a refused content answer left
 * waiting fill once the box answers again. Each scene waits for the fact it needs; the throttle only makes the first
 * sync long enough to act inside it.
 */

import assert from "node:assert/strict"

import { FAST_4G, PHONE, STEP_MS, suite, trustThisDevice } from "./harness"
import { placed } from "./longtasks"

const required = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-e2e.sh`)
  return v
}

const LINES_BASE = required("E2E_BASE_LINES")
const LINES_INSTANCE = required("E2E_LINES_INSTANCE")

const app = await suite("coldlist", PHONE)

// The list's rows, the pinned one first, each by its link and when it was last spoken in: where a row stands, and why.
const ROWS = `[...document.querySelectorAll(".chatsections .chatrow")].map((a) => [a.getAttribute("href"), a.querySelector(".chatwhen")?.textContent ?? ""])`
const UNHELD = `document.querySelector(".chatsections .chatrow.unheld")`

// The lines' content this device keeps, and the content the box's skeleton names.
const KEPT = `new Promise((ok) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => ok(0)
  r.onsuccess = () => {
    const db = r.result
    if (!db.objectStoreNames.contains("content")) return (db.close(), ok(0))
    const n = db.transaction("content", "readonly").objectStore("content").count()
    n.onsuccess = () => (db.close(), ok(n.result))
  }
})`
const NAMED = `(async () => {
  let n = 0
  for (let since = ""; ; ) {
    const a = await fetch("/api/stores/lines/skeleton" + (since === "" ? "" : "?since=" + since)).then((r) => r.json())
    n += a.nodes.length
    if (a.rest === 0) return n
    since = a.frontier.digest
  }
})()`

// A device that holds nothing, its first load throttled.
const cold = async (n: string): Promise<void> => {
  await app.page.navigate(`${LINES_BASE}/manifest.webmanifest`)
  await app.page.evaluate(`new Promise((ok) => { const r = indexedDB.deleteDatabase("suzatary"); r.onsuccess = r.onerror = r.onblocked = () => ok(true) })`)
  await trustThisDevice(app.page, LINES_BASE, LINES_INSTANCE)
  await app.page.attachWorkers()
  await app.page.network(FAST_4G)
  await app.page.navigate(`${LINES_BASE}/?${n}#/conversations`)
}

app.scene("a new device's list stands on the skeleton, its words land in place, and a row opens before the sync ends", async () => {
  await app.page.cold(true)
  try {
    await cold("coldlist")
    await app.page.waitFor(`${UNHELD} !== null`, { timeout: STEP_MS })
    const early = await app.page.evaluate<[string, string][]>(ROWS)
    const said = await app.page.evaluate<string>(`${UNHELD}.textContent`)
    assert.match(said, /Not on this device yet/, "a row whose words have not landed says so")
    assert.match(said, /\d+ turns? · \d+ lines? to come/, "and says what the skeleton knows of it")
    await app.page.waitFor(`${UNHELD} === null && document.querySelectorAll(".chatsections .chatrow").length >= ${early.length}`, { timeout: STEP_MS })
    const later = await app.page.evaluate<[string, string][]>(ROWS)
    // A row whose last instant the skeleton's later pieces did not move keeps its order among the others.
    const kept = new Set(later.map(([href, when]) => `${href} ${when}`))
    const stood = (rows: readonly [string, string][]): string[] => rows.flatMap(([href, when]) => (kept.has(`${href} ${when}`) && early.some(([h, w]) => h === href && w === when) ? [href] : []))
    assert.deepEqual(stood(later), stood(early), "every row drawn before its words stands where it stood, but one a later piece moved")

    const first = early[1]?.[0]
    assert.ok(first !== undefined && first !== null, "a conversation is listed")
    const was = await app.page.evaluate<number>(placed)
    await app.page.click(`.chatsections .chatrow[href="${first}"]`)
    await app.page.waitFor(`${placed} > ${was}`, { timeout: STEP_MS })
    const [held, named] = [await app.page.evaluate<number>(KEPT), await app.page.evaluate<number>(NAMED)]
    assert.ok(held < named, `the conversation opened with ${held} of the ${named} objects' content on the device`)
  } finally {
    await app.page.network(null)
    await app.page.cold(false)
  }
})

app.scene("a search typed during the first sync says how much it read, and reads more as content lands", async () => {
  await app.page.cold(true)
  try {
    await cold("coldsearch")
    await app.page.waitFor(`document.querySelector(".chatsearch") !== null && document.querySelector(".chatsections .chatrow") !== null`, { timeout: STEP_MS })
    await app.page.type(".chatsearch", "registre")
    const covered = `(() => {
      const m = /Searched (\\d+) of (?:the )?(\\d+) conversations/.exec(document.querySelector(".chatbody")?.textContent ?? "")
      return m === null ? null : [Number(m[1]), Number(m[2])]
    })()`
    await app.page.waitFor(`${covered} !== null`, { timeout: STEP_MS })
    const [searched, of] = (await app.page.evaluate<[number, number]>(covered))
    const body = await app.page.evaluate<string>(`document.querySelector(".chatbody").textContent`)
    assert.ok(!body.includes("Nothing said matches"), "a search that has not read everything never says nothing matches")
    await app.page.waitFor(`(${covered} ?? [${of}, ${of}])[0] > ${searched}`, { timeout: STEP_MS })
  } finally {
    await app.page.network(null)
    await app.page.cold(false)
  }
})

app.scene("rows whose words a refused answer left waiting fill once the box answers again", async () => {
  await app.page.cold(true)
  try {
    await app.page.refuseRequests(["/api/stores/lines/content"], "the box is not answering")
    await cold("coldrefused")
    await app.page.waitFor(`${UNHELD} !== null`, { timeout: STEP_MS })
    await app.page.resumeRequests()
    await app.page.waitFor(`${UNHELD} === null && document.querySelectorAll(".chatsections .chatrow").length > 1`, { timeout: STEP_MS })
  } finally {
    await app.page.resumeRequests()
    await app.page.network(null)
    await app.page.cold(false)
  }
})
