// A page's program as its host holds it: drawn, hidden (its tile kept behind another, or in a workspace not drawn), or
// let go. A hidden program keeps running and hears its readings, so its page is current when it is drawn again; what it
// asks of the screen (a focus, a scroll, the keys) waits until then, and a move of the address it asks is dropped,
// since the person is reading another page. A program let go is undrawn for good: a write in flight is the person's,
// not the page's, so it keeps running until each write has answered and each Undo it offered has expired, and performs
// only what a write needs and what says how it went. A refusal is then a toast whatever page is on screen.

import type { Effect } from "../wire/generated/pages"

export namespace drain {
  // Writes not yet answered, and Undo offers not yet expired.
  export type Owed = { readonly writes: number; readonly offers: number }

  export type State =
    | { readonly type: "Drawn"; readonly owed: Owed }
    | { readonly type: "Hidden"; readonly owed: Owed }
    | { readonly type: "Leaving"; readonly owed: Owed }
    | { readonly type: "Gone" }

  export type Event = "Wrote" | "Answered" | "Offered" | "Expired" | "Hid" | "Showed" | "Left"

  const NONE: Owed = { writes: 0, offers: 0 }
  export const DRAWN: State = { type: "Drawn", owed: NONE }
  export const HIDDEN: State = { type: "Hidden", owed: NONE }

  type Kind = Effect<unknown, unknown, unknown>["type"]

  // The effects that carry a write to the box or the replica.
  const WRITES: ReadonlySet<Kind> = new Set<Kind>(["Post", "Act", "NewKey", "Endorse"])

  export const writes = (k: Kind): boolean => WRITES.has(k)

  export type Verdict = "Performs" | "Waits" | "Drops"

  // Hidden, a program reads, writes and says how a write went; what moves the screen waits for it to be drawn, and what
  // moves the address, or the clipboard, which wants the person's gesture, drops.
  const UNDRAWN: { readonly [K in Kind]: Verdict } = {
    Hold: "Performs", Again: "Performs", Rendered: "Performs", Get: "Performs", Post: "Performs", NewKey: "Performs",
    Watch: "Performs", Keep: "Performs", Listen: "Performs", Keys: "Waits", Sync: "Performs", Act: "Performs",
    Endorse: "Performs", Timer: "Performs", Frame: "Performs", Focus: "Waits", Scroll: "Waits", Copy: "Drops",
    Go: "Drops", Leave: "Drops", Replace: "Drops", Back: "Drops", Toast: "Performs", Offer: "Performs",
    Core: "Performs", Reset: "Performs", Mark: "Performs", Release: "Drops",
  }

  // Let go, a program performs only what a write needs and what says how it went.
  const LEAVING: { readonly [K in Kind]: Verdict } = {
    Hold: "Drops", Again: "Drops", Rendered: "Drops", Get: "Drops", Post: "Performs", NewKey: "Performs",
    Watch: "Drops", Keep: "Drops", Listen: "Drops", Keys: "Drops", Sync: "Drops", Act: "Performs",
    Endorse: "Performs", Timer: "Drops", Frame: "Drops", Focus: "Drops", Scroll: "Drops", Copy: "Drops",
    Go: "Drops", Leave: "Drops", Replace: "Drops", Back: "Drops", Toast: "Performs", Offer: "Performs",
    Core: "Drops", Reset: "Drops", Mark: "Drops", Release: "Drops",
  }

  // The place an effect that waits holds: one scroll, one focus, the keys of each ask, the last of each kept.
  export const slot = (e: Effect<unknown, unknown, unknown>): string => (e.type === "Keys" ? `Keys ${JSON.stringify(e.ask)}` : e.type)

  // What a program in `s` does with an effect of kind `k`.
  export const verdict = (s: State, k: Kind): Verdict => {
    switch (s.type) {
      case "Drawn":
        return "Performs"
      case "Hidden":
        return UNDRAWN[k]
      case "Leaving":
        return LEAVING[k]
      case "Gone":
        return "Drops"
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
      case "Hid":
      case "Showed":
      case "Left":
        return o
    }
  }

  const settled = (o: Owed): boolean => o.writes === 0 && o.offers === 0

  export const step = (s: State, e: Event): State => {
    switch (s.type) {
      case "Drawn":
      case "Hidden": {
        const owed = by(s.owed, e)
        if (e === "Left") return settled(owed) ? { type: "Gone" } : { type: "Leaving", owed }
        const type = e === "Hid" ? "Hidden" : e === "Showed" ? "Drawn" : s.type
        return { type, owed }
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
