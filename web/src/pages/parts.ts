// The parts a Rust page names and this tab draws (`Foreign` in the view): the marks drawn in SVG, a mail body with its
// held values' chips, and the todos a message produced as the list's own cards. Each is drawn from its props alone; a
// part that keeps listening says how to stop, which the painter asks when it takes the part out.

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

export namespace parts {
  const stops = new WeakMap<Node, () => void>()

  const mark = (name: string): Node => {
    switch (name) {
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

  export const draw = (kind: string, key: string, props: string): Node => {
    switch (kind) {
      case "mark":
        return mark(key)
      // A stranger's prose, set as text; only a held value's placeholder becomes its chip.
      case "mailbody":
        return heldvalue.within(el("div", { class: "mailbody" }, props), revealHeld(null))
      case "todos":
        return todos(props)
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
