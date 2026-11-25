// Which running pages a drawing of the tiles keeps (`tiles.ts`): the pages it draws, and those it holds behind, up to
// the drawing's bound, the one drawn longest ago let go first.

import type { Undrawn } from "../wire/generated/pages"

export namespace keep {
  // A page made but overtaken before its mount draws nothing, so `mounted` decides whether it can be kept.
  export type Of<T> = {
    readonly key: (t: T) => number
    readonly same: (t: T, href: string) => boolean
    readonly seen: (t: T) => number
    readonly mounted: (t: T) => boolean
  }

  export type Held<T> = ReadonlyMap<number, { readonly tile: T; readonly href: string }>
  export type Kept<T> = { readonly drawn: Held<T>; readonly hidden: Held<T>; readonly gone: readonly T[] }

  // Each mounted page the drawing draws is matched first, then each it holds behind, by the leaf it drew last else by
  // its address; of those behind, the `keep` drawn last stay hidden, and the rest are let go.
  export const kept = <T>(
    live: readonly T[],
    at: { readonly tiles: readonly Undrawn[]; readonly behind: readonly Undrawn[]; readonly keep: number },
    of: Of<T>,
  ): Kept<T> => {
    const taken = new Set<T>()
    const take = (want: Undrawn): [number, { tile: T; href: string }][] => {
      const free = live.filter(t => !taken.has(t) && of.mounted(t))
      const t = free.find(t => of.key(t) === want.key && of.same(t, want.href)) ?? free.find(t => of.same(t, want.href))
      if (t === undefined) return []
      taken.add(t)
      return [[want.key, { tile: t, href: want.href }]]
    }
    const drawn = new Map(at.tiles.flatMap(take))
    const behind = at.behind.flatMap(take).sort(([, a], [, b]) => of.seen(b.tile) - of.seen(a.tile))
    const hidden = new Map(behind.slice(0, at.keep))
    const stay = new Set([...drawn.values(), ...hidden.values()].map(h => h.tile))
    return { drawn, hidden, gone: live.filter(t => !stay.has(t)) }
  }
}
export default keep
