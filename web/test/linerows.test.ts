// The lines' row on the device, one node per row: a landing writes what the device was last confirmed to lack, so a
// write that failed is made good by the next, whatever a reload finds; and a row is answered with every node its
// frontier names, whichever tab's pull moved the frontier on.

import { test } from "node:test"
import assert from "node:assert/strict"

import { fakeIndexedDB } from "./fakeidb"
import linerows from "../src/prodrome/linerows"
import locked from "../src/prodrome/locked"
import type { Frontier, Word } from "../src/prodrome/replica"
import { empty, replicaOf } from "../src/prodrome/replica"
import type { SkeletonNode } from "../src/wire/generated/wire"

const node = (hash: string): SkeletonNode => ({
  hash: hash.repeat(64).slice(0, 64),
  parents: [],
  size: 1,
  header: { node: "line", conversation: "first", at: "2026-10-25T09:00:00", kind: "Said", keys: [] },
})
const at = (count: number): Frontier => ({ type: "Named", digest: String(count).repeat(64).slice(0, 64), count, era: 0 })
// The box's word of a pull that heard `heads` at `fetchedAt`.
const word = (heads: readonly SkeletonNode[], fetchedAt: number): Word => ({ epoch: 1, heads: heads.map(n => n.hash), untrusted: [], root: null, fetchedAt, geneses: [] })
const row = (nodes: SkeletonNode[], frontier: Frontier, words: readonly Word[] = []): locked.Row<SkeletonNode> =>
  locked.of(replicaOf<SkeletonNode>({ ...empty<SkeletonNode>().held, objects: nodes, synced: nodes.map(n => n.hash), frontier, words }))
const names = (r: locked.Row<SkeletonNode>): string[] => r.replica.objects.map(n => n.hash).sort()
const held = async (place: ReturnType<typeof linerows.placeOf>): Promise<locked.Row<SkeletonNode>> => {
  const r = await place.restore()
  assert.ok(r.row.type === "Held", `the device holds the lines' row (${r.row.type})`)
  return r.row.row
}

test("a landing whose write failed is written by the next, and a reload holds every line", async () => {
  const device = fakeIndexedDB()
  const tab = linerows.placeOf()
  await tab.restore()
  const [a, b, c] = [node("a"), node("b"), node("c")]
  assert.equal((await tab.land(row([a], at(1)))).row.type, "Val")
  device.failing = k => k.startsWith("node/")
  assert.equal((await tab.land(row([a, b], at(2)))).row.type, "Error", "the device refused the write")
  device.failing = () => false
  assert.equal((await tab.land(row([a, b, c], at(3)))).row.type, "Val")
  const reloaded = await held(linerows.placeOf())
  assert.deepEqual(names(reloaded), [a, b, c].map(n => n.hash).sort(), "the node whose write failed is on the device")
})

test("a tab landing behind another tab's frontier answers that tab's nodes too, so the frontier names what it holds", async () => {
  fakeIndexedDB()
  const [first, second] = [linerows.placeOf(), linerows.placeOf()]
  await Promise.all([first.restore(), second.restore()])
  const [a, b] = [node("a"), node("b")]
  assert.equal((await second.land(row([b], at(5)))).row.type, "Val")
  const landed = await first.land(row([a], at(2)))
  assert.ok(landed.row.type === "Val")
  assert.deepEqual(names(landed.row.v), [a, b].map(n => n.hash).sort(), "the first tab now holds the second's node")
  assert.deepEqual(landed.row.v.replica.held.frontier, at(5), "at the frontier that names it")
  const reloaded = await held(linerows.placeOf())
  assert.deepEqual(names(reloaded), [a, b].map(n => n.hash).sort())
  assert.deepEqual(reloaded.replica.held.frontier, at(5))
})

test("what a tab behind another lands is the join of both rows, the box's word and frontier included, never its own over the later", async () => {
  fakeIndexedDB()
  const [first, second] = [linerows.placeOf(), linerows.placeOf()]
  await Promise.all([first.restore(), second.restore()])
  const [a, b] = [node("a"), node("b")]
  const later = row([a, b], at(5), [word([b], 20)])
  const behind = row([a], at(2), [word([a], 10)])
  assert.equal((await second.land(later)).row.type, "Val")
  const landed = await first.land(behind)
  const joined = locked.join(later, behind)
  assert.ok(landed.row.type === "Val")
  assert.deepEqual(landed.row.v.replica.held, joined.replica.held, "the landing answers the join")
  const reloaded = await held(linerows.placeOf())
  assert.deepEqual(reloaded.replica.held.words, joined.replica.held.words, "the device holds the later word")
  assert.deepEqual(reloaded.replica.held.frontier, joined.replica.held.frontier)
})
