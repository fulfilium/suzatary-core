// A store's row: the replica of its objects and the keys of their words, joined as a pair of unions, the free
// semilattice on both, within one generation and one epoch of the box's word, over objects or a skeleton's nodes. A key its objects name as erased opens nothing (the core reads that slot as erased whatever key
// is given) and is OWED: held only until the blocks its words were rendered to are forgotten, since nothing else could
// find them, and then dropped.

import type { Obj, Replica } from "./replica"
import type { StoredObject } from "../wire/generated/wire"
import { empty, epochOf, join, reset, same } from "./replica"

// The replicas' own, under names the row's join and reset do not shadow.
const joined = join
const emptied = reset

export namespace locked {
  // A key's bytes in hex, by its name: the name is the bytes' SHA-256, so one name holds one key.
  export type Keys = Readonly<Record<string, string>>
  export type Row<O extends Obj = StoredObject> = { readonly replica: Replica<O>; readonly keys: Keys }
  // The keys a replica's objects name as erased, as the core's worker reads them: monotone in the objects.
  export type Dead<O extends Obj = StoredObject> = (r: Replica<O>) => Promise<ReadonlySet<string>>

  export const NO_KEYS: Keys = {}
  export const none = <O extends Obj>(): Row<O> => ({ replica: empty<O>(), keys: NO_KEYS })
  export const NONE: Row = none()

  export const of = <O extends Obj>(replica: Replica<O>, keys: Keys = NO_KEYS): Row<O> => ({ replica, keys })

  export const names = (k: Keys): string[] => Object.keys(k).sort()

  export const sameKeys = (a: Keys, b: Keys): boolean => {
    const x = Object.keys(a)
    return a === b || (x.length === Object.keys(b).length && x.every(n => a[n] === b[n]))
  }

  // The union, a name spelled twice keeping its least spelling, so either order holds the same keys; `a` itself when `b`
  // adds nothing, in time linear in `b`.
  export const union = (a: Keys, b: Keys): Keys => {
    let out: Record<string, string> | null = null
    for (const [n, y] of Object.entries(b)) {
      const x = (out ?? a)[n]
      if (x !== undefined && x <= y) continue
      out ??= { ...a }
      out[n] = y
    }
    return out ?? a
  }

  const epoch = <O extends Obj>(r: Row<O>): number => epochOf(r.replica.held)

  // A semilattice with `NONE` its unit, ordered by generation and then by the epoch of the box's word: the later row is
  // kept whole, so the join that brings in a later epoch's word is the one that drops every object and key of the
  // epochs before, and no reader holds the word beside them. A store is a copy of the box's alone, so nothing of this
  // device's own goes with them. Within one generation and epoch, both sides united. Nothing new is `a` itself.
  export const join = <O extends Obj>(a: Row<O>, b: Row<O>): Row<O> => {
    if (a.replica.generation === b.replica.generation && epoch(a) !== epoch(b)) return epoch(a) > epoch(b) ? a : b
    const replica = joined(a.replica, b.replica)
    const keys = a.replica.generation === b.replica.generation ? union(a.keys, b.keys) : replica === a.replica ? a.keys : b.keys
    return replica === a.replica && sameKeys(keys, a.keys) ? a : replica === b.replica && sameKeys(keys, b.keys) ? b : of(replica, keys)
  }

  // The keys the row holds that its objects name as erased.
  export const owed = async <O extends Obj>(dead: Dead<O>, r: Row<O>): Promise<string[]> => {
    const gone = await dead(r.replica)
    return gone.size === 0 ? [] : Object.keys(r.keys).filter(n => gone.has(n)).sort()
  }

  // The row less `gone`: what a purge leaves once their blocks are forgotten, the one step down.
  export const without = <O extends Obj>(r: Row<O>, gone: readonly string[]): Row<O> => {
    const drop = new Set(gone)
    return gone.some(n => r.keys[n] !== undefined) ? of(r.replica, Object.fromEntries(Object.entries(r.keys).filter(([n]) => !drop.has(n)))) : r
  }

  const moved = <O extends Obj>(was: Row<O>, next: Row<O>): Row<O> =>
    next === was || (same(next.replica, was.replica) && sameKeys(next.keys, was.keys)) ? was : next

  // A row joined in or keys let go: what either leaves of `was`, `was` itself when nothing moved.
  export type Event<O extends Obj> = { readonly type: "Held"; readonly row: Row<O> } | { readonly type: "Forgot"; readonly keys: readonly string[] }
  export const taken = <O extends Obj>(was: Row<O>, e: Event<O>): Row<O> => moved(was, e.type === "Held" ? join(was, e.row) : without(was, e.keys))

  // The one way down: an empty row of the generation after, holding no key.
  export const reset = <O extends Obj>(r: Row<O>): Row<O> => of(emptied(r.replica))
}
export default locked
