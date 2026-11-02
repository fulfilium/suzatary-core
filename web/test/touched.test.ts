// A move of the lines names the conversations it touched, as the core answers each change it took: those a line of which
// came or went, those whose lines name a key given or taken, and every conversation when the row was reset or heard a
// later epoch, so a page reading one conversation reads it again only when its own lines or the keys of its words moved.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core } from "../src/prodrome/core"
import linesync from "../src/prodrome/linesync"
import locked from "../src/prodrome/locked"
import { empty, replicaOf } from "../src/prodrome/replica"
import type { SkeletonNode, SkeletonPayload } from "../src/wire/generated/wire"

const core = ((): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
})()

const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as { skeleton: SkeletonPayload; keys: Record<string, string> }
const nodes = box.skeleton.nodes
const of = (id: string): readonly SkeletonNode[] => nodes.filter(n => n.header.node === "line" && n.header.conversation === id)
// A key the lines of `trains` alone name.
const key = ((): string => {
  const named = (id: string): Set<string> => new Set(of(id).flatMap(n => (n.header.node === "line" ? n.header.keys : [])))
  const others = new Set([...named("lagging"), ...named("about-the-deed")])
  const k = [...named("trains")].find(k => !others.has(k))
  assert.ok(k !== undefined)
  return k
})()

const row = (held: readonly SkeletonNode[], keys: locked.Keys = {}, generation = 0): locked.Row<SkeletonNode> =>
  locked.of(replicaOf<SkeletonNode>({ ...empty<SkeletonNode>().held, generation, objects: held, synced: held.map(n => n.hash) }), keys)

test("a move names the conversations whose lines came or went or whose keys moved, and a reset names them all", () => {
  const held = new core.Lines()
  try {
    const said: linesync.Moved[] = []
    const { took } = linesync.telling(held, moved => void said.push(moved))
    const all = row(nodes)
    const lagging = new Set(of("lagging").map(n => n.hash))
    const without = row(nodes.filter(n => !lagging.has(n.hash)))
    for (const next of [without, all, without, row(without.replica.objects, { [key]: box.keys[key]! }), without, row(without.replica.objects, {}, 1)]) took(row([]), next)
    assert.deepEqual(said, [["about-the-deed", "trains"], ["lagging"], ["lagging"], ["trains"], ["trains"], "All"])
    took(without, row(without.replica.objects, {}, 1))
    assert.deepEqual(said.at(-1), [], "a move of the box's word alone touches no conversation")
  } finally {
    held.free()
  }
})
