// Suzatary's Typst kit for views compiled in the tab: rows, and the fulfillment marks of the owner's design system.
//
// Every datum arrives in the state as a JSON string, and every text as a `Text`: `(type: "Prose", text)`,
// printed as text, or `(type: "Markup", source)`, a trusted actor's confirmed markup the box rendered, read
// as markup by `shown` and nowhere else. A control is an element carrying `data-intent`, a printed intent the
// shell parses against its closed vocabulary; nothing here runs on its own.

#import "@local/typst-design:0.1.0": fulfillment-color, percent, pie

// A ratio in [0, 1] as the owner's scale samples it, printed for CSS in the space it was sampled in.
#let colour(v) = {
  let (l, c, h, ..) = oklch(fulfillment-color(v)).components()
  "oklch(" + str(calc.round(l / 1%, digits: 2)) + "% " + str(calc.round(c, digits: 4)) + " " + str(calc.round(h / 1deg, digits: 2)) + ")"
}

// The value's colour as a custom property: `--c` colours the row's mark and its number.
#let tint(v) = if v == none { "--c: var(--ink-muted)" } else { "--c: " + colour(v) }

#let el(tag, attrs: (:), ..body) = html.elem(tag, attrs: attrs, body.pos().join())

// The package's pie, drawn as an inline SVG the number beside it says for a reader; a row with no number has none,
// since no value is not a zero.
#let mark(v) = if v != none { el("span", attrs: ("aria-hidden": "true"), pie(v, size: 1em)) }

#let intent(verb, ref) = verb + " " + ref

// A text from the state is never an attribute value, only element text: attributes carry references and numbers.
#let button(cls, verb, ref, ..body) = el("button", attrs: (type: "button", class: cls, "data-intent": intent(verb, ref)), ..body)

#let unseen(text) = el("span", attrs: (class: "tunseen"), text)

// `eval` sees the standard library and the files handed to this compile, nothing of the box or the network;
// what it makes still passes the box's tripwire with the rest of the page. Inside a control a link would be
// a target within a target, so there `links: false` keeps its words and drops the anchor.
#let shown(t, links: true) = if t.type == "Markup" {
  if links { eval(t.source, mode: "markup") } else { show link: it => it.body; eval(t.source, mode: "markup") }
} else { t.text }

#let blank(t) = t.type == "Prose" and t.text == ""

// What an open todo shows, in place inside its row, so nothing above the tapped row moves.
#let opened(o) = (
  el("p", attrs: (class: "tpanehead"), mark(o.row.value), el("span", attrs: (class: "tpct"), percent(o.row.value)), el("span", attrs: (class: "tcat"), o.row.category)),
  el("h2", attrs: (class: "tpanetitle"), shown(o.row.title)),
  if not blank(o.detail) { el("div", attrs: (class: "tdetail"), shown(o.detail)) },
  ..o.why.map(w => el("p", attrs: (class: "twhy"), shown(w))),
  if o.created != "" { el("p", attrs: (class: "tcreated"), "created " + o.created) },
  if o.row.complete { button("tdone tpanedone", "complete", o.row.ref, "Done", unseen([: #shown(o.row.title, links: false)])) },
)

// One row: the mark in a narrow gutter, the title beside it, clamped by the stylesheet (the open todo and the
// control's accessible name keep the whole of it); the whole row opens the todo, the act beside it completes
// it, and neither moves when the other is pressed. The open todo's row holds what it shows under both, placed
// by the shell.
#let row(r) = el(
  "li",
  attrs: (class: "trow", style: tint(r.value)),
  button("topen", "open", r.ref,
    el("span", attrs: (class: "tmark"), mark(r.value), el("span", attrs: (class: "tpct"), percent(r.value))),
    el("span", attrs: (class: "ttitle"), shown(r.title, links: false)),
    if r.unconfirmed { el("span", attrs: (class: "tchip"), "unconfirmed") },
  ),
  if r.complete { button("tdone", "complete", r.ref, "Done", unseen([: #shown(r.title, links: false)])) },
)
