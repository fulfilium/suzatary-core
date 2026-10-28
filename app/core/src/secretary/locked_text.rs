//! Words locked under a key of their own: the history holds the ciphertext and the key's name,
//! the key lives in a box-local store, and destroying the key erases the words while every object
//! stays whole, immutable and verifiable.
//!
//! The cipher is ChaCha20-Poly1305 (RFC 8439). Each key locks exactly one text, so the nonce is
//! fixed at zero, and the types are what make that safe: a [`FreshKey`] is drawn from
//! [`Entropy`], is neither `Clone` nor `Copy`, and is spent by [`Locked::lock`], which hands back
//! the [`TextKey`] the key store keeps; a `TextKey` only opens. No key is derived, since a derived
//! key could be derived again from its master, which is not erasure.
//!
//! The associated data is the store's [`Shape::DOMAIN`] and the canonical print of the slot's
//! [`Shape`], so a ciphertext moved to another slot or another store does not open. Before
//! locking, the words are padded (ISO/IEC 7816-4: `0x80`, then zeros) to a multiple of
//! [`BUCKET`] bytes, so the ciphertext tells their length only to the bucket.

use std::marker::PhantomData;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use sha2::{Digest, Sha256};

use crate::capability::Entropy;
use crate::secretary::salted_text::Words;

/// How many bytes a key is.
pub const KEY_BYTES: usize = 32;

/// The padded words are a multiple of this many bytes: all a ciphertext tells of their length.
pub const BUCKET: usize = 256;

/// The SHA-256 of a key: 64 lowercase hex, the key file's name and what a locked text names.
///
/// A key is random, so its name confirms nothing about the words, and it needs no salt.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct KeyName(String);

/// A name that is no SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a key's name is 64 lowercase hex, got {0:?}")]
pub struct NotAKeyName(pub String);

impl KeyName {
    /// A name read back, checked once.
    ///
    /// # Errors
    /// [`NotAKeyName`] for anything but 64 lowercase hex.
    pub fn new(name: impl Into<String>) -> Result<Self, NotAKeyName> {
        let name = name.into();
        if name.len() == 64 && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Self(name))
        } else {
            Err(NotAKeyName(name))
        }
    }

    fn of(key: &[u8; KEY_BYTES]) -> Self {
        Self(hex::encode(Sha256::digest(key)))
    }

    /// The hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for KeyName {
    type Error = NotAKeyName;
    fn try_from(name: String) -> Result<Self, NotAKeyName> {
        Self::new(name)
    }
}

impl From<KeyName> for String {
    fn from(name: KeyName) -> String {
        name.0
    }
}

impl std::fmt::Display for KeyName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

prodrome_core::text_literal!(
    KeyName => |name| name.as_str().to_owned(),
);

/// A key that has locked nothing: drawn from [`Entropy`], spent by [`Locked::lock`], never kept.
///
/// Locking spends it and answers the key that opens:
///
/// ```
/// # use suzatary_core::secretary::locked_text::{FreshKey, Locked, Shape};
/// # use suzatary_core::secretary::salted_text::Words;
/// # #[derive(Clone, Debug, PartialEq, Eq)] struct W(String);
/// # impl Words for W { fn print(&self) -> std::borrow::Cow<'_, str> { self.0.as_str().into() } fn read(p: &str) -> Option<Self> { Some(W(p.into())) } }
/// # struct S;
/// # impl Shape for S { type Words = W; const DOMAIN: &'static [u8] = b"doc"; fn print(&self) -> Vec<u8> { Vec::new() } }
/// # fn once(key: FreshKey) {
/// let words = W("words".into());
/// let (locked, opens) = Locked::lock(&S, &words, key);
/// assert_eq!(locked.open(&S, &opens), Some(words));
/// # }
/// ```
///
/// so it locks once:
///
/// ```compile_fail,E0382
/// # use suzatary_core::secretary::locked_text::{FreshKey, Locked, Shape};
/// # use suzatary_core::secretary::salted_text::Words;
/// # #[derive(Clone, PartialEq, Eq)] struct W(String);
/// # impl Words for W { fn print(&self) -> std::borrow::Cow<'_, str> { self.0.as_str().into() } fn read(p: &str) -> Option<Self> { Some(W(p.into())) } }
/// # struct S;
/// # impl Shape for S { type Words = W; const DOMAIN: &'static [u8] = b"doc"; fn print(&self) -> Vec<u8> { Vec::new() } }
/// # fn twice(key: FreshKey) {
/// let words = W("words".into());
/// let _ = Locked::lock(&S, &words, key);
/// let _ = Locked::lock(&S, &words, key);
/// # }
/// ```
///
/// and nothing copies it:
///
/// ```compile_fail,E0599
/// # use suzatary_core::secretary::locked_text::FreshKey;
/// fn copies(key: FreshKey) -> (FreshKey, FreshKey) { (key.clone(), key) }
/// ```
pub struct FreshKey([u8; KEY_BYTES]);

impl FreshKey {
    /// A key nobody has: [`KEY_BYTES`] bytes of `entropy`.
    ///
    /// # Errors
    /// The generator's failure, as a sentence.
    pub fn draw(entropy: &dyn Entropy) -> Result<Self, String> {
        let mut key = [0; KEY_BYTES];
        entropy.fill(&mut key)?;
        Ok(Self(key))
    }
}

impl Drop for FreshKey {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

impl std::fmt::Debug for FreshKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FreshKey(..)")
    }
}

/// The key of one locked text, as the key store keeps it: it opens that text and locks nothing.
///
/// ```compile_fail,E0308
/// # use suzatary_core::secretary::locked_text::{Locked, Shape, TextKey};
/// # use suzatary_core::secretary::salted_text::Words;
/// # #[derive(Clone, PartialEq, Eq)] struct W(String);
/// # impl Words for W { fn print(&self) -> std::borrow::Cow<'_, str> { self.0.as_str().into() } fn read(p: &str) -> Option<Self> { Some(W(p.into())) } }
/// # struct S;
/// # impl Shape for S { type Words = W; const DOMAIN: &'static [u8] = b"doc"; fn print(&self) -> Vec<u8> { Vec::new() } }
/// # fn again(key: TextKey) {
/// let _ = Locked::lock(&S, &W("again".into()), key);
/// # }
/// ```
pub struct TextKey([u8; KEY_BYTES]);

impl TextKey {
    /// The name a locked text gives it.
    #[must_use]
    pub fn name(&self) -> KeyName {
        KeyName::of(&self.0)
    }

    /// The key file: the bytes and nothing else.
    #[must_use]
    pub fn bytes(&self) -> &[u8; KEY_BYTES] {
        &self.0
    }

    /// The key a file under `name` holds, or `None` for bytes `name` does not name: a file
    /// swapped under the store reads as absent, never as another key.
    #[must_use]
    pub fn parse(name: &KeyName, bytes: &[u8]) -> Option<Self> {
        let key = Self(bytes.try_into().ok()?);
        (&key.name() == name).then_some(key)
    }
}

impl Drop for TextKey {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

impl std::fmt::Debug for TextKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TextKey").field(&self.name()).finish()
    }
}

/// What a slot of a store is without its words, and so what its words are bound to.
///
/// The law: `print` is injective, and two stores never share a [`Shape::DOMAIN`].
pub trait Shape {
    /// The words a slot of this shape holds.
    type Words: Words;
    /// The store's tag, so one store's ciphertext never opens as another's.
    const DOMAIN: &'static [u8];
    /// The slot's canonical print: equal exactly when the slots are.
    fn print(&self) -> Vec<u8>;
}

/// Words of a slot of shape `S`, locked: the name of the one key that opens them, and the
/// ciphertext with its tag. It means the words, while that key is kept.
pub struct Locked<S> {
    key: KeyName,
    sealed: Vec<u8>,
    shape: PhantomData<fn() -> S>,
}

impl<S: Shape> Locked<S> {
    /// `words`, locked for the slot `shape` under `key`, which is spent here; the [`TextKey`]
    /// that opens them is the only thing to keep.
    ///
    /// # Panics
    /// Never for a text that fits in memory: RFC 8439 locks up to 256 GiB.
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "taking the key by value is what spends it, so it locks once"
    )]
    pub fn lock(shape: &S, words: &S::Words, key: FreshKey) -> (Self, TextKey) {
        let FreshKey(bytes) = key;
        let sealed = cipher(&bytes)
            .encrypt(
                &Nonce::default(),
                Payload {
                    msg: &pad(words.print().as_bytes()),
                    aad: &aad(shape),
                },
            )
            .expect("RFC 8439 locks up to 256 GiB, and no text is that long");
        let key = TextKey(bytes);
        let locked = Self {
            key: key.name(),
            sealed,
            shape: PhantomData,
        };
        (locked, key)
    }

    /// The words, when `key` is the one that locked them for the slot `shape`; `None` for any
    /// other key or slot, a changed byte, or words [`Words::read`] does not read.
    #[must_use]
    pub fn open(&self, shape: &S, key: &TextKey) -> Option<S::Words> {
        if key.name() != self.key {
            return None;
        }
        OPENS.with(|count| count.set(count.get() + 1));
        let padded = cipher(&key.0)
            .decrypt(
                &Nonce::default(),
                Payload {
                    msg: &self.sealed,
                    aad: &aad(shape),
                },
            )
            .ok()?;
        S::Words::read(std::str::from_utf8(unpad(&padded)?).ok()?)
    }

    /// Locked words as a store read them back: whether they are anything is [`Locked::open`]'s.
    #[must_use]
    pub fn read(key: KeyName, sealed: Vec<u8>) -> Self {
        Self {
            key,
            sealed,
            shape: PhantomData,
        }
    }

    /// The name of the key that opens them.
    #[must_use]
    pub fn key(&self) -> &KeyName {
        &self.key
    }

    /// The ciphertext and its tag.
    #[must_use]
    pub fn sealed(&self) -> &[u8] {
        &self.sealed
    }
}

impl<S> Clone for Locked<S> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            sealed: self.sealed.clone(),
            shape: PhantomData,
        }
    }
}

impl<S> PartialEq for Locked<S> {
    fn eq(&self, other: &Self) -> bool {
        (&self.key, &self.sealed) == (&other.key, &other.sealed)
    }
}

impl<S> Eq for Locked<S> {}

impl<S> std::fmt::Debug for Locked<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Locked")
            .field("key", &self.key)
            .field("sealed", &self.sealed.len())
            .finish()
    }
}

thread_local! {
    static OPENS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many sealed texts this thread has decrypted, so a test can show what a reading opens: the
/// core's tests, and the browser scenes, which read it from the tab's core worker.
#[must_use]
pub fn opens() -> usize {
    OPENS.with(std::cell::Cell::get)
}

fn cipher(key: &[u8; KEY_BYTES]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(Key::from_slice(key))
}

/// The domain's length, the domain, the shape's print: the length makes the split unambiguous,
/// and is eight bytes on every target, so the box and the tab bind the same bytes.
fn aad<S: Shape>(shape: &S) -> Vec<u8> {
    let mut aad = (S::DOMAIN.len() as u64).to_be_bytes().to_vec();
    aad.extend_from_slice(S::DOMAIN);
    aad.extend(shape.print());
    aad
}

/// `0x80`, then zeros to the next multiple of [`BUCKET`].
fn pad(words: &[u8]) -> Vec<u8> {
    let mut padded = Vec::with_capacity((words.len() / BUCKET + 1) * BUCKET);
    padded.extend_from_slice(words);
    padded.push(0x80);
    padded.resize(padded.len().div_ceil(BUCKET) * BUCKET, 0);
    padded
}

/// The words [`pad`] padded, or `None` for bytes it never makes.
fn unpad(padded: &[u8]) -> Option<&[u8]> {
    let end = padded.iter().rposition(|&b| b != 0)?;
    (padded[end] == 0x80 && pad(&padded[..end]) == padded).then_some(&padded[..end])
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// Words of any shape.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Plain(String);

    impl Words for Plain {
        fn print(&self) -> std::borrow::Cow<'_, str> {
            self.0.as_str().into()
        }
        fn read(print: &str) -> Option<Self> {
            Some(Self(print.to_owned()))
        }
    }

    /// A slot of one store: a line's number.
    #[derive(Debug, Clone, Copy)]
    struct Line(u32);

    impl Shape for Line {
        type Words = Plain;
        const DOMAIN: &'static [u8] = b"suzatary test lines v1";
        fn print(&self) -> Vec<u8> {
            self.0.to_be_bytes().to_vec()
        }
    }

    /// A slot of another store, printing the same bytes as a [`Line`] of the same number.
    #[derive(Debug, Clone, Copy)]
    struct Card(u32);

    impl Shape for Card {
        type Words = Plain;
        const DOMAIN: &'static [u8] = b"suzatary test cards v1";
        fn print(&self) -> Vec<u8> {
            self.0.to_be_bytes().to_vec()
        }
    }

    /// The same bytes every time, so a vector can be pinned.
    struct Fixed(u8);

    impl Entropy for Fixed {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
            bytes.fill(self.0);
            Ok(())
        }
    }

    fn key(byte: u8) -> FreshKey {
        FreshKey::draw(&Fixed(byte)).expect("a key")
    }

    proptest! {
        #[test]
        fn lock_then_open_is_the_words(words in "(?s).{0,600}", line: u32, byte: u8) {
            let words = Plain(words);
            let (locked, opens) = Locked::lock(&Line(line), &words, key(byte));
            prop_assert_eq!(locked.key(), &opens.name());
            prop_assert_eq!(locked.open(&Line(line), &opens), Some(words));
        }

        /// Another slot, another store with the same print, another key or a changed byte: none
        /// opens.
        #[test]
        fn nothing_but_its_slot_and_its_key_opens_it(
            words in "(?s).{0,300}", line: u32, other: u32, byte: u8, at: usize, flip in 1u8..,
        ) {
            prop_assume!(line != other);
            let (locked, opens) = Locked::lock(&Line(line), &Plain(words), key(byte));
            prop_assert_eq!(locked.open(&Line(other), &opens), None);
            let moved = Locked::<Card>::read(locked.key().clone(), locked.sealed().to_vec());
            prop_assert_eq!(moved.open(&Card(line), &opens), None);
            let (_, wrong) = Locked::lock(&Line(line), &Plain(String::new()), key(byte.wrapping_add(1)));
            prop_assert_eq!(locked.open(&Line(line), &wrong), None);
            let named_wrong = Locked::<Line>::read(wrong.name(), locked.sealed().to_vec());
            prop_assert_eq!(named_wrong.open(&Line(line), &wrong), None);
            let mut sealed = locked.sealed().to_vec();
            let at = at % sealed.len();
            sealed[at] ^= flip;
            let tampered = Locked::<Line>::read(locked.key().clone(), sealed);
            prop_assert_eq!(tampered.open(&Line(line), &opens), None);
        }

        /// Two texts whose lengths fall in one bucket lock to ciphertexts of one length: the
        /// bucket's and the tag's.
        #[test]
        fn a_ciphertext_tells_the_length_only_to_the_bucket(a in 0usize..1024, b in 0usize..1024) {
            let sealed = |n: usize| Locked::lock(&Line(0), &Plain("x".repeat(n)), key(1)).0.sealed().len();
            prop_assert_eq!(sealed(a) % BUCKET, 16);
            prop_assert_eq!(sealed(a) == sealed(b), a / BUCKET == b / BUCKET);
        }

        #[test]
        fn unpad_reads_exactly_what_pad_wrote(words: Vec<u8>, junk: Vec<u8>) {
            let padded = pad(&words);
            prop_assert_eq!(unpad(&padded), Some(words.as_slice()));
            if pad(&junk) != junk {
                prop_assert_ne!(unpad(&junk), Some(junk.as_slice()));
            }
        }

        #[test]
        fn a_key_reads_back_under_its_name_and_no_other(byte: u8, other: u8) {
            let (_, opens) = Locked::lock(&Line(0), &Plain(String::new()), key(byte));
            let name = opens.name();
            let read = TextKey::parse(&name, opens.bytes()).expect("its own file");
            prop_assert_eq!(read.bytes(), opens.bytes());
            prop_assume!(byte != other);
            prop_assert!(TextKey::parse(&name, &[other; KEY_BYTES]).is_none());
            prop_assert!(TextKey::parse(&name, &opens.bytes()[1..]).is_none());
        }
    }

    /// The names a key has always had: the SHA-256 of its bytes.
    #[test]
    fn a_keys_name_is_stable() {
        let (_, opens) = Locked::lock(&Line(0), &Plain(String::new()), key(7));
        assert_eq!(
            opens.name().as_str(),
            "4bb06f8e4e3a7715d201d573d0aa423762e55dabd61a2c02278fa56cc6d294e0",
        );
    }

    /// FROZEN: a fixed key, slot and words lock to these bytes, on the box and in the tab. They
    /// were checked against a second implementation of RFC 8439 (OpenSSL's, through Python's
    /// `cryptography`) over the same key, zero nonce, padded words and associated data, so the
    /// construction is the standard one and not merely this crate's.
    #[test]
    fn the_locked_bytes_are_frozen() {
        let (locked, opens) =
            Locked::lock(&Line(7), &Plain("Mira is the accountant".into()), key(7));
        let sealed = concat!(
            "7aed5b6d400277ddb41a6120a8028009c38460b57f88453dea4f44c9bb4d7408",
            "de0ea89a53f1ecf3f3d0a462fb77d2baf5e0e5dfefb6897bbd7460315e81182a",
            "c2be6756184ff1346102aa1a537271deb82dfde680c1964a263e95de9d92b637",
            "c82fb56107fe74a0d3679f844d9b0a20200b80d0da84bba0976fb889dac184a4",
            "77d8b6ab9c5ec06d295a3c0e72732aec502a43c73518705f3d6e16ba5399778f",
            "e7c7db790eeb2a0e114118f184bfa74186a16687a178bf69d356e928ba53845b",
            "63317642e5a35f68aed00f27d2d0024c34b463114540e8778be0036a398865fb",
            "592e44177f9d6783738930091c1c20e7680a7403423bf6bf40fa452ad9cf97ee",
            "6f356ac48f98a58180af83ef6b15e875",
        );
        assert_eq!(hex::encode(locked.sealed()), sealed);
        let read = Locked::<Line>::read(opens.name(), hex::decode(sealed).expect("hex"));
        assert_eq!(
            read.open(&Line(7), &opens),
            Some(Plain("Mira is the accountant".into()))
        );
    }

    #[test]
    fn a_name_is_64_lowercase_hex() {
        assert!(KeyName::new("a".repeat(64)).is_ok());
        assert!(KeyName::new("A".repeat(64)).is_err());
        assert!(KeyName::new("../x").is_err());
    }

    #[test]
    fn a_key_never_prints_its_bytes() {
        let (locked, opens) = Locked::lock(&Line(0), &Plain("secret".into()), key(0xab));
        let printed = format!("{opens:?}{:?}{locked:?}", key(0xab));
        assert!(!printed.contains(&"ab".repeat(KEY_BYTES)), "{printed}");
    }
}
