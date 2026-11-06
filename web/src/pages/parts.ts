// The parts a Rust page names and this tab draws (`Foreign` in the view), each from its props alone. A part that keeps
// listening says how to stop, which the painter asks when it takes the part out.

import { el } from "../view/dom"
import { checkIcon, chevronIcon, clipIcon, crossIcon, docIcon, lockIcon, plusIcon, sendIcon } from "../view/icons"
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
import pie from "../view/pie"
import { richOrText } from "../rich"
import { textWithReferences } from "../chat/refs"

export namespace parts {
  const stops = new WeakMap<Node, () => void>()

  // By key, so new words (a verb, a count) keep its count; forgotten once no node stands for it.
  const clocks = new Map<string, { readonly from: number; node: Node }>()
  const elapsed = (key: string, lead: string): Node => {
    const node = el("span", { class: "elapsed" })
    const known = clocks.get(key)
    const from = known?.from ?? time.now()
    clocks.set(key, { from, node })
    const tell = (): void => void (node.textContent = `${lead}${Math.max(0, Math.floor((time.now() - from) / 1000))} s`)
    tell()
    const every = setInterval(tell, 1000)
    stops.set(node, () => {
      clearInterval(every)
      if (clocks.get(key)?.node === node) clocks.delete(key)
    })
    return node
  }

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
      case "diamond":
        return marks.diamond()
      case "send":
        return sendIcon()
      case "clip":
        return clipIcon()
      case "check":
        return checkIcon()
      case "cross":
        return crossIcon()
      case "plus":
        return plusIcon()
      case "doc":
        return docIcon()
      case "lock":
        return lockIcon()
      default:
        throw new Error(`no mark ${name}`)
    }
  }

  // "all", not "open": hiding the done ones would make a source look like it produced nothing.
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

  // Props that do not read are the module's mistake, thrown as one.
  const read = <A>(kind: string, decoder: d.Decoder<A>, props: string): A => {
    const r = d.json(decoder)(props)
    if (r.type === "Error") throw new Error(`the ${kind} a page named does not read: ${r.e}`)
    return r.v
  }

  const tags = { div: null, p: null, pre: null, span: null }
  const speakers = { person: null, model: null, verb: null, act: null, box: null }
  const proseOf = d.obj({
    tag: d.oneOf(tags),
    class: d.str,
    text: d.str,
    who: d.nullable(d.oneOf(speakers)),
    held: d.bool,
    conversation: d.nullable(d.str),
  })
  const richOf = d.obj({ html: d.nullable(d.str), text: d.str, class: d.str, kind: d.nullable(d.str) })

  export const draw = (kind: string, key: string, props: string): Node => {
    switch (kind) {
      case "mark":
        return mark(key, props)
      case "rich": {
        const r = read(kind, richOf, props)
        return richOrText(r.html, r.text, r.kind === null ? { class: r.class } : { class: r.class, "data-kind": r.kind })
      }
      // A stranger's or a model's words: text nodes and built links only; a held value's placeholder becomes its chip.
      case "prose": {
        const p = read(kind, proseOf, props)
        const node = el(p.tag, { class: p.class }, ...(p.who === null ? [p.text] : textWithReferences(p.text, p.who)))
        return p.held ? heldvalue.within(node, revealHeld(p.conversation)) : node
      }
      case "todos":
        return todos(props)
      case "elapsed":
        return elapsed(key, props)
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
