// The fold on screen as one value: nothing yet, or one reading's rows and who read them. It carries the numbers
// it is handed and computes none; the screen observes each as a whole percent (`fulfillment.observed`).

import type { Entry, FoldPayload, NextChange } from "../wire/generated/wire"
import type { CachedFold } from "./cache"
import source from "../algebra/source"
import heads from "../prodrome/heads"
import type { Order } from "../prodrome/core"
import fulfillment from "../todo/fulfillment"

export namespace onscreen {
  // Who read the rows: the box, with the heads it read; the tab's own core over the replica it holds; or the device's
  // copy of an earlier box fold, which knows only when it was heard. Only a reading about now is fresh.
  export type By =
    | { readonly type: "Box"; readonly heads: heads.Heads; readonly fetchedAt: number; readonly next: NextChange | null; readonly fresh: boolean }
    | { readonly type: "Local"; readonly heads: heads.Heads; readonly next: NextChange | null; readonly fresh: boolean }
    | { readonly type: "Cached"; readonly fetchedAt: number }

  export type Fold =
    | { readonly type: "Unloaded" }
    | { readonly type: "Shown"; readonly entries: readonly Entry[]; readonly at: string; readonly by: By }

  // How the heads of a fold stand to another's, read off the replica held when it was handed in.
  export type Ancestry = (a: heads.Heads, b: heads.Heads) => Order

  export type Event =
    | { readonly type: "Box"; readonly payload: FoldPayload; readonly fetchedAt: number; readonly ancestry: Ancestry }
    | { readonly type: "Local"; readonly entries: readonly Entry[]; readonly at: string; readonly next: NextChange | null; readonly heads: heads.Heads; readonly fresh: boolean; readonly ancestry: Ancestry }
    | { readonly type: "Cached"; readonly fold: CachedFold }
    | { readonly type: "Stale" }
    | { readonly type: "Reset" }

  export const UNLOADED: Fold = { type: "Unloaded" }

  const observed = (e: Entry): Entry => {
    const value = fulfillment.observed(e.value)
    return value === e.value ? e : { ...e, value }
  }

  // Whether `next` may replace what is shown. The tab's own reading is the truth while it holds objects, so the box's
  // only boots a tab that has none; of two by one reader, the later in the causal order stays whichever arrived last,
  // and of two concurrent ones the one shown stays. Where the order is not known here (the tab holds none of what they
  // name) the later answer is taken, since one reader's answers are asked one at a time. A cached fold only ever
  // boots an empty tab.
  const replaces = (next: By, was: Fold, ancestry: Ancestry): boolean =>
    was.type === "Unloaded" ||
    (next.type !== "Cached" &&
      (was.by.type === "Cached" ||
        (next.type === "Local" && was.by.type === "Box") ||
        (next.type === was.by.type && ["Same", "Above", "Unknown"].includes(ancestry(next.heads, was.by.heads)))))

  const show = (f: Fold, entries: readonly Entry[], at: string, by: By, ancestry: Ancestry = () => "Unknown"): Fold =>
    replaces(by, f, ancestry) ? { type: "Shown", entries: entries.map(observed), at, by } : f

  // A step that changes nothing returns `f` itself, so nobody wakes.
  export const step = (f: Fold, e: Event): Fold => {
    switch (e.type) {
      case "Box":
        return show(f, e.payload.entries, e.payload.at, { type: "Box", heads: heads.of(e.payload.heads), fetchedAt: e.fetchedAt, next: e.payload.next, fresh: true }, e.ancestry)
      case "Local":
        return show(f, e.entries, e.at, { type: "Local", heads: heads.of(e.heads), next: e.next, fresh: e.fresh }, e.ancestry)
      case "Cached":
        return show(f, e.fold.entries, e.fold.at, { type: "Cached", fetchedAt: e.fold.fetchedAt })
      case "Stale":
        return f.type === "Shown" && f.by.type !== "Cached" && f.by.fresh ? { ...f, by: { ...f.by, fresh: false } } : f
      case "Reset":
        return f.type === "Unloaded" ? f : UNLOADED
    }
  }

  export const loaded = (f: Fold): f is Extract<Fold, { type: "Shown" }> => f.type === "Shown"
  export const entries = (f: Fold): readonly Entry[] => (f.type === "Shown" ? f.entries : [])
  // The instant the rows were read at, none before a fold has arrived.
  export const at = (f: Fold): string | null => (f.type === "Shown" ? f.at : null)
  export const entry = (f: Fold, todo: string): Entry | undefined => entries(f).find(e => e.todo === todo)
  // When what is shown next moves; a cached fold does not know, and the box's answer replaces it.
  export const next = (f: Fold): NextChange | null => (f.type === "Shown" && f.by.type !== "Cached" ? f.by.next : null)
  export const fresh = (f: Fold): boolean => f.type === "Shown" && f.by.type !== "Cached" && f.by.fresh
  // Who read the rows, eliminated: a reader of the fold names what it does with each, and never matches on the tag.
  export const by = <R>(b: By, alg: { readonly [K in By["type"]]: (b: Extract<By, { readonly type: K }>) => R }): R => {
    switch (b.type) {
      case "Box":
        return alg.Box(b)
      case "Local":
        return alg.Local(b)
      case "Cached":
        return alg.Cached(b)
    }
  }
}
export default onscreen

/** The app's one fold on screen. */
export const store = source.create<onscreen.Fold, onscreen.Event>(onscreen.UNLOADED, onscreen.step)
