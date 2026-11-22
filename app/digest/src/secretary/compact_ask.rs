//! What the compactor is asked: the log as it stands and one node's children, and every answer
//! it gave that did not fit.
//!
//! The log hides the words of every tainted part, and for a node of nothing but the owner's words
//! every other part too, so a node's label is true of what the compactor read.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::LazyLock;

use prodrome_core::digest::Measure as _;
use prodrome_core::event::Hash;

use crate::secretary::digest::Digest;
use crate::secretary::digest_label::{Kind, Kinds, Label};
use crate::secretary::digest_line::{Line, NODE_BYTES};
use crate::secretary::digest_view::{self, Body, Keep, LINE_BYTES, VIEW_BYTES, rendered};
use crate::secretary::shown_record::{CleanWords, OwnersWords, Shows as _};
use suzatary_base::rule_version::{Version, Versioned};
use suzatary_conversations::secretary::store::Entry;
use suzatary_conversations::secretary::taint::Taint;

pub const PROMPT: &str = include_str!("compact.md");

/// How a node is asked for its summary, as a rule whose version each summary records.
#[derive(Debug)]
pub struct Asking;

static OWNERS_WORDS: LazyLock<Version<Asking>> =
    LazyLock::new(|| Version::of_parts(Beside::OwnersWords));
static CLEAN_WORDS: LazyLock<Version<Asking>> =
    LazyLock::new(|| Version::of_parts(Beside::CleanWords));

impl Versioned for Asking {
    type Case = Beside;

    const WHAT: &'static str = "the compactor's asking";

    /// Raise it when the code that builds a question changes in a way the probe does not show,
    /// or when the redaction rules' name, which every line's handle carries, moves the pinned
    /// questions.
    const REVISION: u32 = 3;

    fn parts(beside: Beside) -> Vec<String> {
        // What the rule does on a fixed probe, not a list of what it reads.
        let question = Ask::new(vec!["log".to_owned()], vec!["span".to_owned()])
            .tried(
                "answer".to_owned(),
                Unfit::Long {
                    bytes: 1,
                    cut: "cut".to_owned(),
                },
            )
            .tried("answer".to_owned(), Unfit::NotALine);
        let log: Vec<String> = probe()
            .into_iter()
            .filter_map(|(label, words)| {
                digest_view::shown_as(label, beside.reads(), || Some(words))
            })
            .map(|body| body.text(2))
            .collect();
        let cut = digest_view::fit(log.clone(), PROBE_CUT, Keep::Earliest);
        [PROMPT.to_owned(), question.text()]
            .into_iter()
            .chain(log)
            .chain(cut)
            .chain([VIEW_BYTES, NODE_BYTES, LINE_BYTES].map(|bytes| bytes.to_string()))
            .collect()
    }

    fn current(beside: Beside) -> &'static Version<Self> {
        match beside {
            Beside::OwnersWords => &OWNERS_WORDS,
            Beside::CleanWords => &CLEAN_WORDS,
        }
    }
}

/// The bytes the probe's log is cut at.
const PROBE_CUT: usize = 160;

/// Parts of every shape a log shows, each labelled as the digest labels it.
fn probe() -> Vec<(Label, Body)> {
    use suzatary_conversations::secretary::model::{ToolCall, Turn};
    use suzatary_conversations::secretary::outcome::Outcome;
    let at =
        || suzatary_base::capability::Datetime::new(2026, 10, 25, 9, 0, 0, 0).expect("an instant");
    let tool = |outcome| Turn::Tool {
        verb: "weather".into(),
        answer: "a verb's answer".to_owned(),
        outcome,
        files: Vec::new(),
    };
    let turns = [
        Turn::Person {
            text: "the owner's turn".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        Turn::Model {
            text: "the secretary's answer".to_owned(),
            calls: vec![ToolCall {
                verb: "remember".into(),
                args: BTreeMap::new(),
            }],
        },
        tool(Outcome::Answered),
        tool(Outcome::Refused),
    ];
    let lines = turns.into_iter().map(|turn| Entry::Said {
        at: at(),
        turn,
        by: None,
    });
    let opened = Entry::Opened {
        at: at(),
        title: suzatary_memory::secretary::words::Words::kept("a title".to_owned()),
        model: "m".to_owned(),
        kind: suzatary_conversations::secretary::store::Kind::Topic,
    };
    let mut parts = Vec::new();
    for taint in [Taint::Clean, Taint::Tainted] {
        for entry in lines.clone().chain([opened.clone()]) {
            parts.push((Label::of(&entry, taint), Body::Said(rendered(&entry))));
        }
        let owners = Label {
            taint,
            kinds: Kinds::one(Kind::Person),
        };
        let mixed = owners.join(&Label {
            taint,
            kinds: Kinds::one(Kind::Model),
        });
        for label in [owners, mixed] {
            let line = Line::new("a summary").expect("a line");
            parts.push((label, Body::Derived(line)));
            parts.push((label, Body::Unsummarised(label)));
        }
    }
    parts
}

/// Which parts of the log a node is asked beside, by its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Beside {
    /// The owner's words alone, so a summary labelled as nothing but theirs is written from
    /// nothing but theirs.
    OwnersWords,
    CleanWords,
}

impl Beside {
    #[must_use]
    pub fn of(label: &Label) -> Self {
        if label.is_owners() {
            Beside::OwnersWords
        } else {
            Beside::CleanWords
        }
    }

    fn reads(self) -> fn(&Label) -> bool {
        match self {
            Beside::OwnersWords => OwnersWords::shows,
            Beside::CleanWords => CleanWords::shows,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    log: Vec<String>,
    span: Vec<String>,
    tries: Vec<Try>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Try {
    /// What was said, its spaces joined.
    pub answer: String,
    pub unfit: Unfit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unfit {
    Long {
        bytes: usize,
        /// Its first `NODE_BYTES` bytes, at a character's edge.
        cut: String,
    },
    /// No line of words: nothing, or what the redaction door refuses.
    NotALine,
}

impl Ask {
    #[must_use]
    pub fn new(log: Vec<String>, span: Vec<String>) -> Self {
        Self {
            log,
            span,
            tries: Vec::new(),
        }
    }

    #[must_use]
    pub fn span(&self) -> &[String] {
        &self.span
    }

    #[must_use]
    pub fn tries(&self) -> &[Try] {
        &self.tries
    }

    #[must_use]
    pub fn tried(mut self, answer: String, unfit: Unfit) -> Self {
        self.tries.push(Try { answer, unfit });
        self
    }

    /// What the compactor reads after [`PROMPT`]. The log comes first and changes least, so it is
    /// the prefix a model's cache can keep.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = format!(
            "<log>\n{}\n</log>\n\n<span>\n{}\n</span>\n",
            self.log.join("\n"),
            self.span.join("\n")
        );
        for tried in &self.tries {
            let why = match &tried.unfit {
                Unfit::Long { bytes, cut } => format!(
                    "That line is {bytes} bytes, and a line holds at most {NODE_BYTES}. Cut at \
                     the limit, it reads:\n<cut>\n{cut}\n</cut>"
                ),
                Unfit::NotALine => "That is not one line of words.".to_owned(),
            };
            let _ = write!(
                text,
                "\nAn earlier answer was:\n<answer>\n{}\n</answer>\n{why}\nSay the whole span \
                 again in one line of at most {NODE_BYTES} bytes.\n",
                tried.answer
            );
        }
        text
    }

    /// What sending it costs: the bytes of [`PROMPT`] and of [`Ask::text`].
    #[must_use]
    pub fn bytes(&self) -> u64 {
        u64::try_from(PROMPT.len() + self.text().len()).unwrap_or(u64::MAX)
    }

    /// The question for `node` of `digest`, and what it is written from. `None` for a node the
    /// tree does not hold or a child with no summary to show.
    #[must_use]
    pub fn of(
        digest: &Digest,
        entries: &std::collections::BTreeMap<
            &Hash,
            &suzatary_conversations::secretary::store::Entry,
        >,
        node: &Hash,
    ) -> Option<(Self, Vec<Hash>)> {
        let asked = digest.node(node)?;
        let over = digest.over(node)?;
        let span = asked
            .children()
            .iter()
            .map(|child| {
                Some(if child.is_leaf() {
                    rendered(entries.get(digest.line(child.name()))?)
                } else {
                    digest.summary(child.name())?.as_str().to_owned()
                })
            })
            .collect::<Option<_>>()?;
        let reads = Beside::of(asked.measure()).reads();
        Some((Self::new(log(digest, entries, reads), span), over))
    }
}

/// The view under [`VIEW_BYTES`] shown to a reader that `reads`, its earliest parts kept.
fn log(
    digest: &Digest,
    entries: &BTreeMap<&Hash, &Entry>,
    reads: fn(&Label) -> bool,
) -> Vec<String> {
    let shown = digest_view::view(digest, entries, VIEW_BYTES, reads)
        .into_iter()
        .map(|shown| shown.body.text(shown.part.span.len()))
        .collect();
    digest_view::fit(shown, VIEW_BYTES, Keep::Earliest)
}
