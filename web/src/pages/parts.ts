// The parts a Rust page names (`part::Part`) and this tab draws; one that keeps listening says how to stop.

import { el } from "../view/dom"
import { checkIcon, chevronIcon, clipIcon, crossIcon, docIcon, lockIcon, plusIcon, searchIcon, sendIcon } from "../view/icons"
import marks from "../view/marks"
import loadmark from "../view/loadmark"
import heldvalue from "../view/heldvalue"
import { revealHeld } from "../wire/api"
import type { Mark, Part, Span } from "../wire/generated/pages"
import type { Entry } from "../wire/generated/wire"
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

  const mark = (m: Mark): Node => {
    switch (m) {
      case "outline":
        return marks.outline()
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
    }
  }

  // "all", not "open": hiding the done ones would make a source look like it produced nothing.
  const todos = (entries: readonly Entry[]): Node => {
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

  // The core's samples, with at least one from now on, or nothing to draw.
  const spanOf = (s: Span): curve.Span => {
    if (s.type === "Unsampled") return curve.unsampled
    const [x, ...rest] = s.drawn.ahead
    return x === undefined ? curve.unsampled : curve.sampled({ window: s.drawn.window, past: s.drawn.past, ahead: [x, ...rest] })
  }

  // Brought to new props in place, so its keyed DOM, focus and scroll survive a redraw.
  const list = typstpage.view<viewstate.Held, viewtree.Part>({
    page: "typst",
    module: "list.typ",
    tree: (v, t) => viewtree.of(viewstate.of(t.core, v), t.named),
    part: viewtree.part,
  })
  const islands = new WeakMap<Node, (v: viewstate.Held) => void>()
  const typstview = (v: viewstate.Held): Node => {
    const mounted = performance.now()
    const drawn = typstpage.island(list, "typstframe")
    const update = (v: viewstate.Held): void => {
      drawn.update(v)
      if (v.entries.length > 0 && performance.getEntriesByName("typst:first-view").length === 0) performance.measure("typst:first-view", { start: mounted, end: performance.now() })
    }
    update(v)
    islands.set(drawn.node, update)
    stops.set(drawn.node, () => drawn.destroy?.())
    return drawn.node
  }

  // Printed by the module from the generated `Part`, as the effects are (`run.ts`).
  const parsed = (props: string): Part => JSON.parse(props)

  // Only the Typst view keeps what it drew.
  export const refresh = (node: Node, _kind: string, _key: string, props: string): boolean => {
    const p = parsed(props)
    const update = p.type === "TypstView" ? islands.get(node) : undefined
    if (update === undefined || p.type !== "TypstView") return false
    update(p)
    return true
  }

  export const draw = (_kind: string, key: string, props: string): Node => {
    const p = parsed(props)
    switch (p.type) {
      case "Mark":
        return mark(p.mark)
      case "Pie":
        return pie.fulfilled(p.share)
      case "Rich":
        return richOrText(p.html, p.text, p.kind === null ? { class: p.class } : { class: p.class, "data-kind": p.kind })
      // A stranger's or a model's words: text nodes and built links only; a held value's placeholder becomes its chip.
      case "Prose": {
        const node = el(p.tag, { class: p.class }, ...(p.who === null ? [p.text] : textWithReferences(p.text, p.who)))
        return p.held ? heldvalue.within(node, revealHeld(p.conversation)) : node
      }
      case "Todos":
        return todos(p.entries)
      case "Elapsed":
        return elapsed(key, p.lead)
      case "Editor": {
        const c = editor.control({ cls: p.class, label: p.label }, p.rows, () => {})
        c.paint(p.text)
        c.node.querySelector("textarea")?.setAttribute("data-field", p.field)
        return c.node
      }
      case "TypstView":
        return typstview(p)
      case "Curve":
        return curve.fold(spanOf(p.span), { Unsampled: () => placeholder.view(placeholder.LINES), Sampled: drawn => curve.line(drawn, app.now().zone) })
    }
  }

  export const gone = (n: Node): void => {
    stops.get(n)?.()
    stops.delete(n)
  }
}
export default parts
