// The parts a Rust page names and this tab draws (`Foreign` in the view): the marks drawn in SVG, Typst's HTML through the
// tripwire, a mail body with its held values' chips, the todos a message produced as the list's own cards, an answer's
// blocks as the conversation draws them (until the conversation is a program too), and the Inbox's proposal cards (until
// the Inbox is). Each is drawn from its props alone; a part that keeps listening says how to stop, which the painter
// asks when it takes the part out.

import { el } from "../view/dom"
import { chevronIcon } from "../view/icons"
import marks from "../view/marks"
import loadmark from "../view/loadmark"
import heldvalue from "../view/heldvalue"
import { revealHeld } from "../wire/api"
import d from "../wire/decoder"
import payload from "../wire/generated/payload"
import app from "../app/app"
import time from "../app/time"
import onscreen, { store } from "../app/store"
import { cardList } from "../todo/card"
import { DEFAULT_FILTER, listing } from "../todo/todos"
import type { Row } from "../inbox/inbox"
import { proposalCard } from "../inbox/proposal"
import kept from "../inbox/kept"
import local from "./local"
import pie from "../view/pie"
import { richOrText } from "../rich"
import { answerBlocks, answerCopies, more } from "../chat/transcript"

export namespace parts {
  const stops = new WeakMap<Node, () => void>()

  const mark = (name: string, props: string): Node => {
    switch (name) {
      case "outline":
        return marks.outline()
      case "pie":
        return pie.fulfilled(Number(props))
      case "chevron":
        return chevronIcon()
      case "rule":
        return marks.rule()
      case "loadmark":
        return loadmark.view()
      default:
        throw new Error(`no mark ${name}`)
    }
  }

  // The box's entries for one message, drawn as the list's cards with their acts: "all", not "open", since hiding the
  // done ones would make a source look like it produced nothing. Each draw reads the tab's own entry where it holds one,
  // so an act's answer shows; the actor arriving draws the buttons.
  const todos = (json: string): Node => {
    const read = d.json(d.arr(payload.Entry))(json)
    if (read.type === "Error") throw new Error(`the todos a page named do not read: ${read.e}`)
    const entries = read.v
    const cards = cardList({ acted: () => draw() }, "This message produced no todos.")
    const draw = (): void => {
      const shown = listing(
        entries.map(e => onscreen.entry(store.state(), e.todo) ?? e),
        { ...DEFAULT_FILTER, state: "all" },
        time.moment(),
      )
      cards.update(shown.priced, shown.unpriced)
    }
    draw()
    stops.set(cards.node, app.hear(app.actor, draw))
    return cards.node
  }

  // The box's own cards, whose sources name earlier cards and never a todo.
  const card = (r: Row, taken?: () => void): HTMLElement => {
    if (r.kind !== "proposal") throw new Error("a card a page named is not a proposal")
    return proposalCard(r.conversation.id, r.proposal, { titles: new Map(), ...(taken === undefined ? {} : { taken }) })
  }

  // A grant drawn inside the link's card it was asked for: allowing the origin sends the link again, once. The props'
  // first line is the grant's reference.
  const allowing = (link: Row, grant: Row): HTMLElement => {
    const node = card(link)
    node.appendChild(el("div", { class: "proposalallow" }, card(grant, () => kept.resend(link))))
    return node
  }
  // A part's props as `app/pages` prints them; props that do not read are the module's mistake, thrown as one.
  const read = <A>(kind: string, decoder: d.Decoder<A>, props: string): A => {
    const r = d.json(decoder)(props)
    if (r.type === "Error") throw new Error(`the ${kind} a page named does not read: ${r.e}`)
    return r.v
  }
  const turn = d.obj({ text: d.str, blocks: d.arr(payload.BlockRow) })

  export const draw = (kind: string, key: string, props: string): Node => {
    switch (kind) {
      case "mark":
        return mark(key, props)
      case "rich": {
        const r = read(kind, d.obj({ html: d.nullable(d.str), text: d.str, class: d.str }), props)
        return richOrText(r.html, r.text, { class: r.class })
      }
      case "answer":
        return answerBlocks(read(kind, turn, props))
      case "answercopies":
        return more(answerCopies(read(kind, turn, props))) ?? document.createTextNode("")
      // A stranger's prose, set as text; only a held value's placeholder becomes its chip.
      case "mailbody":
        return heldvalue.within(el("div", { class: "mailbody" }, props), revealHeld(null))
      case "todos":
        return todos(props)
      case "proposal":
        return card(local.row(key))
      case "allowing":
        return allowing(local.row(key), local.row(props.split("\n")[0] ?? ""))
      default:
        throw new Error(`no part ${kind}`)
    }
  }

  export const gone = (n: Node): void => {
    stops.get(n)?.()
    stops.delete(n)
  }
}
export default parts
