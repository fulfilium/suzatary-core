// A device finds its dead keys in the skeleton it holds, by the box's rule: in every way a key dies (an erasure, a
// migration, two migrations seen together or one at a time, an erasure of a slot sealed again), the keys the nodes
// retire, given the keys the device holds, leave it the box's keys and no other. The vector is the box's own
// (`app/core/tests/web_vectors.rs`, `the_retired_vector_is_every_way_a_key_dies`).

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import linesync from "../src/prodrome/linesync"
import type { SkeletonNode } from "../src/wire/generated/wire"

type Case = { readonly name: string; readonly nodes: readonly SkeletonNode[]; readonly device: readonly string[]; readonly box: readonly string[] }
const cases = JSON.parse(readFileSync("app/core/tests/web/retired.json", "utf8")) as readonly Case[]

test("law: the keys the held nodes retire leave a device the box's keys", () => {
  assert.ok(cases.length > 0)
  for (const c of cases) {
    const held = new Set(c.device)
    const dead = linesync.retiredBy(c.nodes, k => held.has(k))
    assert.deepEqual(c.device.filter(k => !dead.has(k)), c.box, c.name)
  }
})


test("a key given before the row started over supersedes nothing: the box restored without it keeps the first seals", () => {
  const c = cases.find(c => c.name.startsWith("the box restored"))
  assert.ok(c !== undefined)
  // The reseals' own keys, which the device held before it started over and the restored box no longer holds.
  const before = c.nodes.flatMap(n => (n.header.node === "line" && n.header.step.step === "resealed" ? [n.header.keys[0]!] : []))
  assert.ok(before.length > 0 && before.every(k => !c.device.includes(k)))
  const row = new Set(c.device)
  assert.deepEqual(c.device.filter(k => !linesync.retiredBy(c.nodes, k => row.has(k)).has(k)), c.box, "read from the row, the first seals stay")
  const stale = new Set([...c.device, ...before])
  assert.ok(c.box.some(k => linesync.retiredBy(c.nodes, k => stale.has(k)).has(k)), "read from a list that outlived the row, a live key would go")
})
