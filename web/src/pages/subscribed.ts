// The listeners a program keeps, equal to the subscriptions of its model by the patches it sends: a slot's listener is
// started by a `Start`, which lets the earlier one of that slot go, and stopped by a `Stop`. A source is the program's
// data and `start` is the one place that turns it into a listener, handing it the program's own `send`. A program let go
// keeps none: what it had stops, and a patch that reaches it later is dropped.

import type { SubPatch } from "../wire/generated/pages"
import drain from "./drain"

export namespace subscribed {
  export type T<K, V, I> = {
    readonly apply: (patch: SubPatch<K, V>, send: (input: I) => void) => void
    // The program's state changed.
    readonly follow: (life: drain.State) => void
    // The slots that have a listener, as printed to key them.
    readonly slots: () => readonly string[]
  }

  export const of = <K, V, I>(start: (slot: K, source: V, send: (input: I) => void) => () => void, first: drain.State): T<K, V, I> => {
    const live = new Map<string, () => void>()
    let life = first
    const stop = (key: string): void => {
      live.get(key)?.()
      live.delete(key)
    }
    return {
      apply: (patch, send) => {
        if (!drain.listens(life)) return
        const key = JSON.stringify(patch.slot)
        stop(key)
        if (patch.type === "Start") live.set(key, start(patch.slot, patch.source, send))
      },
      follow: next => {
        life = next
        if (!drain.listens(life)) [...live.keys()].forEach(stop)
      },
      slots: () => [...live.keys()],
    }
  }
}
export default subscribed
