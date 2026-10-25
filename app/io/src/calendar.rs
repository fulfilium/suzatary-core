//! The calendar, read in the owner's zone: the one place a calendar's UTC instants meet a day.

use chrono::NaiveDate;
use prodrome_core::store::Replica as _;
use suzatary_core::calendar::{Calendar as Reading, Day, Occurrence};
use suzatary_core::store::StoreError;
use suzatary_core::zone::Zone;

use crate::plugin_store::PluginStore;
use suzatary_core::store::Reads as _;

/// The calendar every calendar plugin fills, read through
/// [`suzatary_core::capability::Calendar`]: the union of the plugins' stores whose declared
/// schema is a calendar's.
#[derive(Debug)]
pub struct Clocked {
    stores: Vec<PluginStore>,
    zone: Zone,
}

impl Clocked {
    /// The calendar over the plugins' stores at `roots`, its days the owner's in `zone`; a
    /// store nothing has written, or one whose schema is not a calendar's, reads as no
    /// occurrences.
    #[must_use]
    pub fn new(roots: impl IntoIterator<Item = std::path::PathBuf>, zone: Zone) -> Self {
        Self {
            stores: roots.into_iter().map(PluginStore::at).collect(),
            zone,
        }
    }
}

impl suzatary_core::capability::Calendar for Clocked {
    fn days(&self, first: NaiveDate, count: u32) -> Result<Vec<Day>, StoreError> {
        let mut read = Reading::default();
        for place in &self.stores {
            let Some(store) = place.read()? else {
                continue;
            };
            if Occurrence::reads(store.inner().schema()) {
                read.join(Reading::of(&store.nodes()?));
            }
        }
        Ok(read.days(first, count, &|utc| self.zone.wall(utc)))
    }
}
