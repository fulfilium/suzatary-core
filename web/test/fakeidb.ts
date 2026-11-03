// IndexedDB as far as the device's stores reach into it: databases of named stores, keyed by the key a write names or
// by a key path into the value, each request answered on a later turn, read whole or by a cursor, a transaction's
// writes kept only when it commits, and a write `failing` names refused, which aborts its transaction whole, as a full
// disk does.

import idb from "../src/app/idb"

type Req = {
  result: unknown
  error: unknown
  transaction?: unknown
  onsuccess: (() => void) | null
  onerror: (() => void) | null
  onupgradeneeded?: ((e: { oldVersion: number }) => void) | null
}
type Range = { readonly includes: (k: string) => boolean }
type Key = string | Range

const later = (f: () => void): void => void setTimeout(f, 0)
const within = (k: Key | undefined) => (x: string): boolean => (k === undefined ? true : typeof k === "string" ? x === k : k.includes(x))

Object.assign(globalThis, { IDBKeyRange: { bound: (lower: string, upper: string, lowerOpen = false): Range => ({ includes: k => (lowerOpen ? lower < k : lower <= k) && k <= upper }) } })

export type Fake = {
  // The keys whose writes are refused while it answers true.
  failing: (key: string) => boolean
  // What the store `store` of the database `name` holds, as committed.
  readonly rows: (name: string, store: string) => ReadonlyMap<string, unknown>
  // Every committed row of every store of every database, each as `<database>/<store>/<key>` and its value, printed.
  readonly printed: () => readonly string[]
  // How many transactions were begun, how many values were read by `get` or `getAll`, and how many deletes and clears
  // were asked, since the device began.
  readonly transactions: () => number
  readonly read: () => number
  readonly deletes: () => number
}

// Bytes printed as hex, so a row holding a key's bytes, as the core's frames and snapshots do, is found by them.
const hexed = (_: string, v: unknown): unknown => (v instanceof Uint8Array ? [...v].map(b => b.toString(16).padStart(2, "0")).join("") : v)

// A device holding nothing, set as the global `indexedDB`; `failing` refuses nothing until a test sets it.
export const fakeIndexedDB = (): Fake => {
  // The connections the app's door kept open belong to the device before.
  idb.reset()
  // Each database's stores, and the transactions over it not yet ended, which a later one over a store they span waits for.
  type Open = { readonly names: readonly string[]; readonly writes: boolean; readonly done: Promise<void> }
  const dbs = new Map<string, { version: number; stores: Map<string, Map<string, unknown>>; paths: Map<string, string>; open: Open[] }>()
  let read = 0
  let transactions = 0
  let deletes = 0
  const fake: Fake = {
    transactions: () => transactions,
    deletes: () => deletes,
    read: () => read,
    failing: () => false,
    rows: (name, store) => dbs.get(name)?.stores.get(store) ?? new Map(),
    printed: () => [...dbs].flatMap(([name, d]) => [...d.stores].flatMap(([store, rows]) => [...rows].map(([k, v]) => `${name}/${store}/${k} ${JSON.stringify(v, hexed)}`))),
  }
  const database = (name: string) => {
    const d = dbs.get(name) ?? { version: 0, stores: new Map<string, Map<string, unknown>>(), paths: new Map<string, string>(), open: [] }
    dbs.set(name, d)
    // A transaction over one store or several, each staged on its own and all committed together. As IndexedDB's, it
    // waits for every transaction begun before it over a store it spans where either writes, and is staged from the
    // stores as that last one left them, so no transaction's commit writes over another's.
    const transaction = (over: string | readonly string[], mode: IDBTransactionMode = "readonly") => {
      transactions += 1
      const names = typeof over === "string" ? [over] : over
      const writes = mode !== "readonly"
      const blockers = d.open.filter(t => (t.writes || writes) && t.names.some(n => names.includes(n))).map(t => t.done)
      let release = (): void => {}
      const mine = { names, writes, done: new Promise<void>(ended => (release = ended)) }
      d.open.push(mine)
      const end = (): void => {
        d.open = d.open.filter(t => t !== mine)
        release()
      }
      let stagedOf = new Map<string, Map<string, unknown>>()
      let begun = false
      const queued: (() => void)[] = []
      const once = (f: () => void): void => (begun ? f() : void queued.push(f))
      let open = 0
      let asked = false
      let failed = false
      let ended = false
      // Once nothing is asked, what was staged is the store's, unless a request failed.
      const settle = (): void => {
        if (open === 0)
          later(() => {
            if (open !== 0 || failed || ended) return
            ended = true
            for (const [n, staged] of stagedOf) d.stores.set(n, staged)
            end()
            tx.oncomplete?.()
          })
      }
      const tx = {
        oncomplete: null as (() => void) | null,
        onerror: null as (() => void) | null,
        onabort: null as (() => void) | null,
        error: null as unknown,
        objectStore: (name?: string) => {
          const store = name ?? names[0]!
          if (!names.includes(store)) throw new Error(`the transaction does not span ${store}`)
          const staged = (): Map<string, unknown> => stagedOf.get(store)!
          const ask = (f: () => unknown, refused = false): Req => {
            asked = true
            open += 1
            const r: Req = { result: undefined, error: null, onsuccess: null, onerror: null }
            once(() =>
              later(() => {
                open -= 1
                if (refused || failed) {
                  failed = true
                  r.error = tx.error = "QuotaExceededError"
                  r.onerror?.()
                  if (!ended) (ended = true), end()
                  return later(() => tx.onerror?.())
                }
                r.result = f()
                r.onsuccess?.()
                settle()
              }),
            )
            return r
          }
          const keys = (k?: Key): string[] => [...staged().keys()].filter(within(k)).sort()
          const path = d.paths.get(store)
          const keyOf = (v: unknown, k?: string): string => k ?? String((v as Record<string, unknown>)[path ?? ""])
          return {
            // Each row in key order, one success each, then a last with no cursor; the rows as they stood when opened.
            // The transaction stays open until the last.
            openCursor: () => {
              asked = true
              open += 1
              const r: Req = { result: undefined, error: null, onsuccess: null, onerror: null }
              once(() => {
                const rows = keys().map(x => structuredClone(staged().get(x)))
                let at = 0
                const step = (): void => {
                  if (at < rows.length) {
                    r.result = { value: rows[at], continue: () => void ((at += 1), later(step)) }
                    return r.onsuccess?.()
                  }
                  r.result = null
                  open -= 1
                  r.onsuccess?.()
                  settle()
                }
                later(step)
              })
              return r
            },
            get: (k: string) => ask(() => ((read += 1), structuredClone(staged().get(k)))),
            // At most `count` rows, the first in key order, as IndexedDB's own.
            getAll: (k?: Key, count?: number) =>
              ask(() => {
                const all = keys(k).slice(0, count)
                read += all.length
                return all.map(x => structuredClone(staged().get(x)))
              }),
            getAllKeys: (k?: Key, count?: number) => ask(() => keys(k).slice(0, count)),
            count: (k?: Key) => ask(() => keys(k).length),
            put: (v: unknown, k?: string) => ask(() => void staged().set(keyOf(v, k), structuredClone(v)), fake.failing(keyOf(v, k))),
            delete: (k: Key) => ((deletes += 1), ask(() => keys(k).forEach(x => staged().delete(x)))),
            clear: () => ((deletes += 1), ask(() => staged().clear())),
          }
        },
      }
      void Promise.all(blockers).then(() => {
        stagedOf = new Map(names.map(n => [n, new Map(d.stores.get(n) ?? [])]))
        begun = true
        for (const f of queued.splice(0)) f()
        // A transaction asked nothing commits, as IndexedDB's does.
        later(() =>
          later(() => {
            if (asked || ended) return
            ended = true
            end()
            tx.oncomplete?.()
          }),
        )
      })
      return tx
    }
    const connection = {
      onversionchange: null as (() => void) | null,
      objectStoreNames: { contains: (n: string) => d.stores.has(n), [Symbol.iterator]: () => [...d.stores.keys()][Symbol.iterator]() },
      createObjectStore: (n: string, o?: { keyPath?: string }) => {
        d.stores.set(n, new Map())
        if (o?.keyPath !== undefined) d.paths.set(n, o.keyPath)
        return { createIndex: () => {} }
      },
      deleteObjectStore: (n: string) => void d.stores.delete(n),
      close: () => {},
      transaction,
      version: () => d.version,
      setVersion: (v: number) => void (d.version = v),
    }
    connections.set(name, [...(connections.get(name) ?? []), connection])
    return connection
  }
  // Every connection opened, by database: a delete asks each to let go, as IndexedDB's `versionchange` does.
  const connections = new Map<string, { onversionchange: (() => void) | null }[]>()
  Object.assign(globalThis, {
    indexedDB: {
      open: (name: string, wanted = 0) => {
        const db = database(name)
        const r: Req = { result: db, error: null, onsuccess: null, onerror: null, onupgradeneeded: null }
        later(() => {
          const from = db.version()
          if (from < wanted) {
            // The version change's own transaction writes straight through: nothing in it is refused here.
            r.transaction = { objectStore: (n: string) => db.transaction(n, "versionchange").objectStore() }
            r.onupgradeneeded?.({ oldVersion: from })
            db.setVersion(wanted)
          }
          later(() => r.onsuccess?.())
        })
        return r
      },
      deleteDatabase: (name: string) => {
        for (const c of connections.get(name) ?? []) c.onversionchange?.()
        connections.delete(name)
        dbs.delete(name)
        const r: Req = { result: undefined, error: null, onsuccess: null, onerror: null }
        later(() => r.onsuccess?.())
        return r
      },
    },
  })
  return fake
}
