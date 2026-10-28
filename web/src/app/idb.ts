// The device's IndexedDB, one door: a database per use, opened once and kept open for every transaction after, and a
// kept row read as absent, held, a row that does not read, or a store that does not open, never one taken for another.

import lo from "../algebra/lo"

export namespace idb {
  export type Db = {
    readonly name: string
    readonly version: number
    readonly store: string
    // Run in the version change's own transaction, from the version the device held (0 for none).
    readonly upgrade: (db: IDBDatabase, tx: IDBTransaction, from: number) => void
  }

  export type Row<A> =
    | { readonly type: "Absent" }
    | { readonly type: "Held"; readonly v: A }
    | { readonly type: "Unreadable"; readonly why: string }
    | { readonly type: "Unavailable"; readonly why: string }

  export const available = (): boolean => typeof indexedDB !== "undefined"

  export const done = <T>(r: IDBRequest<T>): Promise<T> =>
    new Promise((ok, no) => {
      r.onsuccess = () => ok(r.result)
      r.onerror = () => no(new Error(String(r.error)))
    })

  // Each database opened once by this thread, under the `indexedDB` that opened it, and kept open: opening one costs
  // hundreds of milliseconds on a phone, a transaction over an open one almost nothing. A connection is let go when
  // another asks to change or delete the database, which waits on every open one, and when the browser closes it.
  const opened = new Map<string, Promise<IDBDatabase>>()
  const forget = (name: string, db: Promise<IDBDatabase>): void => {
    if (opened.get(name) === db) opened.delete(name)
  }
  const open = (d: Db): Promise<IDBDatabase> => {
    const known = opened.get(d.name)
    if (known !== undefined) return known
    const r = indexedDB.open(d.name, d.version)
    // A version change always runs in its own transaction.
    r.onupgradeneeded = e => d.upgrade(r.result, r.transaction!, e.oldVersion)
    // Another context that holds an older version open and does not let it go (a frozen tab) blocks the upgrade: that is
    // said rather than waited on, and the connection the request opens once it is let go is closed, since nothing holds it.
    let refused = false
    const db = new Promise<IDBDatabase>((ok, no) => {
      r.onsuccess = () => (refused ? r.result.close() : ok(r.result))
      r.onerror = () => no(new Error(String(r.error)))
      r.onblocked = () => {
        refused = true
        no(new Error("another tab holds this device's data open under an older build: close it, and this one reads again"))
      }
    }).then(
      c => {
        c.onversionchange = () => {
          forget(d.name, db)
          c.close()
        }
        c.onclose = () => forget(d.name, db)
        return c
      },
      (e: unknown) => {
        forget(d.name, db)
        throw e
      },
    )
    opened.set(d.name, db)
    return db
  }

  // Every connection this thread holds closed and let go, so the next transaction opens its database again: what a
  // test does between devices.
  export const reset = (): void => {
    for (const db of opened.values()) void db.then(c => c.close(), () => {})
    opened.clear()
  }

  // A connection the browser lost without saying so: one refuses every transaction as no longer valid, and WebKit's
  // answers that its connection to the database's server was lost.
  const lost = (e: unknown): boolean => e instanceof DOMException && (e.name === "InvalidStateError" || e.name === "UnknownError")

  // `work` runs in one transaction over the database `d` names, and answers once that transaction has committed. A lost
  // connection is let go, so the next transaction reopens it.
  const over = async <T>(d: Db, mode: IDBTransactionMode, work: (s: IDBObjectStore) => Promise<T>): Promise<T> => {
    const connection = open(d)
    const db = await connection
    const tx = (() => {
      try {
        return db.transaction(d.store, mode)
      } catch (e: unknown) {
        if (lost(e)) forget(d.name, connection)
        throw e
      }
    })()
    const committed = new Promise<void>((ok, no) => {
      tx.oncomplete = () => ok()
      tx.onerror = tx.onabort = () => {
        if (lost(tx.error)) forget(d.name, connection)
        no(new Error(String(tx.error)))
      }
    })
    const [answer] = await Promise.all([work(tx.objectStore(d.store)), committed])
    return answer
  }

  export const within = async <T>(d: Db, mode: IDBTransactionMode, work: (s: IDBObjectStore) => Promise<T>): Promise<T> => {
    if (!available()) throw new Error("this browser keeps nothing (no IndexedDB)")
    return over(d, mode, work)
  }

  const held = <A>(l: lo.Settled<A>): Row<A> => (l.type === "Val" ? { type: "Held", v: l.v } : { type: "Unreadable", why: l.e })

  // Never throws: a row that cannot be read says why, and nothing is ever written over it here. Only a database that
  // does not open is `Unavailable`; a read or a transaction that fails is the row's.
  export const read = <A>(d: Db, key: string, parse: (u: unknown) => lo.Settled<A>): Promise<Row<A>> =>
    !available()
      ? Promise.resolve({ type: "Absent" })
      : open(d).then(
          () =>
            over(d, "readonly", (s): Promise<unknown> => done(s.get(key))).then(
              (u): Row<A> => (u === undefined ? { type: "Absent" } : held(parse(u))),
              (e: unknown): Row<A> => ({ type: "Unreadable", why: lo.reason(e) }),
            ),
          (e: unknown): Row<A> => ({ type: "Unavailable", why: lo.reason(e) }),
        )
}
export default idb
