// A store's open list as a static page, in the classes of the app's own list: a row per open todo,
// its price and its words, the whole row a link to the todo's page. Every datum is drawn as text.
#import "/page.typ": el, bare
#import "/schema/TodosPage.typ" as page
#import "/schema/Listed.typ" as listed
#import "/schema/OpenRow.typ" as row
#import "/priority.typ": price

#let about(r) = (
  row.id(r), row.category(r), row.kind(r) + " by " + row.authored_by(r),
  ..if row.waiting_on(r) != none { ("waiting on " + row.waiting_on(r),) },
).join(" · ")

#let line(l) = {
  let r = listed.row(l)
  el(
    "li", "trow",
    html.elem("a", attrs: (class: "topen", href: listed.page(l)), (
      el("span", "tmark", el("span", "tpct", price(row.priority(r)))),
      bare("span", el("span", "ttitle", row.body(r)), el("span", "tcat", about(r))),
      if row.unconfirmed(r) { el("span", "tchip", "unconfirmed") },
    ).filter(k => k != none).join()),
  )
}

#let d = json("/state.json")
#el(
  "section", "tlist",
  bare("h1", "Open todos"),
  el("p", "tcount", str(page.rows(d).len()) + " open, priced as of " + page.at(d) + ". Low is urgent."),
  if page.rows(d).len() > 0 { el("ol", "trows", ..page.rows(d).map(line)) },
)
