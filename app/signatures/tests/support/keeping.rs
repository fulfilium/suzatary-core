//! Keyed chains a test names by their stores: test support, never production.

use suzatary_base::chains::ChainName;
use suzatary_record::record::TodoEvent;
use suzatary_store::store::Prodrome;

use super::{Keeper, Keeping, Keyed};

impl<'a> Keeping<Keeper<'a>> {
    /// Named `kept0`, `kept1`, … in order.
    ///
    /// # Panics
    /// Never: each name is a slug.
    #[must_use]
    pub fn stores(stores: &[&'a dyn Prodrome<TodoEvent>]) -> Self {
        let mut keepers = stores.iter().enumerate().map(|(n, store)| Keeper {
            name: ChainName::new(format!("kept{n}")).expect("a slug"),
            store: *store,
        });
        match keepers.next() {
            None => Keeping::Keyless,
            Some(first) => Keeping::Keyed(Keyed {
                first,
                rest: keepers.collect(),
            }),
        }
    }
}
