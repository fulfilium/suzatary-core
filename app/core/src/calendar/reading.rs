//! The store read: each occurrence's content, and the days a reader asks for.

use std::collections::BTreeMap;

use chrono::{NaiveDate, NaiveDateTime};
use prodrome_core::declared::{Declared, Registers};
use prodrome_core::fold::Product;
use prodrome_core::policy::Everything;
use prodrome_core::registers::{Node, Stamp, fold};

use super::{Content, Occurrence, OccurrenceId, When};

/// Every occurrence the store holds, with its reading.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Calendar {
    /// Each occurrence's candidates, least object name first; one is a value, more a conflict.
    occurrences: BTreeMap<OccurrenceId, Vec<Content>>,
    /// Writes that did not read as an occurrence.
    unread: usize,
}

/// One day of the calendar, in a reader's clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day {
    /// The day.
    pub date: NaiveDate,
    /// What it holds: all-day occurrences first, then by start.
    pub entries: Vec<Entry>,
}

/// One occurrence on one day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Which.
    pub id: OccurrenceId,
    /// What it says: the least-named candidate when there are several.
    pub content: Content,
    /// Its start and end in the reader's clock, `None` all day.
    pub local: Option<(NaiveDateTime, NaiveDateTime)>,
    /// Whether two syncs disagree about it and this is one of their readings.
    pub conflicted: bool,
}

impl Entry {
    /// `09:00–09:30`, or `all day`.
    #[must_use]
    pub fn time(&self) -> String {
        match self.local {
            None => "all day".to_owned(),
            Some((start, end)) if start == end => start.format("%H:%M").to_string(),
            Some((start, end)) => format!("{}–{}", start.format("%H:%M"), end.format("%H:%M")),
        }
    }
}

impl Calendar {
    /// The reading of `nodes`, a store at a calendar's declared schema: each occurrence's
    /// candidates, every writer believed, since the store has one writer and no trust to apply.
    /// A candidate is a write some register reads, and one that does not read as an occurrence
    /// is counted, never shown.
    #[must_use]
    pub fn of(nodes: &[Node<Declared>]) -> Self {
        let mut read = Self::default();
        for (_, stream) in fold(nodes).entities() {
            let registers: Registers<'_> = prodrome_core::fold::read(stream, None, &Everything);
            let mut writes: Vec<&Stamp<Declared>> = registers
                .registers()
                .into_iter()
                .flat_map(|slot| registers.reading(slot))
                .collect();
            writes.sort_by(|a, b| a.name.cmp(&b.name));
            writes.dedup_by(|a, b| a.name == b.name);
            for stamp in writes {
                match Occurrence::of(&stamp.event) {
                    Ok(Occurrence { id, content }) => {
                        let candidates = read.occurrences.entry(id).or_default();
                        if !candidates.contains(&content) {
                            candidates.push(content);
                        }
                    }
                    Err(_) => read.unread += 1,
                }
            }
        }
        read
    }

    /// Both readings at once: the calendars of two stores, each a plugin's own namespace.
    pub fn join(&mut self, other: Calendar) {
        for (id, candidates) in other.occurrences {
            let held = self.occurrences.entry(id).or_default();
            for content in candidates {
                if !held.contains(&content) {
                    held.push(content);
                }
            }
        }
        self.unread += other.unread;
    }

    /// How many writes did not read as an occurrence: a plugin's item whose span, calendar or
    /// field its schema admitted and this reader does not.
    #[must_use]
    pub fn unread(&self) -> usize {
        self.unread
    }

    /// How many occurrences it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.occurrences.len()
    }

    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
    }

    /// An occurrence's candidates: one is its content, more a conflict, none unknown.
    #[must_use]
    pub fn candidates(&self, id: &OccurrenceId) -> &[Content] {
        self.occurrences.get(id).map_or(&[], Vec::as_slice)
    }

    /// `count` days from `first` in a reader's clock, `to_local` placing a UTC instant there;
    /// cancelled occurrences are kept and read as cancelled.
    #[must_use]
    pub fn days(
        &self,
        first: NaiveDate,
        count: u32,
        to_local: &dyn Fn(NaiveDateTime) -> NaiveDateTime,
    ) -> Vec<Day> {
        let mut by_day: BTreeMap<NaiveDate, Vec<Entry>> = first
            .iter_days()
            .take(count as usize)
            .map(|date| (date, Vec::new()))
            .collect();
        for (id, candidates) in &self.occurrences {
            let content = &candidates[0];
            let local = match content.when() {
                When::Timed { start, end } => Some((to_local(*start), to_local(*end))),
                When::AllDay { .. } => None,
            };
            for date in content.when().days(to_local) {
                if let Some(entries) = by_day.get_mut(&date) {
                    entries.push(Entry {
                        id: id.clone(),
                        content: content.clone(),
                        local,
                        conflicted: candidates.len() > 1,
                    });
                }
            }
        }
        by_day
            .into_iter()
            .map(|(date, mut entries)| {
                entries.sort_by(|a, b| {
                    (a.local.map(|(start, _)| start), a.content.title(), &a.id).cmp(&(
                        b.local.map(|(start, _)| start),
                        b.content.title(),
                        &b.id,
                    ))
                });
                Day { date, entries }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::declared::Declaration;
    use prodrome_core::event::Actor;
    use prodrome_core::literal::Datetime;

    use super::*;
    use crate::calendar::{CalendarName, Status};
    use crate::store::learn;
    use crate::store::{InMemory, Prodrome};

    fn schema() -> Declaration {
        Declaration::admit(include_str!("../../../plugin-ics/ics.schema")).expect("lawful")
    }

    fn occurrence(uid: &str, key: &str, title: &str, when: &str, status: Status) -> Occurrence {
        Occurrence {
            id: OccurrenceId::new(
                CalendarName::parse("PLUGIN_ICS_HOME").expect("a name"),
                uid,
                key,
            )
            .expect("an id"),
            content: Content::new(
                title.to_owned(),
                When::parse(when).expect("a span"),
                String::new(),
                Vec::new(),
                status,
            )
            .expect("content"),
        }
    }

    fn standup(status: Status) -> Occurrence {
        occurrence(
            "standup",
            "20261005T130000Z",
            "Standup",
            "2026-10-05T13:00:00Z/2026-10-05T13:30:00Z",
            status,
        )
    }

    fn learnt(store: &InMemory<Declared>, occurrences: &[Occurrence]) {
        let schema = schema();
        let at = Datetime::new(2026, 10, 5, 10, 0, 0, 0).expect("an instant");
        let ics = Actor::new("ics").expect("an actor");
        let events = occurrences
            .iter()
            .map(|occurrence| occurrence.event(&schema, at, &ics).expect("an event"))
            .collect();
        learn(store, events).expect("learns");
    }

    #[test]
    fn a_cancellation_supersedes_and_reads_as_cancelled() {
        let store = InMemory::<Declared>::at(schema());
        learnt(&store, &[standup(Status::Confirmed)]);
        learnt(&store, &[standup(Status::Cancelled)]);
        let read = Calendar::of(&store.nodes().expect("reads"));
        let candidates = read.candidates(&standup(Status::Cancelled).id);
        assert_eq!(candidates.len(), 1, "the later write rests on the earlier");
        assert_eq!(candidates[0].status(), Status::Cancelled);
        assert_eq!(read.unread(), 0);
    }

    #[test]
    fn two_stores_read_as_their_union() {
        let one = InMemory::<Declared>::at(schema());
        let two = InMemory::<Declared>::at(schema());
        learnt(&one, &[standup(Status::Confirmed)]);
        learnt(
            &two,
            &[occurrence(
                "retreat",
                "20261004",
                "Retreat",
                "2026-10-04/2026-10-06",
                Status::Confirmed,
            )],
        );
        let mut read = Calendar::of(&one.nodes().expect("reads"));
        read.join(Calendar::of(&two.nodes().expect("reads")));
        assert_eq!(read.len(), 2);
    }
}
