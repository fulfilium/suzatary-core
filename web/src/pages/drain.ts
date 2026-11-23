// A page's program after its page is left (`program.mount`'s `destroy`). A write in flight is the person's, not the
// page's: the program keeps running, undrawn, until each write has answered and each Undo it offered has expired, and
// it performs only what a write needs and what says how it went. A refusal is then a toast whatever page is on screen.

import type { Effect } from "../wire/generated/pages"

export namespace drain {
  // Writes not yet answered, and Undo offers not yet expired.
  export type Owed = { readonly writes: number; readonly offers: number }

  export type State =
    | { readonly type: "Live"; readonly owed: Owed }
    | { readonly type: "Leaving"; readonly owed: Owed }
    | { readonly type: "Gone" }

  export type Event = "Wrote" | "Answered" | "Offered" | "Expired" | "Left"

  export const LIVE: State = { type: "Live", owed: { writes: 0, offers: 0 } }

  type Kind = Effect<unknown, unknown, unknown>["type"]

  // The effects that carry a write to the box or the replica, and the ones that say how it went.
  const WRITES: ReadonlySet<Kind> = new Set<Kind>(["Post", "Act", "NewKey", "Endorse"])
  const SAYS: ReadonlySet<Kind> = new Set<Kind>(["Toast", "Offer"])

  export const writes = (k: Kind): boolean => WRITES.has(k)

  // Whether a program in `s` performs an effect of kind `k`; a leaving one goes nowhere, holds and draws nothing.
  export const performs = (s: State, k: Kind): boolean => {
    switch (s.type) {
      case "Live":
        return true
      case "Leaving":
        return WRITES.has(k) || SAYS.has(k)
      case "Gone":
        return false
    }
  }

  const by = (o: Owed, e: Event): Owed => {
    switch (e) {
      case "Wrote":
        return { ...o, writes: o.writes + 1 }
      case "Answered":
        return { ...o, writes: o.writes - 1 }
      case "Offered":
        return { ...o, offers: o.offers + 1 }
      case "Expired":
        return { ...o, offers: o.offers - 1 }
      case "Left":
        return o
    }
  }

  const settled = (o: Owed): boolean => o.writes === 0 && o.offers === 0

  export const step = (s: State, e: Event): State => {
    switch (s.type) {
      case "Live": {
        const owed = by(s.owed, e)
        return e !== "Left" ? { type: "Live", owed } : settled(owed) ? { type: "Gone" } : { type: "Leaving", owed }
      }
      case "Leaving": {
        const owed = by(s.owed, e)
        return settled(owed) ? { type: "Gone" } : { type: "Leaving", owed }
      }
      case "Gone":
        return s
    }
  }
}
export default drain
