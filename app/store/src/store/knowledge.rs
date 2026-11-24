//! What a declared store already reads: each entity's registers, as the values they hold.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_dag::registers::{Node, fold};
use prodrome_data::fold::Product;
use prodrome_data::policy::Everything;
use prodrome_data::schema::Schema;
use prodrome_declared::{Datum, Declared, Key, Registers, Slot};

/// Each entity's registers, as their maximal values under the declared order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Knowledge(BTreeMap<Key, BTreeMap<Slot, BTreeSet<Datum>>>);

/// The value `event` writes to `slot`, as that register's order compares it.
fn value(event: &Declared, slot: Slot) -> Option<Datum> {
    event
        .written(slot)
        .map(|datum| event.schema().value(slot, datum).value())
}

impl Knowledge {
    /// The reading of `nodes`, every writer believed: a store a plugin fills has one writer.
    #[must_use]
    pub fn of(nodes: &[Node<Declared>]) -> Self {
        let folded = fold(nodes);
        let mut known = BTreeMap::new();
        for (key, stream) in folded.entities() {
            let registers: Registers<'_> = prodrome_dag::fold::read(stream, None, &Everything);
            let read: BTreeMap<Slot, BTreeSet<Datum>> = registers
                .registers()
                .into_iter()
                .map(|slot| {
                    let values = registers
                        .reading(slot)
                        .into_iter()
                        .filter_map(|stamp| value(&stamp.event, slot))
                        .collect();
                    (slot, values)
                })
                .collect();
            known.insert(key.clone(), read);
        }
        Self(known)
    }

    /// Whether every register `event` writes already reads exactly that value.
    /// A conflict is never known, so the next write settles it.
    #[must_use]
    pub fn knows(&self, event: &Declared) -> bool {
        let held = self.0.get(event.key());
        event.writes().all(|slot| {
            held.and_then(|registers| registers.get(&slot))
                == value(event, slot)
                    .map(|value| BTreeSet::from([value]))
                    .as_ref()
        })
    }

    /// Record a write of `event`, which rests on every write to its entity.
    pub fn settle(&mut self, event: &Declared) {
        let registers = self.0.entry(event.key().clone()).or_default();
        for slot in event.writes() {
            registers.insert(slot, value(event, slot).into_iter().collect());
        }
    }
}
