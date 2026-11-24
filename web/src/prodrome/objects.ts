/**
 * The device's database: the stores' replicas in the core's own form (`replicalog`), their content by name, the rendered
 * blocks, and the rows' store, which holds the own lines' rows (`ownlines`) and each chain's acts an earlier build kept
 * that the box had not taken. IndexedDB rather than `localStorage`, which is synchronous and too small.
 */

import lo from "../algebra/lo"
import idb from "../app/idb"

export const DB_NAME = "suzatary"
const STORE_NAME = "chain"
const CONTENT_NAME = "content"
const REPLICA_NAME = "replica"
const BLOCKS_NAME = "blocks"

/** Every row is kept under its name behind this prefix, which builds before version 14 chose for a chain's row. */
const keyFor = (name: string): string => `objects:${name}`
/** A chain's name holds no `/`, which every other row's name does (`own/lines`). */
const CHAIN_NAME = /^objects:([a-z][a-z0-9-]*)$/
/** The key a chain's carried acts are kept under until its core holds them. */
const carryKey = (chain: string): string => `carry:${chain}`

/**
 * The device keeps no format but this build's: a store this build does not read is taken again from the box, and only
 * what this tab wrote and the box has not taken is carried. A device before version 8 keeps nothing; from 8 the rows'
 * store is kept, its rows that are no chain's deleted, and every other store made anew; 13 adds the rendered blocks; 14
 * moves every chain's row to the chain's carry key, which the worker reads into the chain's core as its own writes,
 * once, and then deletes (`carried`).
 */
const DB: idb.Db = {
  name: DB_NAME,
  version: 14,
  store: STORE_NAME,
  upgrade: (db, tx, from) => {
    const blocks = (): void => void db.createObjectStore(BLOCKS_NAME, { keyPath: "key" }).createIndex("used", "used")
    if (from < 12) {
      const kept = from >= 8 && db.objectStoreNames.contains(STORE_NAME)
      for (const name of Array.from(db.objectStoreNames)) if (!(kept && name === STORE_NAME)) db.deleteObjectStore(name)
      if (kept) for (const prefix of OTHERS) tx.objectStore(STORE_NAME).delete(under(keyFor(prefix)))
      else db.createObjectStore(STORE_NAME)
      for (const name of [CONTENT_NAME, REPLICA_NAME]) db.createObjectStore(name)
    }
    if (from < 13) blocks()
    if (from >= 8) carryChains(tx.objectStore(STORE_NAME))
  },
}

/** The prefixes of the rows of the rows' store that are no chain's: every row a build before version 8 kept beside them. */
const OTHERS = ["store/", "pages/"]

/** Each chain's row moved to its carry key in the upgrade's transaction, as it was. */
function carryChains(rows: IDBObjectStore): void {
  const cursor = rows.openCursor(under(keyFor("")))
  cursor.onsuccess = () => {
    const at = cursor.result
    if (at === null) return
    const chain = CHAIN_NAME.exec(String(at.key))?.[1]
    if (chain !== undefined) {
      rows.put(at.value, carryKey(chain))
      at.delete()
    }
    at.continue()
  }
}

/** The device's content by name: one literal per object name, written once and never rewritten. */
export const CONTENT: idb.Db = { ...DB, store: CONTENT_NAME }

/** The replicas the core keeps in its own form, a snapshot and the frames after it, by the store's name (`replicalog`). */
export const REPLICA: idb.Db = { ...DB, store: REPLICA_NAME }

/** Typst's HTML by the key the box names it under, beside the replicas (`rendered.device`). */
export const BLOCKS: idb.Db = { ...DB, store: BLOCKS_NAME }

/**
 * The row an earlier build kept of `chain`, read by `parse` and handed to `keep`, and deleted once `keep` settled: a
 * `keep` that throws leaves it for the next start. A row that does not read, or a device that does not answer, is kept
 * and said to `unread`, never deleted unread. `keep` must write what it is handed twice as once, so a start cut short
 * between the two keeps nothing twice.
 */
export const carried = async <A>(
  chain: string,
  parse: (raw: unknown) => lo.Settled<A>,
  keep: (a: A) => Promise<void>,
  unread: (why: string) => void,
): Promise<void> => {
  if (!idb.available()) return
  const row = await idb.read(DB, carryKey(chain), parse)
  switch (row.type) {
    case "Absent":
      return
    case "Unreadable":
    case "Unavailable":
      return unread(row.why)
    case "Held":
      await keep(row.v)
      await idb.within(DB, "readwrite", s => idb.done(s.delete(carryKey(chain))))
  }
}

/** The row an earlier build kept of `chain` deleted: what a reset that writes over a row that does not read does. */
export const dropCarried = (chain: string): Promise<void> =>
  idb.available() ? idb.within(DB, "readwrite", s => idb.done(s.delete(carryKey(chain)))) : Promise.resolve()

/** What a row holds and how it reads and joins: every row only climbs, so two tabs keeping at once both land. */
export interface Rows<A> {
  readonly parse: (raw: unknown) => lo.Settled<A>
  readonly print: (a: A) => unknown
  readonly join: (a: A, b: A) => A
  readonly nothing: A
}

/** What this device holds of a row: nothing, its value, or a row that does not read, and why. */
export type Read<A> = Exclude<idb.Row<A>, { readonly type: "Unavailable" }>

/**
 * Never throws: a row that cannot be read says so, and is never taken for one that is not there. A store that does not
 * open is said the same way.
 */
export const heldRow = <A>(rows: Pick<Rows<A>, "parse">, name: string): Promise<Read<A>> =>
  idb.read(DB, keyFor(name), rows.parse).then((r): Read<A> => (r.type === "Unavailable" ? { type: "Unreadable", why: r.why } : r))

/**
 * Two rows read in one read-only transaction, so no write lands between the two reads: a reader of both waits for a
 * write queued before it once, never for one that came between. Never throws, as `heldRow` does not.
 */
export const heldRows = <A, B>(
  a: { readonly rows: Pick<Rows<A>, "parse">; readonly name: string },
  b: { readonly rows: Pick<Rows<B>, "parse">; readonly name: string },
): Promise<readonly [Read<A>, Read<B>]> => {
  const readOf = <C>(rows: Pick<Rows<C>, "parse">, raw: unknown): Read<C> => {
    if (raw === undefined) return { type: "Absent" }
    const parsed = rows.parse(raw)
    return parsed.type === "Val" ? { type: "Held", v: parsed.v } : { type: "Unreadable", why: parsed.e }
  }
  if (!idb.available()) return Promise.resolve([{ type: "Absent" }, { type: "Absent" }])
  return idb
    .within(DB, "readonly", async (store): Promise<readonly [Read<A>, Read<B>]> => {
      const first: unknown = await idb.done(store.get(keyFor(a.name)))
      const second: unknown = await idb.done(store.get(keyFor(b.name)))
      return [readOf(a.rows, first), readOf(b.rows, second)]
    })
    .catch((error: unknown): readonly [Read<A>, Read<B>] => {
      const why = lo.reason(error)
      return [{ type: "Unreadable", why }, { type: "Unreadable", why }]
    })
}

/**
 * `value` joined into the row in one read-write transaction, so two tabs saving at once both land; the row it now holds
 * is the answer. A join never writes over a row that does not read, since a join with nothing would shrink it. Every
 * failure says why, since a write that could not be kept is one this browser would forget.
 */
export const holdRow = <A>(rows: Rows<A>, name: string, value: A): Promise<lo.Settled<A>> =>
  idb
    .within(DB, "readwrite", async (store) => {
      const raw: unknown = await idb.done(store.get(keyFor(name)))
      const row = raw === undefined ? lo.val(rows.nothing) : rows.parse(raw)
      if (row.type !== "Val") return lo.error(`the row kept on this device does not read (${row.e}), so nothing was written over it`)
      const now = rows.join(row.v, value)
      await idb.done(store.put(rows.print(now), keyFor(name)))
      return lo.val(now)
    })
    .catch((error: unknown) => lo.error(lo.reason(error)))

/**
 * The row as `step` leaves it, in one read-write transaction, so what another tab joined in meanwhile stays: the one way
 * a row gives up part of itself, a purge's. A row that is not there, or does not read, is left as it is.
 */
export const alterRow = <A>(rows: Rows<A>, name: string, step: (a: A) => A): Promise<lo.Settled<null>> =>
  idb
    .within(DB, "readwrite", async (store) => {
      const raw: unknown = await idb.done(store.get(keyFor(name)))
      const row = raw === undefined ? null : rows.parse(raw)
      if (row !== null && row.type === "Val") await idb.done(store.put(rows.print(step(row.v)), keyFor(name)))
      return lo.val(null)
    })
    .catch((error: unknown) => lo.error(lo.reason(error)))

const under = (prefix: string): IDBKeyRange => IDBKeyRange.bound(prefix, `${prefix}￿`)
