//! A rule's version: the SHA-256 of everything that decides what the rule makes, so each value
//! the rule makes can record the version it was made under, and a reader can find every value
//! made under an older one.
//!
//! A rule is a type implementing [`Versioned`], which says what its parts are, for each of its
//! cases: a rule that acts differently on two kinds of input has a version for each, so a change
//! to one case leaves the values made under the other current. A part is best what the rule DOES
//! on a fixed probe, which no list of names can miss; what a probe cannot reach is covered by the
//! rule's revision, a number raised by hand when that code changes what the rule makes.

use std::marker::PhantomData;

use prodrome_core::literal::{ProdromeError, Value};
use sha2::{Digest as _, Sha256};

use crate::literal::Literal;

/// A rule whose output records the version it was made under.
pub trait Versioned: 'static {
    /// The kinds of input the rule acts on differently, each with a version of its own; `()`
    /// for a rule with one.
    type Case: Copy;

    /// The rule, in a few words, for a refusal to name.
    const WHAT: &'static str;

    /// Raised by hand when code the parts do not show changes what the rule makes.
    const REVISION: u32;

    /// What decides how the rule acts on `case`, in a fixed order.
    fn parts(case: Self::Case) -> Vec<String>;

    /// The version this build makes values of `case` under.
    fn current(case: Self::Case) -> &'static Version<Self>
    where
        Self: Sized;

    /// The rule's pin, which its test asserts: `Ok` when its version on `case` is `version` and
    /// what it did on its test's fixed corpus, `behaviour`, hashes to `did`; otherwise what both
    /// read now. When what the rule does moved and its version did not, a part the version
    /// hashes missed the change: raise [`Versioned::REVISION`], then pin both.
    ///
    /// # Errors
    /// [`Moved`] naming both readings, for a pin that no longer holds.
    fn pinned(
        case: Self::Case,
        behaviour: impl IntoIterator<Item = String>,
        version: &str,
        did: &str,
    ) -> Result<(), Moved>
    where
        Self: Sized,
    {
        let mut hash = Sha256::new();
        for part in behaviour {
            hash.update(part.as_bytes());
            hash.update([0]);
        }
        let moved = Moved {
            what: Self::WHAT,
            version: Self::current(case).as_str().to_owned(),
            did: hex::encode(hash.finalize()),
        };
        if moved.version == version && moved.did == did {
            Ok(())
        } else {
            Err(moved)
        }
    }
}

/// A rule's pin that no longer holds: its version and what it did, as they read now.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{what} moved: its version is {version} and what it did hashes to {did}; if the version did \
     not move with what it did, raise its REVISION; then pin both"
)]
pub struct Moved {
    /// The rule.
    pub what: &'static str,
    /// Its version now.
    pub version: String,
    /// What it did on the corpus, hashed, now.
    pub did: String,
}

/// A version of the rule `R`: 64 lowercase hex.
pub struct Version<R> {
    hex: String,
    of: PhantomData<fn() -> R>,
}

/// A name that is no SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a version of {what} is 64 lowercase hex, got {got:?}")]
pub struct NotAVersion {
    /// The rule.
    pub what: &'static str,
    /// What was read.
    pub got: String,
}

impl<R: Versioned> Version<R> {
    /// The version of `R` on `case` as this build defines it: each of [`Versioned::parts`],
    /// then [`Versioned::REVISION`], each ended by a NUL, which no part holds, so the parts split
    /// one way. A rule's [`Versioned::current`] keeps this once per case.
    #[must_use]
    pub fn of_parts(case: R::Case) -> Self {
        let mut hash = Sha256::new();
        for part in R::parts(case).into_iter().chain([R::REVISION.to_string()]) {
            hash.update(part.as_bytes());
            hash.update([0]);
        }
        Self {
            hex: hex::encode(hash.finalize()),
            of: PhantomData,
        }
    }

    /// A version read back, checked once.
    ///
    /// # Errors
    /// [`NotAVersion`] for anything but 64 lowercase hex.
    pub fn new(hex: impl Into<String>) -> Result<Self, NotAVersion> {
        let hex = hex.into();
        if hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Self {
                hex,
                of: PhantomData,
            })
        } else {
            Err(NotAVersion {
                what: R::WHAT,
                got: hex,
            })
        }
    }

    /// Whether this is the version this build makes values of `case` under.
    #[must_use]
    pub fn is_current(&self, case: R::Case) -> bool {
        self == R::current(case)
    }
}

impl<R: Versioned> Version<R> {
    /// The version this build makes values of `case` under.
    #[must_use]
    pub fn current(case: R::Case) -> &'static Self {
        R::current(case)
    }
}

impl<R> Version<R> {
    /// The hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.hex
    }
}

impl<R> Clone for Version<R> {
    fn clone(&self) -> Self {
        Self {
            hex: self.hex.clone(),
            of: PhantomData,
        }
    }
}

impl<R> PartialEq for Version<R> {
    fn eq(&self, other: &Self) -> bool {
        self.hex == other.hex
    }
}

impl<R> Eq for Version<R> {}

impl<R> PartialOrd for Version<R> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<R> Ord for Version<R> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.hex.cmp(&other.hex)
    }
}

impl<R> std::hash::Hash for Version<R> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hex.hash(state);
    }
}

/// Its hex, as a string.
impl<R> serde::Serialize for Version<R> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex)
    }
}

/// A string, checked as [`Version::new`] checks it.
impl<'de, R: Versioned> serde::Deserialize<'de> for Version<R> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(deserializer)?;
        Self::new(hex).map_err(serde::de::Error::custom)
    }
}

impl<R> std::fmt::Debug for Version<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Version").field(&self.hex).finish()
    }
}

impl<R> std::fmt::Display for Version<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.hex)
    }
}

impl<R: Versioned> Literal for Version<R> {
    fn to_value(&self) -> Value {
        Value::str(self.hex.as_str())
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let hex = String::from_value(value)?;
        Self::new(hex).map_err(|refused| ProdromeError::invalid(refused.to_string()))
    }
}

/// Which version of `R` a stored value was made under.
pub enum Under<R> {
    /// The version named.
    Version(Version<R>),
    /// A version from before values of this kind recorded one.
    Unversioned,
}

impl<R> Clone for Under<R> {
    fn clone(&self) -> Self {
        match self {
            Under::Version(version) => Under::Version(version.clone()),
            Under::Unversioned => Under::Unversioned,
        }
    }
}

impl<R> PartialEq for Under<R> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Under::Version(one), Under::Version(two)) => one == two,
            (Under::Unversioned, Under::Unversioned) => true,
            (Under::Version(_), Under::Unversioned) | (Under::Unversioned, Under::Version(_)) => {
                false
            }
        }
    }
}

impl<R> Eq for Under<R> {}

/// Every version before the unversioned, as the stored forms were declared.
impl<R> PartialOrd for Under<R> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<R> Ord for Under<R> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Under::Version(one), Under::Version(two)) => one.cmp(two),
            (Under::Version(_), Under::Unversioned) => Ordering::Less,
            (Under::Unversioned, Under::Version(_)) => Ordering::Greater,
            (Under::Unversioned, Under::Unversioned) => Ordering::Equal,
        }
    }
}

impl<R> std::hash::Hash for Under<R> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Under::Version(version) => {
                state.write_u8(0);
                version.hash(state);
            }
            Under::Unversioned => state.write_u8(1),
        }
    }
}

impl<R> std::fmt::Debug for Under<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Under::Version(version) => f.debug_tuple("Version").field(version).finish(),
            Under::Unversioned => f.write_str("Unversioned"),
        }
    }
}

impl<R: Versioned> Under<R> {
    /// The version this build makes values of `case` under.
    #[must_use]
    pub fn current(case: R::Case) -> Self {
        Under::Version(R::current(case).clone())
    }

    /// Whether the value was made under the version this build makes values of `case` under.
    #[must_use]
    pub fn is_current(&self, case: R::Case) -> bool {
        match self {
            Under::Version(version) => version.is_current(case),
            Under::Unversioned => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use super::*;

    struct Rule;

    static ONE: LazyLock<Version<Rule>> = LazyLock::new(|| Version::of_parts(false));
    static TWO: LazyLock<Version<Rule>> = LazyLock::new(|| Version::of_parts(true));

    /// A rule of two cases, whose second case reads one more part.
    impl Versioned for Rule {
        type Case = bool;
        const WHAT: &'static str = "a test rule";
        const REVISION: u32 = 1;
        fn parts(second: bool) -> Vec<String> {
            let mut parts = vec!["a".to_owned(), "b".to_owned()];
            if second {
                parts.push("c".to_owned());
            }
            parts
        }
        fn current(second: bool) -> &'static Version<Self> {
            if second { &TWO } else { &ONE }
        }
    }

    #[test]
    fn a_version_prints_and_reads_back_and_only_hex_reads() {
        let current = Rule::current(false).clone();
        assert_eq!(
            Version::<Rule>::from_value(&current.to_value()),
            Ok(current)
        );
        assert!(Version::<Rule>::new("not hex").is_err());
        assert!(Version::<Rule>::new("A".repeat(64)).is_err());
    }

    /// A value is current only under its own case's version.
    #[test]
    fn only_the_current_version_of_its_case_is_current() {
        assert!(Under::<Rule>::current(false).is_current(false));
        assert!(!Under::<Rule>::current(false).is_current(true));
        assert!(!Under::<Rule>::Unversioned.is_current(false));
        let older = Version::<Rule>::new("0".repeat(64)).expect("hex");
        assert!(!Under::Version(older).is_current(false));
    }

    /// A pin holds while both the version and what the rule did read as pinned, and names
    /// both readings once either moves.
    #[test]
    fn a_pin_holds_until_the_version_or_what_it_did_moves() {
        let did = || vec!["scrubbed".to_owned()];
        let version = Rule::current(false).as_str().to_owned();
        let Err(found) = Rule::pinned(false, did(), "", "") else {
            panic!("an empty pin holds");
        };
        assert_eq!(found.version, version);
        assert_eq!(Rule::pinned(false, did(), &version, &found.did), Ok(()));
        let moved = Rule::pinned(false, vec!["other".to_owned()], &version, &found.did);
        assert!(moved.is_err());
        assert!(Rule::pinned(true, did(), &version, &found.did).is_err());
    }

    /// A version prints to JSON as its hex and reads back only as hex; every version orders
    /// before the unversioned.
    #[test]
    fn a_version_round_trips_as_a_string_and_orders_before_none() {
        let current = Rule::current(false).clone();
        let json = serde_json::to_string(&current).expect("prints");
        assert_eq!(json, format!("\"{current}\""));
        assert_eq!(
            serde_json::from_str::<Version<Rule>>(&json).expect("reads"),
            current
        );
        assert!(serde_json::from_str::<Version<Rule>>("\"x\"").is_err());
        assert!(Under::Version(current) < Under::<Rule>::Unversioned);
    }

    /// The parts are ended, so moving a byte from one part to the next moves the version.
    #[test]
    fn the_parts_split_one_way() {
        struct Moved;
        static MOVED: LazyLock<Version<Moved>> = LazyLock::new(|| Version::of_parts(()));
        impl Versioned for Moved {
            type Case = ();
            const WHAT: &'static str = "a moved rule";
            const REVISION: u32 = 1;
            fn parts((): ()) -> Vec<String> {
                vec!["ab".to_owned(), String::new()]
            }
            fn current((): ()) -> &'static Version<Self> {
                &MOVED
            }
        }
        assert_ne!(Rule::current(false).as_str(), Moved::current(()).as_str());
    }
}
