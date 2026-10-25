// The secretary page as one value: the topic filter the route holds, the stream read for it, the sheet open on a
// topic or a round, and the head's search. `step` is pure; `ask` is the page's one effect, reading the stream again
// whenever what the page wants moved past what it asked.

import type storesync from "../prodrome/storesync"
import list from "../chat/list"
import topicbar from "../chat/topicbar"
import lo from "../algebra/lo"

export namespace secretaryState {
  export type Streamed = storesync.Streamed

  // What a topic typed for a write reads as: nothing typed, a topic, or the rule it breaks in the core's words.
  export type Check = { readonly type: "Blank" } | { readonly type: "Fits"; readonly topic: string } | { readonly type: "Breaks"; readonly why: string }

  export type Sheet =
    | { readonly type: "Closed" }
    | { readonly type: "Renaming"; readonly topic: string; readonly check: Check }
    | { readonly type: "Adding"; readonly target: string; readonly check: Check }

  // The last reading landed for the ask numbered `n`, or why it failed.
  export type Shown =
    | { readonly type: "Reading" }
    | { readonly type: "Read"; readonly n: number; readonly streamed: Streamed }
    | { readonly type: "Refused"; readonly n: number; readonly why: string }

  export type T = {
    // The filter, each topic once, sorted, so one filter has one print.
    readonly chosen: readonly string[]
    // How many pages of the stream are asked for; the core sizes a page.
    readonly pages: number
    // Every move of what the page wants (the filter, Earlier, the stores) numbers a new ask; `asked` is the last sent.
    readonly wanted: number
    readonly asked: number
    readonly shown: Shown
    readonly sheet: Sheet
    readonly writes: boolean
    readonly search: list.Search
    readonly now: number
  }

  export type Event =
    | { readonly type: "Chose"; readonly chosen: readonly string[] }
    | { readonly type: "Earlier" }
    | { readonly type: "Moved" }
    | { readonly type: "Asked"; readonly n: number }
    | { readonly type: "Read"; readonly n: number; readonly streamed: Streamed }
    | { readonly type: "Refused"; readonly n: number; readonly why: string }
    | { readonly type: "Rename"; readonly topic: string }
    | { readonly type: "Add"; readonly target: string }
    | { readonly type: "Checked"; readonly check: Check }
    | { readonly type: "Closed" }
    | { readonly type: "Writes"; readonly writes: boolean }
    | { readonly type: "Searched"; readonly event: list.SearchEvent }
    | { readonly type: "Ticked"; readonly now: number }

  const filter = topicbar.filter

  const same = (a: readonly string[], b: readonly string[]): boolean => a.length === b.length && a.every((t, i) => t === b[i])

  export const initial = (chosen: readonly string[], writes: boolean, now: number): T => ({
    chosen: filter(chosen),
    pages: 1,
    wanted: 1,
    asked: 0,
    shown: { type: "Reading" },
    sheet: { type: "Closed" },
    writes,
    search: list.OFF,
    now,
  })

  const want = (t: T, moved: Partial<Pick<T, "chosen" | "pages">>): T => ({ ...t, ...moved, wanted: t.wanted + 1 })

  // A reading lands unless a later ask's already has, so answers that cross keep the newest.
  const landed = (t: T, n: number): boolean => t.shown.type === "Reading" || n >= t.shown.n

  export const step = (t: T, e: Event): T => {
    switch (e.type) {
      case "Chose": {
        const chosen = filter(e.chosen)
        return same(chosen, t.chosen) ? t : want(t, { chosen, pages: 1 })
      }
      case "Earlier":
        return want(t, { pages: t.pages + 1 })
      case "Moved":
        return want(t, {})
      case "Asked":
        return e.n <= t.asked ? t : { ...t, asked: e.n }
      case "Read":
        return landed(t, e.n) ? { ...t, shown: { type: "Read", n: e.n, streamed: e.streamed } } : t
      case "Refused":
        return landed(t, e.n) ? { ...t, shown: { type: "Refused", n: e.n, why: e.why } } : t
      case "Rename":
        return { ...t, sheet: { type: "Renaming", topic: e.topic, check: { type: "Fits", topic: e.topic } } }
      case "Add":
        return { ...t, sheet: { type: "Adding", target: e.target, check: { type: "Blank" } } }
      case "Checked":
        return t.sheet.type === "Closed" ? t : { ...t, sheet: { ...t.sheet, check: e.check } }
      case "Closed":
        return t.sheet.type === "Closed" ? t : { ...t, sheet: { type: "Closed" } }
      case "Writes":
        return e.writes === t.writes ? t : { ...t, writes: e.writes }
      case "Searched": {
        const search = list.searched(t.search, e.event)
        return search === t.search ? t : { ...t, search }
      }
      case "Ticked":
        return e.now === t.now ? t : { ...t, now: e.now }
    }
  }

  // Whether the page has nothing to show yet: its first reading, or a search, on its way.
  export const waits = (t: T): boolean => t.shown.type === "Reading" || list.searchWaits(t.search)

  // The page's one effect: the stream asked for again once what it wants moved past what it asked (`Asked` first, so
  // the next value does not ask again), the answer an event.
  export const ask = (t: T, read: (chosen: readonly string[], pages: number) => Promise<Streamed>, dispatch: (e: Event) => void): void => {
    if (t.asked >= t.wanted) return
    const n = t.wanted
    dispatch({ type: "Asked", n })
    void lo.of(read(t.chosen, t.pages)).then(l =>
      dispatch(l.type === "Val" ? { type: "Read", n, streamed: l.v } : { type: "Refused", n, why: l.e }),
    )
  }
}
export default secretaryState
