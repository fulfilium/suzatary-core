use sha2::{Digest, Sha256};

/// The hash of the set of object names a reading is a function of, so two replicas holding the
/// same objects name one reading, whatever order the objects arrived in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReadingName(String);

impl ReadingName {
    /// Each name once, length-prefixed so no two sets share a print.
    #[must_use]
    pub fn of<I>(names: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let set: std::collections::BTreeSet<String> = names
            .into_iter()
            .map(|name| name.as_ref().to_owned())
            .collect();
        let mut hash = Sha256::new();
        for name in &set {
            // A fixed width, so the box and the tab's wasm (where `usize` is 32 bits) name one
            // reading alike; a `usize` is never wider than 64 bits.
            hash.update((name.len() as u64).to_be_bytes());
            hash.update(name.as_bytes());
        }
        Self(hex::encode(hash.finalize()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ReadingName {
    /// The first twelve hex digits, as a log line names an object.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.get(..12).unwrap_or(&self.0))
    }
}

pub trait Named {
    /// A function of the objects it reads, never of when or how they were read.
    fn name(&self) -> ReadingName;
}
