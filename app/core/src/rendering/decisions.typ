// The recorded decisions, a section per lineage. Titles, rationales and reasons are data, drawn
// as text; a body is markup the door admitted, evaluated in the closed world.
#import "/schema/DecisionsPage.typ" as page
#import "/schema/Shown.typ" as shown
#import "/schema/Standing.typ" as standing

#let ids(xs) = xs.map(raw).join([, ])

#let stands(s) = if standing.is-Withdrawn(s) {
  [*Withdrawn* #standing.day(s): #standing.reason(s)]
} else if standing.is-Superseded(s) {
  [*Superseded* by #ids(standing.by(s))]
}

#let section(d) = [
  == #shown.title(d)

  #raw(shown.id(d)), decided #shown.decided(d). #if shown.standing(d).len() == 0 [*In force.*] else [#shown.standing(d).map(stands).join([; ]).]

  #if shown.cycle(d) [*On a cycle of links:* two writers each superseded the other; the next write settles it.]

  #if shown.conflict(d) != none [*In conflict:* #shown.conflict(d) statements stand; the first is shown.]

  #block(inset: (left: 1em), eval(shown.body(d), mode: "markup"))

  _Why:_ #shown.rationale(d)

  #if shown.supersedes(d).len() > 0 [Supersedes #ids(shown.supersedes(d)).]
]

#let d = json("/state.json")

= Decisions

#page.recorded(d) recorded, #page.in_force(d) in force. A decision stands above what it supersedes.

#for lineage in page.lineages(d) {
  line(length: 100%)
  for one in lineage { section(one) }
}
