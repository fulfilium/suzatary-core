// IndexedDB as far as the device's stores reach into it: databases of named stores, keyed by the key a write names or
// by a key path into the value, each request answered on a later turn, read whole or by a cursor, a transaction's
// writes kept only when it commits, and a write `failing` names refused, which aborts its transaction whole, as a full
// disk does.

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

Object.assign(globalThis, { IDBKeyRange: { bound: (lower: string, upper: string): Range => ({ includes: k => lower <= k && k <= upper }) } })

export type Fake = {
  // The keys whose writes are refused while it answers true.
  failing: (key: string) => boolean
  // What the store `store` of the database `name` holds, as committed.
  readonly rows: (name: string, store: string) => ReadonlyMap<string, unknown>
  // Every committed row of every store of every database, each as `<database>/<store>/<key>` and its value, printed.
  readonly printed: () => readonly string[]
  // How many times a database was opened, and how many values were read by `get` or `getAll`, since the device began.
  readonly opened: () => number
  readonly read: () => number
}

// A device holding nothing, set as the global `indexedDB`; `failing` refuses nothing until a test sets it.
export const fakeIndexedDB = (): Fake => {
  const dbs = new Map<string, { version: number; stores: Map<string, Map<string, unknown>>; paths: Map<string, string> }>()
  let opened = 0
  let read = 0
  const fake: Fake = {
    opened: () => opened,
    read: () => read,
    failing: () => false,
    rows: (name, store) => dbs.get(name)?.stores.get(store) ?? new Map(),
    printed: () => [...dbs].flatMap(([name, d]) => [...d.stores].flatMap(([store, rows]) => [...rows].map(([k, v]) => `${name}/${store}/${k} ${JSON.stringify(v)}`))),
  }
  const database = (name: string) => {
    const d = dbs.get(name) ?? { version: 0, stores: new Map<string, Map<string, unknown>>(), paths: new Map<string, string>() }
    dbs.set(name, d)
    const transaction = (store: string) => {
      const staged = new Map(d.stores.get(store) ?? [])
      let open = 0
      let asked = false
      let failed = false
      // Once nothing is asked, what was staged is the store's, unless a request failed.
      const settle = (): void => {
        if (open === 0)
          later(() => {
            if (open !== 0 || failed) return
            d.stores.set(store, staged)
            tx.oncomplete?.()
          })
      }
      const tx = {
        oncomplete: null as (() => void) | null,
        onerror: null as (() => void) | null,
        onabort: null as (() => void) | null,
        error: null as unknown,
        objectStore: () => {
          const ask = (f: () => unknown, refused = false): Req => {
            asked = true
            open += 1
            const r: Req = { result: undefined, error: null, onsuccess: null, onerror: null }
            later(() => {
              open -= 1
              if (refused || failed) {
                failed = true
                r.error = tx.error = "QuotaExceededError"
                r.onerror?.()
                return later(() => tx.onerror?.())
              }
              r.result = f()
              r.onsuccess?.()
              settle()
            })
            return r
          }
          const keys = (k?: Key): string[] => [...staged.keys()].filter(within(k)).sort()
          const path = d.paths.get(store)
          const keyOf = (v: unknown, k?: string): string => k ?? String((v as Record<string, unknown>)[path ?? ""])
          return {
            // Each row in key order, one success each, then a last with no cursor; the rows as they stood when opened.
            // The transaction stays open until the last.
            openCursor: () => {
              asked = true
              open += 1
              const rows = keys().map(x => structuredClone(staged.get(x)))
              const r: Req = { result: undefined, error: null, onsuccess: null, onerror: null }
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
              return r
            },
            get: (k: string) => ask(() => ((read += 1), structuredClone(staged.get(k)))),
            getAll: (k?: Key) =>
              ask(() => {
                const all = keys(k)
                read += all.length
                return all.map(x => structuredClone(staged.get(x)))
              }),
            getAllKeys: (k?: Key) => ask(() => keys(k)),
            put: (v: unknown, k?: string) => ask(() => void staged.set(keyOf(v, k), structuredClone(v)), fake.failing(keyOf(v, k))),
            delete: (k: Key) => ask(() => keys(k).forEach(x => staged.delete(x))),
            clear: () => ask(() => staged.clear()),
          }
        },
      }
      // A transaction asked nothing commits, as IndexedDB's does.
      later(() => later(() => (asked ? undefined : tx.oncomplete?.())))
      return tx
    }
    return {
      objectStoreNames: { contains: (n: string) => d.stores.has(n) },
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
  }
  Object.assign(globalThis, {
    indexedDB: {
      open: (name: string, wanted = 0) => {
        opened += 1
        const db = database(name)
        const r: Req = { result: db, error: null, onsuccess: null, onerror: null, onupgradeneeded: null }
        later(() => {
          const from = db.version()
          if (from < wanted) {
            // The version change's own transaction writes straight through: nothing in it is refused here.
            r.transaction = { objectStore: (n: string) => db.transaction(n).objectStore() }
            r.onupgradeneeded?.({ oldVersion: from })
            db.setVersion(wanted)
          }
          later(() => r.onsuccess?.())
        })
        return r
      },
      deleteDatabase: (name: string) => {
        dbs.delete(name)
        const r: Req = { result: undefined, error: null, onsuccess: null, onerror: null }
        later(() => r.onsuccess?.())
        return r
      },
    },
  })
  return fake
}
