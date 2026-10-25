// The secretary page: the topic bar over every conversation's rounds as one stream, filtered by the topics the route
// holds; its value and step are `secretary.state.ts`. `?ref=` and `?focus=1` forward to the default conversation;
// `mountChats` is the conversation list as the column beside one.

import app from "../app/app"
import type { ConversationRow, StreamPayload, TopicInUse } from "../wire/generated/wire"
import { openConversation } from "../wire/api"
import storesync from "../prodrome/storesync"
import conversations from "../chat/conversations"
import list from "../chat/list"
import { el } from "../view/dom"
import frame from "../view/frame"
import placeholder from "../view/placeholder"
import lo from "../algebra/lo"
import clock from "../algebra/clock"
import time from "../app/time"
import type { Mounted } from "../view/page"
import { hrefOf } from "../shell/route"
import source from "../algebra/source"
import reading from "../algebra/reading"
import { toast } from "../view/toast"
import address from "../shell/address"
import readonly from "../view/readonly"
import empty from "../view/empty"
import pie from "../view/pie"
import keyed from "../algebra/keyed"
import { loadCore } from "../prodrome/core"
import intentsync from "../prodrome/intentsync"
import type topics from "../prodrome/topics"
import dressed from "../chat/dressed"
import topicbar from "../chat/topicbar"
import streamround from "../chat/streamround"
import topicsheet from "../chat/topicsheet"
import secretaryState from "./secretary.state"

export namespace secretary {
  const DEBOUNCE_MS = 250

  // A new topic opens at once with no name: the box names it after its first answer, and Rename is on its page.
  const starter = (): HTMLElement => {
    const start = (): void =>
      void openConversation("")
        .then(p => { address.go({ type: "conversation", id: p.id, refs: [], focus: true }) })
        .catch((e: unknown) => toast(lo.reason(e)))
    return el("button", { type: "button", class: "act act-newtopic", onclick: start }, "New topic")
  }

  // The search field and New topic, on one line outside the redrawn list, so a redraw keeps what is typed.
  type Head = { readonly node: HTMLElement; readonly listen: () => () => void }
  const head = (dispatch: (e: list.SearchEvent) => void): Head => {
    const field = el("input", { type: "search", class: "search chatsearch", placeholder: "Search", "aria-label": "Search conversations" })
    const writes = el("div", { class: "chatwrites" })
    const offer = (): void => {
      const reach = app.reach(app.now())
      writes.replaceChildren(reach.type === "Writes" ? starter() : readonly.line(reach) ?? "")
    }
    let timer: ReturnType<typeof setTimeout> | null = null
    field.addEventListener("input", () => {
      dispatch({ type: "Typed", q: field.value })
      if (timer !== null) clearTimeout(timer)
      const q = field.value.trim()
      timer = q === "" ? null : setTimeout(() => void lo.of(storesync.search(q)).then(found => dispatch({ type: "Found", q, found: lo.map(found, p => p.hits) })), DEBOUNCE_MS)
    })
    return {
      node: el("div", { class: "chathead" }, field, writes),
      listen: () => {
        offer()
        const unwatch = app.hear(app.reach, offer, app.sameReach)
        return () => (timer !== null && clearTimeout(timer), unwatch())
      },
    }
  }

  // A failure of the reading is the list's own row, so the page never loses its filter or search to it.
  const rows = conversations.rows
  const listed = (r: lo.T<readonly ConversationRow[]>): list.Event => ({ type: "Listed", rows: r })

  // The list's instant moves when the owner's day turns, so a time of today reads "yesterday" from then on. A timer
  // counts only the time the device is awake, so a page shown again reads the instant afresh and arms again.
  const ticking = (tick: (now: number) => void): (() => void) => {
    let stop = (): void => {}
    const arm = (): void => {
      stop()
      tick(time.now())
      stop = clock.wake(clock.nextDay(app.now().zone, time.now()), arm)
    }
    const shown = (): void => (document.visibilityState === "visible" ? arm() : undefined)
    arm()
    document.addEventListener("visibilitychange", shown)
    window.addEventListener("pageshow", shown)
    return () => (stop(), document.removeEventListener("visibilitychange", shown), window.removeEventListener("pageshow", shown))
  }

  type T = secretaryState.T
  type Event = secretaryState.Event

  // A topic write, sealed into the tab's replica of its store and handed to the box.
  const written = (w: topics.Write): Promise<void> => intentsync.asked({ type: "Topic", write: w, at: time.moment() })

  // What is typed, checked by the core's own `Topic::new`, so the refusal is the type's words.
  const check = (dispatch: (e: Event) => void) => (text: string): void => {
    if (text === "") return dispatch({ type: "Checked", check: { type: "Blank" } })
    void loadCore().then(
      core => {
        const why = core.topic_refusal(text)
        dispatch({ type: "Checked", check: why === undefined ? { type: "Fits", topic: text } : { type: "Breaks", why } })
      },
      (e: unknown) => dispatch({ type: "Checked", check: { type: "Breaks", why: lo.reason(e) } }),
    )
  }

  const sheetFx = (v: T, dispatch: (e: Event) => void): topicsheet.Effects => ({
    check: check(dispatch),
    close: () => dispatch({ type: "Closed" }),
    write: topic => {
      const s = v.sheet
      if (s.type === "Closed") return
      const w: topics.Write = s.type === "Renaming" ? { type: "Rename", from: s.topic, to: topic } : { type: "Add", target: s.target, topic }
      void written(w).then(() => {
        dispatch({ type: "Closed" })
        // A filter on the old name follows it to the new one.
        if (s.type === "Renaming" && v.chosen.includes(s.topic)) {
          address.replace({ type: "secretary", refs: [], topics: topicbar.filter(v.chosen.map(t => (t === s.topic ? topic : t))) })
        }
      }, (e: unknown) => toast(lo.reason(e)))
    },
  })

  // Within this many pixels of the end, a reader is reading the newest, so a new round keeps them there.
  const AT_END_PX = 80

  // The rounds kept by their target, so a draw rebuilds only a round that moved; the scroll is the DOM's: a new filter
  // opens at its newest round, Earlier keeps the reader's place, and a reader at the end stays at the end.
  const rounds = (dispatch: (e: Event) => void): frame.Island<T> => {
    const node = el("div", { class: "streamrounds" })
    const list = keyed.list(node)
    let drawn: { readonly chosen: string; readonly pages: number } | null = null
    const fx: streamround.Effects = {
      add: target => dispatch({ type: "Add", target }),
      remove: (target, topic) => void written({ type: "Remove", target, topic }).catch((e: unknown) => toast(lo.reason(e))),
    }
    return {
      node,
      update: v => {
        // A reading that failed draws no rounds, so none of another filter's stand under its problem.
        if (v.shown.type === "Refused") return void (list.draw([]), (drawn = null))
        if (v.shown.type !== "Read") return
        const box = document.scrollingElement ?? document.documentElement
        const fromEnd = box.scrollHeight - box.scrollTop
        const atEnd = fromEnd - box.clientHeight < AT_END_PX
        list.draw(
          v.shown.streamed.stream.rounds.map(r => {
            const said = streamround.when(r, v.now)
            return { key: r.target, sig: streamround.sig(r, v.writes, said), make: () => streamround.view(r, v.writes, said, fx) }
          }),
        )
        const chosen = v.chosen.join(",")
        if (drawn === null || drawn.chosen !== chosen || atEnd) box.scrollTop = box.scrollHeight
        else if (v.pages > drawn.pages) box.scrollTop = box.scrollHeight - fromEnd
        drawn = { chosen, pages: v.pages }
      },
    }
  }

  // How many rounds the filter shows, how many are drawn, and Earlier while some are not.
  const said = (s: StreamPayload, chosen: readonly string[], dispatch: (e: Event) => void): HTMLElement | null => {
    const more = s.rounds.length < s.total
    if (chosen.length === 0 && !more) return null
    const share = s.total === 0 ? 1 : s.rounds.length / s.total
    return el(
      "p",
      { class: "streamsaid legend" },
      more ? [pie.view(share, pie.neutral, `${s.rounds.length} of ${s.total} rounds shown`), " "] : null,
      chosen.length === 0 ? `${s.rounds.length} of ${s.total} rounds` : `${more ? `${s.rounds.length} of ` : ""}${s.total} round${s.total === 1 ? "" : "s"} on ${topicbar.and(chosen)}`,
      more ? [" ", el("button", { type: "button", class: "act act-earlier", onclick: () => dispatch({ type: "Earlier" }) }, "Earlier")] : null,
      chosen.length === 0 ? null : [" ", el("a", { class: "streamall", href: topicbar.href([]) }, "All rounds")],
    )
  }

  const sheetOf = (v: T, bar: readonly TopicInUse[], streamed: secretaryState.Streamed | null, dispatch: (e: Event) => void): HTMLElement | null => {
    const s = v.sheet
    if (s.type === "Closed" || streamed === null) return null
    const round = s.type === "Adding" ? streamed.stream.rounds.find(r => r.target === s.target) : undefined
    return topicsheet.view(s, bar, round, sheetFx(v, dispatch))
  }

  const nothing = (chosen: readonly string[]): string =>
    chosen.length === 0 ? "Nothing asked yet." : `No round carries ${topicbar.and(chosen)}.`

  export const page = (chosen: readonly string[]): frame.Page<T, Event, "rounds"> => {
    let h: Head | null = null
    return {
      className: "chats secretarystream",
      title: "Secretary",
      masthead: "destination",
      now: dispatch => {
        h = head(e => dispatch({ type: "Searched", event: e }))
        return [el("p", { class: "legend chatsmemory" }, el("a", { href: hrefOf({ type: "memory" }) }, "What the secretary remembers")), h.node]
      },
      listen: dispatch => {
        const us = [
          h?.listen() ?? (() => {}),
          ticking(now => dispatch({ type: "Ticked", now })),
          storesync.watch(() => dispatch({ type: "Moved" })),
          app.hear(app.writes, writes => dispatch({ type: "Writes", writes })),
        ]
        return () => us.forEach(u => u())
      },
      reads: () => reading.constant(secretaryState.initial(chosen, app.writes(app.now()), time.now())),
      shape: placeholder.TURNS,
      waits: secretaryState.waits,
      step: secretaryState.step,
      effects: (v, fx) => {
        secretaryState.ask(v, dressed.stream, fx.dispatch)
        const field = document.querySelector<HTMLInputElement>(".topicsheet .topicfield")
        if (field !== null && document.activeElement?.closest(".topicsheet") == null) field.focus()
      },
      absorb: (screen, dispatch) => {
        if (screen.type !== "secretary") return false
        dispatch({ type: "Chose", chosen: screen.topics })
        return true
      },
      islands: dispatch => ({ rounds: rounds(dispatch) }),
      view: (v, dispatch, _, at) => {
        if (v.search.type === "On") return el("div", { class: "chatbody" }, list.hits(v.search.q, v.search.found, v.now, null))
        const streamed = v.shown.type === "Read" ? v.shown.streamed : null
        const bar = streamed?.bar.topics ?? []
        return [
          topicbar.view(bar, v.chosen, v.writes, { rename: topic => dispatch({ type: "Rename", topic }) }),
          v.shown.type === "Refused" ? el("p", { class: "problem" }, `The rounds did not read: ${v.shown.why}`) : null,
          streamed === null ? null : said(streamed.stream, v.chosen, dispatch),
          streamed !== null && streamed.stream.total === 0 ? empty.note(nothing(v.chosen)) : null,
          at.rounds,
          sheetOf(v, bar, streamed, dispatch),
        ]
      },
    }
  }

  // The column beside a conversation, read again when a name in it moved.
  export type Chats = { readonly destroy: () => void; readonly again: () => void }

  // The column beside a conversation: the same head and list, no heading of its own.
  export const mountChats = (target: HTMLElement, current: string | null): Chats => {
    const s = source.create(list.init(current, time.now()), list.step)
    const h = head(s.dispatch)
    const body = el("div", { class: "chatbody" })
    // One placeholder for the column's life: a wait drawn again is the same node, so its 300 ms never start again.
    const waiting = placeholder.view(placeholder.ROWS)
    const draw = (): void => {
      const next = list.view(s.state(), { set: s.dispatch, waiting })
      if (next !== waiting || body.childNodes.length !== 1 || body.firstChild !== waiting) body.replaceChildren(...(next instanceof Node ? [next] : []))
    }
    const read = (): void => lo.fold(rows.now(), { loading: () => {}, error: () => {}, val: r => s.dispatch(listed(r)) })
    const unsub = s.subscribe(draw)
    const unlisten = h.listen()
    const untick = ticking(now => s.dispatch({ type: "Ticked", now }))
    const unheard = rows.hear(read)
    target.replaceChildren(h.node, body)
    draw()
    read()
    return { again: rows.again, destroy: () => (unsub(), unlisten(), untick(), unheard()) }
  }

  export const mount = (root: HTMLElement, topics: readonly string[]): Promise<Mounted> => frame.mount(root, page(topics))
}
export default secretary
