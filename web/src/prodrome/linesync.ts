// The lines as one store of the core's worker (`engine`): nodes that name their content, fetched by name into the
// core's `Lines`, which reads them, and the keys of their words beside them. What is the lines' own: a move names the
// conversations it touched, the device keeps the content of the newest conversations within a budget and fetches it
// ahead of any page, and a reading reads a conversation's lines and what they rest on.

import type { Lines } from "./core"
import { deadOf, erasedOf, nodesInto, objectsOf } from "./core"
import content from "./content"
import engine from "./engine"
import excluded from "./excluded"
import keep from "./keep"
import lines from "./lines"
import locked from "./locked"
import noderows from "./noderows"
import { CHAIN, LINE_ROWS, RENDERED_OWED, dropRow, heldRow } from "./objects"
import type progress from "./progress"
import pull from "./pull"
import { epochOf } from "./replica"
import type { Replica } from "./replica"
import lo from "../algebra/lo"
import once from "../algebra/once"
import serial from "../algebra/serial"
import idb from "../app/idb"
import type rendered from "../app/rendered"
import http from "../wire/http"
import payload from "../wire/generated/payload"
import storereads from "../wire/storereads"
import type { SkeletonNode } from "../wire/generated/wire"

export namespace linesync {
  // Which conversations a move touched: those with a line come or gone or a key of their words given or taken, or all of
  // them, when the row was reset or heard a later epoch.
  export type Moved = "All" | readonly string[]

  // Who hears the lines: a reading's leavings by reader, a pull's progress, a move, and the rendered blocks forgotten
  // (by name, or all of them).
  export type Effects = {
    readonly told: (reader: string, left: excluded.Left) => void
    readonly bars: (e: progress.Event) => void
    readonly moved: (conversations: Moved) => void
    readonly forgot: (blocks: readonly string[]) => void
    readonly cleared: () => void
    readonly replaced: () => void
  }

  export type T = {
    readonly engine: engine.T<SkeletonNode>
    // `read` once every line `scope` reads is held, a conversation's fetched before the whole store's. The scope is
    // asked again after each fetch, of the skeleton then held.
    readonly over: <A>(scope: () => lines.Scope, read: () => A) => Promise<A>
    // Whether the lines hold any line of `id`.
    readonly holds: (id: string) => boolean
    // Whether a page is shown, as the page says: a prefetch sends nothing while none is, and sends what it held back
    // once one is again.
    readonly shown: (on: boolean) => void
    // How many lines `scope` reads that the skeleton names and the device does not hold.
    readonly lacking: (scope: lines.Scope) => number
  }

  // The conversations a move from `was` to `now` touched, `keys` the names of the keys given or taken.
  export const touched = (was: locked.Row<SkeletonNode>, now: locked.Row<SkeletonNode>, keys: ReadonlySet<string>): Moved => {
    if (was.replica.generation !== now.replica.generation || epochOf(was.replica.held) !== epochOf(now.replica.held)) return "All"
    const before = new Set(was.replica.objects.map(n => n.hash))
    const after = new Set(now.replica.objects.map(n => n.hash))
    const out = new Set<string>()
    const of = (n: SkeletonNode, moved: boolean): void => {
      if (n.header.node === "line" && (moved || n.header.keys.some(k => keys.has(k)))) out.add(n.header.conversation)
    }
    for (const n of now.replica.objects) of(n, !before.has(n.hash))
    for (const n of was.replica.objects) if (!after.has(n.hash)) of(n, true)
    return [...out].sort()
  }

  // The form of the lines' nodes on the device: a change of the skeleton's node moves it, and the lines are taken again.
  export const FORMAT = 1

  export const of = (held: Lines, fx: Effects, blocks: rendered.Store): T => {
    // The core opens words with the row's keys and reads the row's skeleton: told what changed, so a purge's key is let
    // go now, and the stream chooses its rounds by the row it reads.
    const took = (was: locked.Row<SkeletonNode>, row: locked.Row<SkeletonNode>): void => {
      const add = Object.fromEntries(Object.entries(row.keys).filter(([n, k]) => was.keys[n] !== k))
      const drop = Object.keys(was.keys).filter(n => row.keys[n] === undefined)
      if (Object.keys(add).length > 0 || drop.length > 0) held.keys(JSON.stringify(add), JSON.stringify(drop))
      if (was.replica.objects !== row.replica.objects) {
        const before = new Set(was.replica.objects.map(n => n.hash))
        const after = new Set(row.replica.objects.map(n => n.hash))
        const came = row.replica.objects.filter(n => !before.has(n.hash))
        const went = was.replica.objects.flatMap(n => (after.has(n.hash) ? [] : [n.hash]))
        if (came.length > 0 || went.length > 0) nodesInto(held, came, went)
      }
      fx.moved(touched(was, row, new Set([...Object.keys(add), ...drop])))
    }
    // What a purge or a retirement takes from the rendered blocks, taken from the page's table as well as the device's.
    // A block forgotten here is gone from the device and marked there before the page hears of it, so no tab's worker,
    // whatever it heard, writes it back.
    const forget = (gone: readonly string[]): Promise<lo.Settled<null>> => blocks.forget(gone).then(done => (fx.forgot(gone), done))
    const clear = (): Promise<lo.Settled<null>> => blocks.clear().then(done => (fx.cleared(), done))
    // The device's rows of the lines, one node per row. Rows of another form go with their keys, so the blocks those
    // keys could find are cleared first.
    const place = noderows.placeOf(LINE_ROWS, payload.SkeletonNode, { version: FORMAT, dropped: clear })

    // Whether a page is shown; none is until one says so.
    let shown = false
    // The content, fetched by name and held by the core as it comes.
    const fetcher = content.fetcher({
      ask: hashes => storereads.content("lines", hashes),
      asked: bytes => fx.bars({ type: "Asked", name: "lines", bytes }),
      came: bytes => fx.bars({ type: "Came", name: "lines", bytes }),
      lost: bytes => fx.bars({ type: "Lost", name: "lines", bytes }),
      atOnce: () => content.atOnce(http.protocol()),
      held: rows => Promise.resolve(held.hold(objectsOf(rows), "[]")),
      shown: () => shown,
    })
    const fetching = fetcher.fetch

    // The device's budget for the lines' content, read once from the origin's quota.
    const budget = once.of(async (): Promise<number> => {
      const s = typeof navigator === "undefined" ? undefined : navigator.storage
      const quota = s === undefined || typeof s.estimate !== "function" ? undefined : (await s.estimate().catch(() => undefined))?.quota
      return keep.budget(quota === undefined ? { type: "Unknown" } : { type: "Known", bytes: quota })
    }).get

    // The plan of a skeleton, made once for its landing's eviction and its prefetch.
    let planned: { readonly skeleton: Replica<SkeletonNode>; readonly nodes: readonly SkeletonNode[] } | null = null
    const planOf = async (skeleton: Replica<SkeletonNode>): Promise<readonly SkeletonNode[]> => {
      const most = await budget()
      if (planned?.skeleton === skeleton) return planned.nodes
      planned = { skeleton, nodes: keep.plan(skeleton, most) }
      return planned.nodes
    }

    // What the device keeps of the lines' content: the plan within the budget, and what a reading read, so a reading
    // that fetched a line is not left fetching it again.
    const keeping = async (skeleton: Replica<SkeletonNode>): Promise<ReadonlySet<string>> => {
      const names = new Set((await planOf(skeleton)).map(n => n.hash))
      const read = content.read()
      for (const n of skeleton.objects) if (read.has(n.hash)) names.add(n.hash)
      return names
    }

    // The skeleton last prefetched, so each is prefetched once, and again after a prefetch that failed.
    let ahead: Replica<SkeletonNode> | null = null
    // The first reading's end, which every prefetch waits for: a prefetch reads the device's every name, and the first
    // draw reads before it.
    let ended = (): void => {}
    const firstRead = new Promise<void>(done => (ended = done))
    // The plan's content the device lacks, fetched behind every reading's fetch, one answer at a time.
    const prefetch = serial.of(async (): Promise<void> => {
      await firstRead
      if (!idb.available()) return
      const skeleton = store.row().replica
      await fetching("Ahead", await planOf(skeleton)).catch(() => {
        ahead = null
      })
    })
    const prefetched = (skeleton: Replica<SkeletonNode>): void => {
      if (ahead === skeleton) return
      ahead = skeleton
      void prefetch()
    }

    // The plaintext conversations a build before the lines kept: deleted once the lines' row holds what replaces them,
    // under a reset's rule, the blocks they rendered cleared first, since nothing names which of those words went since.
    const PLAIN = "store/conversations"
    const retired = once.of(async (): Promise<void> => {
      const was = await heldRow(CHAIN, PLAIN)
      if (was.type === "Absent") return
      const cleared = await clear()
      if (cleared.type === "Error") throw new Error(cleared.e)
      const gone = await dropRow(CHAIN, PLAIN, new Set())
      if (gone.type === "Error") throw new Error(gone.e)
    })
    // A retirement that failed is said, and tried again at the next pull of the lines.
    const retire = (): Promise<void> =>
      retired.get().then(
        () => fx.told(`row/${PLAIN}`, excluded.NOTHING),
        (e: unknown) =>
          fx.told(
            `row/${PLAIN}`,
            excluded.left(excluded.NONE, [], [{ chain: PLAIN, why: `the conversations a build before the lines kept are still on this device (${lo.reason(e)}); it tries again at the next sync`, cure: "None" }]),
          ),
      )

    // The blocks a build before version 5 rendered, cleared once after the upgrade that dropped the lines they were
    // rendered from, since no purge can find them by their words any more; a failure is said and tried again.
    const owedClear = once.of(async (): Promise<void> => {
      const owed = await heldRow({ parse: raw => (raw === true ? lo.val(true) : lo.error("not the upgrade's mark")) }, RENDERED_OWED)
      if (owed.type === "Absent") return
      const done = await clear()
      if (done.type === "Error") throw new Error(done.e)
      const gone = await dropRow(CHAIN, RENDERED_OWED, new Set())
      if (gone.type === "Error") throw new Error(gone.e)
    })
    const cleared = (): Promise<void> =>
      owedClear.get().then(
        () => fx.told(`row/${RENDERED_OWED}`, excluded.NOTHING),
        (e: unknown) =>
          fx.told(
            `row/${RENDERED_OWED}`,
            excluded.left(excluded.NONE, [], [{ chain: RENDERED_OWED, why: `the blocks a build before this one rendered are still on this device (${lo.reason(e)}); it tries again at the next load`, cure: "None" }]),
          ),
      )

    // Every key the box said a reseal that opens on it replaced: the box destroys them, and so does the purge.
    const replaced = new Set<string>()
    const store: engine.T<SkeletonNode> = engine.of<SkeletonNode>(
      {
        name: "lines",
        place,
        ask: since =>
          storereads.skeleton(since).then(a => {
            for (const k of a.replaced) replaced.add(k)
            return {
              said: { ...a, objects: a.nodes, untrusted: [], root: null },
              erased: a.erased,
              piece: r => locked.without(locked.of(r, a.keys), [...a.erased, ...a.replaced]),
              rest: a.rest,
              size: a.size,
              weight: a.weight,
            }
          }),
        content: { type: "ByName", has: content.has, holds: content.holds, fetch: fetching, version: content.version, ended: () => ended() },
        words: {
          // The core reads the content held, the largest down-set of the skeleton it has: a word was only ever rendered
          // from a conversation read whole. Beside it, the keys the box says a reseal replaced.
          dead: () => Promise.resolve(new Set([...deadOf(held), ...replaced])),
          // The blocks a key's words fed are found by opening its lines, so their content is fetched first; a fetch
          // that fails rejects, and the keys stay owed. A replaced key's old words may have fed blocks under the older
          // rules, so it is purged as an erased one is: its blocks forgotten, then the key.
          erased: async (r, erased) => {
            await fetching("First", lines.naming(r.replica, locked.names(r.keys)))
            return erasedOf(held, r.keys, [...erased, ...replaced])
          },
          forget,
        },
        took,
        restoring: cleared,
        landed: async r => {
          if (r.replica.objects.length > 0) await retire()
          const gone = await content.keepOnly(await keeping(r.replica), drop => Promise.resolve(held.hold("[]", JSON.stringify(drop))))
          if (gone.type === "Error") fx.told(`row/${pull.kept("lines")}`, pull.unread("lines", gone.e))
          // Only once a page reads the lines: a page that reads no conversation prefetches nothing.
          if (store.wanted()) prefetched(r.replica)
        },
      },
      { told: fx.told, bars: e => fx.bars({ ...e, name: "lines" }), replaced: fx.replaced },
    )

    return {
      engine: store,
      over: (scope, read) => store.over(scope().type === "Whole" ? "Behind" : "First", () => lines.nodesOf(store.row().replica, scope()), read),
      holds: id => lines.holds(store.row().replica, id),
      shown: on => {
        shown = on
        if (on) fetcher.wake()
      },
      lacking: scope => store.lacking(lines.nodesOf(store.row().replica, scope)).filter(n => n.header.node === "line").length,
    }
  }
}
export default linesync
