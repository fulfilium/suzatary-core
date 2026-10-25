// Typst's HTML on this device, the box's tabulation synced by union (design §6.2): each key the box's own, named by
// the core both run (a function's name and its source's `memo::name`) and sent beside the HTML it keys, so a kept
// copy is never stale and a typesetter the box changed is a new function; the tab's table in front of the device's
// IndexedDB replica.
// Either may drop any entry, the least recently used first; a key the box answers twice, differently, is a
// finding: told to the box, never chosen, and dropped from the device so the next answer fills it.
// The device keeps blocks of this build's functions alone: a purge finds a block by the name this build computes,
// so one another build named would outlive the erasure of its words. A join takes none, and a forget drops every one.

import lo from "../algebra/lo"
import once from "../algebra/once"
import table from "../algebra/table"
import idb from "./idb"

export namespace rendered {
  export type Pair = readonly [table.Key, string]
  export type Row = { key: string; html: string; bytes: number; used: number }
  export type Meta = Omit<Row, "html">
  export type Usage = { count: number; bytes: number }

  export const BUDGET = 20 * 1024 * 1024
  const KEPT = 2048

  const ours = (fs: ReadonlySet<string>) => (k: table.Key): boolean => fs.has(table.fnOf(k))
  // What a forget of `ks` takes from what is `held`: those keys, and every block of a function not `fs`.
  const taken = (fs: ReadonlySet<string>, ks: readonly table.Key[], held: readonly table.Key[]): table.Key[] => [...ks, ...held.filter(k => !ours(fs)(k))]

  // A monoid under combine; none is its identity.
  export const none: Usage = { count: 0, bytes: 0 }
  export const combine = (...us: Usage[]): Usage =>
    us.reduce((a, b) => ({ count: a.count + b.count, bytes: a.bytes + b.bytes }), none)
  export const usageOf = (ms: readonly Meta[]): Usage => combine(...ms.map(m => ({ count: 1, bytes: m.bytes })))

  // UTF-16 code units, twice over: what the string costs in memory, near enough to what the store charges.
  const bytesOf = (html: string) => html.length * 2

  // Newest kept first until the budget is spent; everything after the first row that does not fit goes.
  export const evict = (ms: readonly Meta[], budget: number): string[] => {
    const newest = [...ms].sort((a, b) => b.used - a.used || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0))
    let total = 0
    let spent = false
    return newest.flatMap(m => {
      spent = spent || total + m.bytes > budget
      total += spent ? 0 : m.bytes
      return spent ? [m.key] : []
    })
  }

  // A key holding one value answers it; a miss or a finding answers null. A join answers the findings it made, and
  // keeps no block of another build's function. A forgotten key is gone from the table and the device, with every
  // block of another build's function, true once it is, and no later join of this page brings it back: an answer
  // asked for before an erasure lands after it.
  export type Store = {
    get: (ks: readonly table.Key[]) => Promise<(string | null)[]>
    join: (pairs: readonly Pair[]) => Promise<table.Key[]>
    forget: (ks: readonly table.Key[]) => Promise<lo.Settled<null>>
    // Every key, done once the device holds none.
    clear: () => Promise<lo.Settled<null>>
    usage: () => Promise<Usage>
  }

  // What a page forgot, so a join landing after it takes none of it again.
  const struck = (): {
    readonly add: (ks: readonly table.Key[]) => void
    readonly has: (k: table.Key) => boolean
    readonly fresh: (ps: readonly Pair[]) => Pair[]
  } => {
    const gone = new Set<table.Key>()
    return { add: ks => ks.forEach(k => gone.add(k)), has: k => gone.has(k), fresh: ps => ps.filter(([k]) => !gone.has(k)) }
  }
  const DONE: lo.Settled<null> = lo.val(null)

  // The tab's replica alone: a table in memory, of the functions `fs`.
  export const memory = (fs: ReadonlySet<string>, t: table.Table<string> = table.of(table.lru(KEPT))): Store => {
    const gone = struck()
    return {
      get: ks => Promise.resolve(ks.map(k => table.hit(t.get(k)) ?? null)),
      join: ps => Promise.resolve(t.join(gone.fresh(ps).filter(([k]) => ours(fs)(k)))),
      forget: ks => (gone.add(ks), taken(fs, ks, t.pairs().map(([k]) => k)).forEach(k => t.remove(k)), Promise.resolve(DONE)),
      clear: () => (t.pairs().forEach(([k]) => t.remove(k)), Promise.resolve(DONE)),
      usage: () => Promise.resolve(usageOf(t.pairs().map(([key, html]) => ({ key, bytes: bytesOf(html), used: 0 })))),
    }
  }

  // The value kept under `k`, or the box's answer, joined under the key the box names it by: a box of another build
  // names another function, so its value never lands under this tab's name for it.
  export const through = async (store: Store, k: table.Key | null, ask: () => Promise<{ readonly html: string; readonly key: string }>): Promise<string> => {
    const [kept] = k === null ? [null] : await store.get([k])
    if (kept !== null && kept !== undefined) return kept
    const answered = await ask()
    await store.join([[answered.key, answered.html]])
    return answered.html
  }

  // How many of the box's answers `throughAll` waits on at once: a browser asks one host six at a time anyway, and each
  // ask then starts in the task the one before it ended in, never all in one.
  const ASKING = 6

  // `through` for many at once: the kept values read together, the box asked for each miss, and its answers joined
  // together, so a window of blocks costs one read and one write of the device. A failed ask is null.
  export const throughAll = async (
    store: Store,
    asks: readonly { readonly k: table.Key | null; readonly ask: () => Promise<{ readonly html: string; readonly key: string }> }[],
  ): Promise<(string | null)[]> => {
    const keyed = asks.flatMap(a => (a.k === null ? [] : [a.k]))
    const read = await store.get(keyed)
    const held = new Map(keyed.map((k, i) => [k, read[i] ?? null]))
    type Answered = { readonly html: string; readonly join: Pair | null } | null
    const answered: Answered[] = asks.map(a => {
      const was = a.k === null ? null : (held.get(a.k) ?? null)
      return was === null ? null : { html: was, join: null }
    })
    const misses = asks.flatMap((_, i) => (answered[i] === null ? [i] : []))
    const next = async (): Promise<void> => {
      for (let i = misses.shift(); i !== undefined; i = misses.shift()) {
        answered[i] = await asks[i]!.ask().then(
          (r): Answered => ({ html: r.html, join: [r.key, r.html] }),
          () => null,
        )
      }
    }
    await Promise.all(Array.from({ length: Math.min(ASKING, misses.length) }, next))
    const joins = answered.flatMap(a => (a === null || a.join === null ? [] : [a.join]))
    if (joins.length > 0) await store.join(joins)
    return answered.map(a => a?.html ?? null)
  }

  export const DB = "suzatary-rendered"
  const ROWS = "rows"

  // A store from before keys were the box's own is dropped whole: a cache is a function of the code that reads it.
  const kept: idb.Db = {
    name: DB,
    version: 3,
    store: ROWS,
    upgrade: db => {
      if (db.objectStoreNames.contains(ROWS)) db.deleteObjectStore(ROWS)
      db.createObjectStore(ROWS, { keyPath: "key" }).createIndex("used", "used")
    },
  }

  const isRow = (v: unknown): v is Row =>
    typeof v === "object" &&
    v !== null &&
    "key" in v &&
    "html" in v &&
    "bytes" in v &&
    "used" in v &&
    typeof v.key === "string" &&
    typeof v.html === "string" &&
    typeof v.bytes === "number" &&
    typeof v.used === "number"

  // Any failure (no IndexedDB, a private window, an evicted store) is a miss.
  const within = <T>(mode: IDBTransactionMode, work: (s: IDBObjectStore) => Promise<T>, miss: T): Promise<T> =>
    idb.within(kept, mode, work).catch(() => miss)

  const metas = (s: IDBObjectStore): Promise<Meta[]> =>
    new Promise((ok, no) => {
      const out: Meta[] = []
      const r = s.openCursor()
      r.onsuccess = () => {
        const c = r.result
        if (c === null) return ok(out)
        if (isRow(c.value)) out.push({ key: c.value.key, bytes: c.value.bytes, used: c.value.used })
        c.continue()
      }
      r.onerror = () => no(new Error(String(r.error)))
    })

  export type Effects = {
    readonly budget: number
    readonly now: () => number
    readonly found: (k: table.Key) => void
    readonly own: () => Promise<ReadonlySet<string>>
  }

  // An erased block's mark on the device, beside the rows, read by every worker's join: a block one tab forgot is
  // written back by no other, whatever that tab heard. A mark is no row, so no reading, eviction or usage counts it. It
  // holds for the name for good: the same words said again later are rendered by the box and shown, never kept.
  const STRUCK = "struck:"
  const mark = (k: table.Key): string => `${STRUCK}${k}`
  const marked = (k: string): boolean => k.startsWith(STRUCK)

  // The tab's table over the device's rows: a miss in the table reads the row, the join of one pair. A row and
  // a joined value that differ are a finding: the row goes and the table holds both, answering neither.
  const indexed = (fx: Effects, t: table.Table<string> = table.of(table.lru(KEPT))): Store => {
    const gone = struck()
    // The rows of `ks`, read and marked used in one transaction: a miss, or a store that does not answer, is null.
    const read = (ks: readonly table.Key[]): Promise<(string | null)[]> =>
      within(
        "readwrite",
        s => {
          const at = fx.now()
          return Promise.all(
            ks.map(async k => {
              const row: unknown = await idb.done(s.get(k))
              if (!isRow(row)) return null
              await idb.done(s.put({ ...row, used: at }))
              return row.html
            }),
          )
        },
        ks.map(() => null),
      )
    return {
      get: async ks => {
        const held = ks.map(k => t.get(k))
        const misses = ks.filter((_, i) => held[i]!.type === "Miss")
        const found = misses.length === 0 ? [] : await read(misses)
        const byKey = new Map(misses.map((k, i) => [k, found[i] ?? null]))
        return ks.map((k, i) => {
          const l = held[i]!
          if (l.type !== "Miss") return table.hit(l) ?? null
          const html = byKey.get(k) ?? null
          // A read queued before a forget lands after it: what it found is not taken back.
          if (html === null || gone.has(k)) return null
          t.insert(k, html)
          return html
        })
      },
      // A pair the device marks erased is taken nowhere, the table included, and its mark struck here too.
      join: async given => {
        const mine = ours(await fx.own())
        const ps = gone.fresh(given).filter(([k]) => mine(k))
        if (ps.length === 0) return []
        const onDevice = await within(
          "readwrite",
          async (s): Promise<table.Key[]> => {
            const at = fx.now()
            const marks = await Promise.all(ps.map(([k]): Promise<unknown> => idb.done(s.get(mark(k)))))
            gone.add(ps.filter((_, i) => marks[i] !== undefined).map(([k]) => k))
            const live = ps.filter((_, i) => marks[i] === undefined)
            const inTab = t.join(live)
            const rows = await Promise.all(live.map(([k]): Promise<unknown> => idb.done(s.get(k))))
            const disagree = live.flatMap(([k, html], i) => {
              const row = rows[i]
              return isRow(row) && row.html !== html ? [((): Pair => [k, row.html])()] : []
            })
            t.join(disagree)
            const found = new Set([...inTab, ...disagree.map(([k]) => k)])
            await Promise.all(live.map(([k, html]): Promise<unknown> => (found.has(k) ? idb.done(s.delete(k)) : idb.done(s.put({ key: k, html, bytes: bytesOf(html), used: at })))))
            await Promise.all(evict(await metas(s), fx.budget).map(key => idb.done(s.delete(key))))
            return [...found]
          },
          null,
        )
        // A device that keeps nothing: the table is the only copy.
        const found = onDevice ?? t.join(ps)
        found.forEach(fx.found)
        return found
      },
      // A device that keeps nothing holds nothing to forget; one whose store fails to answer has not forgotten. Each
      // forgotten key is marked, so no tab's join writes it back.
      forget: async ks => {
        gone.add(ks)
        const fs = await fx.own()
        taken(fs, ks, t.pairs().map(([k]) => k)).forEach(k => t.remove(k))
        return !idb.available()
          ? DONE
          : idb
              .within(kept, "readwrite", async s => {
                const rows = (await idb.done(s.getAllKeys())).filter((k): k is string => typeof k === "string" && !marked(k))
                await Promise.all(taken(fs, ks, rows).map(k => idb.done(s.delete(k))))
                await Promise.all(ks.map(k => idb.done(s.put({ key: mark(k) }))))
                return DONE
              })
              .catch((e: unknown) => lo.error(lo.reason(e)))
      },
      // Every block, its marks kept.
      clear: () => {
        t.pairs().forEach(([k]) => t.remove(k))
        return !idb.available()
          ? Promise.resolve(DONE)
          : idb
              .within(kept, "readwrite", async s => {
                const rows = (await idb.done(s.getAllKeys())).filter((k): k is string => typeof k === "string" && !marked(k))
                await Promise.all(rows.map(k => idb.done(s.delete(k))))
                return DONE
              })
              .catch((e: unknown) => lo.error(lo.reason(e)))
      },
      usage: () => within("readonly", async s => usageOf(await metas(s)), none),
    }
  }

  let found: (k: table.Key) => void = k => console.warn(`rendered: the box answered ${k} twice, differently`)
  // Who hears a finding: the beacon, once it is installed.
  export const hear = (f: (k: table.Key) => void): void => void (found = f)

  // The device's rows, made once by each tab's worker (`questions.serve`), their readers and writers, with the functions
  // its core keeps blocks of: an erasure there is one delete and a mark, which every worker's join reads, so no tab
  // writes the block back. A finding is answered to the page that asked the join, which tells it.
  export const device = (own: ReadonlySet<string>): Store => indexed({ budget: BUDGET, now: Date.now, found: () => {}, own: () => Promise.resolve(own) })

  // What the page asks of the worker for the device's rows: blocks by name, a join of the box's answers, and how much
  // the device keeps, with the functions the worker's build keeps blocks of.
  export type Asking = {
    readonly get: (ks: readonly table.Key[]) => Promise<(string | null)[]>
    readonly join: (pairs: readonly Pair[]) => Promise<table.Key[]>
    readonly usage: () => Promise<Usage>
    readonly own: () => Promise<ReadonlySet<string>>
  }

  // The page's blocks: a view in memory by name over the worker's, never written to the device from here. What the view
  // lacks is asked of the worker; the box's answers are handed to the worker to keep, and held in view when they are of
  // a function the worker keeps. The worker forgets erased blocks from the device first and then says so, so `forget`
  // and `clear` take them from the view alone, and a join landing after takes none of them again.
  export const viaWorker = (ask: Asking, t: table.Table<string> = table.of(table.lru(KEPT))): Store => {
    const gone = struck()
    const kept = once.of(() => ask.own().catch(() => new Set<string>()))
    return {
      get: async ks => {
        const held = ks.map(k => t.get(k))
        const misses = ks.filter((_, i) => held[i]!.type === "Miss")
        const read = misses.length === 0 ? [] : await ask.get(misses).catch(() => misses.map(() => null))
        const byKey = new Map(misses.map((k, i) => [k, read[i] ?? null]))
        return ks.map((k, i) => {
          const l = held[i]!
          if (l.type !== "Miss") return table.hit(l) ?? null
          const html = byKey.get(k) ?? null
          if (html === null || gone.has(k)) return null
          t.insert(k, html)
          return html
        })
      },
      join: async given => {
        const ps = gone.fresh(given)
        if (ps.length === 0) return []
        const [findings, fs] = await Promise.all([ask.join(ps).catch(() => []), kept.get()])
        const bad = new Set(findings)
        t.join(gone.fresh(ps).filter(([k]) => ours(fs)(k) && !bad.has(k)))
        findings.forEach(found)
        return findings
      },
      forget: ks => (gone.add(ks), ks.forEach(k => t.remove(k)), Promise.resolve(DONE)),
      clear: () => (t.pairs().forEach(([k]) => t.remove(k)), Promise.resolve(DONE)),
      usage: () => ask.usage().catch(() => none),
    }
  }

}
export default rendered
