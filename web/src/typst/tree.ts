// A view's state, a content-addressed tree: each node named by its own content and its children's names, so
// a change renames exactly the path from it to the root, and the rows a balanced sequence under the list.

import memo from "../algebra/memo"
import type draw from "./draw"
import viewstate from "./state"

export namespace viewtree {
  type Open = Extract<viewstate.Opened, { readonly type: "Todo" }>

  // What each node draws on its own; its children go inside it, and a run of rows draws nothing but its rows.
  export type Node =
    | { readonly type: "View" }
    | { readonly type: "List" }
    | { readonly type: "Count"; readonly n: number }
    | { readonly type: "Rows" }
    | { readonly type: "Run" }
    | { readonly type: "Row"; readonly row: viewstate.Row }
    | { readonly type: "InPlace"; readonly open: Open }
    | { readonly type: "Pane"; readonly open: Open }

  // A node and the bytes it is named by, the same bytes the kit is handed.
  export type Part = { readonly node: Node; readonly own: string }
  export type Tree = memo.Tree<Part>

  // Which thing a node is among its siblings, across versions of it, where its name says which version: a row is
  // its todo, any other node its kind.
  export const key = (n: Node): string => (n.type === "Row" ? n.row.ref : n.type)

  export const part = (p: Part): draw.Part => ({ type: p.node.type === "Run" ? "Run" : "Drawn", kind: p.node.type, key: key(p.node), own: p.own })

  // Least fulfilled first, a todo with no number last, ties by reference, so a redraw never shuffles.
  export const byUrgency = (rs: readonly viewstate.Row[]): viewstate.Row[] =>
    [...rs].sort((a, b) => (a.value ?? 2) - (b.value ?? 2) || (a.ref < b.ref ? -1 : a.ref > b.ref ? 1 : 0))

  export const of = (s: viewstate.State, named: memo.Namer): Tree => {
    const at = (node: Node, kids: Tree[] = []): Tree => {
      const own = JSON.stringify(node)
      return memo.node(named, own, { node, own }, kids)
    }
    const open = s.open.type === "Todo" ? s.open : null
    // Runs close by the rows' references, so the shape is the list's order and a row's content moves no cut.
    const rows = memo.balance(
      byUrgency(s.rows).map(row => ({
        t: at({ type: "Row", row }, open !== null && open.row.ref === row.ref ? [at({ type: "InPlace", open })] : []),
        cut: named(row.ref, []),
      })),
      x => x.cut,
      run => ({ t: at({ type: "Run" }, run.map(x => x.t)), cut: named("run", run.map(x => x.cut)) }),
    )
    return at({ type: "View" }, [
      at({ type: "List" }, [at({ type: "Count", n: s.rows.length }), at({ type: "Rows" }, rows === undefined ? [] : [rows.t])]),
      ...(open === null ? [] : [at({ type: "Pane", open })]),
    ])
  }
}
export default viewtree
