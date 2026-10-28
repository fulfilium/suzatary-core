// The one IndexedDB door tells a database that does not open from a row that does not read.

import { test } from "node:test"
import assert from "node:assert/strict"

import idb from "../src/app/idb"
import lo from "../src/algebra/lo"

// A request answered on a later turn, by `ok` or by an error.
type Req = { result: unknown; error: unknown; transaction: unknown; onsuccess: (() => void) | null; onerror: (() => void) | null; onupgradeneeded: null }
const answered = (result: unknown, error: unknown = null): Req => {
  const r: Req = { result, error, transaction: null, onsuccess: null, onerror: null, onupgradeneeded: null }
  setTimeout(() => (error === null ? r.onsuccess?.() : r.onerror?.()), 0)
  return r
}

// A database whose one store answers `get` with `row`, fails it, or answers it and then aborts its transaction.
const fake = (open: "opens" | "refuses", get: { readonly row: unknown } | "fails" | "aborts") => {
  // The connection the door kept open belongs to the database before.
  idb.reset()
  return {
    open: () =>
      open === "refuses"
        ? answered(null, "UnknownError: Connection to Indexed Database server lost")
        : answered({
            close: () => {},
            transaction: () => {
              const tx: { oncomplete: (() => void) | null; onerror: (() => void) | null; onabort: (() => void) | null; error: unknown; objectStore: () => unknown } = {
                oncomplete: null,
                onerror: null,
                onabort: null,
                error: get === "fails" ? "the read failed" : get === "aborts" ? "AbortError" : null,
                objectStore: () => ({
                  get: () => {
                    const r = get === "fails" ? answered(null, "the read failed") : answered(get === "aborts" ? 7 : get.row)
                    setTimeout(() => (get === "fails" ? tx.onerror?.() : get === "aborts" ? tx.onabort?.() : tx.oncomplete?.()), 5)
                    return r
                  },
                }),
              }
              return tx
            },
          }),
  }
}

const DB: idb.Db = { name: "test", version: 1, store: "rows", upgrade: () => {} }
const parse = (u: unknown) => (typeof u === "number" ? lo.val(u) : lo.error("not a number"))

test("a database that does not open is Unavailable; a read that fails or aborts, or a row that does not parse, is Unreadable", async () => {
  Object.assign(globalThis, { indexedDB: fake("refuses", { row: 1 }) })
  assert.equal((await idb.read(DB, "k", parse)).type, "Unavailable")
  Object.assign(globalThis, { indexedDB: fake("opens", "fails") })
  assert.equal((await idb.read(DB, "k", parse)).type, "Unreadable")
  Object.assign(globalThis, { indexedDB: fake("opens", "aborts") })
  assert.equal((await idb.read(DB, "k", parse)).type, "Unreadable", "a read answered and then aborted is not a row")
  Object.assign(globalThis, { indexedDB: fake("opens", { row: "x" }) })
  assert.equal((await idb.read(DB, "k", parse)).type, "Unreadable")
  Object.assign(globalThis, { indexedDB: fake("opens", { row: 7 }) })
  assert.deepEqual(await idb.read(DB, "k", parse), { type: "Held", v: 7 })
  Object.assign(globalThis, { indexedDB: fake("opens", { row: undefined }) })
  assert.deepEqual(await idb.read(DB, "k", parse), { type: "Absent" })
})
