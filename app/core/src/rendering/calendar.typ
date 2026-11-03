// The calendar's days, a section each. Titles, places and names are other people's words, and
// every one is drawn as text.
#import "/page.typ": el, bare
#import "/schema/CalendarPage.typ" as page
#import "/schema/CalendarDay.typ" as day
#import "/schema/CalendarRow.typ" as row

#let entry(r) = el(
  "li", "calendarentry " + row.status(r),
  el("span", "calendartime", row.time(r)),
  el("span", "calendartitle", if row.title(r) == "" { "(no title)" } else { row.title(r) }),
  if row.place(r) != none { el("span", "calendarplace", row.place(r)) },
  el("span", "calendarname", row.calendar(r)),
  ..row.notes(r).map(note => el("span", "calendarstatus", note)),
)

#let d = json("/state.json")
#{
  for one in page.days(d) {
    el(
      "section", "calendarday",
      bare("h2", day.heading(one)),
      if day.entries(one).len() == 0 { el("p", "empty", "Nothing on the calendar.") } else { el("ul", "calendarlist", ..day.entries(one).map(entry)) },
    )
  }
  el("p", "legend", "Titles, places and names come from the calendars' feeds: other people's words. A meeting is never a todo.")
}
