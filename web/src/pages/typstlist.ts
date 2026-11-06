// The open list drawn by a Typst view compiled in this tab, beside today's list: the state the tab holds, a
// content-addressed tree, drawn by the one Typst page drawer; its controls answered by one listener over a closed
// vocabulary.

import app from "../app/app"
import onscreen, { store } from "../app/store"
import { ensureFold } from "../prodrome/sync"
import { perform } from "../todo/acts"
import { finish } from "../todo/todos"
import frame from "../view/frame"
import reading from "../algebra/reading"
import type { Mounted } from "../view/page"
import { toast } from "../view/toast"
import intent from "../typst/intent"
import typstpage from "../typst/page"
import viewstate from "../typst/state"
import viewtree from "../typst/tree"
import address from "../shell/address"

export namespace typstlist {
  export type T = {
    readonly store: onscreen.Fold
    readonly chain: string
    readonly open: string
    readonly writes: boolean
  }

  export type Event =
    | { readonly type: "Store"; readonly state: onscreen.Fold }
    | { readonly type: "Open"; readonly id: string }
    | { readonly type: "Writes"; readonly writes: boolean }

  export const step = (s: T, e: Event): T =>
    e.type === "Store" ? { ...s, store: e.state }
    : e.type === "Open" ? (s.open === e.id ? s : { ...s, open: e.id })
    : s.writes === e.writes ? s : { ...s, writes: e.writes }

  export const held = (v: T): viewstate.Held => ({ entries: onscreen.entries(v.store), at: onscreen.at(v.store) ?? "", chain: v.chain, open: v.open, writes: v.writes })

  export const view = typstpage.view<T, viewtree.Part>({
    page: "typst",
    module: "list.typ",
    tree: (v, t) => viewtree.of(viewstate.of(t.core, held(v)), t.named),
    part: viewtree.part,
  })

  const act = (on: string) => (i: intent.Intent): void => {
    if (i.todo.chain !== on) return void console.warn(`typst view: an intent for chain ${i.todo.chain} on ${on}`)
    intent.fold(i, {
      Open: t => void (address.go({ type: "typst", open: t.id })),
      Complete: t => {
        const e = onscreen.entry(store.state(), t.id)
        if (e === undefined) toast(`${t.id} is not on this list`)
        else void perform(finish(e), t.id, () => {})
      },
    })
  }

  export const page = (open: string, mountedAt: number): frame.Page<T, Event, "view"> => {
    const on = app.now().chain
    return {
      className: "typstpane",
      wide: true,
      title: "Todos, drawn by Typst",
      reads: () => reading.constant({ store: store.state(), chain: on, open, writes: app.writes(app.now()) }),
      step,
      listen: dispatch => {
        void ensureFold().catch((e: unknown) => toast(String(e)))
        const us = [
          store.subscribe(s => dispatch({ type: "Store", state: s })),
          app.hear(app.writes, writes => dispatch({ type: "Writes", writes })),
        ]
        return () => us.forEach(u => u())
      },
      islands: () => {
        const drawn = typstpage.island(view, "typstframe")
        const unlisten = intent.listen(drawn.node, { heard: act(on), refused: text => console.warn(`typst view: refused intent ${JSON.stringify(text)}`) })
        return {
          view: {
            node: drawn.node,
            update: v => {
              drawn.update(v)
              if (onscreen.loaded(v.store) && performance.getEntriesByName("typst:first-view").length === 0) performance.measure("typst:first-view", { start: mountedAt, end: performance.now() })
            },
            destroy: () => (unlisten(), drawn.destroy?.()),
          },
        }
      },
      absorb: (screen, dispatch) => {
        if (screen.type !== "typst") return false
        dispatch({ type: "Open", id: screen.open })
        return true
      },
      view: (_, __, ___, at) => at.view,
    }
  }

  export const mount = (root: HTMLElement, open: string): Mounted => frame.open(root, page(open, performance.now()))
}
export default typstlist
