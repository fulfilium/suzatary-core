// One store the tab holds, in the core's worker: its row restored from the device (`noderows`), pulled from the box by
// heads and frontier (`pull`), and read once every node a reading needs is held. A store's nodes either carry their
// content, or name it, and then the content is fetched by name before a reading that needs it; that is the store's
// `content`, data on its spec, never a second path. What a store's reader does with a move, and what a landing or a
// restore owes beyond the row, are the store's too. Nothing here names what a store holds.

import type lo from "../algebra/lo"
import type { Erased } from "./core"
import type content from "./content"
import type excluded from "./excluded"
import type heads from "./heads"
import locked from "./locked"
import type noderows from "./noderows"
import pull from "./pull"
import type { Obj } from "./replica"
import { behind } from "./replica"

export namespace engine {
  // Whether a store's nodes carry their content, or name it, fetched by name: `has` asked of a reading, which keeps what
  // it read; `holds` asked of a count, which keeps nothing; `version` counting each move of what is held; and `ended`
  // told once a reading ends.
  export type Content<N extends Obj> =
    | { readonly type: "Inline" }
    | {
        readonly type: "ByName"
        readonly has: (name: string) => boolean
        readonly holds: (name: string) => boolean
        readonly fetch: (lane: content.Lane, nodes: readonly N[]) => Promise<void>
        readonly version: () => number
        readonly ended: () => void
      }

  export const INLINE: { readonly type: "Inline" } = { type: "Inline" }

  // The keys a row's objects name as erased (`dead`), the blocks each key's words fed (`erased`), and how those are
  // forgotten. A store whose words are not locked names no key dead.
  export type Words<N extends Obj> = {
    readonly dead: () => Promise<ReadonlySet<string>>
    readonly erased: (r: locked.Row<N>, told: readonly string[]) => Promise<Erased>
    readonly forget: (blocks: readonly string[]) => Promise<lo.Settled<null>>
  }

  const NO_KEYS: ReadonlySet<string> = new Set()
  export const open = <N extends Obj>(): Words<N> => ({
    dead: () => Promise.resolve(NO_KEYS),
    erased: () => Promise.resolve({}),
    forget: () => Promise.resolve({ type: "Val", v: null }),
  })

  export type Spec<N extends Obj> = {
    readonly name: string
    readonly place: noderows.Place<N>
    readonly ask: (since: string) => Promise<pull.Answer<N>>
    readonly content: Content<N>
    readonly words: Words<N>
    // The row moved from `was` to `now`: its reader told, before any reading of `now`.
    readonly took: (was: locked.Row<N>, now: locked.Row<N>) => void
    // What runs before the row is first read, and after each landing that moved it.
    readonly restoring: () => Promise<void>
    readonly landed: (row: locked.Row<N>) => Promise<void>
  }

  // Who hears the store: what a reading left out, by reader; each move of its pull; and that a later epoch replaced a
  // landing, so what waited on the store waits again.
  export type Effects = {
    readonly told: (reader: string, left: excluded.Left) => void
    readonly bars: (e: pull.Bar) => void
    readonly replaced: () => void
  }

  export type T<N extends Obj> = {
    readonly row: () => locked.Row<N>
    readonly sync: pull.Sync<N>
    // A reading wants the store: pull now, and after every word of the box's stream from now on.
    readonly want: () => Promise<boolean>
    readonly wanted: () => boolean
    // The box's stream named the store's heads: a pull, if a reading wants the store and the tab lacks one.
    readonly heard: (box: heads.Heads) => void
    // `read` once the content of every node `nodes` names is held, fetching what is not in `lane`. The nodes are asked
    // again after each fetch, of the row then held, and `read` runs in the same task as their last ask.
    readonly over: <A>(lane: content.Lane, nodes: () => readonly N[], read: () => A) => Promise<A>
    // Those of `nodes` whose content the device does not hold.
    readonly lacking: (nodes: readonly N[]) => readonly N[]
    // The row written over by a reset and taken again from the box, with whatever the reset could not do.
    readonly reset: () => Promise<readonly string[]>
  }

  // What a store is, whatever it holds: what a set of stores asks of each by its name.
  export type Any = Pick<T<Obj>, "want" | "wanted" | "heard" | "reset"> & { readonly sync: Pick<pull.Sync<Obj>, "restored" | "landed" | "whole"> }

  export const of = <N extends Obj>(spec: Spec<N>, fx: Effects): T<N> => {
    let row = locked.none<N>()
    const take = (e: locked.Event<N>): void => {
      const was = row
      row = locked.taken(row, e)
      if (row !== was) spec.took(was, row)
    }

    const holds = (c: Content<N>): ((name: string) => boolean) => (c.type === "Inline" ? () => true : c.holds)
    const lacking = (nodes: readonly N[], held: (name: string) => boolean): readonly N[] => nodes.filter(n => !held(n.hash))

    // Whether a reading wants the store yet: before one does, nothing pulls it.
    let wanted: "No" | "Yes" = "No"
    const sync = pull.of(
      {
        name: spec.name,
        ...pull.locks<N>({
          name: spec.name,
          held: () => row,
          hold: r => take({ type: "Held", row: r }),
          forgot: keys => take({ type: "Forgot", keys }),
          dead: spec.words.dead,
          erased: spec.words.erased,
          forget: spec.words.forget,
          alter: spec.place.forget,
          told: fx.told,
        }),
        held: () => row,
        place: spec.place,
        ask: spec.ask,
        landed: spec.landed,
        restoring: spec.restoring,
      },
      { told: fx.told, bars: fx.bars, replaced: fx.replaced, again: () => void (wanted === "Yes" ? sync.pull() : undefined) },
    )

    // The row and the content held at the last purge. The keys a store names as dead are found only in content held,
    // and only grow with it, so the purge runs once every node's content is held, once for each row and content held.
    let purged: { readonly row: locked.Row<N>; readonly content: number } | null = null
    const purge = (): void => {
      const version = spec.content.type === "Inline" ? 0 : spec.content.version()
      if (purged?.row === row && purged.content === version) return
      if (lacking(row.replica.objects, holds(spec.content)).length > 0) return
      purged = { row, content: version }
      // On a later task, so the reading's answer goes out first.
      setTimeout(() => void sync.purge(), 0)
    }

    const over = async <A>(lane: content.Lane, nodes: () => readonly N[], read: () => A): Promise<A> => {
      const c = spec.content
      try {
        for (;;) {
          const need = c.type === "Inline" ? [] : lacking(nodes(), c.has)
          if (c.type === "Inline" || need.length === 0) {
            const answer = read()
            purge()
            return answer
          }
          await c.fetch(lane, need)
        }
      } finally {
        if (c.type === "ByName") c.ended()
      }
    }

    const want = (): Promise<boolean> => {
      wanted = "Yes"
      return sync.pull()
    }

    return {
      row: () => row,
      sync,
      want,
      wanted: () => wanted === "Yes",
      heard: box => {
        if (wanted === "Yes" && behind(box, sync.holding())) void sync.pull()
      },
      over,
      lacking: nodes => lacking(nodes, holds(spec.content)),
      reset: async () => {
        const said: string[] = []
        await sync.reset(why => said.push(why))
        await sync.restored.get()
        await want()
        return said
      },
    }
  }
}
export default engine
