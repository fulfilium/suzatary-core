// One conversation as a page: its name and who may rename it are the value; the transcript, the composer,
// the heading's rename field and the list beside it at 62rem are islands. `draft` is a topic before its first message.

import app from "../app/app"
import ui from "../app/ui"
import type { AttachmentRow } from "../wire/generated/wire"
import { openConversation, say } from "../wire/api"
import type { Reference } from "../chat/refs"
import { parse as parseReference } from "../chat/refs"
import intents from "../prodrome/intents"
import intentsync from "../prodrome/intentsync"
import chat from "../chat/chat"
import again from "../chat/again"
import drafts from "../chat/drafts"
import { mountComposer } from "../chat/composer"
import chatlist from "../chat/list"
import { el } from "../view/dom"
import frame from "../view/frame"
import asked from "../app/asked"
import reading from "../algebra/reading"
import marks from "../view/marks"
import type { Mounted } from "../view/page"
import { hrefOf } from "../shell/route"
import { toast } from "../view/toast"
import secretary from "./secretary"
import address from "../shell/address"
import lo from "../algebra/lo"
import once from "../algebra/once"

export namespace conversation {
  // What the heading knows of the conversation's name: nothing yet, nothing because its window could not be read, or
  // the name a window gave ("" for a topic the box has not named yet).
  export type Name =
    | { readonly type: "Reading" }
    | { readonly type: "Unread" }
    | { readonly type: "Named"; readonly title: string; readonly model: string }
  export type T = { readonly name: Name; readonly writes: boolean }
  export type Event =
    | { readonly type: "Named"; readonly title: string; readonly model: string }
    | { readonly type: "Unread" }
    | { readonly type: "Writes"; readonly writes: boolean }
  type Parts = "head" | "chats" | "transcript" | "composer"

  export type Options = {
    readonly refs: readonly string[]
    readonly focus: boolean
    // A search hit's turn: the first window ends at it.
    readonly at?: number
  }

  // A name once read stays: a later window that cannot be read leaves it.
  export const step = (v: T, e: Event): T => {
    switch (e.type) {
      case "Named":
        return v.name.type === "Named" && v.name.title === e.title && v.name.model === e.model ? v : { ...v, name: { type: "Named", title: e.title, model: e.model } }
      case "Unread":
        return v.name.type === "Reading" ? { ...v, name: { type: "Unread" } } : v
      case "Writes":
        return v.writes === e.writes ? v : { ...v, writes: e.writes }
    }
  }

  export const titleOf = (n: Name): string => (n.type === "Named" ? n.title : "")

  // Mounted the first time the width allows, so a phone fetches no list it cannot see; the stylesheet hides it below.
  const WIDE = "(min-width: 62rem)"

  // The name a rename the box took gives, until a reading of the conversation holds it and the set retracts the ask.
  const renamed = (t: intentsync.T, id: string): string | null =>
    [...t.asked].flatMap(([k, v]) => (intents.isIn(t, k) && v.act.type === "Rename" && v.act.conversation === id && v.answer?.type === "Taken" ? [v.act.title] : [])).at(-1) ?? null

  // The heading becomes a field until Enter asks for the name or Escape restores it. The field closes on the box's
  // answer, and the heading says the name the box took until the replica brings it back.
  const head = (id: string, saved: () => void): frame.Island<T> => {
    const heading = el("h1", { class: "pagetitle" })
    const writes = el("span", { class: "convwrites" })
    const button = el("button", { type: "button", class: "act act-rename" }, "Rename")
    let drawn: T | null = null
    const draw = (): void => {
      if (drawn === null) return
      const name = drawn.name
      const taken = renamed(intentsync.held(), id)
      heading.textContent = taken ?? (name.type === "Named" ? chatlist.named(name.title) : name.type === "Unread" ? "Conversation" : "")
      heading.classList.toggle("pagewaiting", taken === null && name.type === "Reading")
    }
    const unwatch = intentsync.watch(draw)
    // Opens on the name as stored, so an unnamed topic stays unnamed.
    const open = (stored: string): void => {
      const field = el("input", { type: "text", class: "renamefield", maxlength: "80", value: stored, "aria-label": "Conversation name" })
      const back = (): void => {
        field.replaceWith(heading)
        button.hidden = false
      }
      field.addEventListener("keydown", e => {
        if (e.key === "Escape") back()
        if (e.key !== "Enter") return
        void intentsync.answered({ type: "Rename", conversation: id, title: field.value })
          .then(() => (back(), saved()))
          .catch((error: unknown) => toast(lo.reason(error)))
      })
      heading.replaceWith(field)
      button.hidden = true
      field.focus()
      field.select()
    }
    return {
      node: marks.head(el("div", { class: "convhead" }, heading, writes)),
      // The model's name is only a tooltip.
      update: v => {
        drawn = v
        draw()
        button.onclick = () => open(renamed(intentsync.held(), id) ?? titleOf(v.name))
        heading.title = v.name.type === "Named" ? v.name.model : ""
        if (writes.contains(button) !== v.writes) writes.replaceChildren(...(v.writes ? [button] : []))
      },
      destroy: unwatch,
    }
  }

  // A name that moved (the title job's, or a rename) moves in the column too.
  const chats = (id: string): frame.Island<T> => {
    const aside = el("aside", { class: "chatsaside", "aria-label": "Conversations" })
    const wide = window.matchMedia(WIDE)
    let list: secretary.Chats | null = null
    let named: string | null = null
    const place = (): void => {
      if (list === null && wide.matches) list = secretary.mountChats(aside, id)
    }
    wide.addEventListener("change", place)
    place()
    return {
      node: aside,
      update: v => {
        if (v.name.type !== "Named") return
        if (named !== null && v.name.title !== named) list?.again()
        named = v.name.title
      },
      destroy: () => (wide.removeEventListener("change", place), list?.destroy()),
    }
  }

  export const page = (id: string, o: Options): frame.Page<T, Event, Parts> => ({
    className: "conversation",
    title: "Conversation",
    reads: () => reading.constant<T>({ name: { type: "Reading" }, writes: app.writes(app.now()) }),
    step,
    listen: dispatch => app.hear(app.writes, writes => dispatch({ type: "Writes", writes })),
    islands: dispatch => {
      const talk = chat.mount({
        id,
        ...(o.at === undefined ? {} : { at: o.at }),
        seeded: o.refs.flatMap(text => {
          const r: Reference | null = parseReference(text)
          return r === null ? [] : [r]
        }),
        focus: o.focus,
        scroll: document.scrollingElement ?? document.documentElement,
        loaded: s => dispatch({ type: "Named", title: s.title, model: s.model }),
        unread: () => dispatch({ type: "Unread" }),
      })
      // The box moving asks the conversation for its newest window, and the name stays meanwhile.
      const unmoved = asked.hearMoves(() => void talk.source.refresh())
      return {
        head: head(id, () => void talk.source.refresh()),
        chats: chats(id),
        transcript: { node: talk.transcript.content, update: () => {}, destroy: () => (unmoved(), talk.destroy()) },
        composer: { node: talk.composer.root, update: () => {} },
      }
    },
    masthead: (_, at) => at.head,
    now: () => el("p", { class: "legend memorylink" }, el("a", { href: hrefOf({ type: "memory" }) }, "What the secretary remembers")),
    around: (column, at) => el("div", { class: "chatswide" }, at.chats, column),
    view: (_, __, ___, at) => [at.transcript, at.composer],
  })

  export const mount = (root: HTMLElement, id: string, o: Options): Promise<Mounted> => frame.mount(root, page(id, o))

  // The draft's composer keeps what is typed under this key, which no conversation can be named.
  const DRAFT = "new"

  // A topic not asked anything yet, its value whether this login writes: its name is typed over the composer, and the
  // first send opens the conversation under that name and says the message in it, so the box keeps no conversation
  // nobody spoke in. Every send waits on the one open, so two sends open one conversation; an open that failed is asked
  // again by the next send.
  export const draft = (): frame.Page<boolean, boolean, "head" | "composer"> => ({
    className: "conversation",
    title: "New topic",
    reads: () => reading.constant(app.writes(app.now())),
    step: (_, writes) => writes,
    listen: dispatch => app.hear(app.writes, dispatch),
    islands: () => {
      const name = el("input", { type: "text", class: "renamefield topicname", maxlength: "80", placeholder: "Name it, or leave it blank", "aria-label": "Topic name" })
      const heading = el("h1", { class: "pagetitle" }, "New topic")
      const head = el("div", { class: "convhead" }, heading)
      const opened = once.of(() => openConversation(name.value))
      const send = (text: string, references: readonly Reference[], files: readonly AttachmentRow[]): Promise<void> =>
        opened
          .get()
          .then(p => say(p.id, text, references, files, 0))
          .then(s => address.replace({ type: "conversation", id: s.conversation.id, refs: [] }))
      const composer = mountComposer({ seeded: [], focus: true, onSend: send, onStop: () => "idle", again: { offer: () => again.none, edit: send }, draft: drafts.of(ui.tab, DRAFT) })
      return {
        head: { node: marks.head(head), update: writes => head.replaceChildren(writes ? name : heading) },
        composer: { node: composer.root, update: () => {}, destroy: composer.destroy },
      }
    },
    masthead: (_, at) => at.head,
    view: (_, __, ___, at) => at.composer,
  })
}
export default conversation
