//! Salted texts on disk, one file per [`Text`] named by its [`TextName`], as a [`Doc`]:
//! box-local and git-ignored. A swapped file, or one whose words do not read, reads as absent.
//!
//! [`Doc`]: suzatary_store::store::Doc

use std::collections::BTreeSet;
use std::path::PathBuf;

use suzatary_base::capability::Entropy;
use suzatary_memory::secretary::salted_text::{Text, TextName, Words};
use suzatary_store::store::{Doc, Gets, Lists, StoreError};

use crate::docs::{DocCodec, Docs};
use crate::present::Present as _;

#[derive(Debug, Clone)]
pub struct Texts<T: 'static> {
    dir: PathBuf,
    docs: Docs<TextName, Text<T>>,
}

impl<T: Words + 'static> Texts<T> {
    /// The texts under `dir`, made on the first write.
    #[must_use]
    pub fn open(dir: PathBuf) -> Self {
        let under = dir.clone();
        Self {
            docs: Docs::new(
                "texts",
                move |name: &TextName| under.join(name.as_str()),
                Self::CODEC,
            ),
            dir,
        }
    }

    /// The name is the bytes' digest, and the bytes' words go through [`Words::read`].
    const CODEC: &'static DocCodec<TextName, Text<T>> = &DocCodec {
        print: |name, text| {
            if &text.name() == name {
                Ok(text.bytes())
            } else {
                Err(format!("this text is not {name}"))
            }
        },
        parse: |name, bytes| Ok(Text::parse(name, &bytes)),
    };
}

impl<T: Sync + 'static> Gets<TextName, Text<T>> for Texts<T> {
    fn get(&self, name: &TextName) -> Result<Option<Text<T>>, StoreError> {
        self.docs.get(name)
    }
}

impl<T: Sync + 'static> Doc<TextName, Text<T>> for Texts<T> {
    fn set(&self, name: &TextName, text: &Text<T>) -> Result<(), StoreError> {
        self.docs.set(name, text)
    }

    fn remove(&self, name: &TextName) -> Result<(), StoreError> {
        self.docs.remove(name)
    }
}

/// A write's temporary is not named as a text is, so it is not listed.
impl<T: Sync + 'static> Lists<TextName, Text<T>> for Texts<T> {
    fn held(&self) -> Result<BTreeSet<TextName>, StoreError> {
        let unreadable = |failed: &std::io::Error| {
            StoreError::unreadable("texts", format!("{}: {failed}", self.dir.display()))
        };
        let Some(entries) = std::fs::read_dir(&self.dir)
            .present()
            .map_err(|failed| unreadable(&failed))?
        else {
            return Ok(BTreeSet::new());
        };
        let mut held = BTreeSet::new();
        for entry in entries {
            let entry = entry.map_err(|failed| unreadable(&failed))?;
            if let Some(name) = entry
                .file_name()
                .to_str()
                .and_then(|name| TextName::new(name).ok())
            {
                held.insert(name);
            }
        }
        Ok(held)
    }
}

/// Bytes from the system's generator.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemEntropy;

impl Entropy for SystemEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), bytes)
            .map_err(|_| "the system's random generator failed".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use suzatary_base::capability::SALT_BYTES;
    use suzatary_memory::secretary::memory::Fact;

    use super::*;

    #[test]
    fn a_text_is_kept_read_back_and_removed() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = Texts::<Fact>::open(dir.path().join("texts"));
        let text = Text::new(
            Fact::new("Mira is the accountant").expect("a fact"),
            SystemEntropy.salt().expect("a salt"),
        );
        let name = text.name();
        store.set(&name, &text).expect("kept");
        assert_eq!(store.get(&name).expect("reads"), Some(text.clone()));
        let files: Vec<_> = std::fs::read_dir(dir.path().join("texts"))
            .expect("the store exists")
            .collect();
        assert_eq!(files.len(), 1, "one file, no temporaries left behind");
        std::fs::write(dir.path().join("texts").join(".tmp-a-write"), b"").expect("a temporary");
        assert_eq!(store.held().expect("lists"), BTreeSet::from([name.clone()]));
        std::fs::remove_file(dir.path().join("texts").join(".tmp-a-write")).expect("gone");
        store.remove(&name).expect("removed");
        store.remove(&name).expect("removing nothing succeeds");
        assert_eq!(store.get(&name).expect("reads"), None);
        assert!(store.held().expect("lists").is_empty());
        assert!(
            Texts::<Fact>::open(dir.path().join("never-written"))
                .held()
                .expect("a store never written holds nothing")
                .is_empty()
        );
        assert_eq!(
            std::fs::read_dir(dir.path().join("texts"))
                .expect("the store exists")
                .count(),
            0
        );
    }

    #[test]
    fn a_swapped_file_reads_as_absent_and_a_text_is_kept_only_under_its_name() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = Texts::<Fact>::open(dir.path().to_path_buf());
        let text = Text::new(
            Fact::new("He takes the 7:40").expect("a fact"),
            [1; SALT_BYTES],
        );
        let other = Text::new(
            Fact::new("He takes the 8:10").expect("a fact"),
            [1; SALT_BYTES],
        );
        assert!(store.set(&other.name(), &text).is_err());
        store.set(&text.name(), &text).expect("kept");
        std::fs::write(dir.path().join(text.name().as_str()), other.bytes()).expect("swapped");
        assert_eq!(store.get(&text.name()).expect("reads"), None);
        assert_ne!(
            SystemEntropy.salt().expect("a salt"),
            SystemEntropy.salt().expect("a salt")
        );
    }
}
