//! A plugin's store in the cache tier: its objects at its declared schema, and the schema's text
//! beside them, so any reader opens the store at the schema it was written at.

use std::path::PathBuf;

use prodrome_core::declared::{Declaration, Declared};
use suzatary_store::store::StoreError;

use crate::objects::Objects;
use crate::present::Present as _;

/// Where one plugin's store lives: `objects/` and its locks, and `schema`, the text of the
/// declaration its objects were learned at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginStore {
    root: PathBuf,
}

/// How a sync found the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// At the schema it already held: everything learned before still reads.
    Kept,
    /// Empty, at a schema it did not hold before: what was learned at another schema is dropped,
    /// and a cursor kept for it is no longer the store's.
    Afresh,
}

/// The store as a sync begins it, held: no other sync begins it (and so none empties it) until
/// this is dropped, so a sync keeps it for as long as it writes.
#[derive(Debug)]
pub struct Begun {
    how: How,
    store: Objects<Declared>,
    _held: crate::durable::Held,
}

impl Begun {
    /// How the store was found.
    #[must_use]
    pub fn how(&self) -> How {
        self.how
    }

    /// The store, however it began, for as long as it is held.
    #[must_use]
    pub fn store(&self) -> &Objects<Declared> {
        &self.store
    }
}

const SCHEMA: &str = "schema";

impl PluginStore {
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn unreadable(&self, why: impl std::fmt::Display) -> StoreError {
        StoreError::unreadable("plugin", format!("{}: {why}", self.root.display()))
    }

    /// The schema's text beside the store; `None` where nothing was ever learned.
    fn schema(&self) -> Result<Option<String>, StoreError> {
        std::fs::read_to_string(self.root.join(SCHEMA))
            .present()
            .map_err(|err| self.unreadable(err))
    }

    /// The store as written, at the schema beside it; `None` where nothing was ever learned.
    ///
    /// # Errors
    /// A schema file that cannot be read, or whose text admission refuses.
    pub fn read(&self) -> Result<Option<Objects<Declared>>, StoreError> {
        let Some(text) = self.schema()? else {
            return Ok(None);
        };
        let schema = Declaration::admit(&text).map_err(|refused| self.unreadable(refused))?;
        Ok(Some(Objects::plugin(&self.root, schema)))
    }

    /// The store at the schema beside it, held as a sync holds it, so no sync at another schema
    /// empties it meanwhile; `None` where nothing was learned.
    ///
    /// # Errors
    /// The hold not taken, or what [`PluginStore::read`] refuses.
    pub fn read_held(&self) -> Result<Option<Begun>, StoreError> {
        let held = crate::durable::hold(&self.hold()).map_err(|why| {
            StoreError::not_appended("plugin", format!("{}: {why}", self.root.display()))
        })?;
        Ok(self.read()?.map(|store| Begun {
            how: How::Kept,
            store,
            _held: held,
        }))
    }

    /// The lock a sync holds from its `begin` to its last write: beside the store, never in it,
    /// since a `begin` at another schema removes the store's directory and every lock inside it.
    fn hold(&self) -> std::path::PathBuf {
        let name = self.root.file_name().map_or_else(
            || "store".into(),
            |name| name.to_string_lossy().into_owned(),
        );
        self.root.with_file_name(format!(".{name}.lock"))
    }

    /// The store at `schema`, held: kept where it holds that schema, else emptied and begun at it.
    /// The hold comes first, so no sync writes into a store being emptied.
    ///
    /// # Errors
    /// The hold not taken, a schema file that cannot be read, a directory that cannot be
    /// emptied or a schema that cannot be written.
    pub fn begin(&self, schema: &Declaration) -> Result<Begun, StoreError> {
        let unwritable = |why: std::io::Error| {
            StoreError::not_appended("plugin", format!("{}: {why}", self.root.display()))
        };
        let held = crate::durable::hold(&self.hold()).map_err(unwritable)?;
        if self.schema()?.as_deref() == Some(schema.text()) {
            return Ok(Begun {
                how: How::Kept,
                store: Objects::plugin(&self.root, schema.clone()),
                _held: held,
            });
        }
        std::fs::remove_dir_all(&self.root)
            .present()
            .map_err(unwritable)?;
        crate::durable::replace(
            &self.root.join(SCHEMA),
            schema.text().as_bytes(),
            crate::durable::Mode::Readable,
        )
        .map_err(unwritable)?;
        Ok(Begun {
            how: How::Afresh,
            store: Objects::plugin(&self.root, schema.clone()),
            _held: held,
        })
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::event::parse_event;
    use suzatary_store::store::learn;

    use super::*;
    use suzatary_store::store::Reads as _;

    const SCHEMA: &str = "Schema(key='door', events=(Event(name='Seen', fields=(Field(name='door', \
        type=Text()), Field(name='at', type=Instant()), Field(name='actor', type=Text()), \
        Field(name='state', type=Enum(alternatives=('open', 'shut'))))),), \
        registers=(Register(name='state', order=Discrete(), inflationary=False, \
        writes=(Write(event='Seen', field='state'),)),))";

    #[test]
    fn a_store_reads_at_the_schema_beside_it_and_another_schema_begins_it_afresh() {
        let dir = suzatary_testkit::scratch("plugin-store");
        let place = PluginStore::at(dir.path().join("store"));
        assert!(
            place.read().expect("reads").is_none(),
            "nothing learned yet"
        );
        let schema = Declaration::admit(SCHEMA).expect("lawful");
        let begun = place.begin(&schema).expect("begins");
        assert_eq!(
            begun.how(),
            How::Afresh,
            "a store never begun is begun afresh"
        );
        let store = begun.store();
        let seen = parse_event(
            &schema,
            "Seen(door='front', at=datetime(2026, 10, 5, 10, 0, 0), actor='probe', state='open')",
        )
        .expect("an event");
        learn(store, vec![seen]).expect("learns");
        drop(begun);
        let read = place.read().expect("reads").expect("a store");
        assert_eq!(
            read.objects().expect("reads").len(),
            2,
            "its genesis and the event"
        );
        assert_eq!(place.begin(&schema).expect("begins").how(), How::Kept);
        let mut names: Vec<String> = std::fs::read_dir(dir.path().join("store"))
            .expect("lists")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                ".gitattributes",
                ".gitignore",
                ".heads",
                ".lock",
                ".writes",
                "objects",
                "schema",
                "store.lock"
            ]
        );

        let other = Declaration::admit(&SCHEMA.replace("'shut'", "'wide'")).expect("lawful");
        let begun = place.begin(&other).expect("begins");
        assert_eq!(
            begun.how(),
            How::Afresh,
            "another schema begins the store afresh"
        );
        assert!(begun.store().objects().expect("reads").is_empty());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("store/schema")).expect("reads"),
            other.text()
        );
    }

    /// LAW: a `begin` at another schema, asked while a sync holds the store, waits for it: the
    /// holder's appends all land in the store it began, and the store is emptied at the other
    /// schema only once the holder lets go.
    #[test]
    fn a_begin_at_another_schema_waits_for_the_sync_that_holds_the_store() {
        let dir = suzatary_testkit::scratch("plugin-store");
        let place = PluginStore::at(dir.path().join("store"));
        let schema = Declaration::admit(SCHEMA).expect("lawful");
        let other = Declaration::admit(&SCHEMA.replace("'shut'", "'wide'")).expect("lawful");
        let seen = |hour: u32| {
            parse_event(
                &schema,
                &format!(
                    "Seen(door='door{hour}', at=datetime(2026, 10, 5, {hour}, 0, 0), actor='probe', \
                     state='open')"
                ),
            )
            .expect("an event")
        };
        let begun = place.begin(&schema).expect("begins");
        let emptying = std::thread::spawn({
            let place = place.clone();
            move || {
                let begun = place.begin(&other).expect("begins");
                (begun.how(), begun.store().objects().expect("reads").len())
            }
        });
        for hour in 10..13 {
            std::thread::sleep(std::time::Duration::from_millis(60));
            learn(begun.store(), vec![seen(hour)]).expect("learns while another begin waits");
        }
        assert_eq!(
            begun.store().objects().expect("reads").len(),
            4,
            "a genesis and three"
        );
        assert!(
            !emptying.is_finished(),
            "the other begin waits for the holder"
        );
        drop(begun);
        let (how, held) = emptying.join().expect("the other begin");
        assert_eq!(
            (how, held),
            (How::Afresh, 0),
            "emptied at the other schema, after"
        );
    }

    /// LAW: a plugin's store is opened `Doors::Alone`, and a handle that has read and written it
    /// decides over what another handle wrote since: its write rests on that write, so the
    /// register holds one candidate and the store one tip; and a handle that has read it reads
    /// what a sync at another schema, which removes the directory, and the syncs after it wrote.
    #[test]
    fn a_warm_handle_decides_over_every_later_write() {
        let dir = suzatary_testkit::scratch("plugin-store");
        let place = PluginStore::at(dir.path().join("store"));
        let schema = Declaration::admit(SCHEMA).expect("lawful");
        let seen = |hour: u32, state: &str| {
            parse_event(
                &schema,
                &format!(
                    "Seen(door='front', at=datetime(2026, 10, 5, {hour}, 0, 0), actor='probe', \
                     state='{state}')"
                ),
            )
            .expect("an event")
        };
        drop(place.begin(&schema).expect("begins"));
        let warm = place.read().expect("reads").expect("a store");
        assert_eq!(warm.inner().doors(), prodrome_core::store::Doors::Alone);
        learn(&warm, vec![seen(10, "open")]).expect("learns");
        let other = place.read().expect("reads").expect("a store");
        learn(&other, vec![seen(11, "shut")]).expect("learns elsewhere");
        learn(&warm, vec![seen(12, "open")]).expect("learns over it");
        let fresh = place.read().expect("reads").expect("a store");
        assert_eq!(fresh.inner().tips().expect("tips").len(), 1, "one tip");
        assert!(
            fresh.inner().verify().is_empty(),
            "{:?}",
            fresh.inner().verify()
        );

        let other_schema = Declaration::admit(&SCHEMA.replace("'shut'", "'wide'")).expect("lawful");
        drop(place.begin(&other_schema).expect("emptied"));
        let begun = place.begin(&schema).expect("begun again");
        learn(begun.store(), vec![seen(13, "shut")]).expect("learns");
        drop(begun);
        assert_eq!(warm.events().expect("reads"), vec![seen(13, "shut")]);
    }
}
