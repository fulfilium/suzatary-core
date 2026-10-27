//! The four kinds of case, as a closed sum: the input as it stood, the incumbent's answer,
//! and the owner's settled truth where one exists.
//!
//! A case carries a message's canonical hash; the case file holds the message itself beside the
//! cases ([`CaseFile::mail`]), and a replay indexes it in a scratch maildir.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use suzatary_core::secretary::store::{Decision, Proposal};

use crate::frozen::FrozenMessage;

// --- identity ----------------------------------------------------------------

/// A case's name, stable across extractions: runs and scores join back to a case by it
/// alone, so a changing name would silently orphan every earlier run's answers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CaseId(String);

/// A case id that is not one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a case id is 1-160 characters of a-z, 0-9, ':', '-' and '_': {0:?}")]
pub struct NotACaseId(pub String);

impl CaseId {
    /// Parse one; the only constructor.
    ///
    /// # Errors
    /// [`NotACaseId`] when the text is empty, too long, or holds a byte
    /// outside `a-z0-9:-_`.
    pub fn new(text: impl Into<String>) -> Result<Self, NotACaseId> {
        let text = text.into();
        let ok = matches!(text.len(), 1..=160)
            && text.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b':' | b'-' | b'_')
            });
        if ok {
            Ok(Self(text))
        } else {
            Err(NotACaseId(text))
        }
    }

    /// The id, as it sorts and as it joins a run's answers back to this case.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CaseId {
    type Error = NotACaseId;
    fn try_from(text: String) -> Result<Self, NotACaseId> {
        Self::new(text)
    }
}

impl From<CaseId> for String {
    fn from(id: CaseId) -> String {
        id.0
    }
}

impl fmt::Display for CaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// --- the two grades of truth --------------------------------------------------

/// The two grades of truth over one case's answer: matching `incumbent` proves only
/// imitation; `settled` is the owner's, rare, and truth wherever the two disagree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Truths<A> {
    /// The incumbent's answer; always present, since a case is built from something this
    /// box already did, even declining.
    pub incumbent: A,
    /// The owner's settled judgement, when recorded; `None` is the common case (most mail
    /// is never revisited), not a gap in extraction.
    pub settled: Option<A>,
}

// --- where a case is anchored in the chain ------------------------------------

/// How precisely [`ChainSnapshot`] pins "as it stood".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotBasis {
    /// Exact: the anchoring object's parent, in causal order, never by clock.
    Anchored,
    /// Approximate: nothing anchors the case (usually a declined message), so this is the
    /// tip at extraction time, not a claim about when the decision was made.
    Extraction,
}

/// The chain as it stood for one case: which chain, and its tip at the anchor.
///
/// A `tip` of `None` means the chain was empty then, not an absent field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainSnapshot {
    /// Which of `instance.toml`'s `[[chains]]`.
    pub chain: String,
    /// The tip's object name, or `None` for an empty chain.
    pub tip: Option<String>,
    /// How precisely `tip` is dated.
    pub basis: SnapshotBasis,
}

// --- triage & adversarial: did a message become a todo? -----------------------

/// What triage did about one message: wrote a todo, or declined. Skip and noise collapse
/// into `Declined`, since the discipline measured is only "did it write, or not".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decided", rename_all = "lowercase")]
pub enum TriageAnswer {
    /// A todo was authored from this message.
    Wrote {
        /// Which chain it landed on.
        chain: String,
        /// The todo's id.
        todo: String,
        /// The object that authored it.
        object: String,
    },
    /// Seen, and deliberately not turned into a todo.
    Declined,
}

/// One triaged message from `state.json`, joined against the chain it may have written to.
/// The input is the hash, never the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriageCase {
    /// This case's stable name.
    pub id: CaseId,
    /// The message's canonical content hash — `state.json`'s key, and what
    /// `Corpus::show` resolves at replay time.
    pub message_hash: String,
    /// The chain as it stood when this message was decided.
    pub chain_as_stood: ChainSnapshot,
    /// What happened, and what the owner later settled, if he ever revisited it.
    pub truths: Truths<TriageAnswer>,
}

/// One real message flagged as carrying instructions, never invented; the same shape as
/// [`TriageCase`] because the question is the same, asked of mail trying to subvert it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdversarialCase {
    /// This case's stable name.
    pub id: CaseId,
    /// The message's canonical content hash.
    pub message_hash: String,
    /// The phrase extraction matched: language worth testing against, never a claim that
    /// the message is an attack.
    pub signal: String,
    /// The chain as it stood when this message was decided.
    pub chain_as_stood: ChainSnapshot,
    /// What happened, and what the owner later settled, if he ever revisited it.
    pub truths: Truths<TriageAnswer>,
}

// --- review: did the secretary settle a batch correctly? ----------------------

/// What the secretary decided about one batch (`Entry::Reviewed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewAnswer {
    /// Accepted or rejected, with the objects settled, or held.
    #[serde(flatten)]
    pub decision: Decision,
    /// The secretary's own reason — `Entry::Reviewed::why`, never empty.
    pub why: String,
}

/// One batch the triage agent proposed, and what the secretary did about it.
///
/// [`Truths::settled`] is always `None` here: the owner overturning a review leaves no
/// record in this box's stores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewCase {
    /// This case's stable name.
    pub id: CaseId,
    /// Which conversation the review happened in.
    pub conversation: String,
    /// Which chain the batch was against.
    pub chain: String,
    /// The batch, by its head.
    pub batch_head: String,
    /// What the secretary decided; `settled` is never recorded.
    pub truths: Truths<ReviewAnswer>,
}

// --- judgement: was a proposal worth tapping? ----------------------------------

/// What the secretary wanted: propose the act, or decline. The same two arms as
/// [`TriageAnswer`], because the discipline measured is "did it act, or hold back".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "lowercase")]
pub enum JudgementAnswer {
    /// Propose this act, for this reason.
    Propose {
        /// The whole act, exactly as it would be shown.
        proposal: Box<Proposal>,
        /// Why it was proposed.
        why: String,
    },
    /// Do not propose anything, and why not.
    Decline {
        /// The owner's dismissal reason; only ever a settled grade, since a proposal the
        /// secretary chose not to make leaves no line to extract.
        why: String,
    },
}

/// One proposal the secretary made, and what became of it.
///
/// The input is the sources, not the draft: the incumbent's draft stays in
/// `truths.incumbent` so a candidate is never handed it as a hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JudgementCase {
    /// This case's stable name.
    pub id: CaseId,
    /// Which conversation the proposal was made in.
    pub conversation: String,
    /// Its index within that conversation's lines — what a tap names.
    pub proposal_index: usize,
    /// What the secretary had read, as text references: the input a replay is handed.
    pub context: Vec<String>,
    /// What the secretary proposed, and, when the owner answered it, whether he took it.
    pub truths: Truths<JudgementAnswer>,
}

// --- the closed sum -------------------------------------------------------------

/// One case, of four kinds; one sum rather than four lists, so scoring walks one `Vec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Case {
    /// A message triage decided about.
    Triage(TriageCase),
    /// A batch the secretary reviewed.
    Review(ReviewCase),
    /// A proposal the secretary made.
    Judgement(JudgementCase),
    /// A message carrying instructions, triaged.
    Adversarial(AdversarialCase),
}

impl Case {
    /// This case's stable name, whichever kind it is.
    #[must_use]
    pub fn id(&self) -> &CaseId {
        match self {
            Case::Triage(case) => &case.id,
            Case::Review(case) => &case.id,
            Case::Judgement(case) => &case.id,
            Case::Adversarial(case) => &case.id,
        }
    }

    /// The canonical hash of the message a triage or adversarial case is about.
    #[must_use]
    pub fn message_hash(&self) -> Option<&str> {
        match self {
            Case::Triage(case) => Some(&case.message_hash),
            Case::Adversarial(case) => Some(&case.message_hash),
            Case::Review(_) | Case::Judgement(_) => None,
        }
    }

    /// The kind's own word, for a count or a filter.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Case::Triage(_) => "triage",
            Case::Review(_) => "review",
            Case::Judgement(_) => "judgement",
            Case::Adversarial(_) => "adversarial",
        }
    }
}

/// The frozen case file: every case, its mail, and when they were pulled.
///
/// Written once: the type cannot enforce it, so `suzatary-dev bench extract` refuses to
/// overwrite an existing file without an explicit flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseFile {
    /// When it was extracted (RFC 3339), for judging how stale the evidence is.
    pub extracted_at: String,
    /// Every case, in extraction order.
    pub cases: Vec<Case>,
    /// The messages the triage and adversarial cases name, by canonical hash; a hash absent here
    /// is a message the box could not find at extraction, and a replay finds it nowhere either.
    pub mail: BTreeMap<String, FrozenMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file a run and a score both read survives its own serialisation.
    #[test]
    fn a_case_file_round_trips_through_json() {
        let file = CaseFile {
            extracted_at: "2026-09-20T00:00:00Z".to_owned(),
            mail: BTreeMap::new(),
            cases: vec![
                Case::Adversarial(AdversarialCase {
                    id: CaseId::new("adversarial:life:def").expect("valid"),
                    message_hash: "def".to_owned(),
                    signal: "\"ignore all previous instructions\"".to_owned(),
                    chain_as_stood: ChainSnapshot {
                        chain: "life".to_owned(),
                        tip: Some("a".repeat(64)),
                        basis: SnapshotBasis::Anchored,
                    },
                    truths: Truths {
                        incumbent: TriageAnswer::Declined,
                        settled: None,
                    },
                }),
                Case::Review(ReviewCase {
                    id: CaseId::new("review:life:beefcafe").expect("valid"),
                    conversation: "2026-09-20-review".to_owned(),
                    chain: "life".to_owned(),
                    batch_head: "beefcafe".to_owned(),
                    truths: Truths {
                        incumbent: ReviewAnswer {
                            decision: Decision::Held,
                            why: "about money".to_owned(),
                        },
                        settled: None,
                    },
                }),
            ],
        };
        let text = serde_json::to_string(&file).expect("serialises");
        let read: CaseFile = serde_json::from_str(&text).expect("parses back");
        assert_eq!(read.cases.len(), file.cases.len());
        assert_eq!(read.cases[0].kind(), "adversarial");
        assert_eq!(read.cases[1].kind(), "review");
    }
}
