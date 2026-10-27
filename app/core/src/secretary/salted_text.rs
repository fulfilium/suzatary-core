//! Words kept apart from the history that names them: a salt and the words, in a file of their
//! own named by the SHA-256 of both.
//!
//! A history names words by that [`TextName`] and never holds them, so deleting the file leaves
//! the history verifiable and the words gone. The salt is why the name tells nothing once the
//! file is gone: a bare digest of a sentence this short would confirm a guess at it to anyone
//! holding the objects, and the salt is deleted with the words. Which words go is
//! [`crate::secretary::texts`]'s.

use sha2::{Digest, Sha256};

/// The SHA-256 of a [`Text`]'s bytes: 64 lowercase hex, the file's name.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct TextName(String);

/// A name that is no SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a text's name is 64 lowercase hex, got {0:?}")]
pub struct NotATextName(pub String);

impl TextName {
    /// A name read back, checked once.
    ///
    /// # Errors
    /// [`NotATextName`] for anything but 64 lowercase hex.
    pub fn new(name: impl Into<String>) -> Result<Self, NotATextName> {
        let name = name.into();
        if name.len() == 64 && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Self(name))
        } else {
            Err(NotATextName(name))
        }
    }

    /// The hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TextName {
    type Error = NotATextName;
    fn try_from(name: String) -> Result<Self, NotATextName> {
        Self::new(name)
    }
}

impl From<TextName> for String {
    fn from(name: TextName) -> String {
        name.0
    }
}

impl std::fmt::Display for TextName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How many random bytes salt a text.
pub const SALT_BYTES: usize = 16;

/// Words a text store can keep: one print, and the door that reads it back.
///
/// The law, tested per instance: `read(print(w)) == Some(w)`. A print may hold line breaks, since
/// the salt before it is fixed-width hex and the first break ends it.
pub trait Words: Clone + Eq {
    /// The words as kept.
    fn print(&self) -> std::borrow::Cow<'_, str>;
    /// The words `print` is, through the type's own door; `None` for text that is no such words.
    fn read(print: &str) -> Option<Self>;
}

/// Words and the salt they are kept with. It means the words: the salt only hides the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text<T> {
    salt: [u8; SALT_BYTES],
    words: T,
}

impl<T: Words> Text<T> {
    /// `words`, salted with `salt`, which the interpreter draws at random.
    #[must_use]
    pub fn new(words: T, salt: [u8; SALT_BYTES]) -> Self {
        Self { salt, words }
    }

    /// The words.
    #[must_use]
    pub fn words(&self) -> &T {
        &self.words
    }

    /// The file: the salt in hex, a line break, the words' print.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        format!("{}\n{}", hex::encode(self.salt), self.words.print()).into_bytes()
    }

    /// The name a history gives it.
    #[must_use]
    pub fn name(&self) -> TextName {
        TextName(hex::encode(Sha256::digest(self.bytes())))
    }

    /// The text a file under `name` holds, or `None` for bytes `name` does not name or that are
    /// not the canonical print of salted words: a file swapped under the store reads as absent,
    /// never as something else, and its words go through [`Words::read`] like every other's.
    #[must_use]
    pub fn parse(name: &TextName, bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes).ok()?;
        let (salt, words) = text.split_once('\n')?;
        let salt: [u8; SALT_BYTES] = hex::decode(salt).ok()?.try_into().ok()?;
        let read = Self::new(T::read(words)?, salt);
        (read.bytes() == bytes && &read.name() == name).then_some(read)
    }
}

// The literal of each is its text, as an inbox act stores it.
prodrome_core::text_literal!(
    TextName => |name| String::from(name.clone()),
);

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::secretary::memory::Fact;

    /// Words of any shape, line breaks included: the store is not the fact's.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Lines(String);

    impl Words for Lines {
        fn print(&self) -> std::borrow::Cow<'_, str> {
            self.0.as_str().into()
        }
        fn read(print: &str) -> Option<Self> {
            Some(Self(print.to_owned()))
        }
    }

    fn fact(text: &str) -> Fact {
        Fact::new(text).expect("a fact")
    }

    /// A text reads back under its name and under no other; the salt is in the name.
    fn reads_back_under_its_name<T: Words + std::fmt::Debug>(words: T, salt: [u8; SALT_BYTES]) {
        let text = Text::new(words.clone(), salt);
        let name = text.name();
        assert_eq!(T::read(&words.print()), Some(words.clone()), "the law");
        assert_eq!(Text::parse(&name, &text.bytes()), Some(text.clone()));
        let mut other_salt = salt;
        other_salt[0] ^= 1;
        let other = Text::new(words, other_salt);
        assert_ne!(other.name(), name);
        assert_eq!(Text::<T>::parse(&name, &other.bytes()), None);
        assert_eq!(Text::<T>::parse(&name, b"no salt"), None);
    }

    proptest! {
        #[test]
        fn any_words_read_back_under_their_name(
            words in "(?s).{0,200}",
            salt in proptest::array::uniform16(any::<u8>()),
        ) {
            reads_back_under_its_name(Lines(words.clone()), salt);
            if let Ok(fact) = Fact::new(words) {
                reads_back_under_its_name(fact, salt);
            }
        }
    }

    /// The bytes a fact was kept as before the store was generic, so the files on disk still read.
    #[test]
    fn a_facts_file_is_the_bytes_it_always_was() {
        let text = Text::new(fact("Mira is the accountant"), [7; SALT_BYTES]);
        assert_eq!(
            text.bytes(),
            b"07070707070707070707070707070707\nMira is the accountant"
        );
        assert_eq!(
            text.name().as_str(),
            hex::encode(Sha256::digest(
                b"07070707070707070707070707070707\nMira is the accountant"
            ))
        );
    }

    #[test]
    fn a_file_whose_words_are_no_fact_reads_as_absent() {
        let bytes = format!("{}\nsee https://evil.test", "00".repeat(SALT_BYTES)).into_bytes();
        let name = TextName(hex::encode(Sha256::digest(&bytes)));
        assert_eq!(Text::<Fact>::parse(&name, &bytes), None);
        let padded = format!("{}\n  padded  ", "00".repeat(SALT_BYTES)).into_bytes();
        let name = TextName(hex::encode(Sha256::digest(&padded)));
        assert_eq!(
            Text::<Fact>::parse(&name, &padded),
            None,
            "a fact is stored trimmed"
        );
        assert!(
            Text::<Lines>::parse(&name, &padded).is_some(),
            "and other words as they are"
        );
    }

    #[test]
    fn a_name_is_64_lowercase_hex() {
        assert!(TextName::new("a".repeat(64)).is_ok());
        assert!(TextName::new("A".repeat(64)).is_err());
        assert!(TextName::new("a".repeat(63)).is_err());
        assert!(TextName::new("../x").is_err());
    }
}
