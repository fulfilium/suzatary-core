//! Salted texts on disk, one file per [`Text`] named by its [`TextName`], as a [`Doc`]:
//! box-local and git-ignored beside the objects that name a text and never hold it. Forgetting
//! removes the file; a file swapped under the store, or whose words do not read, reads as absent.
//!
//! [`Doc`]: suzatary_core::store::Doc

use std::path::PathBuf;

use suzatary_core::capability::Entropy;
use suzatary_core::secretary::salted_text::{Text, TextName, Words};

use crate::docs::{DocCodec, Docs};

/// Salted words of one kind, by name.
pub type Texts<T> = Docs<TextName, Text<T>>;

impl<T: Words + 'static> Texts<T> {
    /// The texts under `dir`, made on the first write.
    #[must_use]
    pub fn open(dir: PathBuf) -> Self {
        Docs::new(
            "texts",
            move |name: &TextName| dir.join(name.as_str()),
            Self::CODEC,
        )
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
    use suzatary_core::secretary::memory::Fact;
    use suzatary_core::secretary::salted_text::SALT_BYTES;
    use suzatary_core::store::{Doc as _, Gets as _};

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
        store.remove(&name).expect("removed");
        store.remove(&name).expect("removing nothing succeeds");
        assert_eq!(store.get(&name).expect("reads"), None);
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
