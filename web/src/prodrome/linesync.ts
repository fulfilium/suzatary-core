// The lines in the core's worker: their row restored from the device, pulled from the box by heads, landed one node
// per row (`linerows`), their content fetched by name into the core's `Lines`, and every reading of them made here once
// each line it reads is held. The page holds none of it: it says when it reads the lines and when the box's stream
// names their heads, and it hears what moved, what a reading left out and how far a pull has come.

import type { Lines } from "./core"
import { deadOf, erasedOf, objectsOf } from "./core"
import content from "./content"
import excluded from "./excluded"
import heads from "./heads"
import keep from "./keep"
import linerows from "./linerows"
import lines from "./lines"
import locked from "./locked"
import { CHAIN, RENDERED_OWED, dropRow, heldRow } from "./objects"
import type progress from "./progress"
import pull from "./pull"
import { epochOf } from "./replica"
import stores from "./stores"
import lo from "../algebra/lo"
import once from "../algebra/once"
import serial from "../algebra/serial"
import idb from "../app/idb"
import type rendered from "../app/rendered"
import http from "../wire/http"
import storereads from "../wire/storereads"
import type { Replica } from "./replica"
import type { SkeletonNode } from "../wire/generated/wire"

export namespace linesync {
  // Which conversations a move touched: those with a line come or gone or a key of their words given or taken, or all of
  // them, when the row was reset or heard a later epoch.
  export type Moved = "All" | readonly string[]

  // What the worker tells the page: a reading's leavings by reader, a pull's progress, that the lines moved, and the
  // rendered blocks it forgot (by name, or all of them), which the page's own table forgets too.
  export type Said =
    | { readonly type: "Told"; readonly reader: string; readonly left: excluded.Left }
    | { readonly type: "Bars"; readonly e: progress.Event }
    | { readonly type: "Moved"; readonly conversations: Moved }
    | { readonly type: "Forgot"; readonly blocks: readonly string[] }
    | { readonly type: "Cleared" }

  export type T = {
    // The page reads the lines: pull now, and after every word of the box's stream from now on.
    readonly want: () => Promise<boolean>
    // The box's stream named the lines' heads: a pull, if a page reads them and the worker lacks one.
    readonly heard: (box: heads.Heads) => void
    // `read` once every line `scope` reads is held, fetching what is not, a conversation's before the whole store's.
    readonly over: <A>(scope: lines.Scope, read: () => A) => Promise<A>
    // Whether the lines hold any line of `id`, once they have landed.
    readonly holds: (id: string) => Promise<boolean>
    // The lines' row written over by a reset and taken again from the box, with whatever the reset could not do.
    readonly reset: () => Promise<readonly string[]>
    // Whether a page is shown, as the page says: a prefetch sends nothing while none is, and sends what it held back
    // once one is again.
    readonly shown: (on: boolean) => void
    // How many lines `scope` reads that the skeleton names and the device does not hold.
    readonly lacking: (scope: lines.Scope) => number
  }

  // Whether a page reads the lines yet: before one does, the worker pulls nothing.
  type Wanted = "No" | "Yes"

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

  export const of = (held: Lines, say: (s: Said) => void, blocks: rendered.Store): T => {
    let row = locked.none<SkeletonNode>()
    const take = (e: { readonly type: "Held"; readonly row: locked.Row<SkeletonNode> } | { readonly type: "Forgot"; readonly keys: readonly string[] }): void => {
      const was = row
      row = stores.taken(row, e)
      if (row === was) return
      // The core opens words with the row's keys: given what changed, so a purge's key is let go now.
      const add = Object.fromEntries(Object.entries(row.keys).filter(([n, k]) => was.keys[n] !== k))
      const drop = Object.keys(was.keys).filter(n => row.keys[n] === undefined)
      if (Object.keys(add).length > 0 || drop.length > 0) held.keys(JSON.stringify(add), JSON.stringify(drop))
      say({ type: "Moved", conversations: touched(was, row, new Set([...Object.keys(add), ...drop])) })
    }
    const told = (reader: string, left: excluded.Left): void => say({ type: "Told", reader, left })
    // What a purge or a retirement takes from the rendered blocks, taken from the page's table as well as the device's.
    // A block forgotten here is gone from the device and marked there before the page hears of it, so no tab's worker,
    // whatever it heard, writes it back.
    const forget = (gone: readonly string[]): Promise<lo.Settled<null>> =>
      blocks.forget(gone).then(done => (say({ type: "Forgot", blocks: gone }), done))
    const clear = (): Promise<lo.Settled<null>> => blocks.clear().then(done => (say({ type: "Cleared" }), done))
    const bars = (e: progress.Event): void => say({ type: "Bars", e })

    // Whether a page is shown; none is until one says so.
    let shown = false
    // The content, fetched by name and held by the core as it comes.
    const fetcher = content.fetcher({
      ask: hashes => storereads.content("lines", hashes),
      asked: bytes => bars({ type: "Asked", name: "lines", bytes }),
      came: bytes => bars({ type: "Came", name: "lines", bytes }),
      lost: bytes => bars({ type: "Lost", name: "lines", bytes }),
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
    // The plan's content the device lacks, fetched behind every reading's fetch, one answer at a time.
    const prefetch = serial.of(async (): Promise<void> => {
      if (!idb.available()) return
      const skeleton = row.replica
      const [plan, stored] = await Promise.all([planOf(skeleton), content.stored()])
      await fetching("Ahead", plan.filter(n => !stored.has(n.hash))).catch(() => {
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
        () => told(`row/${PLAIN}`, excluded.NOTHING),
        (e: unknown) =>
          told(
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
        () => told(`row/${RENDERED_OWED}`, excluded.NOTHING),
        (e: unknown) =>
          told(
            `row/${RENDERED_OWED}`,
            excluded.left(excluded.NONE, [], [{ chain: RENDERED_OWED, why: `the blocks a build before this one rendered are still on this device (${lo.reason(e)}); it tries again at the next load`, cure: "None" }]),
          ),
      )

    const store: pull.Store<SkeletonNode, locked.Row<SkeletonNode>> = {
      name: "lines",
      ...pull.locks<SkeletonNode>({
        name: "lines",
        held: () => row,
        hold: r => take({ type: "Held", row: r }),
        forgot: keys => take({ type: "Forgot", keys }),
        // The core reads the content held, the largest down-set of the skeleton it has: a word was only ever rendered
        // from a conversation read whole.
        dead: () => Promise.resolve(new Set(deadOf(held))),
        // The blocks a key's words fed are found by opening its lines, so their content is fetched first; a fetch that
        // fails rejects, and the keys stay owed.
        erased: async (r, erased) => {
          await fetching("First", lines.naming(r.replica, locked.names(r.keys)))
          return erasedOf(held, r.keys, erased)
        },
        forget,
        alter: linerows.place.forget,
        told,
      }),
      held: () => row,
      place: linerows.place,
      ask: since =>
        storereads.skeleton(since).then(a => ({
          said: { ...a, objects: a.nodes, untrusted: [], root: null },
          erased: a.erased,
          piece: r => locked.without(locked.of(r, a.keys), a.erased),
          rest: a.rest,
          size: a.size,
          weight: a.weight,
        })),
      landed: async r => {
        if (r.replica.objects.length > 0) await retire()
        const gone = await content.keepOnly(await keeping(r.replica), drop => Promise.resolve(held.hold("[]", JSON.stringify(drop))))
        if (gone.type === "Error") told(`row/${pull.kept("lines")}`, pull.unread("lines", gone.e))
        // Only once a page reads the lines: a page that reads no conversation prefetches nothing.
        if (wanted === "Yes") prefetched(r.replica)
      },
      restoring: cleared,
    }

    let wanted: Wanted = "No"
    // Local-first: lines this device landed answer at once and are pulled behind; lines it never landed are waited for
    // until they are whole, so a reading never reads part of them. A later epoch forgets the wait it replaced.
    const ready = once.of(() =>
      sync.restored.get().then(() => {
        void sync.pull()
        return sync.landed() ? undefined : sync.whole()
      }),
    )
    const sync = pull.of(store, {
      told,
      bars,
      replaced: () => ready.forget(),
      again: () => void (wanted === "Yes" ? sync.pull() : undefined),
    })

    const over = async <A>(scope: lines.Scope, read: () => A): Promise<A> => {
      await ready.get()
      for (;;) {
        const need = lines.need(row.replica, content.has, scope)
        if (need.type === "Whole") return read()
        await fetching(scope.type === "Whole" ? "Behind" : "First", need.nodes)
      }
    }

    return {
      want: () => {
        wanted = "Yes"
        return sync.pull()
      },
      heard: box => {
        if (wanted === "Yes" && stores.behind(box, sync.holding())) void sync.pull()
      },
      over,
      holds: id => ready.get().then(() => lines.holds(row.replica, id)),
      reset: async () => {
        const said: string[] = []
        await sync.reset(why => said.push(why))
        await sync.restored.get()
        await sync.pull()
        return said
      },
      shown: on => {
        shown = on
        if (on) fetcher.wake()
      },
      lacking: scope => {
        const need = lines.need(row.replica, content.holds, scope)
        return need.type === "Fetching" ? need.nodes.filter(n => n.header.node === "line").length : 0
      },
    }
  }
}
export default linesync
