use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::{Mutex, PoisonError};

use super::{Decide, Edit, Log, StoreError};
use crate::refusal::Refusal;

/// Logs held in memory, for tests; a refusal names what each holds and its key as `Debug`.
#[derive(Debug)]
pub struct InMemoryLog<K, E> {
    what: &'static str,
    logs: Mutex<BTreeMap<K, Vec<E>>>,
}

impl<K, E> InMemoryLog<K, E> {
    /// No logs yet, each one a `what`.
    #[must_use]
    pub fn new(what: &'static str) -> Self {
        Self::with(what, BTreeMap::new())
    }

    /// These logs, each one a `what`.
    #[must_use]
    pub fn with(what: &'static str, logs: BTreeMap<K, Vec<E>>) -> Self {
        Self {
            what,
            logs: Mutex::new(logs),
        }
    }
}

impl<K, E> InMemoryLog<K, E>
where
    K: Ord + Clone + Debug,
{
    fn absent(&self, key: &K) -> StoreError {
        StoreError::Absent {
            what: self.what,
            name: format!("{key:?}"),
        }
    }
}

impl<K, E> Log<K, E> for InMemoryLog<K, E>
where
    K: Ord + Clone + Debug + Send,
    E: Clone + Send,
{
    fn keys(&self) -> Result<Vec<K>, StoreError> {
        Ok(self
            .logs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect())
    }

    fn lines(&self, key: &K) -> Result<Vec<E>, StoreError> {
        self.logs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
            .ok_or_else(|| self.absent(key))
    }

    fn start(&self, key: &K, first: &E) -> Result<(), StoreError> {
        let mut logs = self.logs.lock().unwrap_or_else(PoisonError::into_inner);
        if logs.contains_key(key) {
            return Err(StoreError::Taken {
                what: self.what,
                name: format!("{key:?}"),
            });
        }
        logs.insert(key.clone(), vec![first.clone()]);
        Ok(())
    }

    fn push(&self, key: &K, line: &E) -> Result<(), StoreError> {
        let mut logs = self.logs.lock().unwrap_or_else(PoisonError::into_inner);
        logs.get_mut(key)
            .ok_or_else(|| self.absent(key))?
            .push(line.clone());
        Ok(())
    }

    fn edit(&self, key: &K, decide: &mut Decide<'_, E>) -> Result<Edit<E>, Refusal> {
        let mut logs = self.logs.lock().unwrap_or_else(PoisonError::into_inner);
        let edit = decide(logs.get(key).map_or(&[][..], Vec::as_slice))?;
        let log = logs.entry(key.clone()).or_default();
        match &edit {
            Edit::Push(line) => log.push(line.clone()),
            Edit::Replace(lines) => log.clone_from(lines),
            Edit::Keep => {}
        }
        Ok(edit)
    }
}
