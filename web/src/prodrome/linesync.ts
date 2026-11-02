// The lines as one store of the core's worker (`engine`): nodes that name their content, fetched by name into the
// core's `Lines`, which reads them, and the keys of their words beside them. What is the lines' own: a move names the
// conversations it touched, the device keeps the content of the newest conversations within a budget and fetches it
// ahead of any page, and a reading reads a conversation's lines and what they rest on.

import type { Lines } from "./core"
import { deadOf, erasedOf, heldOf, holdAdmitted, keysInto, lackingIn, lackingOf, lacksIn, nodesInto, planOf, retiredOf } from "./core"
import content from "./content"
import engine from "./engine"
import excluded from "./excluded"
import keep from "./keep"
import lines from "./lines"
import locked from "./locked"
import noderows from "./noderows"
import { CHAIN, LINE_ROWS, RENDERED_OWED, dropRow, heldRow } from "./objects"
import phases from "./phases"
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

  // The form of the lines' nodes on the device: a change of the skeleton's node moves it, and the lines are taken again.
  // 2: a line's step names a conversation's title lines, the slot each reseal seals and the keys each erasure names,
  // which a round's lines are found by; a node of form 1 reads them as `still`, so the reseals and erasures go unseen.
  // 3: a reseal's step also names the seal it was sealed again from, which the dead keys are read from.
  // 4: the box's own note takes a step of its own, `noted`, and the stream leaves out a round of nothing else; a node of
  // form 3 reads it as `asks`, so the box's notes would be drawn as rounds.
  export const FORMAT = 4

  // How the core is told the lines' row: `took` a move of it, `printed` its snapshot beside the row it holds, and
  // `reader` how a warm load loads it from a snapshot. The core holds the row it was last told, or the snapshot it was
  // loaded from until the rows are read: never less than a core handed that row whole, whatever came between.
  export type Telling = {
    readonly took: (was: locked.Row<SkeletonNode>, now: locked.Row<SkeletonNode>) => void
    // Those of `nodes` the core was not told: every one while it holds a snapshot no row was named for.
    readonly untold: (nodes: readonly SkeletonNode[]) => readonly SkeletonNode[]
    readonly printed: () => noderows.Printed<SkeletonNode>
    readonly reader: pull.Reader<SkeletonNode>
  }

  export const telling = (held: Lines, moved: (conversations: Moved) => void): Telling => {
    type Told = { readonly type: "Row"; readonly row: locked.Row<SkeletonNode> } | { readonly type: "Snapshot" }
    const NOTHING: Told = { type: "Row", row: locked.none() }
    let told: Told = NOTHING
    // The core opens words with the row's keys and reads the row's skeleton: told what changed since what it holds, so a
    // purge's key is let go now, and the stream chooses its rounds by the row it reads. A core that holds a snapshot no
    // row was named for holds no node or key again and is told the row whole. The row is what it holds only once it took
    // the change, so a change it refused is told again with the next.
    const took = (_: locked.Row<SkeletonNode>, row: locked.Row<SkeletonNode>): void => {
      if (told.type === "Snapshot") {
        held.clear()
        told = NOTHING
      }
      const was = told.row
      // The conversations a line of which came or went, then those whose lines name a key given or taken, as the core
      // answers each change; every conversation where the row was reset or heard a later epoch.
      const touched = new Set<string>()
      if (was.replica.objects !== row.replica.objects) {
        const before = new Set(was.replica.objects.map(n => n.hash))
        const after = new Set(row.replica.objects.map(n => n.hash))
        const came = row.replica.objects.filter(n => !before.has(n.hash))
        const went = was.replica.objects.flatMap(n => (after.has(n.hash) ? [] : [n.hash]))
        if (came.length > 0 || went.length > 0) for (const id of nodesInto(held, came, went)) touched.add(id)
        phases.mark("lines nodes held")
      }
      const add = Object.fromEntries(Object.entries(row.keys).filter(([n, k]) => was.keys[n] !== k))
      const drop = Object.keys(was.keys).filter(n => row.keys[n] === undefined)
      if (Object.keys(add).length > 0 || drop.length > 0) for (const id of keysInto(held, add, drop)) touched.add(id)
      phases.mark("lines keys held")
      told = { type: "Row", row }
      const anew = was.replica.generation !== row.replica.generation || epochOf(was.replica.held) !== epochOf(row.replica.held)
      moved(anew ? "All" : [...touched].sort())
    }
    // Beside the rows, the core's snapshot of the row it was told; none while it holds a snapshot no row was named for,
    // or where it does not print.
    const printed = (): noderows.Printed<SkeletonNode> => {
      if (told.type !== "Row") return { type: "Other" }
      try {
        return { type: "Of", row: told.row, bytes: held.snapshot() }
      } catch {
        return { type: "Other" }
      }
    }
    // A warm load's core loaded from the snapshot, and told, once the rows are read, whether they are the rows it was of.
    const reader: pull.Reader<SkeletonNode> = {
      type: "Snapshot",
      load: bytes => {
        try {
          held.load(bytes)
        } catch {
          return false
        }
        told = { type: "Snapshot" }
        return true
      },
      holds: row => void (told = { type: "Row", row }),
      lost: () => {
        held.clear()
        told = NOTHING
      },
    }
    const untold = (nodes: readonly SkeletonNode[]): readonly SkeletonNode[] => {
      if (told.type !== "Row") return nodes
      const known = new Set(told.row.replica.objects.map(n => n.hash))
      return nodes.filter(n => !known.has(n.hash))
    }
    return { took, printed, reader, untold }
  }

  export const of = (held: Lines, fx: Effects, blocks: rendered.Store): T => {
    const { took, printed, reader, untold } = telling(held, fx.moved)
    // What a purge or a retirement takes from the rendered blocks, taken from the page's table as well as the device's.
    // A block forgotten here is gone from the device and marked there before the page hears of it, so no tab's worker,
    // whatever it heard, writes it back.
    const forget = (gone: readonly string[]): Promise<lo.Settled<null>> => blocks.forget(gone).then(done => (fx.forgot(gone), done))
    const clear = (): Promise<lo.Settled<null>> => blocks.clear().then(done => (fx.cleared(), done))
    // The device's rows of the lines, one node per row, and the core's snapshot beside them. Rows of another form go
    // with their keys, so the blocks those keys could find are cleared first.
    const place = noderows.placeOf(LINE_ROWS, payload.SkeletonNode, { version: FORMAT, dropped: clear }, { type: "Snapshot", print: printed })

    // Whether a page is shown; none is until one says so.
    let shown = false
    // The content, fetched by name and held by the core as it comes.
    const fetcher = content.fetcher({
      ask: hashes => storereads.content("lines", hashes),
      asked: bytes => fx.bars({ type: "Asked", name: "lines", bytes }),
      came: bytes => fx.bars({ type: "Came", name: "lines", bytes }),
      lost: bytes => fx.bars({ type: "Lost", name: "lines", bytes }),
      atOnce: () => content.atOnce(http.protocol()),
      lacking: names => lackingOf(held, names),
      held: rows => Promise.resolve(holdAdmitted(held, rows)),
      shown: () => shown,
    })
    const fetching = fetcher.fetch

    // The device's budget for the lines' content, read once from the origin's quota.
    const budget = once.of(async (): Promise<number> => {
      const s = typeof navigator === "undefined" ? undefined : navigator.storage
      const quota = s === undefined || typeof s.estimate !== "function" ? undefined : (await s.estimate().catch(() => undefined))?.quota
      return keep.budget(quota === undefined ? { type: "Unknown" } : { type: "Known", bytes: quota })
    }).get

    // The core's plan of the skeleton it holds, within the budget: what the device keeps of the lines' content (the plan,
    // and every print the core holds, which a reading loaded, since a prefetch hands the core nothing), and what of the
    // plan a prefetch fetches.
    const planned = async (): Promise<ReturnType<typeof planOf>> => planOf(held, await budget())

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
      await fetching("Ahead", (await planned()).ahead).catch(() => {
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

    const store: engine.T<SkeletonNode> = engine.of<SkeletonNode>(
      {
        name: "lines",
        place,
        reader,
        ask: since => {
          // A pull that lacks nothing is not sent the heads again: they are those of what this tab holds.
          const mine = store.sync.holding().heads
          return storereads.skeleton(since).then(a => {
            const row = store.row()
            return {
              said: { ...a, heads: a.heads.are === "held" ? mine : a.heads.heads, objects: a.nodes, untrusted: [], root: null },
              // What this piece's nodes retire beside the skeleton the core holds, given the keys it and the row hold,
              // purged before the pull goes on.
              erased: [...retiredOf(held, untold(a.nodes), [...new Set([...Object.keys(a.keys), ...Object.keys(row.keys)])])],
              piece: r => locked.of(r, a.keys),
              rest: a.rest,
              size: a.size,
              weight: a.weight,
            }
          })
        },
        content: { type: "ByName", lacking: names => lackingOf(held, names), fetch: fetching, version: () => held.version(), ended: () => ended() },
        words: {
          // The keys the row's nodes retire, given the keys the row holds, read by the core over its skeleton and the
          // row's nodes it was not told yet; and the keys the content held names as dead: the core reads the largest
          // down-set of the skeleton whose content it has. The nodes name every erasure and reseal, so a key a round's
          // lines held alone retire is found here too.
          dead: r => Promise.resolve(new Set([...deadOf(held), ...retiredOf(held, untold(r.replica.objects), locked.names(r.keys))])),
          // The blocks a key's words fed are found by opening its lines, so their content is fetched first; a fetch
          // that fails rejects, and the keys stay owed. Every owed key is dead, a replaced one too: its old words may
          // have fed blocks under the older rules, so its blocks are forgotten before the key, as an erased key's are.
          erased: async r => {
            await fetching("First", lackingIn(held, { type: "Naming", keys: locked.names(r.keys) }, untold(r.replica.objects)))
            return erasedOf(held, r.keys, locked.names(r.keys))
          },
          forget,
        },
        took,
        restoring: cleared,
        landed: async r => {
          if (r.replica.objects.length > 0) await retire()
          const gone = await content.keepOnly(new Set((await planned()).keep), heldOf(held), drop => Promise.resolve(held.hold("[]", JSON.stringify(drop))))
          if (gone.type === "Error") fx.told(`row/${pull.kept("lines")}`, pull.unread("lines", gone.e))
          // Only once a page reads the lines: a page that reads no conversation prefetches nothing.
          if (store.wanted()) prefetched(r.replica)
        },
      },
      { told: fx.told, bars: e => fx.bars({ ...e, name: "lines" }), replaced: fx.replaced },
    )

    return {
      engine: store,
      over: (scope, read) => store.over(scope().type === "Whole" ? "Behind" : "First", () => lackingIn(held, scope()), read),
      holds: id => held.holds(id),
      // A page hidden may be closed: the snapshot owed is written now.
      shown: on => {
        shown = on
        if (on) fetcher.wake()
        else void place.settle()
      },
      lacking: scope => lacksIn(held, scope).lines,
    }
  }
}
export default linesync
