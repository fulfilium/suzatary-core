//! The secretary's memory: facts about the owner kept between conversations, one memory for all.
//!
//! The history, pushed, names facts by id; each fact's words live in a salted box-local file, which
//! forgetting deletes.
//!
//! A memory an email caused persists into every later session, so [`Fact::new`] is the only door
//! and refuses a URL or a line break.
//! `secretary::taint` decides who may write, and when.

use serde::{Deserialize, Serialize};

use prodrome_data::literal::{ProdromeError, Value};

use prodrome_data::literal::codec::{Constructor, Fields, Literal, record};
use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::refusal::{Refusal, shown};

/// The longest fact, in characters.
pub const MAX_FACT_CHARS: usize = 280;

/// How many facts may stand at once, so a model reads the memory whole.
pub const MAX_LIVE: usize = 200;

/// One line of prose, at most [`MAX_FACT_CHARS`], holding no URL; `serde` goes through the same
/// constructor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Fact(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAFact {
    #[error("a fact says something: this one is empty")]
    Empty,
    #[error("a fact is at most {MAX_FACT_CHARS} characters; this one is {0}")]
    TooLong(usize),
    /// A link in every later session is the exfiltration half of an injection attack.
    #[error("a fact holds no URL: say what the page said instead")]
    Url,
    #[error("a fact is one line: no line break or control character")]
    LineBreak,
}

/// What reads as a link, compared case-insensitively.
const LINKS: [&str; 4] = ["://", "www.", "data:", "javascript:"];

impl Fact {
    /// Trims the text.
    pub fn new(text: impl Into<String>) -> Result<Self, NotAFact> {
        let text = text.into().trim().to_owned();
        if text.is_empty() {
            return Err(NotAFact::Empty);
        }
        if text
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        {
            return Err(NotAFact::LineBreak);
        }
        let lower = text.to_lowercase();
        if LINKS.iter().any(|link| lower.contains(link)) {
            return Err(NotAFact::Url);
        }
        let chars = text.chars().count();
        if chars > MAX_FACT_CHARS {
            return Err(NotAFact::TooLong(chars));
        }
        Ok(Self(text))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Fact {
    type Error = NotAFact;
    fn try_from(text: String) -> Result<Self, NotAFact> {
        Self::new(text)
    }
}

impl From<Fact> for String {
    fn from(fact: Fact) -> String {
        fact.0
    }
}

impl crate::secretary::salted_text::Words for Fact {
    fn print(&self) -> std::borrow::Cow<'_, str> {
        self.0.as_str().into()
    }
    fn read(print: &str) -> Option<Self> {
        Self::new(print).ok()
    }
}

impl std::fmt::Display for Fact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a fact is about, closed so a model cannot invent a fifth kind.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "FactKind")]
pub enum Kind {
    Owner,
    Person,
    /// How the owner likes a thing done.
    Preference,
    /// A standing instruction from the owner.
    Standing,
}

impl Kind {
    pub const ALL: [Self; 4] = [Self::Owner, Self::Person, Self::Preference, Self::Standing];

    /// The word a verb takes and an event stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Person => "person",
            Self::Preference => "preference",
            Self::Standing => "standing",
        }
    }

    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == word)
    }
}

/// A fact's number, and the memory's version when it was written: one counter.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Default,
    ts_rs::TS,
    Literal,
)]
#[literal(crate = prodrome_data)]
pub struct FactId(
    // A fact's number stays far below the 2^53 a page's number holds exactly.
    #[ts(type = "number")] pub u64,
);

/// The standing facts one new fact replaces, at least one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Literal)]
#[serde(
    try_from = "std::collections::BTreeSet<FactId>",
    into = "std::collections::BTreeSet<FactId>"
)]
// A literal that reads no source is refused, as the serde form is.
#[literal(crate = prodrome_data, new = Sources::stored)]
pub struct Sources(std::collections::BTreeSet<FactId>);

impl Sources {
    #[must_use]
    pub fn new(ids: std::collections::BTreeSet<FactId>) -> Option<Self> {
        (!ids.is_empty()).then_some(Self(ids))
    }

    fn stored(ids: std::collections::BTreeSet<FactId>) -> Result<Self, ProdromeError> {
        Self::try_from(ids).map_err(ProdromeError::invalid)
    }

    #[must_use]
    pub fn ids(&self) -> &std::collections::BTreeSet<FactId> {
        &self.0
    }
}

impl TryFrom<std::collections::BTreeSet<FactId>> for Sources {
    type Error = &'static str;
    fn try_from(ids: std::collections::BTreeSet<FactId>) -> Result<Self, Self::Error> {
        Self::new(ids).ok_or("a consolidation replaces at least one fact")
    }
}

impl From<Sources> for std::collections::BTreeSet<FactId> {
    fn from(sources: Sources) -> Self {
        sources.0
    }
}

impl std::fmt::Display for Sources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<String> = self.0.iter().map(|id| format!("fact {id}")).collect();
        f.write_str(&ids.join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Replaces {
    Nothing,
    Facts(Sources),
}

impl std::str::FromStr for FactId {
    type Err = std::num::ParseIntError;

    fn from_str(said: &str) -> Result<Self, Self::Err> {
        said.parse().map(Self)
    }
}

impl std::fmt::Display for FactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Who said a fact, read off the transcript (`secretary::taint::opener`), never a model's
/// parameter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "lowercase")]
pub enum Origin {
    /// The round's opening turn, in the owner's own words.
    Turn {
        conversation: ConversationId,
        /// The turn's absolute index in the conversation.
        turn: usize,
    },
    /// The owner's tap on a `Proposal::Remember`.
    Tapped {
        conversation: ConversationId,
        proposal: usize,
    },
}

pub(crate) const TURN: Constructor = ("Turn", &["conversation", "turn"]);
pub(crate) const TAPPED: Constructor = ("Tapped", &["conversation", "proposal"]);

/// An origin is its arm's own constructor.
impl Literal for Origin {
    fn to_value(&self) -> Value {
        match self {
            Origin::Turn { conversation, turn } => {
                record(TURN, vec![conversation.to_value(), turn.to_value()])
            }
            Origin::Tapped {
                conversation,
                proposal,
            } => record(TAPPED, vec![conversation.to_value(), proposal.to_value()]),
        }
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::among(value, "Origin", &[TURN, TAPPED])?;
        let conversation = fields.get("conversation")?;
        Ok(match fields.name() {
            "Turn" => Origin::Turn {
                conversation,
                turn: fields.get("turn")?,
            },
            "Tapped" => Origin::Tapped {
                conversation,
                proposal: fields.get("proposal")?,
            },
            other => return Err(prodrome_data::literal::codec::no_arm("Origin", other)),
        })
    }
}

impl Origin {
    #[must_use]
    pub fn conversation(&self) -> &ConversationId {
        match self {
            Self::Turn { conversation, .. } | Self::Tapped { conversation, .. } => conversation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    /// The memory's version when it was written.
    pub id: FactId,
    pub at: Datetime,
    pub fact: Fact,
    pub kind: Kind,
    pub said: crate::secretary::provenance::Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Merge {
    pub fact: FactId,
    pub into: FactId,
}

/// A fact forgotten: the id, never the text, whose file is deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forgotten {
    pub id: FactId,
    pub at: Datetime,
    /// The gate's actor, or `assistant` for the verb.
    pub by: String,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Memory {
    /// Oldest first.
    pub facts: Vec<Remembered>,
    /// In the order the forgettings happened.
    pub forgotten: Vec<Forgotten>,
    pub merged: Vec<Merge>,
    /// 0 for an empty memory.
    pub version: u64,
}

impl Memory {
    /// A fact whose words are not found is absent, while its number still counts in the version.
    #[must_use]
    pub fn from_history(
        history: &crate::secretary::two_phase::TwoPhase,
        words: impl Fn(&crate::secretary::salted_text::TextName) -> Option<Fact>,
    ) -> Self {
        let facts = history
            .live()
            .filter_map(|(id, kept)| {
                Some(Remembered {
                    id,
                    at: kept.at,
                    fact: words(&kept.text)?,
                    kind: kept.kind,
                    said: history.provenance(id)?.clone(),
                })
            })
            .collect();
        let mut forgotten: Vec<Forgotten> = history
            .remembered()
            .keys()
            .chain(history.forgotten().keys())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter_map(|&id| {
                let gone = history.forgetting(id)?;
                Some(Forgotten {
                    id,
                    at: gone.at,
                    by: gone.by.as_str().to_owned(),
                    version: gone.version,
                })
            })
            .collect();
        forgotten.sort_by_key(|forgotten| (forgotten.version, forgotten.id));
        let mut merged: Vec<Merge> = history
            .replaced()
            .iter()
            .flat_map(|(&fact, into)| into.iter().map(move |&into| Merge { fact, into }))
            .collect();
        merged.sort_by_key(|merge| (merge.into, merge.fact));
        Self {
            facts,
            forgotten,
            merged,
            version: history.version(),
        }
    }

    #[must_use]
    pub fn get(&self, id: FactId) -> Option<&Remembered> {
        self.facts.iter().find(|remembered| remembered.id == id)
    }
}

/// A `kind` argument, parsed, with the refusal naming the four words.
pub fn kind_of(word: &str) -> Result<Kind, Refusal> {
    Kind::parse(word).ok_or_else(|| {
        Refusal::bad_request(format!(
            "kind must be one of {}, got {}",
            Kind::ALL.map(Kind::as_str).join(", "),
            shown(word)
        ))
    })
}

prodrome_data::word_literal!(Kind: Kind::ALL => |kind| kind.as_str());

#[cfg(test)]
mod tests {
    use super::*;

    /// Serde's words for a kind match `ALL`'s literals.
    #[test]
    fn a_kinds_serde_word_is_its_literals() {
        suzatary_base::words::assert_serde_words(&Kind::ALL, |kind| kind.as_str());
    }

    #[test]
    fn a_fact_is_one_short_line_with_no_link() {
        assert_eq!(
            Fact::new("  Mira is her accountant  ").map(String::from),
            Ok("Mira is her accountant".to_owned())
        );
        assert_eq!(Fact::new(" \t "), Err(NotAFact::Empty));
        assert_eq!(Fact::new("one\ntwo"), Err(NotAFact::LineBreak));
        assert_eq!(Fact::new("one\u{2028}two"), Err(NotAFact::LineBreak));
        for link in [
            "see https://evil.test/x",
            "WWW.example.org",
            "ftp://a",
            "data:image/png",
            "JavaScript:alert(1)",
        ] {
            assert_eq!(Fact::new(link), Err(NotAFact::Url), "{link}");
        }
        assert!(Fact::new("x".repeat(MAX_FACT_CHARS)).is_ok());
        assert_eq!(
            Fact::new("x".repeat(MAX_FACT_CHARS + 1)),
            Err(NotAFact::TooLong(MAX_FACT_CHARS + 1))
        );
        assert!(
            serde_json::from_str::<Fact>("\"https://a.test\"").is_err(),
            "a hand-edited line goes through the same door"
        );
    }
}
