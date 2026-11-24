// A register in conflict, folded by the core in this process: every candidate reaches the page,
// priced as its most urgent world, the numbers `app/io-chains/tests/fixtures/legacy/readings.txt` pins.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readdirSync, readFileSync } from "node:fs"

import type { Core } from "../src/prodrome/core"
import { entriesOf, foldObjects, verifyObjects } from "../src/prodrome/core"
import register from "../src/algebra/register"
import { describeConflicts } from "../src/todo/conflicts"

const core = (): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
}
const wasm = core()

const dir = "app/io-chains/tests/fixtures/legacy/conflicted/objects"
const links = readdirSync(dir).map(f => ({ hash: f.slice(0, -".py".length), literal: readFileSync(`${dir}/${f}`, "utf8") }))
const NOW = "2026-09-08T10:00:00"
const rows = entriesOf(wasm, links, NOW, ["triage"], "life", new wasm.Chains()).entries
const row = (todo: string) => {
  const r = rows.find(e => e.todo === todo)
  assert.ok(r !== undefined, todo)
  return r
}

test("a content conflict is every candidate, and the price is the most urgent", () => {
  const plants = row("water-plants")
  assert.deepEqual(plants.candidates.map(c => c.body).sort(), ["Water the cactus", "Water the ferns"])
  assert.equal(plants.authored?.body, plants.candidates[0]?.body)
  assert.equal(plants.value, 0.3)
  assert.deepEqual(Object.keys(plants.conflicts).sort(), ["content", "spec"])
})

test("a state conflict is every candidate, filed open while one is", () => {
  const card = row("renew-card")
  assert.deepEqual(card.states, ["open", "completed"])
  assert.equal(card.state, "open")
  assert.equal(card.value, 0.5)
  assert.deepEqual(card.candidates, [])
})

test("the fold's history reads the state register, a conflict as its candidates", () => {
  const steps = foldObjects(wasm, links, NOW, ["triage"]).history.bindings["renew-card"] ?? []
  assert.deepEqual(steps.at(-1)?.binding, [null, { kind: "Completed", at: "2026-09-07T09:00:00" }])
})

test("the store's one root is its one genesis", () => {
  assert.equal(verifyObjects(wasm, links).genesis.length, 1)
})

test("the one reading of a row's candidate values agrees with the store's own conflicts, register by register", () => {
  // Two folds of one history: the core's frontier width per register (`Entry.conflicts`), and the reading the tab
  // shares (`register.of`) over the values the row carries, the state register's and the content register's.
  for (const r of rows) {
    const kinds = new Set(describeConflicts(r).map(c => c.kind))
    assert.equal(register.of(r.states).type === "Conflict", kinds.has("state"), `${r.todo} state`)
    assert.equal(register.of(r.candidates.map(c => JSON.stringify(c))).type === "Conflict", kinds.has("content"), `${r.todo} content`)
  }
  assert.ok(rows.some(r => register.of(r.states).type === "Conflict"), "the fixture holds a state conflict")
  assert.ok(rows.some(r => register.of(r.candidates.map(c => JSON.stringify(c))).type === "Conflict"), "and a content conflict")
})

test("a spec register reads as its frontier: one candidate as [x], a conflict as every candidate", () => {
  const specs = foldObjects(wasm, links, NOW, ["triage"]).specs
  assert.equal(specs["water-plants"]?.length, 2, "the conflict")
  assert.equal(specs["renew-card"]?.length, 1, "one spec, read as a frontier of one")
  assert.ok(Object.values(specs).every(Array.isArray))
})
