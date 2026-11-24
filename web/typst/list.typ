// The open todo list as a view: one function per node of the state's tree (`web/src/typst/tree.ts`), each one
// element, which the shell places with its children inside; compiled over a wave of nodes, one element each, so a
// change compiles only the nodes it renamed.

#import "/kit.typ": *

#let draw(n) = (
  View: n => el("div", attrs: (class: "tview")),
  List: n => el("section", attrs: (class: "tlist", "aria-label": "open todos")),
  Count: n => el("p", attrs: (class: "tcount"), str(n.n) + " open"),
  Rows: n => el("ol", attrs: (class: "trows")),
  Row: n => row(n.row),
  InPlace: n => el("div", attrs: (class: "tinplace"), ..opened(n.open)),
).at(n.type)(n)

#for n in json("/state.json") { draw(n) }
