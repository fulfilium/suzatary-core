// A page of the static site: the shell every page shares around its body. No stylesheet and no
// script; `color-scheme` lets the browser's own defaults follow a dark theme. Every string is
// data, drawn as text.
#import "/schema/SitePage.typ" as page
#import "/schema/Expr.typ" as expr
#import "/schema/Fact.typ" as fact
#import "/schema/Tag.typ" as tags

#let el(name, ..kids) = html.elem(name, kids.pos().join(default: none))

#let named(t) = if tags.is-h1(t) { "h1" } else if tags.is-h2(t) { "h2" } else if tags.is-h3(t) { "h3" } else if tags.is-p(t) { "p" } else if tags.is-span(t) { "span" } else if tags.is-pre(t) { "pre" } else if tags.is-code(t) { "code" } else if tags.is-ul(t) { "ul" } else if tags.is-li(t) { "li" } else if tags.is-article(t) { "article" }

#let hashed(h) = h.clusters().chunks(8).map(c => c.join()).join(html.elem("wbr"))

#let draw(e) = if expr.is-Text(e) {
  expr.text(e)
} else if expr.is-Tag(e) {
  el(named(expr.tag(e)), ..expr.kids(e).map(draw))
} else if expr.is-Strong(e) {
  strong(expr.kids(e).map(draw).join(default: []))
} else if expr.is-Link(e) {
  link(expr.href(e), expr.kids(e).map(draw).join(default: []))
} else if expr.is-Hash(e) {
  hashed(expr.hash(e))
} else if expr.is-Facts(e) {
  el("dl", ..expr.pairs(e).map(f => el("dt", fact.name(f)) + el("dd", draw(fact.value(f)))))
} else if expr.is-Rows(e) {
  el(
    "table",
    el("thead", el("tr", ..expr.head(e).map(h => el("th", h)))),
    el("tbody", ..expr.rows(e).map(r => el("tr", ..r.map(c => el("td", draw(c)))))),
  )
}

#let d = json("/state.json")
#html.elem("html", attrs: (lang: "en"), {
  html.elem("head", {
    html.elem("meta", attrs: (charset: "utf-8"))
    html.elem("meta", attrs: (name: "viewport", content: "width=device-width, initial-scale=1"))
    html.elem("meta", attrs: (name: "color-scheme", content: "light dark"))
    html.elem("title", page.title(d))
  })
  html.elem("body", {
    el("nav", link(page.prefix(d) + "index.html", "todos"), " ", link(page.prefix(d) + "chain.html", "chain"), " ", link(page.prefix(d) + "objects/", "objects/"))
    el("main", ..page.body(d).map(draw))
    el("footer", page.footer(d))
  })
})
