// One todo of a store as a static page: its state and price, every content candidate whole, and
// every object of its history. Every datum is drawn as text. A body and a detail are never
// evaluated here, where markup could ask about the page: each is an empty element that the verb
// replaces with the field's drawing, compiled alone, the bytes door 1 read. A reference to a todo
// of this store spelled in the page's text, or named by a field's drawn text, leads to its page.
#import "/schema/TodoPage.typ" as page
#import "/schema/Linked.typ" as linked
#import "/schema/Shown.typ" as shown
#import "/schema/Candidate.typ" as candidate
#import "/schema/Drawn.typ" as drawn
#import "/schema/Field.typ" as field
#import "/schema/Event.typ" as event
#import "/schema/FieldChange.typ" as change
#import "/priority.typ": price

#let d = json("/state.json")
#let s = page.shown(d)
#let pages = page.links(d).map(l => (linked.reference(l), linked.page(l))).to-dict()

// A spelled reference inside a link stays that link's text, since an anchor holds no anchor, and
// inside code it stays as stored: `within` counts the links and code a text sits in. A reference is
// read whole from a word's start to its end, the dashes typst makes of `--` and `---` included,
// so neither `todo:<chain>/roofTop` nor `todo:<chain>/roof--a` names a todo `roof`.
#let within = state("within-a-link-or-code", 0)
#let inside(it) = {
  within.update(n => n + 1)
  it
  within.update(n => n - 1)
}
#show regex("\\btodo:[A-Za-z0-9_-]+/[A-Za-z0-9_\\-–—]+"): it => context {
  let to = pages.at(it.text.slice(5), default: none)
  if to == none or within.get() > 0 { it } else { link(to, it) }
}
#show raw: inside
#show link: it => {
  let todo = type(it.dest) == str and it.dest.starts-with("todo:")
  let to = if todo { pages.at(it.dest.slice(5), default: none) } else { it.dest }
  inside(if to == none { it.body } else if to == it.dest { it } else { link(to, it.body) })
}

#link(page.list(d))[The open list]

= #shown.todo(s)

#strong(shown.state(s)), #price(shown.priority(s)), as of #page.at(d).

#if shown.candidates(s).len() > 1 [*In conflict:* #shown.candidates(s).len() records stand, each shown whole below; the next write settles it.]

#let shown-field(f, source) = if field.is-Empty(f) {
  none
} else if field.is-Drawn(f) {
  html.elem("div", attrs: ("data-drawn": field.marker(f)))
} else if field.is-Source(f) {
  raw(source)
}

#for (c, f) in shown.candidates(s).zip(page.drawn(d)) [
  == #candidate.kind(c) by #candidate.authored_by(c)

  #shown-field(drawn.body(f), candidate.body(c))

  #shown-field(drawn.detail(f), candidate.detail(c))

  #if drawn.refers(f).len() > 0 [Refers to #drawn.refers(f).map(l => link(linked.page(l), "todo:" + linked.reference(l))).join(", ").]

  Object #raw(candidate.object(c)), as stored:

  #raw(candidate.block(c), block: true)
]

== History

Every object that names this todo, oldest first; an earlier record and a reversed event stay, as the store keeps them.

#let changed(f) = if change.before(f) == "" [#change.field(f): #raw(change.after(f))] else [#change.field(f): #raw(change.before(f)) → #raw(change.after(f))]

#enum(..shown.history(s).map(e => [
  #raw(event.object(e)) #event.kind(e) by #event.actor(e) at #event.at(e).#if event.note(e) != "" [ #event.note(e)]
  #list(..event.changed(e).map(changed))
]))
