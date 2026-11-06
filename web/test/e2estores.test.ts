// The browser scenes reach into the device's IndexedDB by a store's name, in a string the type checker never reads. A
// scene that names a store the database no longer creates throws inside an IndexedDB callback, so its promise never
// settles and the scene hangs until its whole budget runs out. The law: every store a scene names is one the app's
// databases hold at their current versions. A scene that builds an older database on purpose (`indexedDB.open` of
// the app's database at a version) is the one it may name older stores in.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync, readdirSync } from "node:fs"

import { REPLICA } from "../src/prodrome/objects"
import signer from "../src/prodrome/signer"

// The stores the app's database creates from nothing, as its own upgrade creates them.
const created = (): Set<string> => {
  const stores = new Set<string>()
  const db = {
    objectStoreNames: { contains: (n: string) => stores.has(n) },
    createObjectStore: (n: string) => (stores.add(n), { createIndex: () => {} }),
    deleteObjectStore: (n: string) => void stores.delete(n),
  }
  REPLICA.upgrade(db as unknown as IDBDatabase, {} as IDBTransaction, 0)
  return stores
}

const DIR = "web/e2e"
const OLDER = /indexedDB\.open\("suzatary",/
// `transaction("a"`, `transaction(["a", "b"]` and `objectStore("a")`: each quoted name inside.
const NAMING = /(?:transaction\(\s*\[?|objectStore\()((?:\s*"[^"]+"\s*,?)+)/g
const namesIn = (source: string): string[] => [...source.matchAll(NAMING)].flatMap(m => [...(m[1] ?? "").matchAll(/"([^"]+)"/g)].map(n => n[1] ?? ""))
const MODES = new Set(["readonly", "readwrite"])

test("law: every IndexedDB store a browser scene names is one the app's databases hold", () => {
  const held = new Set([...created(), signer.STORE])
  const scenes = readdirSync(DIR).filter(f => f.endsWith(".ts"))
  const read = scenes.map(f => [f, readFileSync(`${DIR}/${f}`, "utf8")] as const).filter(([, s]) => !OLDER.test(s))
  const named = read.flatMap(([f, s]) => namesIn(s).filter(n => !MODES.has(n)).map(n => `${f}: ${n}`))
  assert.ok(named.length > 0, "no scene names a store: the pattern reads nothing")
  assert.deepEqual(
    named.filter(n => !held.has(n.slice(n.indexOf(": ") + 2))),
    [],
    `stores the app's databases hold: ${[...held].join(", ")}`,
  )
})
