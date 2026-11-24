// A Typst view's draw, one memoised fold's algebra: a wave of nodes compiled at once by the view, each node one
// element through the box's tripwire and the door, never placed itself (`place` copies it).

import type memo from "../algebra/memo"
import table from "../algebra/table"
import parsed from "../algebra/parsed"
import type { Checked } from "../rich"
import { el } from "../view/dom"
import compiler from "./compiler"

export namespace draw {
  // A node's own bytes for the kit (`Drawn`), or its children alone (`Run`).
  export type Part =
    | { readonly type: "Drawn"; readonly kind: string; readonly key: string; readonly own: string }
    | { readonly type: "Run"; readonly kind: string; readonly key: string; readonly own: string }

  // A node's own element with its children's drawings inside it; or, for a run of rows, its children in its place.
  export type Drawn =
    | {
        readonly type: "Element"
        readonly key: string
        readonly name: memo.Name
        readonly own: string
        readonly el: Element
        readonly kids: readonly Drawn[]
      }
    | { readonly type: "Run"; readonly kids: readonly Drawn[] }

  // The door, handed in: a compiled page's body elements, or the tripwire's reason.
  export type Door = (html: string) => Checked

  const problem = (reason: string): Element => el("div", { class: "problem typstproblem" }, reason)

  // `owned` tabulates each node's element by its own content, so a node renamed by its children alone compiles nothing.
  export type Effects = {
    readonly world: compiler.World
    readonly main: string
    readonly door: Door
    readonly owned: table.Table<Element>
    readonly compiled: (nodes: number) => void
  }

  // One compile for the wave's nodes whose own element is not held. A wave that fails, is refused or draws other
  // than one element per node is compiled again node by node, so a failure is its own node's and the rest draws.
  export const algebra = <A>(name: memo.Name, fx: Effects, part: (a: A) => Part): memo.Algebra<A, Drawn> => {
    const elements = (owns: readonly string[]): parsed.T<Element[]> =>
      compiler.fold(fx.world.compile(fx.main, `[${owns.join(",")}]`), {
        html: h => {
          const c = fx.door(h)
          return c.type === "Refused" ? parsed.refused(c.reason)
            : c.elements.length === owns.length ? parsed.ok(c.elements)
            : parsed.refused(`the view drew ${c.elements.length} elements for ${owns.length} nodes`)
        },
        failed: es => parsed.refused(`the view did not compile: ${es.join("; ")}`),
      })
    const alone = (own: string): Element => parsed.fold(elements([own]), { Ok: es => es[0] ?? problem("the view drew nothing"), Refused: problem })
    return {
      name,
      apply: ls => {
        const parts = ls.map(l => part(l.v))
        const owns = [...new Set(parts.flatMap(p => (p.type === "Drawn" ? [p.own] : [])))]
        const held = new Map<string, Element>()
        for (const o of owns) {
          const e = table.hit(fx.owned.get(o))
          if (e !== undefined) held.set(o, e)
        }
        const missing = owns.filter(o => !held.has(o))
        fx.compiled(missing.length)
        const made = missing.length === 0 ? [] : parsed.fold(elements(missing), { Ok: es => es, Refused: () => missing.map(alone) })
        missing.forEach((o, i) => {
          const e = made[i] ?? problem("the view drew nothing")
          held.set(o, e)
          fx.owned.insert(o, e)
        })
        return ls.map((l, i): Drawn => {
          const p = parts[i] ?? part(l.v)
          return p.type === "Run" ? { type: "Run", kids: l.rs }
            : { type: "Element", key: p.key, name: l.name, own: p.own, el: held.get(p.own) ?? problem("the view drew nothing"), kids: l.rs }
        })
      },
    }
  }
}
export default draw
