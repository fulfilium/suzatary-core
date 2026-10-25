// The secretary's stores as replicas this tab holds. The box alone writes the inbox and the lines, so a replica is a
// down-set of the box's objects, and two of one store join as replicas join. The lines alone lock words: their row holds
// the keys that open them beside the replica, and joins as `locked` joins. The page holds the inbox and the two topic
// stores, each its replica alone, the box taking a tab's topics by `POST`; the core's worker holds the lines' skeleton
// (`linesync`), each line's content kept apart by its name.

import heads from "./heads"
import locked from "./locked"
import type { Box, Frontier, Obj, Replica } from "./replica"
import { NOTHING, join, said } from "./replica"
import type { RefusedObject, SkeletonNode, StoreName, StoredObject } from "../wire/generated/wire"

export namespace stores {
  export type Name = StoreName
  export const NAMES: readonly Name[] = ["inbox", "lines", "topics", "topic-aliases"]

  // What each store's replica holds.
  export type Of = { readonly inbox: StoredObject; readonly lines: SkeletonNode; readonly topics: StoredObject; readonly "topic-aliases": StoredObject }

  // The stores the page holds.
  export type Paged = Exclude<Name, "lines">
  export const PAGED: readonly Paged[] = ["inbox", "topics", "topic-aliases"]

  // The page's stores lock no words, so each is its replica alone.
  export type T = { readonly [N in Paged]: Replica<Of[N]> }
  // A replica taken in, joined.
  export type Event = { readonly type: "Held"; readonly name: Paged; readonly replica: Replica<StoredObject> }

  export const NONE: T = { inbox: NOTHING, topics: NOTHING, "topic-aliases": NOTHING }

  // What a reading or a pull can see: the generation, the objects, what the box is known to hold and refused, the box's
  // heads, the policy they are read under, the frontier the next pull asks from and the keys, never the instant they
  // were heard. A reset of an empty row holds what it held, and is still a step; so is a pull that moved only the
  // frontier, which the next pull must ask from. The objects only grow within a generation, so their count says whether
  // they moved; what the box holds and refused are compared whole, since a refusal can replace another and keep the count.
  export const same = <O extends Obj>(a: Replica<O>, b: Replica<O>): boolean =>
    a.generation === b.generation &&
    a.objects.length === b.objects.length &&
    sameSet(a.synced, b.synced) &&
    sameSet(a.refused.map(refusal), b.refused.map(refusal)) &&
    sameBox(a.box, b.box) &&
    sameFrontier(a.held.frontier, b.held.frontier)

  const refusal = (r: RefusedObject): string => JSON.stringify([r.hash, r.kind, r.reason])

  const sameFrontier = (a: Frontier, b: Frontier): boolean =>
    a.type === "Named" && b.type === "Named" ? a.digest === b.digest && a.era === b.era : a.type === b.type

  // Two words are one when both are unheard, or both name the same heads under the same policy.
  const sameBox = (a: Box, b: Box): boolean =>
    a.type === "Heard" && b.type === "Heard"
      ? heads.same(heads.of(a.word.heads), heads.of(b.word.heads)) && sameSet(a.word.untrusted, b.word.untrusted)
      : a.type === b.type

  // Two lists of names are one set when they hold the same names, in any order and however often: two policies, or what
  // the box holds.
  export const sameSet = (a: readonly string[], b: readonly string[]): boolean => {
    const x = new Set(a), y = new Set(b)
    return x.size === y.size && [...x].every(n => y.has(n))
  }

  const moved = <O extends Obj>(was: locked.Row<O>, next: locked.Row<O>): locked.Row<O> =>
    next === was || (same(next.replica, was.replica) && locked.sameKeys(next.keys, was.keys)) ? was : next

  // The row `e` leaves of `was`, the store it names: `was` itself when nothing moved.
  export const taken = <O extends Obj>(was: locked.Row<O>, e: { readonly type: "Held"; readonly row: locked.Row<O> } | { readonly type: "Forgot"; readonly keys: readonly string[] }): locked.Row<O> =>
    moved(was, e.type === "Held" ? locked.join(was, e.row) : locked.without(was, e.keys))

  // A replica joined into `was`: `was` itself when nothing moved.
  const joined = <O extends Obj>(was: Replica<O>, incoming: Replica<O>): Replica<O> => {
    const next = join(was, incoming)
    return next === was || same(next, was) ? was : next
  }

  export const step = (t: T, e: Event): T => {
    const next = joined(t[e.name], e.replica)
    return next === t[e.name] ? t : { ...t, [e.name]: next }
  }


  // Whether the box answered in a later epoch than a word any part of what is held heard (the row pages read, or the
  // pieces kept beside it): its store no longer extends what it served (a migration rewrote objects, or it went back
  // to an earlier checkpoint), so what is held may name objects the box no longer holds, and the store, a copy of the
  // box's alone, starts over and is taken again whole. Each part is asked, not their join: a piece of the later epoch
  // another tab kept beside a row of the epoch before joins to the piece alone, which hides the row it overtook.
  export const overtaken = (held: readonly Replica<Obj>[], answer: { readonly epoch: number }): boolean =>
    held.some(r => said(r.box, false, w => w.epoch < answer.epoch))

  // Whether the box's heads name an object the tab does not hold, landed or not: a move of the store, or a pull that was
  // cut before it took everything. A frame that says so pulls; one that does not pulls nothing.
  export const behind = <O extends Obj>(box: heads.Heads, held: Replica<O>): boolean => {
    const names = new Set(held.objects.map(o => o.hash))
    return box.some(h => !names.has(h))
  }
}
export default stores
