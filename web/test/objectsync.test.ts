// The carry of a store's rows a build before version 9 kept in the rows' store: a row that reads is joined whole into
// the store's own rows, a tab's unsent object included, and the old rows go; a row that does not read is kept, and the
// restore says so.

import { test } from "node:test"
import assert from "node:assert/strict"

import { fakeIndexedDB } from "./fakeidb"
import idb from "../src/app/idb"
import excluded from "../src/prodrome/excluded"
import { CHAIN, TOPIC_ROWS, heldNames, holdRow } from "../src/prodrome/objects"
import objectsync from "../src/prodrome/objectsync"
import { NOTHING, replicaOf, unsynced } from "../src/prodrome/replica"

const taken = { hash: "a".repeat(64), literal: "A" }
const own = { hash: "b".repeat(64), literal: "B" }

const store = () => {
  const told: [string, excluded.Left][] = []
  const e = objectsync.of("topics", TOPIC_ROWS, { told: (reader, left) => told.push([reader, left]), bars: () => {}, moved: () => {}, replaced: () => {} })
  return { e, told }
}

test("a row a build before this one kept is carried whole, the tab's unsent object with it, and the old rows go", async () => {
  fakeIndexedDB()
  const held = replicaOf({ ...NOTHING.held, objects: [taken, own], synced: [taken.hash] })
  assert.equal((await holdRow(CHAIN, "pages/topics", { type: "Join", replica: held })).type, "Val")
  const { e } = store()
  await e.sync.restored.get()
  assert.deepEqual(unsynced(e.row().replica), [own], "the unsent object is still to send")
  assert.deepEqual(await heldNames("pages/topics"), { type: "Val", v: [] })
})

test("a row that does not read is kept, and the restore says so", async () => {
  fakeIndexedDB()
  await idb.within({ ...TOPIC_ROWS, store: "chain" }, "readwrite", s => idb.done(s.put({ objects: "not a list" }, "objects:pages/topics")))
  const { e, told } = store()
  await e.sync.restored.get()
  const said = told.filter(([reader]) => reader.startsWith("carry/")).map(([, left]) => JSON.stringify(left))
  assert.ok(said.some(s => s.includes("does not read")), said.join("\n"))
  assert.deepEqual(await heldNames("pages/topics"), { type: "Val", v: ["objects:pages/topics"] }, "the row is kept")
})

test("a device that kept no such row reads nothing more and deletes nothing", async () => {
  const device = fakeIndexedDB()
  const { e } = store()
  await e.sync.restored.get()
  assert.equal(device.deletes(), 0, "no delete was asked")
})
