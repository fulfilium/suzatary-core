// The replica kept on the device is shared by every tab of one origin, and it only climbs: two tabs offline at once
// both keep what they sealed, whichever saves last.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { StoredObject } from "../src/wire/generated/wire"
import type { Replica } from "../src/prodrome/replica"
import { NOTHING, appended, join, landed, losses } from "../src/prodrome/replica"
import type { Write } from "../src/prodrome/objects"
import { CHAIN, alterRow, heldRow, holdRow, keepRow } from "../src/prodrome/objects"
import { replicaWith } from "./held"
import door from "../src/app/idb"

// IndexedDB as far as `objects.ts` reaches into it: one database of named stores, each request answered on a later
// turn, a transaction committing once its last request has answered.
type Req = {
  result: unknown
  error: unknown
  transaction?: unknown
  onsuccess: (() => void) | null
  onerror: (() => void) | null
  onupgradeneeded?: ((e: { oldVersion: number }) => void) | null
}
const later = (f: () => void): void => void setTimeout(f, 0)
// The one key range the rows' store is asked for: every name from one bound to the other.
type Range = { readonly includes: (k: string) => boolean }
Object.assign(globalThis, { IDBKeyRange: { bound: (lower: string, upper: string, lowerOpen = false): Range => ({ includes: k => (lowerOpen ? lower < k : lower <= k) && k <= upper }) } })
// A device at `version`, holding `rows` in its one store; an upgrade runs before the open answers, as IndexedDB's does.
const fakeIndexedDB = (at = 0, rows: Readonly<Record<string, unknown>> = {}) => {
  // The connections the app's door kept open belong to the device before.
  door.reset()
  const stores = new Map<string, Map<string, unknown>>(at === 0 ? [] : [["chain", new Map(Object.entries(rows))]])
  let version = at
  const db = {
    objectStoreNames: { contains: (n: string) => stores.has(n), [Symbol.iterator]: () => [...stores.keys()][Symbol.iterator]() },
    createObjectStore: (n: string) => void stores.set(n, new Map()),
    deleteObjectStore: (n: string) => void stores.delete(n),
    close: () => {},
    transaction: (n: string) => {
      let open = 0
      let aborted = false
      const tx = {
        oncomplete: null as (() => void) | null,
        onerror: null as (() => void) | null,
        onabort: null as (() => void) | null,
        error: null,
        abort: () => {
          aborted = true
          later(() => tx.onabort?.())
        },
        objectStore: () => {
          const rows = stores.get(n) ?? new Map<string, unknown>()
          const ask = (f: () => unknown): Req => {
            open += 1
            const r: Req = { result: undefined, error: null, onsuccess: null, onerror: null }
            later(() => {
              r.result = f()
              r.onsuccess?.()
              open -= 1
              if (open === 0) later(() => (open === 0 && !aborted ? tx.oncomplete?.() : undefined))
            })
            return r
          }
          const openCursor = (): Req => {
            const ks = [...rows.keys()]
            const r: Req = { result: null, error: null, onsuccess: null, onerror: null }
            const next = (): void =>
              later(() => {
                const k = ks.shift()
                r.result = k === undefined ? null : { value: structuredClone(rows.get(k)), update: (v: unknown) => void rows.set(k, structuredClone(v)), continue: next }
                r.onsuccess?.()
              })
            next()
            return r
          }
          return {
            get: (k: string) => ask(() => structuredClone(rows.get(k))),
            put: (v: unknown, k: string) => ask(() => void (aborted || rows.set(k, structuredClone(v)))),
            delete: (k: string | Range) => ask(() => (typeof k === "string" ? void rows.delete(k) : [...rows.keys()].filter(x => k.includes(x)).forEach(x => rows.delete(x)))),
            openCursor,
          }
        },
      }
      return tx
    },
  }
  return {
    open: (_: string, wanted = 0) => {
      const r: Req = { result: db, error: null, transaction: db.transaction("chain"), onsuccess: null, onerror: null, onupgradeneeded: null }
      later(() => {
        if (version >= wanted) return r.onsuccess?.()
        r.onupgradeneeded?.({ oldVersion: version })
        version = wanted
        setTimeout(() => r.onsuccess?.(), 20)
      })
      return r
    },
  }
}

const name = (seed: string): string => seed.repeat(64).slice(0, 64)
const change = (hash: string, deps: readonly string[], todo: string): StoredObject => ({
  hash,
  literal: `Change(genesis='${name("0")}', deps=(${deps.map(d => `'${d}',`).join(" ")}), event=Completed(todo='${todo}', at=datetime(2026, 9, 7, 12, 0, 0), actor='ada', note=''))`,
})

const BOX = change(name("a"), [], "alpha")
const synced: Replica = replicaWith({ objects: [BOX], synced: [BOX.hash], boxHeads: [BOX.hash], fetchedAt: 1 })
const keep_ = (replica: Replica): Write<Replica> => ({ type: "Join", replica })
const resetting = (shown: readonly string[], over: "Read" | "Unread" = "Read", floor = 0): Write<Replica> => ({ type: "Reset", shown: new Set(shown), floor, over })
const row = async (chain = "life"): Promise<Replica | null> => {
  const read = await heldRow(CHAIN, chain)
  return read.type === "Held" ? read.v : null
}
const spoil = async (idb: ReturnType<typeof fakeIndexedDB>): Promise<void> => {
  const db = idb.open("suzatary").result as { transaction: (n: string) => { objectStore: () => { put: (v: unknown, k: string) => unknown } } }
  db.transaction("chain").objectStore().put({ objects: "not a list" }, "objects:life")
  await new Promise(r => setTimeout(r, 5))
}

test("two tabs offline: the second to save keeps the first's unsent act on the device", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB() })
  assert.equal((await holdRow(CHAIN, "life", keep_(synced))).type, "Val")
  const inB = appended(synced, [change(name("b"), [BOX.hash], "beta")])
  assert.equal((await holdRow(CHAIN, "life", keep_(inB))).type, "Val")
  const inA = appended(synced, [change(name("c"), [BOX.hash], "gamma")])
  assert.equal((await holdRow(CHAIN, "life", keep_(inA))).type, "Val")
  const held = new Set((await row())?.objects.map(o => o.hash))
  assert.ok(held.has(name("b")), "the act tab B sealed is still on the device")
  assert.ok(held.has(name("c")), "and so is tab A's")
})

test("a reset stands, and an act sealed before it in another tab is reported lost, never kept and dropped unsaid", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB() })
  const inB = appended(synced, [change(name("b"), [BOX.hash], "beta")])
  await holdRow(CHAIN, "life", keep_(inB))
  assert.equal((await holdRow(CHAIN, "life", resetting([name("b")]))).type, "Val")
  const actInB = appended(inB, [change(name("c"), [BOX.hash], "gamma")])
  const afterB = await holdRow(CHAIN, "life", keep_(actInB))
  assert.equal(afterB.type, "Val")
  const said = afterB.type === "Val" ? landed(actInB, afterB.v) : afterB
  assert.equal(said.type, "Error", "the save that came back on a later generation is not reported kept")
  assert.ok(said.type === "Error" && said.e.includes("reset"), "and it says why")
  assert.deepEqual((await row())?.objects, [], "the row is the reset's")
  assert.equal((await row())?.generation, 1)
  assert.deepEqual(afterB.type === "Val" ? join(inB, afterB.v).objects : null, [], "and tab B, joining the row it got back, holds the reset too")
})

test("a reset says everything it loses: a row holding an unsent act it was not shown refuses it", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB() })
  const inA = appended(synced, [change(name("b"), [BOX.hash], "beta")])
  await holdRow(CHAIN, "life", keep_(inA))
  // Tab A drew its notice over its own act; tab B saved one more before the press.
  const shown = losses(inA).map(o => o.hash)
  assert.deepEqual(shown, [name("b")], "every unsent act, not only the refused ones")
  await holdRow(CHAIN, "life", keep_(appended(synced, [change(name("c"), [BOX.hash], "gamma")])))
  const refused = await holdRow(CHAIN, "life", resetting(shown))
  assert.ok(refused.type === "Error" && refused.e.includes("1 more unsent act"), "the reset is refused, saying how many it would have lost unsaid")
  assert.equal((await row())?.objects.length, 3, "and nothing was dropped")
  const again = losses((await row()) ?? NOTHING).map(o => o.hash)
  assert.equal((await holdRow(CHAIN, "life", resetting(again))).type, "Val", "shown everything, it goes")
})

test("two resets in turn take two generations, so neither keeps what the other dropped", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB() })
  await holdRow(CHAIN, "life", keep_(synced))
  // Both tabs held generation 0 when they pressed; the generation is the row's, read in the transaction.
  const first = await holdRow(CHAIN, "life", resetting([], "Read", 0))
  const second = await holdRow(CHAIN, "life", resetting([], "Read", 0))
  assert.equal(first.type === "Val" ? first.v.generation : null, 1)
  assert.equal(second.type === "Val" ? second.v.generation : null, 2)
  assert.deepEqual((await row())?.objects, [])
})

test("a row is altered in its own transaction, and one not there or not read is left as it is", async () => {
  const idb = fakeIndexedDB()
  Object.assign(globalThis, { indexedDB: idb })
  const inB = appended(synced, [change(name("b"), [BOX.hash], "beta")])
  await holdRow(CHAIN, "life", keep_(inB))
  // What another tab joined in before the alteration stays: the step sees the row as the transaction reads it.
  await holdRow(CHAIN, "life", keep_(appended(synced, [change(name("c"), [BOX.hash], "gamma")])))
  const seen: number[] = []
  assert.equal((await alterRow(CHAIN, "life", r => (seen.push(r.objects.length), { ...r, held: { ...r.held, synced: [] } }))).type, "Val")
  assert.deepEqual(seen, [3], "the step got the row with both tabs' acts")
  assert.deepEqual((await row())?.held.synced, [], "and its answer is what the row now holds")
  assert.equal((await alterRow(CHAIN, "nowhere", () => assert.fail("no row to alter"))).type, "Val")
  assert.equal((await heldRow(CHAIN, "nowhere")).type, "Absent", "a row not there is not made")
  await spoil(idb)
  assert.equal((await alterRow(CHAIN, "life", () => assert.fail("a row that does not read is not altered"))).type, "Val")
  assert.equal((await heldRow(CHAIN, "life")).type, "Unreadable", "and it is left as it was")
})

test("a row that does not read is said, and never written over by a save", async () => {
  const idb = fakeIndexedDB()
  Object.assign(globalThis, { indexedDB: idb })
  await holdRow(CHAIN, "life", keep_(synced))
  await spoil(idb)
  const read = await heldRow(CHAIN, "life")
  assert.equal(read.type, "Unreadable", "never taken for a row that is not there")
  const answer = await holdRow(CHAIN, "life", keep_(appended(synced, [change(name("c"), [BOX.hash], "gamma")])))
  assert.ok(answer.type === "Error" && answer.e.includes("does not read"), "the save is refused, saying why")
  assert.equal((await holdRow(CHAIN, "life", resetting([]))).type, "Error", "and so is a reset that was not told it writes over that copy")
  assert.equal((await heldRow(CHAIN, "life")).type, "Unreadable", "the unread row is still what is kept")
})

test("a reset told it writes over a copy that does not read is the one write that may, and saves go on from it", async () => {
  const idb = fakeIndexedDB()
  Object.assign(globalThis, { indexedDB: idb })
  await holdRow(CHAIN, "life", keep_(synced))
  await spoil(idb)
  const answer = await holdRow(CHAIN, "life", resetting([], "Unread"))
  assert.equal(answer.type, "Val")
  assert.equal((await row())?.generation, 1, "the row reads again, as the reset")
  const fresh = answer.type === "Val" ? answer.v : NOTHING
  assert.equal((await holdRow(CHAIN, "life", keep_(appended(fresh, [BOX])))).type, "Val", "and saves go on from it")
})

test("a save that fails over a row that reads says the row reads, and one over a row that does not says why", async () => {
  const idb = fakeIndexedDB()
  Object.assign(globalThis, { indexedDB: idb })
  await holdRow(CHAIN, "life", keep_(appended(synced, [change(name("b"), [BOX.hash], "beta")])))
  // Refused, while the row reads: the reset was not shown the unsent act.
  const refused = await keepRow(CHAIN, "life", resetting([]))
  assert.equal(refused.row.type, "Error")
  assert.equal(refused.unread, null, "no false 'does not read'")
  const saved = await keepRow(CHAIN, "life", keep_(synced))
  assert.deepEqual([saved.row.type, saved.unread], ["Val", null], "a good save clears it")
  await spoil(idb)
  const spoiled = await keepRow(CHAIN, "life", keep_(synced))
  assert.equal(spoiled.row.type, "Error")
  assert.ok(spoiled.unread !== null && spoiled.unread.length > 0, "and a row that does not read says why")
})

// What the device's rows' store holds under `key`, as it was written.
const rawOf = (key: string): Promise<unknown> =>
  new Promise<unknown>(done => {
    const db = (globalThis as unknown as { indexedDB: { open: (n: string) => { result: { transaction: (n: string) => { objectStore: () => { get: (k: string) => { result: unknown; onsuccess: (() => void) | null } } } } } } }).indexedDB.open("suzatary").result
    const r = db.transaction("chain").objectStore().get(key)
    r.onsuccess = () => done(r.result)
  })

test("a device from version 8 keeps its chains' rows as they were, the tab's unsent act among them, and no other row", async () => {
  const unsent = change(name("c"), [BOX.hash], "beta")
  const kept = { generation: 0, objects: [BOX, unsent], synced: [BOX.hash], refused: [], words: [], frontier: { type: "Nothing" } }
  for (const from of [8, 9, 10]) {
    Object.assign(globalThis, {
      indexedDB: fakeIndexedDB(from, { "objects:life": kept, "objects:store/inbox": { replica: kept, keys: {} }, "objects:store/rendered-owed": true, "objects:pages/topics": kept }),
    })
    const life = await row()
    assert.deepEqual(life?.objects.map(o => o.hash), [BOX.hash, unsent.hash], `the unsent act is kept, from ${from}`)
    assert.deepEqual(await rawOf("objects:life"), kept, "as it was")
    for (const other of ["objects:store/inbox", "objects:store/rendered-owed", "objects:pages/topics"]) assert.equal(await rawOf(other), undefined, `${other} goes`)
  }
})

test("a device before version 8 keeps no row, and every chain is pulled again", async () => {
  for (const from of [1, 2, 3, 5, 7]) {
    Object.assign(globalThis, { indexedDB: fakeIndexedDB(from, { "objects:life": { objects: [BOX] }, "objects:store/inbox": { replica: { objects: [BOX] }, keys: {} } }) })
    assert.equal((await heldRow(CHAIN, "life")).type, "Absent", `from ${from}`)
    assert.equal(await rawOf("objects:store/inbox"), undefined)
    assert.equal((await holdRow(CHAIN, "life", keep_(synced))).type, "Val")
    assert.deepEqual(await row(), join(NOTHING, synced), "what the pull brings is joined into nothing left of the old row")
  }
})
