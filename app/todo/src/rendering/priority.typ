// A price as `todos open` spells it: `∅` for no function, `—` for one that does not link.
#import "/schema/Priority.typ" as priority

#let price(p) = if priority.is-Percent(p) { priority.percent(p) } else if priority.is-Absent(p) { "∅" } else if priority.is-Unlinked(p) { "—" }
