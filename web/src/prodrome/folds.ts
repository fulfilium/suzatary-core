// The box's fold of each chain, kept while nothing it read has moved (the box's heads of every chain, as the tab knows
// them, and the fold's own next change): a fold prices through every chain, so any chain moving is news to each, and a
// fold asked while its answer is on its way waits on that ask.

import type { FoldPayload } from "../wire/generated/wire"
import heads from "./heads"
import time from "../app/time"

export namespace folds {
  // Each chain the box serves, and its heads as the tab last heard them from the box, `null` for never.
  export type Known = ReadonlyMap<string, heads.Heads | null>

  // What names what a fold read, or `null` when the tab has not heard the box's heads of every chain: such a fold is
  // never reused, since nothing would say when it went stale.
  export const keyOf = (known: Known): string | null => {
    const each = [...known].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    return each.some(([, h]) => h === null) ? null : each.map(([name, h]) => `${name}:${(h ?? heads.NONE).join(",")}`).join(";")
  }

  // `until` is the instant the fold next moves, `null` for never.
  type Kept = { readonly key: string; readonly until: number | null; readonly payload: Promise<FoldPayload> }

  const fits = (k: Kept, key: string | null, now: number): boolean => key !== null && k.key === key && (k.until === null || now < k.until)

  const until = (p: FoldPayload): number | null => (p.next === null ? null : time.ms(p.next.at))

  export type Cache = {
    readonly of: (chain: string) => Promise<FoldPayload>
    // What names a fold asked now; read before the ask, so a fold is kept under what the tab knew when it asked.
    readonly key: () => string | null
    // A fold the box answered by another way, asked when the tab knew `key`, kept as if asked here.
    readonly put: (chain: string, key: string | null, payload: FoldPayload) => void
  }

  export const cache = (known: () => Known, ask: (chain: string) => Promise<FoldPayload>, now: () => number): Cache => {
    const kept = new Map<string, Kept>()
    const keep = (chain: string, key: string | null, payload: Promise<FoldPayload>): void => {
      if (key === null) return
      // Its instant is known once it lands; until then it is the ask on its way, which any reader of the same key shares.
      kept.set(chain, { key, until: null, payload })
      void payload.then(
        p => kept.get(chain)?.payload === payload && kept.set(chain, { key, until: until(p), payload }),
        () => kept.get(chain)?.payload === payload && kept.delete(chain),
      )
    }
    return {
      of: chain => {
        const key = keyOf(known())
        const k = kept.get(chain)
        if (k !== undefined && fits(k, key, now())) return k.payload
        const payload = ask(chain)
        keep(chain, key, payload)
        return payload
      },
      key: () => keyOf(known()),
      put: (chain, key, payload) => keep(chain, key, Promise.resolve(payload)),
    }
  }
}
export default folds
