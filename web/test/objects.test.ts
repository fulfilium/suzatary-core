// The replica kept on the device is shared by every tab of one origin, and it only climbs: two tabs offline at once
// both keep what they sealed, whichever saves last.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { StoredObject } from "../src/wire/generated/wire"
import type { Replica } from "../src/prodrome/replica"
import { NOTHING, appended, join, landed, losses } from "../src/prodrome/replica"
import type { Write } from "../src/prodrome/objects"
import { CHAIN, alterRow, dropRow, heldRow, holdRow, keepRow } from "../src/prodrome/objects"
import { replicaWith } from "./held"

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
Object.assign(globalThis, { IDBKeyRange: { bound: (lower: string, upper: string): Range => ({ includes: k => lower <= k && k <= upper }) } })
// A device at `version`, holding `rows` in its one store; an upgrade runs before the open answers, as IndexedDB's does.
const fakeIndexedDB = (at = 0, rows: Readonly<Record<string, unknown>> = {}) => {
  const stores = new Map<string, Map<string, unknown>>(at === 0 ? [] : [["chain", new Map(Object.entries(rows))]])
  let version = at
  const db = {
    objectStoreNames: { contains: (n: string) => stores.has(n) },
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

test("a row a build no longer reads is deleted under a reset's rule, and never with an unsent act it was not shown", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB() })
  await holdRow(CHAIN, "store/conversations", keep_(appended(synced, [change(name("b"), [BOX.hash], "beta")])))
  const refused = await dropRow(CHAIN, "store/conversations", new Set())
  assert.ok(refused.type === "Error" && refused.e.includes("unsent act"), "a row holding an act it was not shown stays")
  assert.equal((await heldRow(CHAIN, "store/conversations")).type, "Held")
  await holdRow(CHAIN, "store/lines", keep_(synced))
  assert.equal((await dropRow(CHAIN, "store/conversations", new Set([name("b")]))).type, "Val")
  assert.equal((await heldRow(CHAIN, "store/conversations")).type, "Absent", "nothing of it is left")
  assert.equal((await heldRow(CHAIN, "store/lines")).type, "Held", "and no other row moved")
  assert.equal((await dropRow(CHAIN, "store/conversations", new Set())).type, "Val", "deleting what is gone is done")
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

test("a row version 2 kept is dropped with its database's rows, never read as a replica, and the chain is pulled again", async () => {
  const old = { objects: [BOX], synced: [BOX.hash], refused: [{ hash: name("b"), reason: "its event is triage's" }], boxHeads: [BOX.hash], untrusted: ["triage"], root: null, fetchedAt: 5 }
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(2, { "objects:life": old, "objects:store/inbox": old }) })
  assert.equal((await heldRow(CHAIN, "life")).type, "Absent")
  assert.equal((await heldRow(CHAIN, "store/inbox")).type, "Absent", "every row of the old version goes")
  assert.equal((await holdRow(CHAIN, "life", keep_(synced))).type, "Val")
  assert.deepEqual(await row(), join(NOTHING, synced), "what the pull brings is joined into nothing left of the old row")
})

test("a row version 3 kept is carried: its objects stay, and a word of the box that said no geneses is dropped", async () => {
  const said = { epoch: 1, heads: [BOX.hash], untrusted: [], root: null, fetchedAt: 5, geneses: [name("0")] }
  const unsaid = { ...said, epoch: 2, fetchedAt: 6, geneses: null }
  const unsent = change(name("c"), [BOX.hash], "beta")
  const kept = { generation: 0, objects: [BOX, unsent], synced: [BOX.hash], refused: [], words: [said, unsaid] }
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(3, { "objects:life": kept, "objects:store/inbox": { replica: { ...kept, words: [unsaid] }, keys: {} } }) })
  const life = await row()
  assert.ok(life !== null, "the row reads")
  assert.deepEqual(life.objects.map(o => o.hash), [BOX.hash, unsent.hash], "the unsent act is kept")
  assert.deepEqual(life.held.words, [said], "the word that said no geneses is gone, the other stays")
  const raw = await new Promise<unknown>(done => {
    const db = (globalThis as unknown as { indexedDB: { open: (n: string) => { result: { transaction: (n: string) => { objectStore: () => { get: (k: string) => { result: unknown; onsuccess: (() => void) | null } } } } } } }).indexedDB.open("suzatary").result
    const r = db.transaction("chain").objectStore().get("objects:store/inbox")
    r.onsuccess = () => done(r.result)
  })
  assert.deepEqual(raw, { replica: { ...kept, words: [], frontier: { type: "Nothing" } }, keys: {} }, "a store's row is carried the same way")
})

// What the device's rows' store holds under `key`, as it was written.
const rawOf = (key: string): Promise<unknown> =>
  new Promise<unknown>(done => {
    const db = (globalThis as unknown as { indexedDB: { open: (n: string) => { result: { transaction: (n: string) => { objectStore: () => { get: (k: string) => { result: unknown; onsuccess: (() => void) | null } } } } } } }).indexedDB.open("suzatary").result
    const r = db.transaction("chain").objectStore().get(key)
    r.onsuccess = () => done(r.result)
  })

test("version 5 drops the lines' row and its pieces, kept as literals, marks their blocks owed, and keeps every other row, from version 3 or 4", async () => {
  const kept = { generation: 0, objects: [BOX], synced: [BOX.hash], refused: [], words: [] }
  const lines = { replica: kept, keys: { [name("k")]: "00" } }
  for (const from of [3, 4]) {
    Object.assign(globalThis, {
      indexedDB: fakeIndexedDB(from, {
        "objects:life": kept,
        "objects:store/inbox": { replica: kept, keys: {} },
        "objects:store/lines": lines,
        "objects:store/lines/piece/000000000001": lines,
      }),
    })
    assert.ok((await row()) !== null, "a chain's row reads")
    assert.equal(await rawOf("objects:store/lines"), undefined, `from ${from}`)
    assert.equal(await rawOf("objects:store/lines/piece/000000000001"), undefined, "and its pieces with it")
    assert.deepEqual(await rawOf("objects:store/inbox"), { replica: { ...kept, frontier: { type: "Nothing" } }, keys: {} }, "the inbox's row stays")
    assert.equal(await rawOf("objects:store/rendered-owed"), true, "and the tab is told to clear the blocks the lines fed")
  }
})

test("version 6 gives every row of version 5 a frontier of nothing, so its first pull asks from nothing", async () => {
  const kept = { generation: 0, objects: [BOX], synced: [BOX.hash], refused: [], words: [] }
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(5, { "objects:life": kept, "objects:store/lines": { replica: kept, keys: {} } }) })
  assert.ok((await row()) !== null, "a chain's row reads")
  assert.deepEqual(await rawOf("objects:life"), { ...kept, frontier: { type: "Nothing" } })
  assert.deepEqual(await rawOf("objects:store/lines"), { replica: { ...kept, frontier: { type: "Nothing" } }, keys: {} }, "the lines' row is kept, framed")
})

test("a row version 1 kept is dropped, never read as a replica sealing onto heads it did not compute", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(1, { "objects:life": { objects: [BOX] } }) })
  assert.equal((await heldRow(CHAIN, "life")).type, "Absent")
})
