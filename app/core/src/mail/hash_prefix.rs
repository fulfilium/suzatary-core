//! A canonical content hash named by a prefix of it, as a model and a todo's `source.hash` name
//! one, and what such a name resolves to among the hashes a box holds.

/// A canonical-hash prefix, `[0-9a-f]{6,64}`; a mixed-case id is a notmuch id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashPrefix(String);

impl HashPrefix {
    /// `id` as a hash prefix, or `None` when it is not one.
    #[must_use]
    pub fn parse(id: &str) -> Option<Self> {
        ((6..=64).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f')))
        .then(|| Self(id.to_owned()))
    }

    /// The prefix as it was written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The one entry of `entries` whose hash begins with this prefix.
    pub fn resolve<T>(&self, entries: impl IntoIterator<Item = (String, T)>) -> Resolved<T> {
        let mut matches: Vec<(String, T)> = entries
            .into_iter()
            .filter(|(hash, _)| hash.starts_with(&self.0))
            .collect();
        match matches.len() {
            0 => Resolved::Unlisted,
            1 => {
                let (hash, value) = matches.remove(0);
                Resolved::One(hash, value)
            }
            _ => {
                let mut hashes: Vec<String> = matches.into_iter().map(|(hash, _)| hash).collect();
                hashes.sort();
                Resolved::Ambiguous(Ambiguous {
                    prefix: self.0.clone(),
                    matches: hashes,
                })
            }
        }
    }
}

/// What a [`HashPrefix`] names among the hashes a box holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved<T> {
    /// No hash begins with it.
    Unlisted,
    /// One does: the full hash, and what is held under it.
    One(String, T),
    /// More than one does.
    Ambiguous(Ambiguous),
}

/// A hash prefix matched more than one message.
///
/// It names the matches, since "use more characters" alone is a dead end.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error(
    "hash prefix {prefix:?} matches {} messages ({}) — use more characters",
    matches.len(),
    matches.iter().map(|h| h.chars().take(12).collect::<String>()).collect::<Vec<_>>().join(", ")
)]
pub struct Ambiguous {
    /// What was asked for.
    pub prefix: String,
    /// Every hash it matched, sorted.
    pub matches: Vec<String>,
}
