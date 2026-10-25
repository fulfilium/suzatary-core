//! A plugin's store in the cache tier: its objects at its declared schema, and the schema's text
//! beside them, so any reader opens the store at the schema it was written at.

use std::path::PathBuf;

use prodrome_core::declared::{Declaration, Declared};
use suzatary_core::store::StoreError;

use crate::objects::Objects;
use crate::present::Present as _;

/// Where one plugin's store lives: `objects/` and its locks, and `schema`, the text of the
/// declaration its objects were learned at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginStore {
    root: PathBuf,
}

/// The store as a sync begins it.
#[derive(Debug)]
pub enum Begun {
    /// At the schema it already held: everything learned before still reads.
    Kept(Objects<Declared>),
    /// Empty, at a schema it did not hold before. A cache is a tabulation of its source, so what
    /// was learned at another schema is dropped and learned again, and a cursor kept for it is
    /// no longer the store's.
    Afresh(Objects<Declared>),
}

impl Begun {
    /// The store, however it began.
    #[must_use]
    pub fn store(self) -> Objects<Declared> {
        match self {
            Begun::Kept(store) | Begun::Afresh(store) => store,
        }
    }
}

const SCHEMA: &str = "schema";

impl PluginStore {
    /// The store at `root`.
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

    /// The store at `schema`: kept where it already holds that schema, else emptied and begun
    /// at it, the schema's text written first.
    ///
    /// # Errors
    /// A schema file that cannot be read, a directory that cannot be emptied or a schema that
    /// cannot be written.
    pub fn begin(&self, schema: &Declaration) -> Result<Begun, StoreError> {
        if self.schema()?.as_deref() == Some(schema.text()) {
            return Ok(Begun::Kept(Objects::plugin(&self.root, schema.clone())));
        }
        let unwritable = |why: std::io::Error| {
            StoreError::not_appended("plugin", format!("{}: {why}", self.root.display()))
        };
        std::fs::remove_dir_all(&self.root)
            .present()
            .map_err(unwritable)?;
        crate::durable::replace(
            &self.root.join(SCHEMA),
            schema.text().as_bytes(),
            crate::durable::Mode::Readable,
        )
        .map_err(unwritable)?;
        Ok(Begun::Afresh(Objects::plugin(&self.root, schema.clone())))
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::event::parse_event;
    use suzatary_core::store::learn;

    use super::*;
    use suzatary_core::store::Reads as _;

    const SCHEMA: &str = "Schema(key='door', events=(Event(name='Seen', fields=(Field(name='door', \
        type=Text()), Field(name='at', type=Instant()), Field(name='actor', type=Text()), \
        Field(name='state', type=Enum(alternatives=('open', 'shut'))))),), \
        registers=(Register(name='state', order=Discrete(), inflationary=False, \
        writes=(Write(event='Seen', field='state'),)),))";

    #[test]
    fn a_store_reads_at_the_schema_beside_it_and_another_schema_begins_it_afresh() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let place = PluginStore::at(dir.path().join("store"));
        assert!(
            place.read().expect("reads").is_none(),
            "nothing learned yet"
        );
        let schema = Declaration::admit(SCHEMA).expect("lawful");
        let store = match place.begin(&schema).expect("begins") {
            Begun::Afresh(store) => store,
            Begun::Kept(_) => panic!("a store never begun is begun afresh"),
        };
        let seen = parse_event(
            &schema,
            "Seen(door='front', at=datetime(2026, 10, 5, 10, 0, 0), actor='probe', state='open')",
        )
        .expect("an event");
        learn(&store, vec![seen]).expect("learns");
        let read = place.read().expect("reads").expect("a store");
        assert_eq!(
            read.objects().expect("reads").len(),
            2,
            "its genesis and the event"
        );
        assert!(matches!(place.begin(&schema), Ok(Begun::Kept(_))));
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
            [".gitattributes", ".lock", "objects", "schema", "store.lock"]
        );

        let other = Declaration::admit(&SCHEMA.replace("'shut'", "'wide'")).expect("lawful");
        let Begun::Afresh(store) = place.begin(&other).expect("begins") else {
            panic!("another schema begins the store afresh");
        };
        assert!(store.objects().expect("reads").is_empty());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("store/schema")).expect("reads"),
            other.text()
        );
    }
}
