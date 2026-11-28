//! One occurrence as the store holds it: an event of the calendar's declared schema, read into
//! its identity and its content, and printed back from them.

use prodrome_data::event::Actor;
use prodrome_data::literal::{Datetime, ProdromeError, Value};
use prodrome_data::schema::Schema;
use prodrome_declared::{Datum, Declaration, Declared};

use super::bounds::cut;
use super::{
    Attendees, CalendarName, Content, MAX_TEXT, NotAnOccurrence, OccurrenceId, Point, Status, When,
};

/// An occurrence a plugin reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub id: OccurrenceId,
    pub content: Content,
}

/// What a provider's event says, before the calendar holds it to its limits.
#[derive(Debug, Clone)]
pub struct Said<'a> {
    /// The event's identity across syncs.
    pub uid: &'a str,
    pub when: When,
    /// Where the instance started before it was moved, for an instance of a series; its key
    /// stays the same however it moves.
    pub moved_from: Option<Point>,
    pub title: &'a str,
    pub place: &'a str,
    /// Each attendee's display name, `""` for one with none; an address is never read.
    pub attendees: Vec<&'a str>,
    pub status: Status,
}

/// The event every occurrence is learned by.
const LEARNED: &str = "Learned";

/// The registers a calendar reads, each written by the field of its name.
const REGISTERS: [&str; 6] = ["attendees", "place", "status", "title", "unnamed", "when"];

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
    /// What `said` is as the calendar keeps it, in `name`: each text cut to the calendar's limit,
    /// each attendee counted and named only by a display name. `None` for an event the calendar
    /// cannot read, such as one with no identity.
    #[must_use]
    pub fn said(name: &CalendarName, said: Said<'_>) -> Option<Self> {
        let key = said.moved_from.unwrap_or_else(|| said.when.into()).key();
        Some(Self {
            id: OccurrenceId::new(name.clone(), &cut(said.uid, 500), &key).ok()?,
            content: Content::new(
                cut(said.title, MAX_TEXT),
                said.when,
                cut(said.place, MAX_TEXT),
                Attendees::of(said.attendees, 0),
                said.status,
            )
            .ok()?,
        })
    }

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
        let names = attendees
            .iter()
            .map(|name| match name {
                Datum::Text(name) => Ok(name.as_str()),
                _ => Err(NotAnOccurrence::Field("attendees")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let Datum::Integer(unnamed) = written(event, "unnamed")? else {
            return Err(NotAnOccurrence::Field("unnamed"));
        };
        let unnamed = u32::try_from(*unnamed).map_err(|_| NotAnOccurrence::Field("unnamed"))?;
        let attendees = Attendees::of(names, unnamed);
        let content = Content::new(
            text(event, "title")?,
            When::parse(&text(event, "when")?)?,
            text(event, "place")?,
            attendees,
            Status::parse(&text(event, "status")?)?,
        )?;
        Ok(Self { id, content })
    }

    /// `event` as a store keeps it: read as an occurrence and printed again, so an attendee whose
    /// name is not an [`AttendeeName`](super::AttendeeName) is counted and never stored. The host
    /// asks this of a plugin's item, since the plugin is the less trusted side of its door.
    pub fn kept(event: &Declared) -> Result<Declared, NotAnOccurrence> {
        Self::of(event)?
            .event(event.schema(), event.at(), event.actor())
            .map_err(|refused| NotAnOccurrence::Unprinted(refused.to_string()))
    }

    /// The event a sync by `actor` at `at` learns this occurrence by, at `schema`: what
    /// [`Occurrence::of`] reads back as this occurrence.
    pub fn event(
        &self,
        schema: &Declaration,
        at: Datetime,
        actor: &Actor,
    ) -> Result<Declared, ProdromeError> {
        let attendees = self.content.attendees();
        let fields = [
            ("occurrence", Value::str(self.id.key())),
            ("at", Value::Datetime(at)),
            ("actor", Value::str(actor.as_str())),
            ("title", Value::str(self.content.title())),
            ("when", Value::str(self.content.when().print())),
            ("place", Value::str(self.content.place())),
            (
                "attendees",
                Value::Tuple(
                    attendees
                        .named()
                        .iter()
                        .map(|name| Value::str(name.as_str()))
                        .collect(),
                ),
            ),
            ("unnamed", Value::int(i64::from(attendees.unnamed()))),
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
    use prodrome_data::event::{canonical, parse_event};

    use super::*;
    use crate::calendar::CalendarName;

    /// The calendar plugin's schema, byte for byte (`cli/tests/google.rs` holds the two equal).
    pub(crate) fn schema() -> Declaration {
        Declaration::admit(crate::calendar::SCHEMA).expect("lawful")
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
                Attendees::of(attendees.iter().copied(), 1),
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
            ("Review", vec!["", "Ada", "mira@example.invalid"]),
        ] {
            let occurrence = standup(title, &attendees, "PLUGIN_ICS_HOME");
            let event = occurrence.event(&schema, at(), &ics).expect("an event");
            let printed = canonical(&event);
            let back: Declared = parse_event(&schema, &printed).expect("parses");
            assert_eq!(Occurrence::of(&back), Ok(occurrence), "{printed}");
        }
    }

    #[test]
    fn a_stored_name_that_is_an_address_reads_as_an_attendee_unnamed() {
        let schema = schema();
        let ics = Actor::new("ics").expect("an actor");
        let printed = canonical(
            &standup("Standup", &["Ada"], "PLUGIN_ICS_HOME")
                .event(&schema, at(), &ics)
                .expect("an event"),
        );
        let address = format!("{}@example.invalid", "mira");
        let stored = printed.replace("('Ada',)", &format!("('Ada', '{address}')"));
        assert_ne!(stored, printed);
        let read = Occurrence::of(&parse_event(&schema, &stored).expect("parses")).expect("reads");
        let attendees = read.content.attendees();
        assert_eq!(
            (attendees.named().len(), attendees.unnamed()),
            (1, 2),
            "{stored}"
        );
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
