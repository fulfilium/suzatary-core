// The lines' content on this device: each object's literal by its name, taken only when its bytes hash to that name,
// kept in the device's store and handed to the core's worker, which reads it; this thread keeps only the names.

import type { ContentPayload, SkeletonNode } from "../wire/generated/wire"
import idb from "../app/idb"
import lo from "../algebra/lo"
import { CONTENT } from "./objects"
import { sha256Hex } from "./verify"

export namespace content {
  // Who asks, first to last: a conversation a page opens, the whole store behind it, or a prefetch ahead of any page.
  export type Lane = "First" | "Behind" | "Ahead"
  const LANES: readonly Lane[] = ["First", "Behind", "Ahead"]

  export type Effects = {
    readonly ask: (hashes: readonly string[]) => Promise<ContentPayload>
    // The bytes a page's fetch will ask the box for, said once as it starts; then each batch's bytes as it comes or is
    // lost. A prefetch says none of them.
    readonly asked: (bytes: number) => void
    readonly came: (bytes: number) => void
    readonly lost: (bytes: number) => void
    // How many answers may be out at once, asked as each goes out.
    readonly atOnce: () => number
    // Literals now held, handed to the reader once each; it settles once the reader has them.
    readonly held: (rows: readonly Admitted[]) => Promise<void>
    // Whether the page is shown: a prefetch sends nothing while it is not.
    readonly shown: () => boolean
  }

  export type Fetcher = {
    // The content of `nodes` held; rejects when an answer their names were in did not come or was not kept.
    readonly fetch: (lane: Lane, nodes: readonly SkeletonNode[]) => Promise<void>
    // Sends what a hidden page held back.
    readonly wake: () => void
  }

  // The box's bounds on one answer (`query::MOST_HASHES`, `routes::MOST_CONTENT_BYTES`).
  export const MOST_HASHES = 64
  export const MOST_BYTES = 1 << 20

  // Over HTTP/2 one connection carries every request. Over HTTP/1.1 a browser opens at most six to the box, a page holds
  // three open for streams (its chain's, the stores', a conversation's) and a pull piece may hold one more, so one answer
  // is out at a time and a connection stays free for every other request.
  export const atOnce = (protocol: string): number => (protocol === "h2" || protocol === "h3" ? 4 : 1)

  export type Row = { readonly hash: string; readonly literal: string }

  // A literal known to hash to its name: `admit` checked it, or the device's store held it, which keeps only what `admit`
  // took. Only this module holds the symbol, so nothing else makes one, and a reader may hold it without hashing again.
  const ADMITTED: unique symbol = Symbol("admitted")
  export type Admitted = { readonly [ADMITTED]: Row }
  const admitted = (r: Row): Admitted => ({ [ADMITTED]: r })
  export const rowOf = (a: Admitted): Row => a[ADMITTED]

  const memory = new Set<string>()
  let moves = 0

  // Every name a reading asked for: the page keeps their content while it is open.
  const asked = new Set<string>()
  export const has = (name: string): boolean => {
    asked.add(name)
    return memory.has(name)
  }
  // The same, without keeping it: what a count reads.
  export const holds = (name: string): boolean => memory.has(name)
  export const read = (): ReadonlySet<string> => asked
  // Counts every change of what is held, so a reading of it is named by it.
  export const version = (): number => moves

  const byName = (x: SkeletonNode, y: SkeletonNode): number => (x.hash < y.hash ? -1 : x.hash > y.hash ? 1 : 0)

  // The longest run from the front of `queue` within `MOST_HASHES` items and `MOST_BYTES` by `size`, one item alone
  // whatever its size. A fetch sorts its run by name, so one set has one URL.
  export const batch = <A>(queue: readonly A[], size: (a: A) => number): A[] => {
    const out: A[] = []
    let bytes = 0
    for (const n of queue) {
      if (out.length > 0 && (out.length === MOST_HASHES || bytes + size(n) > MOST_BYTES)) break
      out.push(n)
      bytes += size(n)
    }
    return out
  }

  // The whole store newest first: a line by when it was written, structure before every line.
  const newest = (x: SkeletonNode, y: SkeletonNode): number => {
    const at = (n: SkeletonNode): string => (n.header.node === "line" ? n.header.at : "￿")
    return at(x) < at(y) ? 1 : at(x) > at(y) ? -1 : 0
  }

  // An answer's literals, each checked against the name asked for in the same place; one that is not refuses the answer.
  export const admit = async (asked: readonly string[], answer: ContentPayload): Promise<lo.Settled<readonly Admitted[]>> => {
    if (answer.objects.length !== asked.length) return lo.error(`the box answered ${answer.objects.length} objects for ${asked.length} names`)
    const hashed = await Promise.all(answer.objects.map(o => sha256Hex(o.literal)))
    const wrong = asked.filter((name, i) => hashed[i] !== name || answer.objects[i]?.hash !== name)
    return wrong.length > 0 ? lo.error(`the box's bytes for ${wrong.join(", ")} are not what those names hash to`) : lo.val(answer.objects.map(o => admitted({ hash: o.hash, literal: o.literal })))
  }

  // Rows handed to the reader in runs of at most `MOST_BYTES`, so no one message holds the thread long.
  const take = async (fx: Effects, rows: readonly Admitted[]): Promise<void> => {
    const fresh = rows.filter(r => !memory.has(rowOf(r).hash))
    for (let at = 0; at < fresh.length; ) {
      const run = batch(fresh.slice(at), r => rowOf(r).literal.length)
      await fx.held(run)
      for (const r of run) memory.add(rowOf(r).hash)
      at += run.length
    }
    if (fresh.length > 0) moves += 1
  }

  // What the device's store holds of `names`, read in one transaction over one opened database: the names alone for a
  // prefetch, whose rows the reader does not take; else the rows, every row of the range the names span where they are
  // most of its rows, else each by name. A store that does not open holds nothing.
  const stored = async (lane: Lane, names: readonly string[]): Promise<{ readonly present: readonly string[]; readonly rows: readonly Admitted[] }> => {
    const sorted = [...names].sort()
    const [first, last] = [sorted[0], sorted[sorted.length - 1]]
    if (first === undefined || last === undefined || !idb.available()) return { present: [], rows: [] }
    const wanted = new Set(names)
    const range = IDBKeyRange.bound(first, last)
    const rows = (keys: readonly IDBValidKey[], values: readonly unknown[]): Admitted[] =>
      keys.flatMap((k, i) => {
        const v = values[i]
        return typeof k === "string" && wanted.has(k) && typeof v === "string" ? [admitted({ hash: k, literal: v })] : []
      })
    return idb
      .within(CONTENT, "readonly", async s => {
        const keys = await idb.done(s.getAllKeys(range))
        const present = keys.filter((k): k is string => typeof k === "string" && wanted.has(k))
        if (lane === "Ahead") return { present, rows: [] }
        const read = present.length * 2 >= keys.length ? rows(keys, await idb.done(s.getAll(range))) : rows(present, await Promise.all(present.map(k => idb.done(s.get(k)))))
        return { present: read.map(r => rowOf(r).hash), rows: read }
      })
      .catch(() => ({ present: [], rows: [] }))
  }

  // A prefetch's answer goes to the device's store alone, so the reader holds only what a reading loaded.
  const keep = async (fx: Effects, lane: Lane, rows: readonly Admitted[]): Promise<void> => {
    if (idb.available()) await idb.within(CONTENT, "readwrite", s => Promise.all(rows.map(rowOf).map(r => idb.done(s.put(r.literal, r.hash)))))
    if (lane !== "Ahead") await take(fx, rows)
  }

  // One name asked for: queued in a lane, or in an answer that is out; settled once that answer comes or is lost.
  type Wait = { readonly node: SkeletonNode; readonly done: Promise<void>; readonly come: () => void; readonly fail: (e: unknown) => void }

  const wait = (node: SkeletonNode): Wait => {
    let come = (): void => {}
    let fail = (_: unknown): void => {}
    const done = new Promise<void>((ok, no) => ((come = ok), (fail = no)))
    // Its failure is each fetch's that waits on it; one already failed by another name is not a second failure.
    done.catch(() => {})
    return { node, done, come, fail }
  }

  // The content of the nodes it is given held, what the device keeps read first and the rest asked of the box, at most
  // `fx.atOnce` answers out. Each name is read once, by the first call that names it, whatever its lane: a later call
  // waits on that read or fetch. A name asked in a lane goes before every later lane, taken out of its queue if waiting
  // there. A prefetch is read from the device by name alone, and what the device lacks is asked of the box; its answer
  // goes out only when no other is out and the page is shown, and its first failure drops the rest of the prefetch.
  export const fetcher = (fx: Effects): Fetcher => {
    const waits = new Map<string, Wait>()
    const queues: { [L in Lane]: SkeletonNode[] } = { First: [], Behind: [], Ahead: [] }
    // The names being read from the device, each with the lane it is asked of the box in if the device lacks it.
    const reading = new Map<string, Lane>()
    // The names whose bytes the bar was told of and not yet told came or were lost.
    const told = new Set<string>()
    let out = 0

    const settled = (names: readonly string[], said: (bytes: number) => void): void => {
      const bytes = names.reduce((k, n) => k + (told.has(n) ? (waits.get(n)?.node.size ?? 0) : 0), 0)
      for (const n of names) told.delete(n)
      if (bytes > 0) said(bytes)
    }

    const settle = (names: readonly string[], e: { readonly type: "Came" } | { readonly type: "Failed"; readonly e: unknown }): void => {
      for (const n of names) {
        if (e.type === "Came") waits.get(n)?.come()
        else waits.get(n)?.fail(e.e)
        waits.delete(n)
      }
    }

    // The bar told of each of `nodes` a page waits on that it was not told of yet.
    const tell = (nodes: readonly SkeletonNode[]): void => {
      const unsaid = nodes.filter(n => !told.has(n.hash))
      for (const n of unsaid) told.add(n.hash)
      if (unsaid.length > 0) fx.asked(unsaid.reduce((k, n) => k + n.size, 0))
    }

    const send = async (lane: Lane, sent: readonly SkeletonNode[]): Promise<void> => {
      const names = sent.map(n => n.hash)
      try {
        const rows = await fx.ask(names).then(a => admit(names, a))
        if (rows.type === "Error") throw new Error(rows.e)
        await keep(fx, lane, rows.v)
        settled(names, fx.came)
        settle(names, { type: "Came" })
      } catch (e: unknown) {
        settled(names, fx.lost)
        const dropped = lane === "Ahead" ? queues.Ahead.map(n => n.hash) : []
        queues.Ahead = lane === "Ahead" ? [] : queues.Ahead
        settle([...names, ...dropped], { type: "Failed", e })
      } finally {
        out -= 1
        pump()
      }
    }

    const next = (): Lane | null =>
      queues.First.length > 0 ? "First" : queues.Behind.length > 0 ? "Behind" : queues.Ahead.length > 0 && out === 0 && fx.shown() ? "Ahead" : null

    const pump = (): void => {
      for (let lane = next(); lane !== null && out < fx.atOnce(); lane = next()) {
        const sent = batch(queues[lane], n => n.size).sort(byName)
        const going = new Set(sent.map(n => n.hash))
        queues[lane] = queues[lane].filter(n => !going.has(n.hash))
        out += 1
        void send(lane, sent)
      }
    }

    // One batch read from the device in `lane`: what it holds taken in (a prefetch's only counted), the rest queued for
    // the box in the lane each name is asked in by now.
    const read = async (lane: Lane, nodes: readonly SkeletonNode[]): Promise<void> => {
      const names = nodes.map(n => n.hash)
      try {
        const found = await stored(lane, names)
        await take(fx, found.rows)
        settle(found.present, { type: "Came" })
      } catch (e: unknown) {
        settle(names, { type: "Failed", e })
        for (const n of names) reading.delete(n)
        return
      }
      const lacking = nodes.filter(n => waits.has(n.hash))
      for (const l of LANES) queues[l] = [...queues[l], ...lacking.filter(n => reading.get(n.hash) === l)]
      tell(lacking.filter(n => reading.get(n.hash) !== "Ahead"))
      for (const n of names) reading.delete(n)
      queues.Behind.sort(newest)
      pump()
    }

    const fetch = async (lane: Lane, nodes: readonly SkeletonNode[]): Promise<void> => {
      const lacking = nodes.filter(n => !memory.has(n.hash))
      const fresh = lacking.filter(n => !waits.has(n.hash))
      for (const n of fresh) {
        waits.set(n.hash, wait(n))
        reading.set(n.hash, lane)
      }
      const named = new Set(lacking.map(n => n.hash))
      const later = LANES.slice(LANES.indexOf(lane) + 1)
      for (const n of lacking) if (later.includes(reading.get(n.hash) ?? lane)) reading.set(n.hash, lane)
      const moved = later.flatMap(l => queues[l].filter(n => named.has(n.hash)))
      if (moved.length > 0) {
        for (const l of later) queues[l] = queues[l].filter(n => !named.has(n.hash))
        queues[lane] = [...queues[lane], ...moved]
        if (lane === "Behind") queues.Behind.sort(newest)
      }
      if (lane !== "Ahead") tell(lacking.filter(n => !reading.has(n.hash)))
      pump()
      if (fresh.length > 0) void read(lane, fresh)
      await Promise.all(lacking.flatMap(n => waits.get(n.hash)?.done ?? []))
    }

    return { fetch, wake: pump }
  }

  // Every row no name in `named` holds dropped, on the device, here and by the reader (`letGo`): content of an object
  // the skeleton no longer names is read by nothing.
  export const keepOnly = async (named: ReadonlySet<string>, letGo: (names: readonly string[]) => Promise<void>): Promise<lo.Settled<null>> => {
    const gone = [...memory].filter(n => !named.has(n))
    for (const n of gone) memory.delete(n)
    if (gone.length > 0) {
      moves += 1
      await letGo(gone)
    }
    if (!idb.available()) return lo.val(null)
    return idb
      .within(CONTENT, "readwrite", async s => {
        const keys = await idb.done(s.getAllKeys())
        await Promise.all(keys.flatMap(k => (typeof k === "string" && !named.has(k) ? [idb.done(s.delete(k))] : [])))
        return lo.val(null)
      })
      .catch((e: unknown) => lo.error(lo.reason(e)))
  }
}
export default content
