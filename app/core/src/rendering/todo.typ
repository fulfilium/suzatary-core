// A todo's reading part: the header, the detail, and "Why this number". Every datum is a string
// of the value, drawn as text; only a trusted record's markup (`Rendered`) is evaluated.
#import "/page.typ": el, bare, fold
#import "/schema/TodoPage.typ" as page
#import "/schema/Head.typ" as head
#import "/schema/Fulfil.typ" as fulfil
#import "/schema/Chip.typ" as chip
#import "/schema/ChipKind.typ" as kind
#import "/schema/Prose.typ" as prose
#import "/schema/Why.typ" as why
#import "/schema/Part.typ" as part

#let shown(p, class) = if prose.is-Rendered(p) {
  el("div", class + " rich", eval(prose.source(p), mode: "markup"))
} else if prose.is-Source(p) {
  el("div", class + " assource", prose.text(p))
}

#let class(k) = if kind.is-plain(k) { "chip" } else if kind.is-done(k) { "chip done" } else if kind.is-cat(k) { "chip cat" } else if kind.is-kind(k) { "chip kind" } else if kind.is-prov(k) { "chip prov" }

#let number(f) = if fulfil.is-Priced(f) {
  el("span", "todopct", fulfil.shown(f))
} else if fulfil.is-Unpriced(f) {
  el("span", "chip", "unpriced")
} else if fulfil.is-Unlinked(f) {
  el("span", "chip", "does not link")
}

#let header(h) = el(
  "div", "todohead",
  el("div", "todofulfil", number(head.fulfil(h))),
  if head.body(h) != none { el("div", "todolead", shown(head.body(h), "todobody")) },
  el("code", "todoid", head.id(h)),
  el("div", "cardmeta", ..head.chips(h).map(c => el("span", class(chip.kind(c)), chip.label(c)))),
  if head.note(h) != none { el("p", "legend", head.note(h)) },
)

#let item(p) = bare(
  "li",
  el("div", "node", part.says(p)),
  if part.parts(p).len() > 0 { el("ul", "tree", ..part.parts(p).map(item)) },
)

#let reasons(w) = if why.is-Unpriced(w) {
  el("section", "whytree", bare("h2", "Why this number"), bare("p", why.says(w)))
} else if why.is-Priced(w) {
  let parts = why.parts(w)
  el(
    "section", "whytree",
    bare("h2", why.heading(w)),
    el("p", "says", why.says(w)),
    ..why.passes(w).map(line => el("p", "pass", line)),
    if parts.len() > 0 { el("ul", "tree", ..parts.map(item)) },
    if parts.len() > 0 { el("p", "legend", "Each part is priced at the moment its parent read it. The box computed every number.") },
  )
}

#let d = json("/state.json")
#{
  header(page.head(d))
  if page.detail(d) != none { fold("Detail", shown(page.detail(d), "todobody")) }
  reasons(page.why(d))
}
