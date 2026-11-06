// The parts a Rust page names and this tab draws (`Foreign` in the view), each from its props alone. A part that keeps
// listening says how to stop, which the painter asks when it takes the part out.

import { el } from "../view/dom"
import { checkIcon, chevronIcon, clipIcon, crossIcon, docIcon, lockIcon, plusIcon, searchIcon, sendIcon } from "../view/icons"
import marks from "../view/marks"
import loadmark from "../view/loadmark"
import heldvalue from "../view/heldvalue"
import { revealHeld } from "../wire/api"
import d from "../wire/decoder"
import lo from "../algebra/lo"
import payload from "../wire/generated/payload"
import app from "../app/app"
import time from "../app/time"
import onscreen, { store } from "../app/store"
import { cardList } from "../todo/card"
import { DEFAULT_FILTER, listing } from "../todo/todos"
import pie from "../view/pie"
import { richOrText } from "../rich"
import { textWithReferences } from "../chat/refs"
import editor from "../typst/editor"
import curve from "../todo/curve"
import placeholder from "../view/placeholder"
import typstpage from "../typst/page"
import viewstate from "../typst/state"
import viewtree from "../typst/tree"

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
      case "search":
        return searchIcon()
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

  // A Typst field, opened on its words; what is typed reaches the page as the words of the field inside the part.
  const editorOf = d.obj({ field: d.str, class: d.str, label: d.str, rows: d.num, text: d.str })
  // The thirty days as the core sampled them (`thirty.Thirty`'s drawn arm).
  const sample = d.obj({ t: d.num, v: d.num })
  const samples = d.bind(d.arr(sample), (xs): lo.Settled<curve.Samples> => {
    const [x, ...rest] = xs
    return x === undefined ? lo.error("no sample from now on") : lo.val([x, ...rest])
  })
  const drawnOf = d.obj({
    type: d.oneOf({ Drawn: null }),
    span: d.sum<curve.Span, "type">("type", {
      Unsampled: {},
      Sampled: { drawn: d.obj({ window: d.obj({ from: d.num, now: d.num, to: d.num }), past: d.arr(sample), ahead: samples }) },
    }),
  })

  // The open list as the Typst view draws it, from what the tab holds (`viewstate.Held`), kept and brought to new props in
  // place, so its keyed DOM, focus and scroll survive a redraw. Its controls' intents reach the page around it.
  const list = typstpage.view<viewstate.Held, viewtree.Part>({
    page: "typst",
    module: "list.typ",
    tree: (v, t) => viewtree.of(viewstate.of(t.core, v), t.named),
    part: viewtree.part,
  })
  const heldOf = d.obj({ entries: d.arr(payload.Entry), at: d.str, chain: d.str, open: d.str, writes: d.bool })
  const islands = new WeakMap<Node, (v: viewstate.Held) => void>()
  const typstview = (props: string): Node => {
    const mounted = performance.now()
    const drawn = typstpage.island(list, "typstframe")
    const update = (v: viewstate.Held): void => {
      drawn.update(v)
      if (v.entries.length > 0 && performance.getEntriesByName("typst:first-view").length === 0) performance.measure("typst:first-view", { start: mounted, end: performance.now() })
    }
    update(read("typstview", heldOf, props))
    islands.set(drawn.node, update)
    stops.set(drawn.node, () => drawn.destroy?.())
    return drawn.node
  }

  // A part brought to new props where it stands, when it can be: only the Typst view keeps what it drew.
  export const refresh = (node: Node, kind: string, _key: string, props: string): boolean => {
    const update = kind === "typstview" ? islands.get(node) : undefined
    if (update === undefined) return false
    update(read(kind, heldOf, props))
    return true
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
      case "editor": {
        const e = read(kind, editorOf, props)
        const c = editor.control({ cls: e.class, label: e.label }, e.rows, () => {})
        c.paint(e.text)
        c.node.querySelector("textarea")?.setAttribute("data-field", e.field)
        return c.node
      }
      case "typstview":
        return typstview(props)
      case "curve": {
        const t = read(kind, drawnOf, props)
        return curve.fold(t.span, { Unsampled: () => placeholder.view(placeholder.LINES), Sampled: drawn => curve.line(drawn, app.now().zone) })
      }
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
