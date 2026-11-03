// What this device keeps: whether the browser may evict it, how much of the render budget is spent, and the key it signs with.

import rendered from "./rendered"
import blocks from "./blocks"
import once from "../algebra/once"
import type { Standing } from "../prodrome/core"

export namespace device {
  export type Persist = { type: "Unsupported" } | { type: "Granted" } | { type: "Denied" }
  // Who this device writes as, by its key (SPEC law 42): none before it has read the box's root.
  export type Signing =
    | { type: "Unheard" }
    | { type: "Reads" }
    | { type: "Unsupported"; e: string }
    | { type: "Unreadable"; e: string }
    | { type: "Keyed"; key: string; standing: Standing }
  export type Kept = { persist: Persist; usage: rendered.Usage; budget: number }

  export const persistOf = (supported: boolean, granted: boolean): Persist =>
    !supported ? { type: "Unsupported" } : granted ? { type: "Granted" } : { type: "Denied" }

  // Asked once per load: a browser that said no keeps saying no until the site is used more, and asking changes nothing;
  // a browser that failed to answer is asked again.
  export const persist = once.of(async (): Promise<Persist> => {
    const s = typeof navigator === "undefined" ? undefined : navigator.storage
    if (s === undefined || typeof s.persist !== "function") return persistOf(false, false)
    return persistOf(true, (await s.persisted()) || (await s.persist()))
  }).get

  // What this device keeps, read with the box's overview: it moves when the page is read again, not with the replica.
  export const kept = async (store: rendered.Store = blocks.store): Promise<Kept> => ({
    persist: await persist().catch(() => persistOf(false, false)),
    usage: await store.usage(),
    budget: rendered.BUDGET,
  })

  // Who this device writes as, read again whenever the replica, the key or the login moves.
  export const signing = (read: () => Promise<Signing>): Promise<Signing> =>
    read().catch((e: unknown): Signing => ({ type: "Unsupported", e: String(e) }))
}
export default device
