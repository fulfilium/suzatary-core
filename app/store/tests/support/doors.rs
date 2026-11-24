//! A declared schema of doors, seen open or shut: what the learning laws learn.

use prodrome_data::event::parse_event;
use prodrome_declared::{Declaration, Declared};

pub const SCHEMA: &str = "Schema(key='door', events=(Event(name='Seen', fields=(Field(name='door', \
    type=Text()), Field(name='at', type=Instant()), Field(name='actor', type=Text()), \
    Field(name='state', type=Enum(alternatives=('open', 'shut'))))),), \
    registers=(Register(name='state', order=Discrete(), inflationary=False, \
    writes=(Write(event='Seen', field='state'),)),))";

#[must_use]
pub fn seen(schema: &Declaration, door: &str, minute: u32, state: &str) -> Declared {
    parse_event(
        schema,
        &format!("Seen(door='{door}', at=datetime(2026, 10, 5, 10, {minute}), actor='probe', state='{state}')"),
    )
    .expect("an event")
}
