//! Learning what a source reported into a declared store, idempotently.

use prodrome_declared::Declared;

use super::{Knowledge, Prodrome, StoreError};

/// What one learning did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Learnt {
    pub written: usize,
    /// Items the store already read as reported.
    pub known: usize,
}

/// Write each of `items` the store does not already read as it is, under the store's hold.
///
/// # Errors
/// The store's refusal; what was written before it stays written.
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

#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/doors.rs"]
pub mod doors;

#[cfg(test)]
mod tests {
    use prodrome_declared::Declaration;

    use super::Knowledge;
    use super::doors::{SCHEMA, seen};

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
