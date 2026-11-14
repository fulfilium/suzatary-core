//! A declared schema of doors, seen open or shut: what the learning laws learn.

use prodrome_core::declared::{Declaration, Declared};
use prodrome_core::event::parse_event;

/// The doors' schema: one register, a door's state.
pub const SCHEMA: &str = "Schema(key='door', events=(Event(name='Seen', fields=(Field(name='door', \
    type=Text()), Field(name='at', type=Instant()), Field(name='actor', type=Text()), \
    Field(name='state', type=Enum(alternatives=('open', 'shut'))))),), \
    registers=(Register(name='state', order=Discrete(), inflationary=False, \
    writes=(Write(event='Seen', field='state'),)),))";

/// `door` seen `state` at `minute` past ten.
#[must_use]
pub fn seen(schema: &Declaration, door: &str, minute: u32, state: &str) -> Declared {
    parse_event(
        schema,
        &format!("Seen(door='{door}', at=datetime(2026, 10, 5, 10, {minute}), actor='probe', state='{state}')"),
    )
    .expect("an event")
}
