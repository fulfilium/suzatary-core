/**
 * A DEVICE HOLDING A LINE THE BOX NO LONGER HOLDS: the box answers the lines in a later epoch, and a tab that kept a
 * line the box replaced, or a piece of a pull from before, drops its copy and takes the store again whole, so no
 * replaced line stays a head. Its own server and its own file, so the reset it makes reaches no other scene's origin or
 * state.
 */

import assert from "node:assert/strict"
import { createHash } from "node:crypto"

import type { Landed } from "./harness"
import { LANDED_ROW, PHONE, suite, trustThisDevice, withCore, opened } from "./harness"

const app = await suite("epoch")

const required = (name: string): string => {
  const value = process.env[name]
  if (value === undefined || value === "") throw new Error(`${name} is unset — scripts/web-e2e.sh starts the epoch's server`)
  return value
}
const EPOCH_BASE = required("E2E_BASE_EPOCH")
const EPOCH_INSTANCE = required("E2E_EPOCH_INSTANCE")


async function converse(text: string): Promise<string> {
  await app.page.navigate(`${EPOCH_BASE}/#/secretary/default`)
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 })
  const id = await app.page.evaluate<string>(
    opened("Untitled"),
  )
  await app.page.navigate(`${EPOCH_BASE}/#/secretary/${id}`)
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 })
  await app.page.type(".composebox", text)
  await app.page.click(".composesend")
  return id
}

app.scene("a device holding a line the box rewrote drops its copy at the later epoch and holds only the box's lines", async () => {
  await app.resize(PHONE)
  await trustThisDevice(app.page, EPOCH_BASE, EPOCH_INSTANCE)
  await app.open(EPOCH_BASE)
  // The owner's own turn is enough to hold a line and its key: the scene is about the epoch, not the model's answer.
  const id = await converse("give me a rendered answer")
  await app.page.waitFor(`document.querySelector(".turn-person") !== null`, { timeout: 20000 })
  const box = await app.page.evaluate<{ epoch: number; nodes: { hash: string; header: { node: string; keys?: string[] } }[]; keys: Record<string, string>; rest: number }>(`fetch("/api/stores/lines/skeleton").then((r) => r.json())`)
  assert.equal(box.rest, 0, "the fixture's skeleton is one piece")
  assert.ok(box.epoch > 1, "the box answers in a later epoch than the first")
  // What a device that heard the box before a migration holds: the line the box rewrote, a head beside its image as the
  // box now holds it, at the epoch before; and a piece of a pull from before it, kept and not landed.
  const tip = [...box.nodes].reverse().find((o) => o.header.node === "line" && (o.header.keys ?? []).length > 0)!
  const old = { ...tip, hash: createHash("sha256").update(`${tip.hash} rewritten`, "utf8").digest("hex") }
  assert.ok(!box.nodes.some((o) => o.hash === old.hash), "the rewritten line is no object the box holds")
  // The device is written from a page of the same origin that runs no app: an app tab still open would sync over it.
  await app.page.navigate(`${EPOCH_BASE}/api/about`)
  await app.page.waitFor(`document.readyState === "complete"`)
  const before = { ...box, epoch: 1 }
  await app.page.evaluate(
    withCore(`
      const lines = new core.Lines()
      const before = ${JSON.stringify(before)}
      lines.piece(JSON.stringify({ ...before, nodes: [...before.nodes, ${JSON.stringify(old)}] }), JSON.stringify(before.keys), 0)
      const landed = lines.land()
      const frames = [lines.piece(JSON.stringify({ ...before, nodes: [${JSON.stringify(old)}], keys: {} }), "{}", 0)]
      frames.unshift(landed.delta)
      landed.free()
      lines.free()
      await new Promise((done, fail) => {
        const r = indexedDB.open("suzatary")
        r.onerror = () => fail(String(r.error))
        r.onsuccess = () => {
          const tx = r.result.transaction("replica", "readwrite")
          const s = tx.objectStore("replica")
          s.clear()
          frames.forEach((f, i) => s.put(f, "lines/log/" + String(i + 1).padStart(12, "0")))
          tx.oncomplete = () => (r.result.close(), done(true))
          tx.onerror = () => fail(String(tx.error))
        }
      })
    `),
  )
  const was = await app.page.evaluate<Landed>(LANDED_ROW)
  assert.equal(was.epoch, 1, "the device holds the epoch before")
  assert.ok(was.nodes.includes(old.hash), "and the line the box rewrote")
  await app.page.navigate(`${EPOCH_BASE}/?overtaken=1#/secretary/${id}`)
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 })
  await app.page.waitFor(`${LANDED_ROW}.then((row) => row.epoch > 1 && row.nodes.length > 0 && !row.nodes.includes("${old.hash}"))`, { timeout: 20000 })
  const after = await app.page.evaluate<Landed>(LANDED_ROW)
  // The round goes on meanwhile (its claim is a line of the box's), so the device is read against the box's lines as they
  // stand after it landed: every line it held then, and none the box does not hold.
  const now = await app.page.evaluate<{ nodes: { hash: string }[] }>(`fetch("/api/stores/lines/skeleton").then((r) => r.json())`)
  const held = new Set(now.nodes.map((o) => o.hash))
  assert.deepEqual(after.nodes.filter((n) => !held.has(n)), [], "the device holds the box's lines and nothing else")
  assert.deepEqual(box.nodes.map((o) => o.hash).filter((n) => !after.nodes.includes(n)), [], "every line the box held before")
  assert.ok(after.generation > was.generation, "in the generation after, so no copy from before reaches it")
  assert.ok(after.keys.length > 0, "the keys taken again with the lines")
  await app.page.waitFor(`document.querySelector(".turn-person") !== null`, { timeout: 20000 })
})
