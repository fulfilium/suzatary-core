// What is connected: how far setup is, for the first screen, and a card's reference as a line of the overview names it.

import type { LineStanding, Overview } from "../wire/generated/wire"
import type { Row } from "../inbox/inbox"

export namespace overview {
  /** Whether a line's standing asks something of the owner; Settings says each in words (`app/pages/src/settings.rs`). */
  const asks = (s: LineStanding): boolean => {
    switch (s.kind) {
      case "kept":
      case "synced":
      case "answered":
      case "unobserved":
        return false
      case "locked":
      case "store_failed":
      case "unreadable":
      case "by_hand":
      case "failing":
      case "missing":
        return true
    }
  }

  /** How far setup is: a step per line, done while it asks nothing of the owner. */
  export type Progress = { readonly type: "Finished" } | { readonly type: "Unfinished"; readonly done: number; readonly of: number }

  export const progress = (o: Overview): Progress => {
    const lines = o.groups.flatMap(g => g.lines)
    const done = lines.filter(l => !asks(l.standing)).length
    return done === lines.length ? { type: "Finished" } : { type: "Unfinished", done, of: lines.length }
  }

  /** The first screen's line while setup is unfinished; `null` once it is. */
  export const unfinished = (p: Progress): string | null =>
    p.type === "Finished" ? null : `Finish setting up · ${p.done} of ${p.of} done`

  /** A row's reference, as the box names a card. */
  export const reference = (r: Row): string | null => (r.kind === "proposal" ? `proposal:${r.conversation.id}/${r.proposal.index}` : null)
}
export default overview
