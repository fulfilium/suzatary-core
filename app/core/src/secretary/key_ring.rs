//! The keys a replica was given, as the store's pull hands them: each key's bytes in hex under
//! its name. A tab reads its lines through one, as the box reads through its key store.

use std::collections::BTreeMap;

use crate::secretary::locked_text::{KEY_BYTES, KeyName, TextKey};
use crate::secretary::words::{Gone, Words};

/// Keys by name; a name given bytes that are not its key is not held. It prints as each key's
/// bytes under its name, for a snapshot of a replica, and reads back only where every name is
/// its bytes' own.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "BTreeMap<KeyName, [u8; KEY_BYTES]>")]
pub struct KeyRing(BTreeMap<KeyName, [u8; KEY_BYTES]>);

/// A key whose bytes are not the key its name names.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the bytes under {0} are not the key it names")]
pub struct NotItsKey(KeyName);

impl TryFrom<BTreeMap<KeyName, [u8; KEY_BYTES]>> for KeyRing {
    type Error = NotItsKey;

    fn try_from(keys: BTreeMap<KeyName, [u8; KEY_BYTES]>) -> Result<Self, NotItsKey> {
        let ring = Self(keys);
        match ring
            .0
            .iter()
            .find(|(name, bytes)| TextKey::parse(name, *bytes).is_none())
        {
            Some((name, _)) => Err(NotItsKey(name.clone())),
            None => Ok(ring),
        }
    }
}

impl Drop for KeyRing {
    fn drop(&mut self) {
        for key in self.0.values_mut() {
            zeroize::Zeroize::zeroize(key);
        }
    }
}

impl KeyRing {
    /// The keys `given` names, each kept only when its bytes are the key its name names.
    #[must_use]
    pub fn of(given: &BTreeMap<KeyName, String>) -> Self {
        Self(
            given
                .iter()
                .filter_map(|(name, hex)| {
                    let key = TextKey::parse(name, &hex::decode(hex).ok()?)?;
                    Some((name.clone(), *key.bytes()))
                })
                .collect(),
        )
    }

    /// The keys `given` names joined in, each kept only when its bytes are the key its name names,
    /// and `gone` taken out; the names whose standing moved, held or not.
    pub fn change(
        &mut self,
        given: &BTreeMap<KeyName, String>,
        gone: &[KeyName],
    ) -> std::collections::BTreeSet<KeyName> {
        let mut moved = std::collections::BTreeSet::new();
        for name in gone {
            if let Some(mut bytes) = self.0.remove(name) {
                zeroize::Zeroize::zeroize(&mut bytes);
                moved.insert(name.clone());
            }
        }
        for (name, bytes) in &Self::of(given).0 {
            if self.0.insert(name.clone(), *bytes).is_none() {
                moved.insert(name.clone());
            }
        }
        moved
    }

    /// Whether the key named `name` is held.
    #[must_use]
    pub fn holds(&self, name: &KeyName) -> bool {
        self.0.contains_key(name)
    }

    /// The key named `name`, or missing.
    #[must_use]
    pub fn key(&self, name: &KeyName) -> Words<TextKey> {
        self.0
            .get(name)
            .and_then(|bytes| TextKey::parse(name, bytes))
            .map_or_else(|| Gone::Missing.into(), Words::kept)
    }
}

/// A key as the pull spells it: its bytes in hex.
#[must_use]
pub fn spelled(key: &TextKey) -> String {
    hex::encode(key.bytes())
}
