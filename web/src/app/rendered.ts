// Typst's HTML on this device, the box's tabulation synced by union: each key the box's own, named by the core both run
// (a function's name and its source's `memo::name`), so a kept copy is never stale. The worker's table stands in front
// of the device's `blocks` store, beside the replicas. Either may drop any entry, the least recently used first; a key
// the box answers twice, differently, is a finding, told to the box and dropped. ⚠️ The device keeps an answer block
// only while the core's held keys open the words it renders (`Lines.admits`), and only of this build's functions, so
// a block of erased words is written by no join and swept by the next compaction.

import table from "../algebra/table"
import idb from "./idb"
import { BLOCKS, REPLICA } from "../prodrome/objects"

export namespace rendered {
  export type Pair = readonly [table.Key, string]
  export type Row = { key: string; html: string; bytes: number; used: number }
  export type Meta = Omit<Row, "html">
  export type Usage = { count: number; bytes: number }

  export const BUDGET = 20 * 1024 * 1024
  const KEPT = 2048

  const ours = (fs: ReadonlySet<string>) => (k: table.Key): boolean => fs.has(table.fnOf(k))

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

  // A key holding one value answers it; a miss or a finding answers null. A join answers the findings it made.
  export type Store = {
    get: (ks: readonly table.Key[]) => Promise<(string | null)[]>
    join: (pairs: readonly Pair[]) => Promise<table.Key[]>
    usage: () => Promise<Usage>
  }

  // The worker's store: also what a compaction sweeps of the `kept` keys, those of another build's function and those
  // the core does not admit, let go from the table too.
  export type Device = Store & { readonly swept: (kept: readonly table.Key[], admitted: readonly table.Key[]) => table.Key[] }

  // Of `ks`, those the core keeps blocks of, asked in the join's transaction with the replicas' store `s`; or `Behind`,
  // where another tab wrote what this tab's core has not taken, which `behind` takes.
  export type Admit = {
    readonly admits: (ks: readonly table.Key[], s: IDBObjectStore) => Promise<readonly table.Key[] | "Behind">
    readonly behind: () => Promise<void>
  }

  // A table in memory alone, of the functions `fs`.
  export const memory = (fs: ReadonlySet<string>, t: table.Table<string> = table.of(table.lru(KEPT))): Store => ({
    get: ks => Promise.resolve(ks.map(k => table.hit(t.get(k)) ?? null)),
    join: ps => Promise.resolve(t.join(ps.filter(([k]) => ours(fs)(k)))),
    usage: () => Promise.resolve(usageOf(t.pairs().map(([key, html]) => ({ key, bytes: bytesOf(html), used: 0 })))),
  })

  // The value kept under `k`, or the box's answer, joined under the key the box names it by.
  export const through = async (store: Store, k: table.Key | null, ask: () => Promise<{ readonly html: string; readonly key: string }>): Promise<string> => {
    const [kept] = k === null ? [null] : await store.get([k])
    if (kept !== null && kept !== undefined) return kept
    const answered = await ask()
    await store.join([[answered.key, answered.html]])
    return answered.html
  }

  // How many of the box's answers `throughAll` waits on at once: a browser asks one host six at a time anyway.
  const ASKING = 6

  // `through` for many at once, in one read and one write of the device. A failed ask is null.
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

  // The database a build before the blocks moved beside the replicas kept them in, deleted at the worker's start.
  export const DB = "suzatary-rendered"

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
    idb.within(BLOCKS, mode, work).catch(() => miss)

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
    readonly own: ReadonlySet<string>
    readonly admit: Admit
  }

  // The worker's table over the device's rows. A row and a joined value that differ are a finding: the row goes and the
  // table holds both, answering neither.
  const indexed = (fx: Effects, t: table.Table<string> = table.of(table.lru(KEPT))): Device => {
    const mine = ours(fx.own)
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
    // `ps` the core admits joined into the rows, in one transaction with the replicas, so it is decided against every
    // frame on the device; `Behind` where another tab wrote one this tab has not taken. A device that keeps nothing
    // answers `null`.
    const joinOnce = (ps: readonly Pair[]): Promise<table.Key[] | "Behind" | null> =>
      idb
        .withBeside(BLOCKS, [REPLICA], "readwrite", async (s, [replica]): Promise<table.Key[] | "Behind"> => {
          if (replica === undefined) throw new Error("a join spans the blocks and the replicas")
          const admitted = await fx.admit.admits(ps.map(([k]) => k), replica)
          if (admitted === "Behind") return "Behind"
          const live = ps.filter(([k]) => admitted.includes(k))
          const at = fx.now()
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
        })
        .catch(() => null)
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
          if (html !== null) t.insert(k, html)
          return html
        })
      },
      join: async given => {
        const ps = given.filter(([k]) => mine(k))
        if (ps.length === 0) return []
        const first = await joinOnce(ps)
        const done = first === "Behind" ? await fx.admit.behind().then(() => joinOnce(ps)) : first
        // A device that keeps nothing: the table is the only copy, and holds what the core admits.
        return done === "Behind" ? [] : (done ?? t.join(ps))
      },
      swept: (kept, admitted) => {
        const keep = new Set(admitted)
        const gone = kept.filter(k => !mine(k) || !keep.has(k))
        gone.forEach(k => t.remove(k))
        return gone
      },
      usage: () => within("readonly", async s => usageOf(await metas(s)), none),
    }
  }

  let found: (k: table.Key) => void = k => console.warn(`rendered: the box answered ${k} twice, differently`)
  // Who hears a finding: the beacon, once it is installed.
  export const hear = (f: (k: table.Key) => void): void => void (found = f)

  // The device's rows, made once by each tab's worker (`questions.serve`), with the functions its core keeps blocks of.
  export const device = (own: ReadonlySet<string>, admit: Admit): Device => indexed({ budget: BUDGET, now: Date.now, own, admit })

  // What the page asks of the worker for the device's rows.
  export type Asking = {
    readonly get: (ks: readonly table.Key[]) => Promise<(string | null)[]>
    readonly join: (pairs: readonly Pair[]) => Promise<table.Key[]>
    readonly usage: () => Promise<Usage>
  }

  // The page's blocks: the worker's, asked for each draw. The page keeps none, so an erasure the worker swept is gone
  // from every page at its next draw.
  export const viaWorker = (ask: Asking): Store => ({
    get: ks => ask.get(ks).catch(() => ks.map(() => null)),
    join: async ps => {
      const findings = await ask.join(ps).catch(() => [])
      findings.forEach(found)
      return findings
    },
    usage: () => ask.usage().catch(() => none),
  })

}
export default rendered
