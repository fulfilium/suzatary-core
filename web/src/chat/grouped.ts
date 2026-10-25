// A conversation's window grouped into rounds where it is read, in the core's worker: each round by the indices of its
// turns and cards, with the owner's day it opened on and its label, so the page draws the rounds it is given. A page
// holds one answer, never a join of several: it asks for every turn it holds, back to its first (`covering`), so its
// rounds are always the rounds of the turns it holds, however long a round runs.

import type { ConversationPayload, ProposalRow, TurnRow } from "../wire/generated/wire"
import time from "../app/time"
import { dayLabel, dayOf } from "./days"
import { type Round, rounds } from "./rounds"

// The window the core reads.
type CoreWindow = import("../prodrome/core").Window

export namespace grouped {
  export type Shape = {
    readonly key: number
    readonly startIndex: number
    readonly endIndex: number
    readonly opening: number | null
    readonly trace: readonly number[]
    readonly answer: number | null
    readonly proposals: readonly number[]
    readonly earlier: readonly number[]
    // The owner's day the round opened on, `YYYY-MM-DD`, or "" when undated; its label; and whether a separator
    // carrying the label stands above it.
    readonly day: string
    readonly label: string
    readonly marked: boolean
  }

  export type Window = ConversationPayload & { readonly rounds: readonly Shape[] }

  // How many turns the core's window holds when it is asked for none (its `DEFAULT_TURN_WINDOW`).
  export const WINDOW = 60

  // A window as the page asks for it: the core's, and the first turn it must reach back to.
  export type Asked = CoreWindow & { readonly from?: number }

  // The window `asked` read by `read`, read further back when it starts after `from`: a page asking for every turn it
  // holds gets them, whatever came since. A window about a reference is paged over the turns it keeps, so it is asked
  // by count alone.
  export const covering = ({ from, ...window }: Asked, read: (w: CoreWindow) => ConversationPayload): ConversationPayload => {
    const p = read(window)
    return from === undefined || window.about !== undefined || p.first <= from ? p : read({ ...window, limit: p.turns.length + (p.first - from) })
  }

  const dated = (r: Round): string => r.opening?.turn.at ?? r.trace[0]?.turn.at ?? r.answer?.turn.at ?? ""

  // A separator stands above each dated round whose day differs from the dated round's before it.
  export const of = (p: ConversationPayload, zone: time.Zone): Window => {
    let last = ""
    const shapes = rounds(p.turns, p.proposals).map((r): Shape => {
      const at = dated(r)
      const day = at === "" ? "" : dayOf(zone, at)
      const marked = day !== "" && day !== last
      if (day !== "") last = day
      return {
        key: r.key,
        startIndex: r.startIndex,
        endIndex: r.endIndex,
        opening: r.opening?.index ?? null,
        trace: r.trace.map(t => t.index),
        answer: r.answer?.index ?? null,
        proposals: r.proposals.map(c => c.index),
        earlier: r.earlier.map(t => t.index),
        day,
        label: day === "" ? "" : dayLabel(day),
        marked,
      }
    })
    return { ...p, rounds: shapes }
  }

  // A round as the page draws it, its turns and cards read from what the page holds by their indices.
  export const resolve = (s: Shape, turns: ReadonlyMap<number, TurnRow>, cards: ReadonlyMap<number, ProposalRow>): Round => {
    const placed = (index: number) => {
      const turn = turns.get(index)
      return turn === undefined ? [] : [{ turn, index }]
    }
    const one = (index: number | null) => (index === null ? null : (placed(index)[0] ?? null))
    return {
      key: s.key,
      startIndex: s.startIndex,
      endIndex: s.endIndex,
      opening: one(s.opening),
      trace: s.trace.flatMap(placed),
      answer: one(s.answer),
      proposals: s.proposals.flatMap(i => {
        const c = cards.get(i)
        return c === undefined ? [] : [c]
      }),
      earlier: s.earlier.flatMap(placed),
    }
  }
}
export default grouped
