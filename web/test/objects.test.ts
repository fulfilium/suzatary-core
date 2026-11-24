// The device's rows are shared by every tab of one origin, and each only climbs: two tabs keeping at once both keep
// what they wrote, whichever saves last; and the upgrade that moved the chains into the core keeps each chain's row for
// the worker to carry, as it was, and every own row.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { Rows } from "../src/prodrome/objects"
import { alterRow, heldRow, heldRows, holdRow } from "../src/prodrome/objects"
import door from "../src/app/idb"
import ownlines from "../src/prodrome/ownlines"
import { forAllAsync } from "./gen"

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
    createObjectStore: (n: string) => (stores.set(n, new Map()), { createIndex: () => {} }),
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
          const openCursor = (range?: Range): Req => {
            const ks = [...rows.keys()].filter(k => range === undefined || range.includes(k)).sort()
            const r: Req = { result: null, error: null, onsuccess: null, onerror: null }
            const next = (): void =>
              later(() => {
                const k = ks.shift()
                r.result =
                  k === undefined
                    ? null
                    : { key: k, value: structuredClone(rows.get(k)), update: (v: unknown) => void rows.set(k, structuredClone(v)), delete: () => void rows.delete(k), continue: next }
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

// A row of names that only grows: a join of sets.
type Names = { readonly names: readonly string[] }
const NAMES: Rows<Names> = {
  parse: raw => (typeof raw === "object" && raw !== null && "names" in raw && Array.isArray(raw.names) ? { type: "Val", v: { names: raw.names.map(String) } } : { type: "Error", e: "not a row of names" }),
  print: r => r,
  join: (a, b) => ({ names: [...new Set([...a.names, ...b.names])].sort() }),
  nothing: { names: [] },
}
const row = async (key = "own/test"): Promise<Names | null> => {
  const read = await heldRow(NAMES, key)
  return read.type === "Held" ? read.v : null
}
const spoil = async (idb: ReturnType<typeof fakeIndexedDB>): Promise<void> => {
  const db = idb.open("suzatary").result as { transaction: (n: string) => { objectStore: () => { put: (v: unknown, k: string) => unknown } } }
  db.transaction("chain").objectStore().put({ names: "not a list" }, "objects:own/test")
  await new Promise(r => setTimeout(r, 5))
}

test("a row is altered in its own transaction, and one not there or not read is left as it is", async () => {
  const idb = fakeIndexedDB()
  Object.assign(globalThis, { indexedDB: idb })
  await holdRow(NAMES, "own/test", { names: ["a"] })
  // What another tab joined in before the alteration stays: the step sees the row as the transaction reads it.
  await holdRow(NAMES, "own/test", { names: ["b"] })
  const seen: number[] = []
  assert.equal((await alterRow(NAMES, "own/test", r => (seen.push(r.names.length), { names: [] }))).type, "Val")
  assert.deepEqual(seen, [2], "the step got the row with both tabs' names")
  assert.deepEqual(await row(), { names: [] }, "and its answer is what the row now holds")
  assert.equal((await alterRow(NAMES, "nowhere", () => assert.fail("no row to alter"))).type, "Val")
  assert.equal((await heldRow(NAMES, "nowhere")).type, "Absent", "a row not there is not made")
  await spoil(idb)
  assert.equal((await alterRow(NAMES, "own/test", () => assert.fail("a row that does not read is not altered"))).type, "Val")
  assert.equal((await heldRow(NAMES, "own/test")).type, "Unreadable", "and it is left as it was")
})

test("a row that does not read is said, and never written over by a save", async () => {
  const idb = fakeIndexedDB()
  Object.assign(globalThis, { indexedDB: idb })
  await holdRow(NAMES, "own/test", { names: ["a"] })
  await spoil(idb)
  assert.equal((await heldRow(NAMES, "own/test")).type, "Unreadable", "never taken for a row that is not there")
  const answer = await holdRow(NAMES, "own/test", { names: ["c"] })
  assert.ok(answer.type === "Error" && answer.e.includes("does not read"), "the save is refused, saying why")
  assert.equal((await heldRow(NAMES, "own/test")).type, "Unreadable", "the unread row is still what is kept")
})

// What the device's rows' store holds under `key`, as it was written.
const rawOf = (key: string): Promise<unknown> =>
  new Promise(done => {
    const db = (globalThis as unknown as { indexedDB: { open: (n: string) => { result: { transaction: (n: string) => { objectStore: () => { get: (k: string) => { result: unknown; onsuccess: (() => void) | null } } } } } } }).indexedDB.open("suzatary").result
    const r = db.transaction("chain").objectStore().get(key)
    r.onsuccess = () => done(r.result)
  })

test("the upgrade to version 14 moves each chain's row to its carry key as it was, keeps every own row, and drops what no build keeps", async () => {
  const kept = { generation: 0, objects: [{ hash: name("a"), literal: "Genesis(…)" }], synced: [], refused: [], words: [], frontier: { type: "Nothing" } }
  const own = { records: [] }
  for (const from of [8, 11, 12, 13]) {
    Object.assign(globalThis, {
      indexedDB: fakeIndexedDB(from, { "objects:life": kept, "objects:work-2": kept, "objects:own/lines": own, "objects:store/inbox": { replica: kept, keys: {} } }),
    })
    assert.deepEqual(await ownlines.kept(), { type: "Val", v: [] }, `the own row reads, from ${from}`)
    assert.deepEqual([await rawOf("carry:life"), await rawOf("carry:work-2")], [kept, kept], `each chain's row carried as it was, from ${from}`)
    assert.deepEqual([await rawOf("objects:life"), await rawOf("objects:work-2")], [undefined, undefined], "and its row gone")
    assert.deepEqual(await rawOf("objects:own/lines"), own, "an own row stays")
    assert.deepEqual(await rawOf("objects:store/inbox"), from < 12 ? undefined : { replica: kept, keys: {} }, "a row no build keeps goes with the upgrade before 12")
  }
})

test("a device before version 8 keeps no row", async () => {
  for (const from of [1, 2, 3, 5, 7]) {
    Object.assign(globalThis, { indexedDB: fakeIndexedDB(from, { "objects:life": { objects: [] }, "objects:own/lines": { records: [] } }) })
    assert.equal((await heldRow(NAMES, "life")).type, "Absent", `from ${from}`)
    assert.equal(await rawOf("carry:life"), undefined)
    assert.equal(await rawOf("objects:own/lines"), undefined)
  }
})

// This tab's own lines (`ownlines`): a row of the rows' store that only climbs, kept by every upgrade from version 8.
const ANSWERS: readonly unknown[] = ["Unsent", { Refused: { kind: "Trust", reason: "a key" } }, { Refused: { kind: "Outdated", reason: "reload" } }, "Taken"]
test("law: no own line is dropped, and each holds its furthest answer, whatever order and upgrade", async () => {
  await forAllAsync(40, async r => {
    Object.assign(globalThis, { indexedDB: fakeIndexedDB(r.pick([8, 11, 12, 13])) })
    const furthest = new Map<string, number>()
    const moves = Array.from({ length: 1 + r.int(6) }, () =>
      Array.from({ length: 1 + r.int(3) }, () => {
        const literal = `Titled(conversation='default', at=datetime(2026, 11, 17, 9, 0, ${r.int(4)}), …)`
        const climbed = r.int(ANSWERS.length)
        furthest.set(literal, Math.max(furthest.get(literal) ?? 0, climbed))
        return { literal, keys: { [name(String(r.int(9)))]: name("7") }, answer: ANSWERS[climbed] }
      }),
    )
    for (const move of moves) {
      assert.equal((await ownlines.keep(move)).type, "Val")
      // A reload, which reopens the device, between some moves.
      if (r.bool(0.3)) door.reset()
    }
    const kept = await ownlines.kept()
    assert.equal(kept.type, "Val")
    if (kept.type !== "Val") return
    assert.deepEqual(
      new Map(kept.v.map(k => [k.literal, ANSWERS.indexOf(ANSWERS.find(a => JSON.stringify(a) === JSON.stringify(k.answer)))])),
      furthest,
    )
  })
})

test("a device from version 8 keeps this tab's own lines through the upgrade", async () => {
  const own = { records: [{ literal: "Opened(…)", keys: { [name("1")]: name("2") }, answer: "Unsent" }] }
  for (const from of [8, 11, 12, 13]) {
    Object.assign(globalThis, { indexedDB: fakeIndexedDB(from, { "objects:own/lines": own }) })
    assert.deepEqual(await ownlines.kept(), { type: "Val", v: own.records }, `from ${from}`)
  }
})

test("a load lets go of exactly the own records the core names, and two tabs acting on one refusal act once", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(13) })
  const records = ANSWERS.map((answer, n) => ({ literal: `Titled(… ${n})`, keys: {}, answer }))
  assert.equal((await ownlines.keep(records)).type, "Val")
  const answers = async (): Promise<unknown> => {
    const kept = await ownlines.kept()
    return kept.type === "Val" ? kept.v.map(r => r.answer) : kept
  }
  assert.equal((await ownlines.settled(new Set([records[2]?.literal ?? "", records[3]?.literal ?? ""]))).type, "Val")
  assert.deepEqual(await answers(), [ANSWERS[0], ANSWERS[1]], "the named records go, and only they")
  await ownlines.acted(["2".repeat(64)])
  await ownlines.acted(["2".repeat(64)])
  assert.deepEqual([...(await ownlines.actedOn())], ["2".repeat(64)], "a join of names: acted once")
})

test("a load keeps a taken own line whose keys the landed rows do not hold yet", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(13) })
  const records = ANSWERS.map((answer, n) => ({ literal: `Titled(… ${n})`, keys: {}, answer }))
  assert.equal((await ownlines.keep(records)).type, "Val")
  assert.equal((await ownlines.settled(new Set())).type, "Val")
  const kept = await ownlines.kept()
  assert.deepEqual(kept.type === "Val" ? kept.v.map(r => r.answer) : kept, [ANSWERS[0], ANSWERS[1], ANSWERS[2], "Taken"])
})

test("the other tabs are woken only when a keep moved the row", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(13) })
  const heard: number[] = []
  const listener = new BroadcastChannel("rows/own/lines")
  listener.onmessage = () => heard.push(1)
  try {
    const records = [{ literal: "Titled(… once)", keys: {}, answer: { Refused: { kind: "Trust", reason: "a key" } } }]
    assert.equal((await ownlines.keep(records)).type, "Val")
    assert.equal((await ownlines.keep(records)).type, "Val")
    await new Promise(done => setTimeout(done, 50))
    assert.equal(heard.length, 1, "the same records kept again wake no tab")
  } finally {
    listener.close()
  }
})

test("the device's lock is waited for a bound alone: a holder that never lets go leaves a tab to go on by itself", async () => {
  type Request = (name: string, options: { signal: AbortSignal }, f: () => Promise<unknown>) => Promise<unknown>
  const frozen: Request = (_, options) =>
    new Promise((_, refuse) => options.signal.addEventListener("abort", () => refuse(new DOMException("timed out", "TimeoutError"))))
  const granted: Request = (_, __, f) => f()
  const was = Object.getOwnPropertyDescriptor(globalThis.navigator, "locks")
  try {
    for (const [request, held] of [[frozen, false], [granted, true]] as const) {
      Object.defineProperty(globalThis.navigator, "locks", { value: { request }, configurable: true })
      let ran = 0
      const answer = await ownlines.device("own/lines", async () => (ran++, "sealed"), 20)
      assert.equal(answer, "sealed", `held: ${held}`)
      assert.equal(ran, 1, "it runs once, under the lock or alone")
    }
  } finally {
    if (was === undefined) Reflect.deleteProperty(globalThis.navigator, "locks")
    else Object.defineProperty(globalThis.navigator, "locks", was)
  }
})

// A tab's core as the row sees it, at the cheapest level: records by print, each resting on the prints its `keys`
// name, and what it lets go is what the core's `releasable` answers: a refused record the person acted on, unless a
// record it holds that is not going rests on it. `log` says in which order the row met it.
const NEVER = { Refused: { kind: "Final", reason: "never" } }
const tab = (log: string[] = []) => {
  const held = new Map<string, ownlines.Kept>()
  const acted = new Set<string>()
  const core: ownlines.Core<{ readonly acted: number }> = {
    take: (records, done) => {
      for (const r of records) held.set(r.literal, r)
      for (const n of done) acted.add(n)
      log.push(`take ${records.length} records, ${done.size} acts`)
      return { acted: acted.size }
    },
    releasable: () => {
      log.push("releasable")
      const going = new Set([...held.values()].filter(r => JSON.stringify(r.answer) === JSON.stringify(NEVER) && acted.has(r.literal)).map(r => r.literal))
      for (const r of held.values()) if (!going.has(r.literal)) for (const parent of Object.keys(r.keys)) going.delete(parent)
      return { free: going, refused: [...going] }
    },
  }
  return { core, held }
}

test("law: a line one tab keeps between another tab's landing and its settle is never orphaned", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(13) })
  const parent = { literal: "Said(… refused)", keys: {}, answer: NEVER }
  assert.equal((await ownlines.keep([parent])).type, "Val")
  assert.equal((await ownlines.acted([parent.literal])).type, "Val")
  // The first tab landed, holding the refused record and the act.
  const first = tab()
  assert.equal((await ownlines.read(first.core)).type, "Val")
  // The second keeps a line resting on it, under the device's lock.
  const child = { literal: "Said(… over it)", keys: { [parent.literal]: "" }, answer: "Unsent" }
  assert.equal((await ownlines.device(ownlines.LOCK, () => ownlines.keep([child]))).type, "Val")
  assert.ok(first.core.releasable(new Set()).free.has(parent.literal), "deciding on what it held, the first would let it go")
  // The first settles: the row read into its core, then decided.
  assert.deepEqual((await ownlines.settle(first.core, async () => {})).cleanup, null)
  const kept = await ownlines.kept()
  assert.deepEqual(kept.type === "Val" ? kept.v.map(r => r.literal).sort() : kept, [child.literal, parent.literal].sort())
})

test("a settle reads the records and the acts before it decides, and answers its reading after the removal", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(13) })
  const refused = { literal: "Said(… dismissed)", keys: {}, answer: NEVER }
  assert.equal((await ownlines.keep([refused])).type, "Val")
  assert.equal((await ownlines.acted([refused.literal])).type, "Val")
  const log: string[] = []
  const settled = await ownlines.settle(tab(log).core, async () => void log.push("content kept"))
  log.push("published")
  assert.deepEqual(settled, { reading: { type: "Val", v: { taken: { acted: 1 }, acts: null } }, cleanup: null }, "the reading it answers holds the act")
  assert.deepEqual(log, ["take 1 records, 1 acts", "content kept", "releasable", "published"])
  const kept = await ownlines.kept()
  assert.deepEqual(kept, { type: "Val", v: [] }, "the dismissed refusal went before the reading was published")
})

test("two rows read in one transaction say each what a read of it alone says", async () => {
  const parse = (raw: unknown) => (typeof raw === "string" ? { type: "Val" as const, v: raw } : { type: "Error" as const, e: "not a string" })
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(13, { "objects:own/a": "held", "objects:own/b": 7 }) })
  const [a, b] = await heldRows({ rows: { parse }, name: "own/a" }, { rows: { parse }, name: "own/b" })
  assert.deepEqual([a, b], [await heldRow({ parse }, "own/a"), await heldRow({ parse }, "own/b")])
  assert.deepEqual([a.type, b.type], ["Held", "Unreadable"])
  const [c] = await heldRows({ rows: { parse }, name: "own/c" }, { rows: { parse }, name: "own/a" })
  assert.deepEqual(c, { type: "Absent" })
})

test("a settle whose cleanup fails answers its reading all the same, and why the cleanup failed", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(13) })
  const refused = { literal: "Said(… dismissed)", keys: {}, answer: NEVER }
  assert.equal((await ownlines.keep([refused])).type, "Val")
  assert.equal((await ownlines.acted([refused.literal])).type, "Val")
  const settled = await ownlines.settle(tab().core, () => Promise.reject(new Error("the device is full")))
  assert.deepEqual(settled.reading, { type: "Val", v: { taken: { acted: 1 }, acts: null } }, "the reading, published")
  assert.match(settled.cleanup ?? "", /the device is full/)
  const kept = await ownlines.kept()
  assert.deepEqual(kept.type === "Val" ? kept.v.map(r => r.literal) : kept, [refused.literal], "let go at the next settle")
  assert.deepEqual((await ownlines.settle(tab().core, async () => {})).cleanup, null)
  assert.deepEqual(await ownlines.kept(), { type: "Val", v: [] })
})

test("a settle keeps the names it lets go as refused as acted on, and writes nothing where it lets nothing go", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(13) })
  const wordless = { literal: "Opened(… untitled)", keys: {}, answer: NEVER }
  assert.equal((await ownlines.keep([wordless])).type, "Val")
  const telling = tab()
  const core: ownlines.Core<{ readonly acted: number }> = { ...telling.core, releasable: () => ({ free: new Set([wordless.literal]), refused: ["9".repeat(64)] }) }
  assert.deepEqual((await ownlines.settle(core, async () => {})).cleanup, null)
  assert.deepEqual([...(await ownlines.actedOn())], ["9".repeat(64)], "kept as acted on, so a tab holding it unsent never posts it")
  assert.deepEqual(await ownlines.kept(), { type: "Val", v: [] })
  const before = await ownlines.told()
  assert.deepEqual(await ownlines.settled(new Set()), { type: "Val", v: null })
  assert.deepEqual(await ownlines.told(), before)
})

test("an acts' row that does not read is said by the reading, and taken as no act", async () => {
  Object.assign(globalThis, { indexedDB: fakeIndexedDB(13, { "objects:own/acted": 7 }) })
  const reading = await ownlines.read(tab().core)
  assert.equal(reading.type, "Val")
  if (reading.type !== "Val") return
  assert.deepEqual(reading.v.taken, { acted: 0 })
  assert.notEqual(reading.v.acts, null)
})
