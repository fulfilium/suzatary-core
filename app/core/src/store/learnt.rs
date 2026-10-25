//! Learning what a source reported into a declared store: an item the store already reads as
//! reported is learned once, so a timer may sync as often as it likes.

use prodrome_core::declared::Declared;

use super::{Knowledge, Prodrome, StoreError};

/// What one learning did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Learnt {
    /// Items written: new or changed.
    pub written: usize,
    /// Items the store already read as reported.
    pub known: usize,
}

/// Write each of `items` the store does not already read as it is, under the store's hold, so
/// two syncs at once do not both write one.
///
/// # Errors
/// The store's refusal; what was written before it stays written, and the next sync learns the
/// rest, since learning is idempotent.
pub fn learn(store: &dyn Prodrome<Declared>, items: Vec<Declared>) -> Result<Learnt, StoreError> {
    let _hold = store.locked()?;
    let mut known = Knowledge::of(&store.nodes()?);
    let mut learnt = Learnt::default();
    for item in items {
        if known.knows(&item) {
            learnt.known += 1;
            continue;
        }
        known.settle(&item);
        store.append(item)?;
        learnt.written += 1;
    }
    Ok(learnt)
}

#[cfg(test)]
mod tests {
    use prodrome_core::declared::Declaration;
    use prodrome_core::event::parse_event;

    use super::*;
    use crate::store::InMemory;
    use crate::store::Reads as _;

    const SCHEMA: &str = "Schema(key='door', events=(Event(name='Seen', fields=(Field(name='door', \
        type=Text()), Field(name='at', type=Instant()), Field(name='actor', type=Text()), \
        Field(name='state', type=Enum(alternatives=('open', 'shut'))))),), \
        registers=(Register(name='state', order=Discrete(), inflationary=False, \
        writes=(Write(event='Seen', field='state'),)),))";

    fn seen(schema: &Declaration, door: &str, minute: u32, state: &str) -> Declared {
        parse_event(
            schema,
            &format!("Seen(door='{door}', at=datetime(2026, 10, 5, 10, {minute}), actor='probe', state='{state}')"),
        )
        .expect("an event")
    }

    #[test]
    fn learning_twice_is_learning_once_whenever_it_was_seen() {
        let schema = Declaration::admit(SCHEMA).expect("lawful");
        let store = InMemory::<Declared>::at(schema.clone());
        let first = learn(&store, vec![seen(&schema, "front", 0, "open")]).expect("learns");
        assert_eq!((first.written, first.known), (1, 0));
        let again = learn(&store, vec![seen(&schema, "front", 5, "open")]).expect("learns");
        assert_eq!((again.written, again.known), (0, 1), "the instant is data");
        let shut = learn(
            &store,
            vec![
                seen(&schema, "front", 6, "shut"),
                seen(&schema, "front", 7, "shut"),
            ],
        )
        .expect("learns");
        assert_eq!((shut.written, shut.known), (1, 1));
        assert_eq!(
            store.objects().expect("reads").len(),
            3,
            "the genesis the first write began, and two changes"
        );
    }

    #[test]
    fn a_conflict_is_never_known_so_the_next_write_settles_it() {
        let schema = Declaration::admit(SCHEMA).expect("lawful");
        let open = seen(&schema, "back", 0, "open");
        let shut = seen(&schema, "back", 1, "shut");
        let mut known = Knowledge::default();
        assert!(!known.knows(&open));
        known.settle(&open);
        assert!(known.knows(&open) && !known.knows(&shut));
        known.settle(&shut);
        assert!(known.knows(&shut) && !known.knows(&open));
    }
}
