// One block of the secretary's answer, compiled in the closed world. The model may call the
// owner's design system by these names; each draws with typst's own elements, since the
// package's own draw with shapes HTML export drops.
#import "/schema/Renderable.typ" as answer

#let sep(..items) = items.pos().join([ ◆ ])
#let panel(body, ..rest) = quote(block: true, body)
#let note(body, title: none, ..rest) = quote(block: true, {
  if title != none { strong(title); parbreak() }
  body
})
#let notice(body, ..rest) = note([_*Notice:*_ #body])
#let dated(left, right) = [#left — #right]
#let minor(body) = smallcaps(lower(body))
#let accent(body) = emph(body)

#eval(
  answer.source(json("/state.json")),
  mode: "markup",
  scope: (sep: sep, panel: panel, note: note, notice: notice, dated: dated, minor: minor, accent: accent),
)
