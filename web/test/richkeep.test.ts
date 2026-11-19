// richkeep over the Rust's fold vector: what the box rendered is kept, and only a confirmed row reads it back.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"

import d from "../src/wire/decoder"
import wire from "../src/wire/generated/payload"
import type { Entry } from "../src/wire/generated/wire"
import rendered from "../src/app/rendered"
import table from "../src/algebra/table"
import richkeep from "../src/app/richkeep"
import type { Core } from "../src/prodrome/core"
import { functionsOf } from "../src/prodrome/core"

const core = ((): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
})()

const decoded = d.json(wire.FoldPayload)(readFileSync("app/api/tests/web/fold.json", "utf8"))
if (decoded.type !== "Val") throw new Error("fold.json does not decode")
const entries = decoded.v.entries

const memory = (): rendered.Store & { rows: table.Table<string> } => {
  const rows = table.of<string>(table.keep())
  return { ...rendered.memory(functionsOf(core), rows), rows }
}

const unrendered = (es: readonly Entry[]): Entry[] =>
  es.map(e => (e.authored === null ? e : { ...e, authored: { ...e.authored, rich: { ...e.authored.rich, body: null, detail: null, rationale: null, waiting_on: null } } }))

test("a fold the box rendered, kept and read back into the same fold unrendered, is the fold again", async () => {
  const store = memory()
  const fx = { store }
  await richkeep.keep(entries, fx)
  assert.equal(store.rows.size(), new Set(richkeep.given(entries).map(g => g.k)).size)
  assert.deepEqual(await richkeep.fill(unrendered(entries), fx), entries)
})

test("an unconfirmed row is never filled, whatever this device holds for its source", async () => {
  const store = memory()
  const fx = { store }
  const trusted = entries.find(e => !e.unconfirmed && e.authored !== null)
  const provisional = entries.find(e => e.unconfirmed && e.authored !== null)
  assert.ok(trusted !== undefined && provisional !== undefined && provisional.authored !== null && trusted.authored !== null)
  const copied: Entry = { ...provisional, authored: { ...provisional.authored, body: trusted.authored.body, rich: { ...provisional.authored.rich, keys: trusted.authored.rich.keys } } }
  await richkeep.keep([trusted], fx)
  const [back] = await richkeep.fill(unrendered([copied]), fx)
  assert.equal(back?.authored?.rich.body, null)
})

test("a field the core named no key for is neither kept nor looked up", async () => {
  const keyless = (es: readonly Entry[]): Entry[] =>
    es.map(e => (e.authored === null ? e : { ...e, authored: { ...e.authored, rich: { ...e.authored.rich, keys: { body: null, detail: null, rationale: null, waiting_on: null } } } }))
  const none = { store: memory() }
  await richkeep.keep(keyless(entries), none)
  assert.equal(none.store.rows.size(), 0)
  assert.deepEqual(await richkeep.fill(unrendered(keyless(entries)), none), unrendered(keyless(entries)))
})
