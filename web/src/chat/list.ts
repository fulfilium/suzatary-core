/** The secretary's conversations: the default pinned, topics by their last turn, reviews behind a filter, a search. */

import type { ConversationRow, SearchHit } from "../wire/generated/wire"
import empty from "../view/empty"
import type { Child } from "../view/dom"
import { el } from "../view/dom"
import { recent } from "../view/format"
import app from "../app/app"
import time from "../app/time"
import lo from "../algebra/lo"
import { hrefOf } from "../shell/route"

export namespace list {
  export type Filter = { type: "Topics" } | { type: "Reviews" }
  export type Search =
    | { type: "Off" }
    | { type: "On"; q: string; found: lo.T<readonly SearchHit[]> }
  export type T = {
    rows: lo.T<readonly ConversationRow[]>
    filter: Filter
    search: Search
    current: string | null
    // The instant the times are read against, moved at the owner's next day.
    now: number
    // How many of a section's rows are drawn, the newest first: a store of thousands draws a page of them at a time.
    shown: number
  }
  export type Event =
    | { type: "Listed"; rows: lo.T<readonly ConversationRow[]> }
    | { type: "Filtered"; filter: Filter }
    | { type: "Typed"; q: string }
    | { type: "Found"; q: string; found: lo.T<readonly SearchHit[]> }
    | { type: "Ticked"; now: number }
    | { type: "More" }
  export type SearchEvent = Extract<Event, { type: "Typed" | "Found" }>
  export type Sections = { pinned: readonly ConversationRow[]; rows: readonly ConversationRow[] }

  export const TOPICS: Filter = { type: "Topics" }
  export const REVIEWS: Filter = { type: "Reviews" }
  export const OFF: Search = { type: "Off" }
  export const PAGE = 30
  export const init = (current: string | null, now: number): T => ({ rows: lo.loading, filter: TOPICS, search: OFF, current, now, shown: PAGE })

  /** A stale answer — for a query no longer typed — is dropped. */
  export const searched = (s: Search, e: SearchEvent): Search => {
    switch (e.type) {
      case "Typed": {
        const q = e.q.trim()
        return q === "" ? OFF : s.type === "On" && s.q === q ? s : { type: "On", q, found: lo.loading }
      }
      case "Found": return s.type === "On" && s.q === e.q.trim() ? { ...s, found: e.found } : s
    }
  }

  export const step = (t: T, e: Event): T => {
    switch (e.type) {
      case "Listed": return { ...t, rows: e.rows }
      case "Filtered": return e.filter.type === t.filter.type ? t : { ...t, filter: e.filter, shown: PAGE }
      case "Typed":
      case "Found": {
        const search = searched(t.search, e)
        return search === t.search ? t : { ...t, search }
      }
      case "Ticked": return e.now === t.now ? t : { ...t, now: e.now }
      case "More": return { ...t, shown: t.shown + PAGE }
    }
  }

  /** Listing opens nothing, so the default is pinned before it exists; following it opens it. */
  export const UNOPENED: ConversationRow = { id: "default", title: "Secretary", kind: "default", opened: "", turns: 0, standing: 0, unplaced: [], last: "", last_at: "" }

  // A topic opened with no name is named by the box after its first answer; until then it reads as what it is.
  export const named = (title: string): string => (title === "" ? "New topic" : title)

  // Instants and ids are ASCII, so their code units order them, with none of a collator's cost.
  const order = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0)
  const byLast = (a: ConversationRow, b: ConversationRow): number => order(b.last_at, a.last_at) || order(a.id, b.id)
  const pinned = (rows: readonly ConversationRow[]): readonly ConversationRow[] => {
    const d = rows.filter(r => r.kind === "default")
    return d.length === 0 ? [UNOPENED] : d
  }
  const sectioned = (rows: readonly ConversationRow[], filter: Filter): Sections => ({
    pinned: pinned(rows),
    rows: rows.filter(r => r.kind === (filter.type === "Topics" ? "topic" : "review")).slice().sort(byLast),
  })
  // The last rows sectioned, kept by the rows and the filter: a redraw of the same listing sorts nothing again.
  let last: { readonly rows: readonly ConversationRow[]; readonly filter: Filter["type"]; readonly s: Sections } | null = null
  export const sections = (rows: readonly ConversationRow[], filter: Filter): Sections => {
    if (last === null || last.rows !== rows || last.filter !== filter.type) last = { rows, filter: filter.type, s: sectioned(rows, filter) }
    return last.s
  }
  export const reviews = (rows: readonly ConversationRow[]): number => rows.filter(r => r.kind === "review").length

  const link = (id: string, at?: number): string =>
    hrefOf(at === undefined ? { type: "conversation", id, refs: [] } : { type: "conversation", id, refs: [], at })

  // Whether a search is still on its way: what the page waits on before it has anything to show.
  export const searchWaits = (s: Search): boolean => s.type === "On" && s.found.type === "Loading"
  export const searching = (t: T): boolean => searchWaits(t.search)

  const row = (r: ConversationRow, current: string | null, today: string): HTMLElement =>
    el("a", { class: `chatrow${r.id === current ? " current" : ""}`, href: link(r.id), "aria-current": r.id === current ? "page" : null },
      el("span", { class: "chattitle" }, named(r.title)),
      r.standing > 0 || r.unplaced.length > 0
        ? el("span", { class: "chatbadges" },
            r.standing > 0 ? el("span", { class: "chatbadge", title: `${r.standing} waiting on you` }, String(r.standing)) : null,
            r.unplaced.length > 0
              ? el("span", { class: "chatbadge problem", title: `${r.unplaced.length} proposal${r.unplaced.length === 1 ? "" : "s"} with no card on this device` }, `${r.unplaced.length}?`)
              : null)
        : null,
      el("span", { class: "chatwhen" }, recent(r.last_at, today, app.now().zone)),
      r.last === "" ? null : el("span", { class: "chatlast" }, r.last))

  const hit = (today: string) => (h: SearchHit): HTMLElement =>
    el("a", { class: "chatrow chathit", href: link(h.conversation, h.turn) },
      el("span", { class: "chattitle" }, named(h.title)),
      el("span", { class: "chatwhen" }, recent(h.at, today, app.now().zone)),
      el("span", { class: "chatlast" }, `${h.side === "person" ? "You" : "Secretary"}: ${h.excerpt}`))

  const card = (rows: readonly HTMLElement[]): HTMLElement => el("div", { class: "chatcard" }, rows)
  const failed = (text: string): HTMLElement => el("p", { class: "problem chatsaid" }, text)

  const toggle = (f: Filter, label: string, t: T, set: (e: Event) => void): HTMLElement =>
    el("button", {
      type: "button",
      class: `segment chatfilter${f.type === t.filter.type ? " on" : ""}`,
      "aria-pressed": f.type === t.filter.type ? "true" : "false",
      onclick: () => set({ type: "Filtered", filter: f }),
    }, label)

  const rows = (t: T, set: (e: Event) => void, waiting: Child): Child =>
    lo.fold<readonly ConversationRow[], Child>(t.rows, {
      loading: () => waiting,
      error: e => failed(`The conversations did not load: ${e}`),
      val: all => {
        const s = sections(all, t.filter)
        const today = time.date(app.now().zone, t.now)
        return el("div", { class: "chatsections" },
          card(s.pinned.map(r => row(r, t.current, today))),
          el("div", { class: "segmented chatfilters", role: "group", "aria-label": "Which conversations" },
            toggle(TOPICS, "Topics", t, set), toggle(REVIEWS, `Reviews · ${reviews(all)}`, t, set)),
          s.rows.length === 0 ? empty.note(t.filter.type === "Topics" ? "No topics yet." : "No reviews yet.") : card(s.rows.slice(0, t.shown).map(r => row(r, t.current, today))),
          s.rows.length > t.shown
            ? el("button", { type: "button", class: "chatmore", onclick: () => set({ type: "More" }) }, `Show older · ${s.rows.length - t.shown}`)
            : null)
      },
    })

  export const hits = (q: string, found: lo.T<readonly SearchHit[]>, now: number, waiting: Child): Child =>
    lo.fold<readonly SearchHit[], Child>(found, {
      loading: () => waiting,
      error: e => failed(`The search did not run: ${e}`),
      val: hs => hs.length === 0 ? empty.note(`Nothing said matches “${q}”.`) : card(hs.map(hit(time.date(app.now().zone, now)))),
    })

  // `waiting` is what stands while the list or a search is on its way: a placeholder kept for the wait's life, or
  // nothing where the page's frame draws its own.
  export const view = (t: T, { set, waiting }: { set: (e: Event) => void; waiting: Child }): Child =>
    t.search.type === "On" ? hits(t.search.q, t.search.found, t.now, waiting) : rows(t, set, waiting)
}
export default list
