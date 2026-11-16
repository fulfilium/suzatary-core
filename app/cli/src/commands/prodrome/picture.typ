// A public store's open list as one picture (`prodrome draw`): typst's HTML export of one frame,
// which typst draws as SVG. Each row is the design system's pie and percent and the todo's body as
// plain text the box made of it, clipped to the row, so no body draws past the picture's edge.
// Every datum here is data: nothing is evaluated.
#import "@local/typst-design:0.1.0": fulfillment-color, percent, pie
#import "/tokens.typ": tokens
#import "/schema/Picture.typ" as picture
#import "/schema/Theme.typ" as theme
#import "/schema/Listed.typ" as listed
#import "/schema/Line.typ" as line
#import "/schema/Price.typ" as price

#let d = json("/state.json")
#let t = if theme.is-Dark(picture.theme(d)) { tokens.dark } else { tokens.light }
#set text(font: "Libertinus Serif", size: 13pt, fill: t.ink, lang: "en")

#let quiet(body) = text(fill: t.ink-muted, size: 0.85em, body)

#let mark(p) = if price.is-Valued(p) { pie(price.value(p), size: 0.9em) }
#let number(p) = if price.is-Valued(p) {
  text(fill: fulfillment-color(price.value(p)), weight: "bold", percent(price.value(p)))
} else if price.is-Absent(p) { quiet(percent("absent")) } else { quiet(percent(none)) }

#let title(r) = block(width: 100%, clip: true, inset: (y: 0.2em), {
  line.title(r)
  if line.records(r) > 1 { quiet[ · one of #line.records(r) records] }
})

#let list = picture.list(d)
#html.frame(block(width: 400pt, fill: t.ground, inset: 14pt, {
  if listed.is-Begun(list) {
    quiet[Nothing is open: the store holds no task yet.]
  } else {
    let rows = listed.rows(list)
    let open = rows.len() + listed.more(list)
    let priced = [priced at #listed.at(list) UTC]
    quiet(if open == 0 [Nothing is open · #priced] else [#open open, most urgent first · #priced])
    v(6pt)
    grid(
      columns: (1.1em, 2.8em, 1fr),
      column-gutter: 5pt,
      row-gutter: 4pt,
      align: (left + horizon, right + horizon, left + horizon),
      ..rows.map(r => (mark(line.price(r)), number(line.price(r)), title(r))).flatten(),
    )
    if listed.more(list) > 0 { v(4pt); quiet[and #listed.more(list) more on the page this picture links to] }
  }
}))
