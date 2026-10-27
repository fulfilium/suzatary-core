//! A reference: one value for "the thing I am pointing at", on a person's turn, in a
//! proposal's sources and in the model's own prose.
//!
//! The text form (`todo:life/x`) is the protocol with the model, which writes text and cannot
//! hand back a tagged union, so it prints and parses by one pair of functions, pinned by a
//! round-trip law. It is also the serde form, which `web/src/chat/refs.ts` parses.
//!
//! Every part is parsed into a type that refuses `..`, a slash and an empty segment, so a
//! `Reference` off the wire cannot name a path and nothing downstream has to check.

use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use prodrome_core::event::TodoId;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::chains::ChainName;
use crate::secretary::store::ConversationId;
use crate::secretary::topics::target::Target;

/// A string that is not a reference, and which part of it is wrong; one type for the whole
/// grammar, since a caller only shows the sentence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAReference {
    /// No `todo:` / `mail:` / `thread:` / `briefing:` / `proposal:` /
    /// `batch:` / `round:` / `note:` prefix.
    #[error(
        "a reference is todo:<chain>/<id>, mail:<hash>, thread:<id>, \
         briefing:<YYYY-MM-DD>, proposal:<conversation>/<n>, \
         batch:<chain>/<head>, round:<conversation>/<turn> or \
         note:<conversation>/<turn>, got {0:?}"
    )]
    Unknown(String),
    /// The prefix was right and what followed it was not.
    #[error("{scheme}: {said}")]
    Shape {
        /// Which prefix was read.
        scheme: &'static str,
        /// What is wrong with the rest, in the part's own words.
        said: String,
    },
}

impl NotAReference {
    fn shape(scheme: &'static str, said: impl Into<String>) -> Self {
        Self::Shape {
            scheme,
            said: said.into(),
        }
    }
}

/// A message's name: the canonical hash, whole or as a prefix of six to sixty-four lowercase
/// hex. A prefix is deliberate, since todos' `source.hash` already holds prefixes and the
/// reverse index resolves them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MailHash(String);

impl TryFrom<String> for MailHash {
    type Error = NotAReference;

    fn try_from(text: String) -> Result<Self, NotAReference> {
        Self::new(text)
    }
}

impl From<MailHash> for String {
    fn from(hash: MailHash) -> Self {
        hash.0
    }
}

impl MailHash {
    /// Parse one.
    ///
    /// # Errors
    /// Anything that is not six to sixty-four lowercase hex characters.
    pub fn new(text: impl Into<String>) -> Result<Self, NotAReference> {
        let text = text.into();
        let ok = matches!(text.len(), 6..=64)
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if ok {
            Ok(Self(text))
        } else {
            Err(NotAReference::shape(
                "mail",
                format!("a message hash is 6-64 lowercase hex, got {text:?}"),
            ))
        }
    }

    /// The hash, as everything spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Notmuch's thread id. The id is notmuch's, so this refuses only what would make the text
/// form ambiguous or a path; it is passed back, never interpreted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(String);

impl ThreadId {
    /// Parse one.
    ///
    /// # Errors
    /// An empty id, one over 128 characters, or one holding anything but
    /// letters, digits, `.`, `_` and `-`.
    pub fn new(text: impl Into<String>) -> Result<Self, NotAReference> {
        let text = text.into();
        let ok = matches!(text.len(), 1..=128)
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-');
        if ok {
            Ok(Self(text))
        } else {
            Err(NotAReference::shape(
                "thread",
                format!("a thread id is 1-128 of A-Z a-z 0-9 . _ -, got {text:?}"),
            ))
        }
    }

    /// The id, as notmuch spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A triage batch's name from before the agent filed cards: the head object of its replica.
///
/// Eight is the floor, not [`MailHash`]'s six: a short prefix of a young store collides sooner
/// than a message corpus's does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BatchHead(String);

impl BatchHead {
    /// Parse one.
    ///
    /// # Errors
    /// Anything that is not eight to sixty-four lowercase hex characters.
    pub fn new(text: impl Into<String>) -> Result<Self, NotAReference> {
        let text = text.into();
        let ok = matches!(text.len(), 8..=64)
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if ok {
            Ok(Self(text))
        } else {
            Err(NotAReference::shape(
                "batch",
                format!("a batch head is 8-64 lowercase hex, got {text:?}"),
            ))
        }
    }

    /// The head, as everything spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The one spelling of a briefing's day, in a URL and a file name alike.
const DAY: &str = "%Y-%m-%d";

/// A todo, on the chain that holds it: an id is unique only within one store. Prints
/// `todo:<chain>/<id>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TodoRef {
    /// Which store.
    pub chain: ChainName,
    /// The id in it.
    pub id: TodoId,
}

/// One proposal in one of the secretary's conversations: the card a tap names. Prints
/// `proposal:<conversation>/<n>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProposalRef {
    /// Which conversation.
    pub conversation: ConversationId,
    /// Its index among that conversation's proposals, the number a tap names.
    pub index: usize,
}

impl fmt::Display for TodoRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "todo:{}/{}", self.chain, self.id.as_str())
    }
}

impl fmt::Display for ProposalRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "proposal:{}/{}", self.conversation, self.index)
    }
}

/// A reference of another kind, read where only one kind is meant.
fn other_kind(scheme: &'static str, read: &Reference) -> NotAReference {
    NotAReference::shape(
        scheme,
        format!("a {scheme} reference is meant here, got {read}"),
    )
}

impl FromStr for TodoRef {
    type Err = NotAReference;

    fn from_str(text: &str) -> Result<Self, NotAReference> {
        match text.parse::<Reference>()? {
            Reference::Todo(todo) => Ok(todo),
            read @ (Reference::Mail { .. }
            | Reference::Thread { .. }
            | Reference::Briefing { .. }
            | Reference::Proposal(_)
            | Reference::Batch { .. }
            | Reference::Target(_)) => Err(other_kind("todo", &read)),
        }
    }
}

impl FromStr for ProposalRef {
    type Err = NotAReference;

    fn from_str(text: &str) -> Result<Self, NotAReference> {
        match text.parse::<Reference>()? {
            Reference::Proposal(proposal) => Ok(proposal),
            read @ (Reference::Todo(_)
            | Reference::Mail { .. }
            | Reference::Thread { .. }
            | Reference::Briefing { .. }
            | Reference::Batch { .. }
            | Reference::Target(_)) => Err(other_kind("proposal", &read)),
        }
    }
}

impl From<TodoRef> for Reference {
    fn from(todo: TodoRef) -> Self {
        Reference::Todo(todo)
    }
}

impl From<ProposalRef> for Reference {
    fn from(proposal: ProposalRef) -> Self {
        Reference::Proposal(proposal)
    }
}

impl TryFrom<String> for TodoRef {
    type Error = NotAReference;
    fn try_from(text: String) -> Result<Self, NotAReference> {
        text.parse()
    }
}

impl From<TodoRef> for String {
    fn from(todo: TodoRef) -> String {
        todo.to_string()
    }
}

impl TryFrom<String> for ProposalRef {
    type Error = NotAReference;
    fn try_from(text: String) -> Result<Self, NotAReference> {
        text.parse()
    }
}

impl From<ProposalRef> for String {
    fn from(proposal: ProposalRef) -> String {
        proposal.to_string()
    }
}

/// What a turn, a source or a sentence points at. Closed, because each arm is a destination
/// the app has a page for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Reference {
    /// A todo, on the chain that holds it.
    Todo(TodoRef),
    /// One message in the corpus.
    Mail {
        /// Its canonical hash, whole or as a prefix.
        hash: MailHash,
    },
    /// One conversation in the corpus.
    Thread {
        /// Notmuch's thread id.
        id: ThreadId,
    },
    /// One saved briefing.
    Briefing {
        /// The day it covers.
        date: chrono::NaiveDate,
    },
    /// One proposal in one of the secretary's conversations.
    Proposal(ProposalRef),
    /// A triage run held rather than accepted or rejected.
    Batch {
        /// The chain it is proposed against.
        chain: ChainName,
        /// The run's own head, its only name.
        head: BatchHead,
    },
    /// A round or a note of the secretary's, what a topic is put on.
    Target(Target),
}

impl Reference {
    /// The prefix this one prints with, which the app's tokeniser dispatches on.
    #[must_use]
    pub fn scheme(&self) -> &'static str {
        match self {
            Reference::Todo(_) => "todo",
            Reference::Mail { .. } => "mail",
            Reference::Thread { .. } => "thread",
            Reference::Briefing { .. } => "briefing",
            Reference::Proposal(_) => "proposal",
            Reference::Batch { .. } => "batch",
            Reference::Target(Target::Round { .. }) => "round",
            Reference::Target(Target::Note { .. }) => "note",
        }
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reference::Todo(todo) => write!(f, "{todo}"),
            Reference::Mail { hash } => write!(f, "mail:{}", hash.as_str()),
            Reference::Thread { id } => write!(f, "thread:{}", id.as_str()),
            Reference::Briefing { date } => write!(f, "briefing:{}", date.format(DAY)),
            Reference::Proposal(proposal) => write!(f, "{proposal}"),
            Reference::Batch { chain, head } => write!(f, "batch:{chain}/{}", head.as_str()),
            Reference::Target(target) => write!(f, "{target}"),
        }
    }
}

impl FromStr for Reference {
    type Err = NotAReference;

    fn from_str(text: &str) -> Result<Self, NotAReference> {
        let Some((scheme, rest)) = text.split_once(':') else {
            return Err(NotAReference::Unknown(text.to_owned()));
        };
        match scheme {
            "todo" => {
                let (chain, id) = rest.split_once('/').ok_or_else(|| {
                    NotAReference::shape(
                        "todo",
                        "a todo reference names its chain: todo:<chain>/<id>",
                    )
                })?;
                Ok(Reference::Todo(TodoRef {
                    chain: ChainName::new(chain)
                        .map_err(|failed| NotAReference::shape("todo", failed.to_string()))?,
                    id: TodoId::new(id)
                        .map_err(|failed| NotAReference::shape("todo", failed.to_string()))?,
                }))
            }
            "mail" => Ok(Reference::Mail {
                hash: MailHash::new(rest)?,
            }),
            "thread" => Ok(Reference::Thread {
                id: ThreadId::new(rest)?,
            }),
            "briefing" => {
                let wrong = || {
                    NotAReference::shape("briefing", format!("a day is YYYY-MM-DD, got {rest:?}"))
                };
                let date = chrono::NaiveDate::parse_from_str(rest, DAY).map_err(|_| wrong())?;
                // One spelling per day: chrono also reads `2026-9-10`, which would print
                // back differently, so the canonical print must be what was read.
                if date.format(DAY).to_string() != rest {
                    return Err(wrong());
                }
                Ok(Reference::Briefing { date })
            }
            "proposal" => {
                let (conversation, index) = rest.split_once('/').ok_or_else(|| {
                    NotAReference::shape(
                        "proposal",
                        "a proposal reference names its conversation: proposal:<conversation>/<n>",
                    )
                })?;
                Ok(Reference::Proposal(ProposalRef {
                    conversation: ConversationId::new(conversation)
                        .map_err(|failed| NotAReference::shape("proposal", failed.to_string()))?,
                    index: index.parse::<usize>().map_err(|_| {
                        NotAReference::shape(
                            "proposal",
                            format!("an index is a number, got {index:?}"),
                        )
                    })?,
                }))
            }
            "batch" => {
                let (chain, head) = rest.split_once('/').ok_or_else(|| {
                    NotAReference::shape(
                        "batch",
                        "a batch reference names its chain: batch:<chain>/<head>",
                    )
                })?;
                Ok(Reference::Batch {
                    chain: ChainName::new(chain)
                        .map_err(|failed| NotAReference::shape("batch", failed.to_string()))?,
                    head: BatchHead::new(head)?,
                })
            }
            "round" => target(text, "round"),
            "note" => target(text, "note"),
            _ => Err(NotAReference::Unknown(text.to_owned())),
        }
    }
}

/// A round or a note, read by [`Target`]'s own parser.
fn target(text: &str, scheme: &'static str) -> Result<Reference, NotAReference> {
    text.parse().map(Reference::Target).map_err(|_| {
        NotAReference::shape(
            scheme,
            format!(
                "a {scheme} reference is {scheme}:<conversation>/<turn>, the turn a number with \
                 no leading zero, got {text:?}"
            ),
        )
    })
}

prodrome_core::text_literal!(
    Reference => |reference| reference.to_string(),
    TodoRef => |todo| todo.to_string(),
    ProposalRef => |proposal| proposal.to_string(),
);

impl TryFrom<String> for Reference {
    type Error = NotAReference;
    fn try_from(text: String) -> Result<Self, NotAReference> {
        text.parse()
    }
}

impl From<Reference> for String {
    fn from(reference: Reference) -> String {
        reference.to_string()
    }
}

/// The eight spellings as one scan, mirrored by `tokenise` in `web/src/chat/refs.ts`.
///
/// Group 1 is the boundary, so `notodo:x` is a word and not a link (a group, since `regex` has
/// no lookbehind). Group 2 only finds candidates; `FromStr` still decides what is real.
static MENTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(^|[^A-Za-z0-9_-])(",
        r"todo:[a-z][a-z0-9-]*/[a-z0-9_-]+",
        r"|mail:[0-9a-f]{6,64}",
        r"|thread:[A-Za-z0-9_-](?:[A-Za-z0-9._-]{0,126}[A-Za-z0-9_-])?",
        r"|briefing:\d{4}-\d{2}-\d{2}",
        r"|proposal:[a-z0-9][a-z0-9-]*/\d+",
        r"|batch:[a-z][a-z0-9-]*/[0-9a-f]{8,64}",
        r"|(?:round|note):[a-z0-9][a-z0-9-]*/[0-9]+",
        r")",
    ))
    .expect("a literal pattern")
});

/// Every reference named in a turn's prose, in order. A candidate [`FromStr`] refuses is
/// dropped: erring toward "not a link" is the safe direction.
#[must_use]
pub fn mentions(text: &str) -> Vec<Reference> {
    MENTION
        .captures_iter(text)
        .filter_map(|caps| caps.get(2))
        .filter_map(|found| found.as_str().parse::<Reference>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn chain() -> impl Strategy<Value = ChainName> {
        "[a-z][a-z0-9-]{0,31}".prop_map(|text| ChainName::new(text).expect("a slug"))
    }

    fn todo_id() -> impl Strategy<Value = TodoId> {
        "[a-z0-9_-]{1,40}".prop_map(|text| TodoId::new(text).expect("an id"))
    }

    fn conversation() -> impl Strategy<Value = ConversationId> {
        "[a-z0-9][a-z0-9-]{0,30}".prop_map(|text| ConversationId::new(text).expect("a slug"))
    }

    fn reference() -> impl Strategy<Value = Reference> {
        prop_oneof![
            (chain(), todo_id()).prop_map(|(chain, id)| Reference::Todo(TodoRef { chain, id })),
            "[0-9a-f]{6,64}".prop_map(|hash| Reference::Mail {
                hash: MailHash::new(hash).expect("hex"),
            }),
            "[A-Za-z0-9._-]{1,64}".prop_map(|id| Reference::Thread {
                id: ThreadId::new(id).expect("an id"),
            }),
            (2000i32..2100, 1u32..=12, 1u32..=28).prop_map(|(y, m, d)| Reference::Briefing {
                date: chrono::NaiveDate::from_ymd_opt(y, m, d).expect("a day"),
            }),
            (conversation(), 0usize..1000).prop_map(|(conversation, index)| {
                Reference::Proposal(ProposalRef {
                    conversation,
                    index,
                })
            }),
            (chain(), "[0-9a-f]{8,64}").prop_map(|(chain, head)| Reference::Batch {
                chain,
                head: BatchHead::new(head).expect("hex"),
            }),
            (conversation(), 0usize..100_000, any::<bool>()).prop_map(
                |(conversation, turn, round)| {
                    Reference::Target(if round {
                        Target::Round { conversation, turn }
                    } else {
                        Target::Note { conversation, turn }
                    })
                }
            ),
        ]
    }

    proptest! {
        /// The text form is the protocol with the model: whatever this prints reads back as
        /// the same value.
        #[test]
        fn a_reference_reads_back_as_what_it_printed(reference in reference()) {
            let printed = reference.to_string();
            prop_assert_eq!(printed.parse::<Reference>().expect("it parses"), reference);
        }

        /// A stored line holds the same text form, nothing nested.
        #[test]
        fn serde_stores_the_text_form(reference in reference()) {
            let json = serde_json::to_string(&reference).expect("it serialises");
            prop_assert_eq!(&json, &format!("{:?}", reference.to_string()));
            let read: Reference = serde_json::from_str(&json).expect("it reads back");
            prop_assert_eq!(read, reference);
        }
    }

    /// A field that names one kind of reference reads only that kind: a todo is no proposal, a
    /// proposal no todo, and each prints what it read.
    #[test]
    fn a_typed_reference_reads_only_its_own_kind() {
        let card: ProposalRef = "proposal:default/3".parse().expect("a proposal");
        assert_eq!(card.index, 3);
        assert_eq!(card.to_string(), "proposal:default/3");
        assert!("todo:life/reply".parse::<ProposalRef>().is_err());
        let todo: TodoRef = "todo:life/reply".parse().expect("a todo");
        assert_eq!(todo.to_string(), "todo:life/reply");
        assert!("proposal:default/3".parse::<TodoRef>().is_err());
        assert!("mail:abcdef012345".parse::<TodoRef>().is_err());
    }

    /// Every part is parsed, so a reference cannot carry a path, an empty segment or a name in
    /// the wrong vocabulary.
    #[test]
    fn a_string_that_is_not_a_reference_is_refused() {
        for bad in [
            "",
            "todo",
            "todo:",
            "todo:life",
            "todo:life/",
            "todo:/x",
            "todo:../x",
            "todo:life/../etc",
            "todo:Life/x",
            "mail:",
            "mail:abc",
            "mail:ABCDEF",
            "mail:zzzzzz",
            "thread:",
            "thread:a/b",
            "thread:a b",
            "briefing:",
            "briefing:2026-9-10",
            "briefing:tomorrow",
            "proposal:2026-09-09-1",
            "proposal:2026-09-09-1/x",
            "proposal:../etc/0",
            "batch:",
            "batch:life",
            "batch:life/",
            "batch:life/abcdef1",
            "batch:Life/abcdef1234",
            "batch:../etc/abcdef1234",
            "note:hello",
            "note:default/",
            "round:default/07",
            "round:Default/1",
            "round:../etc/1",
            "https://example.test",
        ] {
            assert!(bad.parse::<Reference>().is_err(), "{bad:?} must be refused");
        }
    }

    /// The seven spellings `SECRETARY.md` shows the model, and a triage batch's, which held
    /// decisions on the chain still name, so the manual, the tokeniser and this grammar cannot
    /// drift apart.
    #[test]
    fn the_text_forms_are_the_ones_the_manual_names() {
        for text in [
            "todo:life/book-flight",
            "mail:a1b2c3d4e5f6",
            "thread:0000000000001b2c",
            "briefing:2026-09-10",
            "proposal:2026-09-09-1/1",
            "batch:life/2fdf1691",
            "round:default/12",
            "note:default/14",
        ] {
            let reference: Reference = text.parse().expect(text);
            assert_eq!(reference.to_string(), text);
        }
    }

    /// A mail hash may be the six-character prefix a todo's source carries; a batch head's
    /// floor is eight.
    #[test]
    fn a_hash_has_its_floor_and_its_ceiling() {
        for (text, expected) in [
            ("abc123".to_owned(), true),
            ("abc12".to_owned(), false),
            ("a".repeat(64), true),
            ("a".repeat(65), false),
        ] {
            assert_eq!(MailHash::new(text.clone()).is_ok(), expected, "{text}");
        }
        for (text, expected) in [
            ("2fdf1691".to_owned(), true),
            ("2fdf169".to_owned(), false),
            ("a".repeat(64), true),
            ("a".repeat(65), false),
        ] {
            assert_eq!(BatchHead::new(text.clone()).is_ok(), expected, "{text}");
        }
    }

    /// The eight spellings, found in a sentence; `web/test/refs.test.ts` pins the same prose.
    #[test]
    fn mentions_finds_the_eight_spellings_in_prose() {
        let text = "Checked todo:life/book-flight and mail:a1b2c3d4e5f6, then \
                     thread:0000000000001b2c and briefing:2026-09-10 stand on \
                     proposal:2026-09-09-1/1, which the held batch:life/2fdf1691 \
                     also touches, as round:default/12 said and note:default/14 kept.";
        let found = mentions(text);
        assert_eq!(
            found,
            vec![
                "todo:life/book-flight".parse().expect("a reference"),
                "mail:a1b2c3d4e5f6".parse().expect("a reference"),
                "thread:0000000000001b2c".parse().expect("a reference"),
                "briefing:2026-09-10".parse().expect("a reference"),
                "proposal:2026-09-09-1/1".parse().expect("a reference"),
                "batch:life/2fdf1691".parse().expect("a reference"),
                "round:default/12".parse().expect("a reference"),
                "note:default/14".parse().expect("a reference"),
            ]
        );
    }

    /// A word that merely ends in a scheme is not a reference, and plain prose holds none.
    #[test]
    fn mentions_respects_the_word_boundary() {
        for text in [
            "xmail:a1b2c3d4e5f6",
            "notodo:life/x",
            "xbatch:life/2fdf1691",
            "around:default/1",
            "The deck is done and it was never yours.",
        ] {
            assert_eq!(mentions(text), Vec::new(), "{text}");
        }
    }
}
