// A store's row on the device, one node per row: a landing writes what the device was last confirmed to lack, so a
// write that failed is made good by the next, whatever a reload finds; and a row is answered with every node its
// frontier names, whichever tab's pull moved the frontier on; and what the box has not taken reads back as unsent.

import { test } from "node:test"
import assert from "node:assert/strict"

import { fakeIndexedDB } from "./fakeidb"
import idb from "../src/app/idb"
import lo from "../src/algebra/lo"
import linesync from "../src/prodrome/linesync"
import noderows from "../src/prodrome/noderows"
import { CONTENT, DB_NAME, INBOX_ROWS, LINE_ROWS } from "../src/prodrome/objects"
import payload from "../src/wire/generated/payload"
import locked from "../src/prodrome/locked"
import type { Frontier, Word } from "../src/prodrome/replica"
import { empty, replicaOf, unsynced } from "../src/prodrome/replica"
import type { LineStep, SkeletonNode, StoredObject } from "../src/wire/generated/wire"

// The rows of the lines and of the inbox in the form `version`, saying when rows of another form are dropped.
const KEEP = (): Promise<lo.Settled<null>> => Promise.resolve(lo.val(null))
const lineRows = (version = 1, dropped = KEEP): noderows.Place<SkeletonNode> => noderows.placeOf(LINE_ROWS, payload.SkeletonNode, { version, dropped })
const inboxRows = (): noderows.Place<StoredObject> => noderows.placeOf(INBOX_ROWS, payload.StoredObject, { version: 1, dropped: KEEP })

const node = (hash: string): SkeletonNode => ({
  hash: hash.repeat(64).slice(0, 64),
  parents: [],
  size: 1,
  header: { node: "line", conversation: "first", at: "2026-10-25T09:00:00", kind: "Said", keys: [], step: { step: "asks" } },
})
const at = (count: number): Frontier => ({ type: "Named", digest: String(count).repeat(64).slice(0, 64), count, era: 0 })
// The box's word of a pull that heard `heads` at `fetchedAt`.
const word = (heads: readonly SkeletonNode[], fetchedAt: number): Word => ({ epoch: 1, heads: heads.map(n => n.hash), untrusted: [], root: null, fetchedAt, geneses: [] })
const row = (nodes: SkeletonNode[], frontier: Frontier, words: readonly Word[] = []): locked.Row<SkeletonNode> =>
  locked.of(replicaOf<SkeletonNode>({ ...empty<SkeletonNode>().held, objects: nodes, synced: nodes.map(n => n.hash), frontier, words }))
const names = (r: locked.Row<SkeletonNode>): string[] => r.replica.objects.map(n => n.hash).sort()
const held = async (place: noderows.Place<SkeletonNode>): Promise<locked.Row<SkeletonNode>> => {
  const r = await place.restore()
  assert.ok(r.row.type === "Held", `the device holds the lines' row (${r.row.type})`)
  return r.row.row
}

test("a landing whose write failed is written by the next, and a reload holds every line", async () => {
  const device = fakeIndexedDB()
  const tab = lineRows()
  await tab.restore()
  const [a, b, c] = [node("a"), node("b"), node("c")]
  assert.equal((await tab.land(row([a], at(1)))).row.type, "Val")
  device.failing = k => k.startsWith("node/")
  assert.equal((await tab.land(row([a, b], at(2)))).row.type, "Error", "the device refused the write")
  device.failing = () => false
  assert.equal((await tab.land(row([a, b, c], at(3)))).row.type, "Val")
  const reloaded = await held(lineRows())
  assert.deepEqual(names(reloaded), [a, b, c].map(n => n.hash).sort(), "the node whose write failed is on the device")
})

test("a tab landing behind another tab's frontier answers that tab's nodes too, so the frontier names what it holds", async () => {
  fakeIndexedDB()
  const [first, second] = [lineRows(), lineRows()]
  await Promise.all([first.restore(), second.restore()])
  const [a, b] = [node("a"), node("b")]
  assert.equal((await second.land(row([b], at(5)))).row.type, "Val")
  const landed = await first.land(row([a], at(2)))
  assert.ok(landed.row.type === "Val")
  assert.deepEqual(names(landed.row.v), [a, b].map(n => n.hash).sort(), "the first tab now holds the second's node")
  assert.deepEqual(landed.row.v.replica.held.frontier, at(5), "at the frontier that names it")
  const reloaded = await held(lineRows())
  assert.deepEqual(names(reloaded), [a, b].map(n => n.hash).sort())
  assert.deepEqual(reloaded.replica.held.frontier, at(5))
})

test("what a tab behind another lands is the join of both rows, the box's word and frontier included, never its own over the later", async () => {
  fakeIndexedDB()
  const [first, second] = [lineRows(), lineRows()]
  await Promise.all([first.restore(), second.restore()])
  const [a, b] = [node("a"), node("b")]
  const later = row([a, b], at(5), [word([b], 20)])
  const behind = row([a], at(2), [word([a], 10)])
  assert.equal((await second.land(later)).row.type, "Val")
  const landed = await first.land(behind)
  const joined = locked.join(later, behind)
  assert.ok(landed.row.type === "Val")
  assert.deepEqual(landed.row.v.replica.held, joined.replica.held, "the landing answers the join")
  const reloaded = await held(lineRows())
  assert.deepEqual(reloaded.replica.held.words, joined.replica.held.words, "the device holds the later word")
  assert.deepEqual(reloaded.replica.held.frontier, joined.replica.held.frontier)
})

test("a row holding objects the box has not taken reads back with them unsent, and a landing that syncs them says so", async () => {
  fakeIndexedDB()
  const place = inboxRows()
  await place.restore()
  const [taken, own] = [{ hash: "a".repeat(64), literal: "A" }, { hash: "b".repeat(64), literal: "B" }]
  const mine = locked.of(replicaOf<StoredObject>({ ...empty<StoredObject>().held, objects: [taken, own], synced: [taken.hash] }))
  assert.equal((await place.land(mine)).row.type, "Val")
  const read = await inboxRows().restore()
  assert.ok(read.row.type === "Held")
  assert.deepEqual(unsynced(read.row.row.replica), [own], "the tab's own object is still to send")
  const sent = locked.of(replicaOf<StoredObject>({ ...mine.replica.held, synced: [taken.hash, own.hash] }))
  assert.equal((await place.land(sent)).row.type, "Val")
  const again = await inboxRows().restore()
  assert.ok(again.row.type === "Held")
  assert.deepEqual(unsynced(again.row.row.replica), [], "the box's receipt is kept")
})

test("a head that does not read is said and never written over by a landing, and its reset writes over it", async () => {
  const device = fakeIndexedDB()
  const place = lineRows()
  assert.equal((await place.land(row([node("a")], at(1)))).row.type, "Val")
  await idb.within(LINE_ROWS, "readwrite", s => idb.done(s.put({ objects: "not a list" }, "head")))
  const read = await lineRows().restore()
  assert.ok(read.unread.some(u => u.includes("does not read")), JSON.stringify(read.unread))
  const landed = await place.land(row([node("a"), node("b")], at(2)))
  assert.equal(landed.row.type, "Error")
  assert.ok(device.printed().some(r => r.includes('"not a list"')), "the copy is as it was")
  assert.equal((await place.reset(locked.none())).type, "Val")
  assert.equal((await place.land(row([node("b")], at(1)))).row.type, "Val")
})

test("a version 8 lines' head reads after version 9's upgrade, with its nodes and keys", async () => {
  fakeIndexedDB()
  const v8: idb.Db = { name: DB_NAME, version: 8, store: "lines", upgrade: db => ["chain", "content", "lines"].forEach(n => db.createObjectStore(n)) }
  const a = node("a")
  const head = { generation: 2, words: [word([a], 10)], refused: [], frontier: at(3) }
  await idb.within(v8, "readwrite", s => Promise.all([idb.done(s.put(head, "head")), idb.done(s.put(a, `node/${a.hash}`)), idb.done(s.put("00", "key/k"))]))
  // A load of this build: the upgrade runs as the first store is opened, here the content's.
  idb.reset()
  await idb.within(CONTENT, "readonly", s => idb.done(s.getAllKeys()))
  const read = await lineRows().restore()
  assert.deepEqual(read.unread, [])
  assert.ok(read.row.type === "Held", `the lines' row reads (${read.row.type})`)
  const row = read.row.row
  assert.deepEqual(names(row), [a.hash])
  assert.deepEqual(row.keys, { k: "00" })
  assert.equal(row.replica.generation, 2)
  assert.deepEqual(row.replica.held.frontier, at(3))
  assert.deepEqual(row.replica.held.words, head.words)
  assert.deepEqual(row.replica.synced, [a.hash], "every node is the box's")
})

test("rows of another format are dropped at restore, what they found first, and taken again; the content by name stays", async () => {
  const device = fakeIndexedDB()
  const old = lineRows(1)
  assert.equal((await old.land(locked.of(row([node("a")], at(1)).replica, { k: "00" }))).row.type, "Val")
  await idb.within(CONTENT, "readwrite", s => idb.done(s.put("A", "a".repeat(64))))
  let dropped = 0
  let heldWhileDropped = false
  const read = await lineRows(2, () => {
    dropped += 1
    heldWhileDropped = device.printed().some(r => r.startsWith(`suzatary/lines/node/${node("a").hash}`))
    return KEEP()
  }).restore()
  assert.equal(dropped, 1, "what the old rows found is dropped first")
  assert.ok(heldWhileDropped, "while the old rows are still on the device")
  assert.ok(read.row.type === "Held", read.row.type)
  assert.deepEqual(names(read.row.row), [], "the old nodes are gone")
  assert.deepEqual(read.row.row.keys, {}, "and their keys")
  assert.equal(read.row.row.replica.generation, 1, "a row of the generation after, so no copy from before reaches it")
  assert.equal(read.row.row.replica.box.type, "Unheard", "taken again from the box")
  assert.ok(device.printed().some(r => r.startsWith("suzatary/content/") && r.includes('"A"')), "the content by name stays")
  const again = await lineRows(2).restore()
  assert.ok(again.row.type === "Held" && again.row.row.replica.generation === 1, "a restore in the same form keeps it")
})

// Form 1's nodes read a reseal, an erasure and a title as `still`, so a round would read an old seal and skip an erasure:
// the lines' rows of form 1 are reset under this build's form, and the nodes it lands name what a round reads.
test("the lines' rows of form 1 are reset under linesync.FORMAT, whose nodes name reseals, erasures and titles", async () => {
  fakeIndexedDB()
  assert.ok(linesync.FORMAT > 1, `the lines moved past form 1 (${linesync.FORMAT})`)
  const line = (hash: string, kind: string, keys: readonly string[], step: LineStep): SkeletonNode => ({
    ...node(hash),
    header: { node: "line", conversation: "first", at: "2026-10-25T09:00:00", kind, keys: [...keys], step },
  })
  assert.equal((await lineRows(1).land(locked.of(row([line("a", "Resealed", [], { step: "still" })], at(1)).replica, { k: "00" }))).row.type, "Val")
  const now = lineRows(linesync.FORMAT)
  const read = await now.restore()
  assert.ok(read.row.type === "Held", read.row.type)
  assert.deepEqual(names(read.row.row), [], "the form 1 nodes are gone, and the skeleton is taken again")
  const k = "b".repeat(64)
  const named = [
    line("b", "Resealed", [k], { step: "resealed", of: "c".repeat(64), from: "c".repeat(64) }),
    line("c", "Erased", [], { step: "erased", keys: [k] }),
    line("d", "Titled", [], { step: "titled" }),
  ]
  // Landed in the generation the reset began, as the next pull lands it.
  const after = replicaOf<SkeletonNode>({ ...read.row.row.replica.held, objects: named, synced: named.map(n => n.hash), frontier: at(2) })
  assert.equal((await now.land(locked.of(after, {}))).row.type, "Val")
  const steps = (nodes: readonly SkeletonNode[]): unknown[] => nodes.map(n => (n.header.node === "line" ? n.header.step : null))
  assert.deepEqual(steps((await held(now)).replica.objects), steps(named), "each step reads back as the box sent it")
})

// Form 2's reseals name their slot and not the seal they were sealed again from, so a device would keep that seal's key
// for good: the lines' rows of form 2 are reset as well, and taken again with each reseal's `from`.
test("the lines' rows of form 2 are reset under linesync.FORMAT, whose reseals name the seal they were sealed from", async () => {
  fakeIndexedDB()
  assert.ok(linesync.FORMAT > 2, `the lines moved past form 2 (${linesync.FORMAT})`)
  const reseal = { ...node("a"), header: { node: "line", conversation: "first", at: "2026-10-25T09:00:00", kind: "Resealed", keys: ["b".repeat(64)], step: { step: "resealed", of: "c".repeat(64) } } } as unknown as SkeletonNode
  assert.equal((await lineRows(2).land(locked.of(row([reseal], at(1)).replica, { k: "00" }))).row.type, "Val")
  const read = await lineRows(linesync.FORMAT).restore()
  assert.ok(read.row.type === "Held", read.row.type)
  assert.deepEqual(names(read.row.row), [], "the form 2 nodes are gone, and the skeleton is taken again")
  assert.deepEqual(read.row.row.keys, {}, "and their keys")
})

test("rows of another format whose findings could not be dropped are kept and said", async () => {
  fakeIndexedDB()
  assert.equal((await lineRows(1).land(row([node("a")], at(1)))).row.type, "Val")
  const read = await lineRows(2, () => Promise.resolve(lo.error("the blocks did not clear"))).restore()
  assert.ok(read.row.type === "Unreadable" && read.row.why.includes("the blocks did not clear"), JSON.stringify(read.row))
  assert.ok((await lineRows(1).restore()).row.type === "Held", "the old rows are still there")
})

test("a landing never writes this build's format over rows of another, so the next restore drops them", async () => {
  fakeIndexedDB()
  assert.equal((await lineRows(1).land(row([node("a")], at(1)))).row.type, "Val")
  const fails = (): Promise<lo.Settled<null>> => Promise.resolve(lo.error("the blocks did not clear"))
  const place = lineRows(2, fails)
  assert.equal((await place.restore()).row.type, "Unreadable")
  const landed = await place.land(row([node("b")], at(2)))
  assert.equal(landed.row.type, "Error", "the landing writes nothing")
  assert.ok(landed.unread?.includes("format 1"), String(landed.unread))
  let dropped = 0
  const again = await lineRows(2, () => ((dropped += 1), KEEP())).restore()
  assert.equal(dropped, 1, "the next restore drops them")
  assert.ok(again.row.type === "Held" && names(again.row.row).length === 0)
})

test("a reformat whose write fails keeps the old rows, and the next restore drops and resets them", async () => {
  const device = fakeIndexedDB()
  assert.equal((await lineRows(1).land(row([node("a")], at(1)))).row.type, "Val")
  device.failing = k => k === "head"
  assert.equal((await lineRows(2).restore()).row.type, "Unreadable")
  device.failing = () => false
  assert.ok((await lineRows(1).restore()).row.type === "Held", "the old rows are as they were")
  let dropped = 0
  const again = await lineRows(2, () => ((dropped += 1), KEEP())).restore()
  assert.equal(dropped, 1)
  assert.ok(again.row.type === "Held" && names(again.row.row).length === 0 && again.row.row.replica.generation === 1)
})

test("a reformat keeps what this tab wrote and the box has not taken, or keeps every old row when one does not read", async () => {
  fakeIndexedDB()
  const taken = { hash: "a".repeat(64), literal: "A" }
  const own = { hash: "b".repeat(64), literal: "B" }
  const inbox = (version: number): noderows.Place<StoredObject> => noderows.placeOf(INBOX_ROWS, payload.StoredObject, { version, dropped: KEEP })
  const mine = locked.of(replicaOf<StoredObject>({ ...empty<StoredObject>().held, objects: [taken, own], synced: [taken.hash] }))
  assert.equal((await inbox(1).land(mine)).row.type, "Val")
  const read = await inbox(2).restore()
  assert.ok(read.row.type === "Held", read.row.type)
  assert.deepEqual(read.row.row.replica.objects, [own], "the box's object goes, this tab's stays")
  assert.deepEqual(unsynced(read.row.row.replica), [own], "still to send")

  fakeIndexedDB()
  assert.equal((await inbox(1).land(mine)).row.type, "Val")
  await idb.within(INBOX_ROWS, "readwrite", s => idb.done(s.put({ hash: own.hash, literal: 7 }, `node/${own.hash}`)))
  const refused = await inbox(2).restore()
  assert.ok(refused.row.type === "Unreadable" && refused.row.why.includes("do not read"), JSON.stringify(refused.row))
  assert.ok((await inbox(1).restore()).row.type === "Held", "the old rows are kept")
})
