// A page drawn by a Typst view, whatever the page: its state's content-addressed tree, folded by the view's one
// memoised algebra through the box's tripwire, placed in keyed DOM, so a change compiles and places only its spine;
// every redraw reports, per node and by depth, what hit, what was computed and what was placed.

import type { Core } from "../prodrome/core"
import { loadCore } from "../prodrome/core"
import memo from "../algebra/memo"
import reading from "../algebra/reading"
import table from "../algebra/table"
import lo from "../algebra/lo"
import { el } from "../view/dom"
import placeholder from "../view/placeholder"
import type frame from "../view/frame"
import { checked } from "../rich"
import compiler from "./compiler"
import draw from "./draw"
import kit from "./kit"
import place from "./place"
import once from "../algebra/once"

export namespace typstpage {
  export type Spec<S, A> = {
    readonly page: string
    readonly module: kit.View
    readonly tree: (s: S, tools: Tools) => memo.Tree<A>
    readonly part: (a: A) => draw.Part
  }
  export type View<S, A> = Spec<S, A> & { readonly fold: (t: Tools) => Promise<Fold<A>> }

  // A view's fold: its algebra and the table it is tabulated in, kept for the tab's life, so a page mounted again
  // over a state it drew hits at the root.
  export type Fold<A> = { readonly algebra: memo.Algebra<A, draw.Drawn>; readonly cache: table.Table<draw.Drawn> }

  export type Tools = {
    readonly core: Core
    readonly named: memo.Namer
    readonly made: <A>(module: kit.View, part: (a: A) => draw.Part) => Promise<Fold<A>>
  }

  // A view, made once per tab: its fold is made the first time it is drawn and kept, and a fold that failed is made
  // again the next time.
  export const view = <S, A>(spec: Spec<S, A>): View<S, A> => {
    let made: once.T<Fold<A>> | null = null
    return { ...spec, fold: t => (made ??= once.of(() => t.made(spec.module, spec.part))).get() }
  }

  // How many nodes the redraw under way compiled: User Timing's, never read by the app.
  let compiled = 0

  const tools: reading.Reading<Tools> = reading.once(async (): Promise<Tools> => {
    const t0 = performance.now()
    const [typst, core] = await Promise.all([compiler.load(), loadCore()])
    performance.measure("typst:load", { start: t0, end: performance.now(), detail: typst.phases })
    const named = memo.tabulatedNamer((own, kids) => core.memo_name(own, kids), table.of(table.lru(8192)))
    const made = <A>(module: kit.View, part: (a: A) => draw.Part): Promise<Fold<A>> =>
      kit.load(module).then((m): Fold<A> => {
        const fx: draw.Effects = { world: typst.world(m.files), main: m.main, door: h => checked(h, core.suspicious), owned: table.of(table.lru(2048)), compiled: n => (compiled += n) }
        return { algebra: draw.algebra(kit.name(m, named), fx, part), cache: table.of(table.lru(4096)) }
      })
    return { core, named, made }
  })

  // What a redraw computed, node by node, and what it placed: its cost is the spines that moved.
  export const report = <A>(t: memo.Tree<A>, r: memo.Report, part: (a: A) => draw.Part) => {
    const index = new Map<memo.Name, draw.Part>()
    const walk = (n: memo.Tree<A>): void => (index.set(n.name, part(n.v)), n.kids.forEach(walk))
    walk(t)
    const computed = r.visits.filter(x => x.outcome !== "Hit").map(x => {
      const p = index.get(x.name)
      return { type: p?.kind ?? "", key: p?.key ?? null, depth: x.depth, ms: x.ms, finding: x.outcome === "Finding" }
    })
    return { computed, byDepth: memo.byDepth(r), waves: r.waves }
  }

  // `s` drawn into `host`; the User Timing measure is read by the e2e scenes that measure pages, never by the app.
  export const into = <S, A>(host: Element, v: View<S, A>, tools: Tools, f: Fold<A>, s: S): void => {
    const t0 = performance.now()
    const t = v.tree(s, tools)
    const t1 = performance.now()
    compiled = 0
    const [d, r] = memo.fold(f.algebra, t, f.cache)
    const t2 = performance.now()
    const placed = place.into(host, d)
    const t3 = performance.now()
    performance.measure("typst:draw", { start: t0, end: t3, detail: { page: v.page, tree: t1 - t0, fold: t2 - t1, place: t3 - t2, compiled, placed, ...report(t, r, v.part) } })
  }

  // The view, an island of a frame page: handed every value, drawn once the tab's Typst is here, the latest value
  // then; a node that fails to compile is said in its place.
  export const island = <S, A>(v: View<S, A>, cls: string): frame.Island<S> & { readonly node: HTMLElement } => {
    const loading = placeholder.view(placeholder.ROWS)
    const host = el("div", { class: cls }, loading)
    let latest: { readonly s: S } | null = null
    let ready: { readonly tools: Tools; readonly fold: Fold<A> } | null = null
    const drawn = (): void => (ready !== null && latest !== null ? (loading.remove(), into(host, v, ready.tools, ready.fold, latest.s)) : undefined)
    const arrived = (): void =>
      lo.fold(tools.now(), {
        loading: () => {},
        error: e => void host.replaceChildren(el("p", { class: "problem" }, `Typst did not load: ${e}`)),
        val: t =>
          void v.fold(t).then(
            fold => ((ready = { tools: t, fold }), drawn()),
            (e: unknown) => void host.replaceChildren(el("p", { class: "problem" }, `The view did not load: ${String(e)}`)),
          ),
      })
    const unheard = tools.hear(arrived)
    arrived()
    return { node: host, update: s => ((latest = { s }), drawn()), destroy: unheard }
  }
}
export default typstpage
