use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use super::{Doc, Gets, StoreError};
use crate::secretary::locked_text::{KEY_BYTES, KeyName, TextKey};

/// Keys held in memory as their bytes, for tests: a key is read back as a fresh [`TextKey`], as
/// a key file is.
#[derive(Debug, Default)]
pub struct InMemoryKeys {
    keys: Mutex<BTreeMap<KeyName, [u8; KEY_BYTES]>>,
}

impl InMemoryKeys {
    /// Every key's name held, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<KeyName> {
        self.keys
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }
}

impl Gets<KeyName, TextKey> for InMemoryKeys {
    fn get(&self, name: &KeyName) -> Result<Option<TextKey>, StoreError> {
        Ok(self
            .keys
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .and_then(|bytes| TextKey::parse(name, bytes)))
    }
}

impl Doc<KeyName, TextKey> for InMemoryKeys {
    fn set(&self, name: &KeyName, key: &TextKey) -> Result<(), StoreError> {
        if &key.name() != name {
            return Err(StoreError::unwritable(
                "keys",
                format!("this key is not {name}"),
            ));
        }
        self.keys
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(name.clone(), *key.bytes());
        Ok(())
    }

    fn remove(&self, name: &KeyName) -> Result<(), StoreError> {
        self.keys
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(name);
        Ok(())
    }
}
