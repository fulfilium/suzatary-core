//! One occurrence as the store holds it: an event of the calendar's declared schema, read into
//! its identity and its content, and printed back from them.

use prodrome_core::declared::{Datum, Declaration, Declared};
use prodrome_core::event::Actor;
use prodrome_core::literal::{Datetime, ProdromeError, Value};
use prodrome_core::schema::Schema;

use super::{Content, NotAnOccurrence, OccurrenceId, Status, When};

/// An occurrence a plugin reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub id: OccurrenceId,
    pub content: Content,
}

/// The event every occurrence is learned by.
const LEARNED: &str = "Learned";

/// The registers a calendar reads, each written by the field of its name.
const REGISTERS: [&str; 5] = ["attendees", "place", "status", "title", "when"];

fn text(event: &Declared, field: &'static str) -> Result<String, NotAnOccurrence> {
    match written(event, field)? {
        Datum::Text(text) | Datum::Alternative(text) => Ok(text.clone()),
        _ => Err(NotAnOccurrence::Field(field)),
    }
}

fn written<'e>(event: &'e Declared, field: &'static str) -> Result<&'e Datum, NotAnOccurrence> {
    event
        .schema()
        .register(field)
        .and_then(|slot| event.written(slot))
        .ok_or(NotAnOccurrence::Field(field))
}

impl Occurrence {
    /// Whether a store at `schema` is a calendar: its every register is one this reader reads.
    #[must_use]
    pub fn reads(schema: &Declaration) -> bool {
        REGISTERS.iter().all(|name| schema.register(name).is_some())
    }

    /// Read `event` as an occurrence: its key an [`OccurrenceId`] in the namespace of the plugin
    /// that wrote it, and each register through its smart constructor.
    pub fn of(event: &Declared) -> Result<Self, NotAnOccurrence> {
        let id = OccurrenceId::parse(event.key().as_ref())?;
        if !id.calendar().of_plugin(event.actor().as_str()) {
            return Err(NotAnOccurrence::NotItsCalendar(id.calendar().to_string()));
        }
        let Datum::List(attendees) = written(event, "attendees")? else {
            return Err(NotAnOccurrence::Field("attendees"));
        };
        let attendees = attendees
            .iter()
            .map(|name| match name {
                Datum::Text(name) => Ok(name.clone()),
                _ => Err(NotAnOccurrence::Field("attendees")),
            })
            .collect::<Result<_, _>>()?;
        let content = Content::new(
            text(event, "title")?,
            When::parse(&text(event, "when")?)?,
            text(event, "place")?,
            attendees,
            Status::parse(&text(event, "status")?)?,
        )?;
        Ok(Self { id, content })
    }

    /// The event a sync by `actor` at `at` learns this occurrence by, at `schema`: what
    /// [`Occurrence::of`] reads back as this occurrence.
    pub fn event(
        &self,
        schema: &Declaration,
        at: Datetime,
        actor: &Actor,
    ) -> Result<Declared, ProdromeError> {
        let fields = [
            ("occurrence", Value::str(self.id.key())),
            ("at", Value::Datetime(at)),
            ("actor", Value::str(actor.as_str())),
            ("title", Value::str(self.content.title())),
            ("when", Value::str(self.content.when().print())),
            ("place", Value::str(self.content.place())),
            (
                "attendees",
                Value::Tuple(self.content.attendees().iter().map(Value::str).collect()),
            ),
            ("status", Value::str(self.content.status().as_str())),
        ];
        Declared::from_value(
            schema,
            &Value::call(
                LEARNED,
                fields
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect(),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::event::{canonical, parse_event};

    use super::*;
    use crate::calendar::CalendarName;

    /// The calendar plugin's schema, byte for byte (`cli/tests/google.rs` holds the two equal).
    pub(crate) fn schema() -> Declaration {
        Declaration::admit(crate::google::SCHEMA).expect("lawful")
    }

    fn standup(title: &str, attendees: &[&str], calendar: &str) -> Occurrence {
        Occurrence {
            id: OccurrenceId::new(
                CalendarName::parse(calendar).expect("a name"),
                "standup@example.test",
                "20261005T130000Z",
            )
            .expect("an id"),
            content: Content::new(
                title.to_owned(),
                When::parse("2026-10-05T13:00:00Z/2026-10-05T13:30:00Z").expect("a span"),
                String::new(),
                attendees.iter().map(|name| (*name).to_owned()).collect(),
                Status::Confirmed,
            )
            .expect("content"),
        }
    }

    fn at() -> Datetime {
        Datetime::new(2026, 10, 5, 10, 0, 0, 0).expect("an instant")
    }

    #[test]
    fn an_occurrence_prints_and_reads_back_whatever_its_text() {
        let schema = schema();
        assert!(Occurrence::reads(&schema));
        let ics = Actor::new("ics").expect("an actor");
        for (title, attendees) in [
            ("Standup", vec!["Ada", "Solberg, Mira"]),
            ("", vec![]),
            ("')\nLearned(occurrence='PLUGIN_X_Y' #evil", vec!["\\"]),
        ] {
            let occurrence = standup(title, &attendees, "PLUGIN_ICS_HOME");
            let event = occurrence.event(&schema, at(), &ics).expect("an event");
            let printed = canonical(&event);
            let back: Declared = parse_event(&schema, &printed).expect("parses");
            assert_eq!(Occurrence::of(&back), Ok(occurrence), "{printed}");
        }
    }

    #[test]
    fn a_plugin_reports_only_its_own_calendars() {
        let event = standup("Standup", &[], "PLUGIN_BANK_HOME")
            .event(&schema(), at(), &Actor::new("ics").expect("an actor"))
            .expect("an event");
        assert_eq!(
            Occurrence::of(&event),
            Err(NotAnOccurrence::NotItsCalendar(
                "PLUGIN_BANK_HOME".to_owned()
            ))
        );
    }
}
