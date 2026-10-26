//! What the compactor is asked: the log as it stands and one node's children, each rendered as
//! one line of text with no id and no object name, and every answer it gave that did not fit.
//!
//! A call renders as its verb's name and never its arguments ([`digest_view::rendered`]), so a
//! remember card's words, which a `remember` call carries and every later line names only by a
//! salted name, never reach a compactor. The log, while over its budget, keeps its earliest
//! parts and says how many later ones it left out, so its first lines, the prefix a model's cache
//! keeps, stay as they were. It hides the words of every tainted part, so a clean node's summary is
//! written from clean words alone, and for a node of nothing but the owner's words it hides every
//! other part too, so that summary is written from the owner's words alone: either way its label,
//! the join of its lines', is true of what the compactor read.
//!
//! How a node is asked is a versioned rule, [`Asking`], and every summary records the version it
//! was asked under, so a change to the asking finds every summary asked the old way.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::LazyLock;

use prodrome_core::digest::Measure as _;
use prodrome_core::event::Hash;

use crate::rule_version::{Version, Versioned};
use crate::secretary::digest::Digest;
use crate::secretary::digest_label::{Kind, Kinds, Label};
use crate::secretary::digest_line::{Line, NODE_BYTES};
use crate::secretary::digest_view::{self, Body, Keep, LINE_BYTES, VIEW_BYTES, rendered};
use crate::secretary::store::Entry;
use crate::secretary::taint::Taint;

/// The compactor's rules, the file the code sends: `app/core/src/secretary/compact.md`.
pub const PROMPT: &str = include_str!("compact.md");

/// How a node is asked for its summary, as a rule whose version each summary records, one
/// version for each [`Beside`]: the prompt, the question's text, what the log shows a node of
/// that class, and the sizes that cut them. A change to what one class is shown leaves the other
/// class's summaries current.
#[derive(Debug)]
pub struct Asking;

static OWNERS_WORDS: LazyLock<Version<Asking>> =
    LazyLock::new(|| Version::of_parts(Beside::OwnersWords));
static CLEAN_WORDS: LazyLock<Version<Asking>> =
    LazyLock::new(|| Version::of_parts(Beside::CleanWords));

impl Versioned for Asking {
    type Case = Beside;

    const WHAT: &'static str = "the compactor's asking";

    /// Raise it when the code that builds a question changes in a way the probe does not show
    /// (which children a span holds, or which parts a log keeps).
    const REVISION: u32 = 1;

    fn parts(beside: Beside) -> Vec<String> {
        // What the rule does on a fixed probe, not a list of what it reads: the question's text
        // stands for its template, and the probe's log as this class is shown it stands for
        // which parts it reads, how each line and summary renders and the note a cut log ends in.
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

/// The bytes the probe's log is cut at, so its note is part of the version.
const PROBE_CUT: usize = 160;

/// Parts of every shape a log shows, each labelled as the digest labels it: a line of each kind
/// of turn, clean and tainted, a summary, and a node not summarised yet.
fn probe() -> Vec<(Label, Body)> {
    use crate::capability::{ToolCall, Turn};
    use crate::secretary::outcome::Outcome;
    let at = || crate::capability::Datetime::new(2026, 10, 25, 9, 0, 0, 0).expect("an instant");
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
        title: crate::secretary::words::Words::kept("a title".to_owned()),
        model: "m".to_owned(),
        kind: crate::secretary::store::Kind::Topic,
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
    /// The owner's words alone: a node of nothing but the owner's words, so its summary,
    /// labelled as nothing but theirs, is written from nothing but theirs.
    OwnersWords,
    /// Every clean part's words.
    CleanWords,
}

impl Beside {
    /// What a node labelled `label` is asked beside.
    #[must_use]
    pub fn of(label: &Label) -> Self {
        if label.is_owners() {
            Beside::OwnersWords
        } else {
            Beside::CleanWords
        }
    }

    /// The parts it shows in words.
    fn reads(self) -> fn(&Label) -> bool {
        match self {
            Beside::OwnersWords => Label::is_owners,
            Beside::CleanWords => Label::is_clean,
        }
    }
}

/// One question to the compactor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    log: Vec<String>,
    span: Vec<String>,
    tries: Vec<Try>,
}

/// An answer that did not fit, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Try {
    /// What was said, its spaces joined.
    pub answer: String,
    /// Why it is no summary.
    pub unfit: Unfit,
}

/// Why an answer is no summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unfit {
    /// Longer than a node holds: its length, and the answer cut at the limit.
    Long {
        /// Its bytes.
        bytes: usize,
        /// Its first [`NODE_BYTES`] bytes, at a character's edge.
        cut: String,
    },
    /// No line of words: nothing, or what the redaction door refuses.
    NotALine,
}

impl Ask {
    /// The log `log` and the span `span`, each a line per element, nothing tried yet.
    #[must_use]
    pub fn new(log: Vec<String>, span: Vec<String>) -> Self {
        Self {
            log,
            span,
            tries: Vec::new(),
        }
    }

    /// The span's lines, in order.
    #[must_use]
    pub fn span(&self) -> &[String] {
        &self.span
    }

    /// The answers already given that did not fit, in order.
    #[must_use]
    pub fn tries(&self) -> &[Try] {
        &self.tries
    }

    /// The same question, with `answer` tried and found `unfit`.
    #[must_use]
    pub fn tried(mut self, answer: String, unfit: Unfit) -> Self {
        self.tries.push(Try { answer, unfit });
        self
    }

    /// What the compactor reads after [`PROMPT`], as plain text with no speaker labelled: the
    /// log and the span, then each answer that did not fit and why. The log comes first and
    /// changes least between questions, so it is the prefix a model's cache can keep.
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

    /// The question for `node` of `digest`, whose lines are `lines`, and what it is written
    /// from: each child's line, or the object of the summary of it that is shown. `None` for a
    /// node the tree does not hold or a child with no summary to show.
    #[must_use]
    pub fn of(
        digest: &Digest,
        entries: &std::collections::BTreeMap<&Hash, &crate::secretary::store::Entry>,
        node: &Hash,
    ) -> Option<(Self, Vec<Hash>)> {
        let asked = digest.tree()?.node(node)?;
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

/// The view under [`VIEW_BYTES`] as [`digest_view::view`] shows it to a reader that `reads`, its
/// earliest parts kept while the rendering is over the budget.
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
