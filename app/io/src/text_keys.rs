//! The keys of locked texts on disk, one file per [`TextKey`] named by its [`KeyName`], as a
//! [`Doc`]: box-local, readable by this user alone, in the vault and never pushed. A key is put
//! once, read by name, and leaves only by `remove`, which destroys it and succeeds when it is
//! already gone; a file swapped under the store reads as absent.
//!
//! [`Doc`]: suzatary_core::store::Doc

use std::path::PathBuf;

use suzatary_core::secretary::locked_text::{KeyName, TextKey};

use crate::docs::{DocCodec, Docs};

/// The keys of one store's locked texts, by name.
pub type TextKeys = Docs<KeyName, TextKey>;

/// The keys under `dir`, made on the first put.
#[must_use]
pub fn open(dir: PathBuf) -> TextKeys {
    Docs::new(
        "keys",
        move |name: &KeyName| dir.join(name.as_str()),
        &CODEC,
    )
    .private()
}

/// The file is the key's bytes, and its name is their digest.
const CODEC: DocCodec<KeyName, TextKey> = DocCodec {
    print: |name, key| {
        if &key.name() == name {
            Ok(key.bytes().to_vec())
        } else {
            Err(format!("this key is not {name}"))
        }
    },
    parse: |name, bytes| Ok(TextKey::parse(name, &bytes)),
};

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use suzatary_core::secretary::locked_text::{FreshKey, Locked, Shape};
    use suzatary_core::secretary::memory::Fact;
    use suzatary_core::store::{Doc as _, Gets as _};

    use super::*;
    use crate::texts::SystemEntropy;

    struct Slot;

    impl Shape for Slot {
        type Words = Fact;
        const DOMAIN: &'static [u8] = b"suzatary test keys v1";
        fn print(&self) -> Vec<u8> {
            Vec::new()
        }
    }

    fn locked(words: &str) -> (Locked<Slot>, TextKey) {
        let key = FreshKey::draw(&SystemEntropy).expect("a key");
        Locked::lock(&Slot, &Fact::new(words).expect("a fact"), key)
    }

    #[test]
    fn a_key_is_put_read_back_opens_and_is_destroyed_once_and_for_all() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let keys = open(dir.path().join("keys"));
        let (text, key) = locked("He takes the 7:40");
        let name = key.name();
        keys.set(&name, &key).expect("put");
        assert_eq!(
            std::fs::metadata(dir.path().join("keys"))
                .expect("a directory")
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "the store lists to this user alone"
        );
        let read = keys.get(&name).expect("reads").expect("kept");
        assert_eq!(text.open(&Slot, &read), Fact::new("He takes the 7:40").ok());
        let file = dir.path().join("keys").join(name.as_str());
        assert_eq!(
            std::fs::metadata(&file)
                .expect("a file")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        keys.remove(&name).expect("destroyed");
        keys.remove(&name)
            .expect("destroying what is gone succeeds");
        assert!(keys.get(&name).expect("reads").is_none());
        assert_eq!(
            std::fs::read_dir(dir.path().join("keys"))
                .expect("the store")
                .count(),
            0
        );
    }

    #[test]
    fn a_swapped_file_reads_as_absent_and_a_key_is_kept_only_under_its_name() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let keys = open(dir.path().to_path_buf());
        let (_, key) = locked("Mira is the accountant");
        let (_, other) = locked("Mira is the accountant");
        assert_ne!(key.name(), other.name(), "two draws are two keys");
        assert!(keys.set(&other.name(), &key).is_err());
        keys.set(&key.name(), &key).expect("put");
        std::fs::write(dir.path().join(key.name().as_str()), other.bytes()).expect("swapped");
        assert!(keys.get(&key.name()).expect("reads").is_none());
    }
}
