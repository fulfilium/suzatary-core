// A move of the lines names the conversations it touched, so a page reading one conversation reads it again only when
// its own lines or the keys of its words moved, and every conversation when the row was reset.

import { test } from "node:test"
import assert from "node:assert/strict"

import linesync from "../src/prodrome/linesync"
import locked from "../src/prodrome/locked"
import { empty, replicaOf } from "../src/prodrome/replica"
import type { SkeletonNode } from "../src/wire/generated/wire"

const line = (hash: string, conversation: string, keys: string[] = []): SkeletonNode => ({
  hash,
  parents: [],
  size: 1,
  header: { node: "line", conversation, at: "2026-10-25T09:00:00", kind: "Said", keys },
})

const row = (nodes: SkeletonNode[], keys: Record<string, string> = {}, generation = 0): locked.Row<SkeletonNode> =>
  locked.of(replicaOf<SkeletonNode>({ ...empty<SkeletonNode>().held, generation, objects: nodes, synced: nodes.map(n => n.hash) }), keys)

test("a move names the conversations whose lines came or went or whose keys moved, and a reset names them all", () => {
  const was = row([line("a", "first", ["k1"]), line("b", "second", ["k2"])])
  assert.deepEqual(linesync.touched(was, row([...was.replica.objects, line("c", "third")]), new Set()), ["third"])
  assert.deepEqual(linesync.touched(was, row([line("a", "first", ["k1"])]), new Set()), ["second"])
  assert.deepEqual(linesync.touched(was, row(was.replica.objects, { k2: "00" }), new Set(["k2"])), ["second"])
  assert.deepEqual(linesync.touched(was, was, new Set()), [], "a move of the box's word alone touches no conversation")
  assert.equal(linesync.touched(was, row(was.replica.objects, {}, 1), new Set()), "All")
})
