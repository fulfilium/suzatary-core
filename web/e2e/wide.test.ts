/**
 * The wide server's lines are 400 conversations of one line each, so a tab holds 400 heads, which named one by one in
 * a pull's URL are refused with 431; after a reload every pull asks from one frontier, in under a kilobyte, and is
 * answered. The list read is the conversations' tile a conversation's address opens at a desktop's width, since those
 * conversations hold no round for the stream to draw.
 */

import assert from "node:assert/strict"

import { DESKTOP, suite, trustThisDevice } from "./harness"

const required = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-e2e.sh`)
  return v
}

const BASE = required("E2E_BASE_WIDE")
const INSTANCE = required("E2E_WIDE_INSTANCE")

const app = await suite("wide", DESKTOP)

const drawn = `document.querySelectorAll('.conversations .chatrow[href*="wide-"]').length > 0`

app.scene("a tab holding 400 heads pulls in a request under a kilobyte, and the box answers it", async () => {
  await trustThisDevice(app.page, BASE, INSTANCE)
  const pulls = await app.page.statuses("/api/stores/")
  await app.page.navigate(`${BASE}/#/secretary/wide-0`)
  await app.page.waitFor(drawn, { timeout: 60000 })
  const first = pulls().filter(r => r.url.includes("/skeleton"))
  assert.ok(first.length > 0, "the list pulled the lines' skeleton")

  const again = await app.page.statuses("/api/stores/")
  await app.page.reload()
  await app.page.waitFor(drawn, { timeout: 60000 })
  // The core's worker pulls, so its request is in no entry of the page's: it is waited for as the browser reports it.
  for (const until = Date.now() + 60000; !again().some(r => r.url.includes("/api/stores/lines/skeleton?since=")); ) {
    if (Date.now() > until) throw new Error(`no pull after the reload asked from a frontier: ${JSON.stringify(again())}`)
    await new Promise(done => setTimeout(done, 100))
  }
  const seen = again()
  const skeletons = seen.filter(r => r.url.includes("/skeleton"))
  assert.ok(skeletons.some(r => /since=[0-9a-f]{64}/.test(r.url)), `the second pull asks from a frontier: ${JSON.stringify(skeletons)}`)
  for (const r of [...first, ...seen]) {
    assert.ok(r.status < 400, `${r.url.slice(0, 120)} was answered ${r.status}`)
    assert.ok(new URL(r.url).pathname.length + new URL(r.url).search.length < 1024 || r.url.includes("/content?hashes="), `${r.url.length} bytes asked`)
  }
  assert.ok(skeletons.every(r => /since=[0-9a-f]{64}/.test(r.url)), `no pull after the reload asked again from nothing: ${skeletons.map(r => r.url).join(" ")}`)
})
