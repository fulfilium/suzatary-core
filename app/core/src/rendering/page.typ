// The vocabulary every page is drawn in: an element with a class, and one without. Ours, so it
// may use the `html` module a model's block may not. A child that is `none` is left out.

#let kids(body) = body.pos().filter(k => k != none).join()
#let el(tag, class, ..body) = html.elem(tag, attrs: (class: class), kids(body))
#let bare(tag, ..body) = html.elem(tag, kids(body))
#let chevron = html.elem("svg", attrs: (viewBox: "0 0 24 24", width: "22", height: "22", fill: "none", stroke: "currentColor", "stroke-width": "1.7", "stroke-linecap": "round", "stroke-linejoin": "round", "aria-hidden": "true"), html.elem("path", attrs: (d: "M8.5 5.5l7 6.5-7 6.5")))
#let fold(summary, ..body) = html.elem("details", attrs: (class: "why", open: ""), { html.elem("summary", { summary; chevron }); kids(body) })
