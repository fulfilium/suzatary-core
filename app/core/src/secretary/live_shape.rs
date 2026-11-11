//! A synthetic memory of the live shape, for the tests and benches that measure reading it: lines
//! over conversations, one in four of the owner's notes alone, and every node of the digest's
//! tree summarised, each summary after the call that asked for it. Built in memory; a bench that
//! measures the disk stores hands the objects over to them.

use std::collections::BTreeSet;

use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;

use crate::capability::Entropy as _;
use crate::conversation_id::ConversationId;
use crate::model_name::ModelName;
use crate::secretary::compact_ask::Beside;
use crate::secretary::compacting::Compacting;
use crate::secretary::conversations::{Counting, InMemory as Lines};
use crate::secretary::digest_event::{self, Attempt, DigestEvent};
use crate::secretary::digest_line::Line;
use crate::secretary::digests::{Digests, compactor};
use crate::secretary::line_event::LineEvent;
use crate::secretary::memories::ReadMemories;
use crate::secretary::memory::Fact;
use crate::secretary::memory_event::MemoryEvent;
use crate::secretary::model::Turn;
use crate::secretary::recollection::Recollection;
use crate::secretary::salted_text::{Text, TextName};
use crate::secretary::store::{Entry, Kind};
use crate::secretary::words::Words;
use crate::store::{Doc as _, InMemory, InMemoryDoc, Prodrome as _};

/// How large a memory to build.
#[derive(Debug, Clone, Copy)]
pub struct Shape {
    /// How many lines, shared evenly over the conversations.
    pub lines: usize,
    /// How many conversations.
    pub conversations: usize,
    /// How many bytes each of the secretary's answers holds; every other line is one of them in
    /// three conversations of four.
    pub answer_bytes: usize,
    /// How much of the digest's tree the compactor has summarised.
    pub summaries: Summaries,
}

/// How much of the digest's tree is summarised: what a compactor that runs at a budget's pace,
/// and finds some nodes unfit, has written so far.
#[derive(Debug, Clone, Copy)]
pub enum Summaries {
    /// Every node.
    Every,
    /// The first this many nodes the compactor takes: the smallest first, level by level.
    First(usize),
}

/// The conversations, the digest and the facts, in memory.
#[derive(Default)]
pub struct Stores {
    /// The conversations.
    pub lines: Lines,
    /// Whose retraction the digest believes.
    pub owners: BTreeSet<Actor>,
    /// The digest's history.
    pub digests: InMemory<DigestEvent>,
    /// The summaries' words.
    pub summaries: InMemoryDoc<TextName, Text<Line>>,
    /// The facts' history.
    pub facts: InMemory<MemoryEvent>,
    /// The facts' words.
    pub words: InMemoryDoc<TextName, Text<Fact>>,
}

impl Stores {
    /// The digest's stores.
    #[must_use]
    pub fn digests(&self) -> Digests<'_> {
        Digests {
            history: &self.digests,
            texts: &self.summaries,
            lines: self.lines.conversations(),
            owners: &self.owners,
            compacting: &Compacting::Off,
        }
    }

    /// The memory as the agent's record holds it, carried within `budget` bytes.
    #[must_use]
    pub fn recollection(&self, budget: usize) -> Recollection<'_> {
        Recollection::of(
            ReadMemories {
                history: &self.facts,
                texts: &self.words,
                digests: self.digests().reads(),
            },
            budget,
        )
    }
}

fn ada() -> Actor {
    Actor::new("ada").expect("a constant actor name")
}

/// The `second`th second of the store's year.
fn at(second: usize) -> Datetime {
    let start = prodrome_core::fpl::instant_of(
        Datetime::new(2026, 1, 1, 0, 0, 0, 0).expect("a constant instant"),
    );
    let offset = chrono::Duration::seconds(i64::try_from(second).expect("a year of seconds"));
    prodrome_core::fpl::datetime_of(start + offset).expect("an instant in 2026")
}

impl Shape {
    /// Line `line` of a conversation: its opening, then the owner's question and, in a
    /// conversation the secretary answers, its answer, alternating.
    fn entry(&self, line: usize, second: usize, answered: bool) -> Entry {
        let person = |text: String| Entry::Said {
            at: at(second),
            turn: Turn::Person {
                text,
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        };
        match (line, answered && line.is_multiple_of(2)) {
            (0, _) => Entry::Opened {
                at: at(second),
                title: Words::kept(format!("Conversation at second {second}")),
                model: "a-cli/small".to_owned(),
                kind: Kind::Topic,
            },
            (_, false) => person(format!("Question {line} about the permit, asked again.")),
            (_, true) => {
                let sentence = format!("Answer {line} about the permit. ");
                Entry::Said {
                    at: at(second),
                    turn: Turn::Model {
                        text: sentence.repeat(self.answer_bytes.div_ceil(sentence.len())),
                        calls: Vec::new(),
                    },
                    by: None,
                }
            }
        }
    }

    /// The memory of this shape: its lines, then the digest's tree summarised as [`Shape::summaries`]
    /// says, level by level, each summary after the call that asked for it.
    ///
    /// # Panics
    /// Never for a shape of at least one conversation: every write is one the stores take.
    #[must_use]
    pub fn build(&self) -> Stores {
        let stores = Stores {
            owners: [ada()].into_iter().collect(),
            ..Stores::default()
        };
        let mut second = 0;
        for conversation in 0..self.conversations {
            let id = ConversationId::new(format!("c-{conversation}")).expect("a slug");
            for held in 0..self.lines / self.conversations {
                second += 1;
                let (line, keys) = LineEvent::write(
                    id.clone(),
                    ada(),
                    u32::try_from(held).expect("few lines"),
                    &self.entry(held, second, conversation % 4 != 0),
                    &stores.lines.entropy,
                )
                .expect("a line");
                for key in keys {
                    stores.lines.keys.set(&key.name(), &key).expect("kept");
                }
                stores.lines.lines.append(line).expect("said");
            }
        }
        let salts = Counting::default();
        let mut left = match self.summaries {
            Summaries::Every => usize::MAX,
            Summaries::First(most) => most,
        };
        loop {
            let read = stores.digests().reads().read().expect("the digest reads");
            let pending = read.pending();
            if pending.is_empty() || left == 0 {
                return stores;
            }
            let taken = pending.len().min(left);
            left -= taken;
            for node in pending.into_iter().take(taken) {
                let label = *read.node(&node).expect("in the tree").measure();
                let text = Text::new(
                    Line::new(&format!("The owner asked about the permit, node {node}."))
                        .expect("a line"),
                    salts.salt().expect("a salt"),
                );
                stores.summaries.set(&text.name(), &text).expect("kept");
                let asked = digest_event::asked(node.clone(), at(second + 1), compactor(), 4_096)
                    .expect("a call");
                stores.digests.append(asked).expect("appended");
                let summary = digest_event::summarized(
                    node.clone(),
                    at(second + 1),
                    compactor(),
                    &text,
                    Attempt {
                        over: read.over(&node).expect("its children read"),
                        model: ModelName::parse("a-model").expect("a model"),
                        taint: label.taint,
                        beside: Beside::of(&label),
                    },
                )
                .expect("a summary");
                stores.digests.append(summary).expect("appended");
            }
        }
    }
}

/// A manual of one line.
struct Library;

impl crate::capability::Manuals for Library {
    fn document(&self, _: crate::capability::Manual) -> Option<String> {
        Some("The operator's manual.".to_owned())
    }
}

/// The agent's `context` answer over `memory`, as its verb makes it, under a manual of one line
/// and a deployment of one chain.
///
/// # Panics
/// Never: the deployment's names are constants.
#[must_use]
pub fn context(memory: &Recollection<'_>) -> serde_json::Value {
    use crate::chains::{ChainName, ChainPolicy, Chains};
    let deployment = crate::deployment::Deployment {
        viewers: BTreeSet::new(),
        actors: std::collections::BTreeMap::new(),
        chains: Chains::of(vec![ChainPolicy {
            name: ChainName::new("life").expect("a constant slug"),
            untrusted: prodrome_core::policy::Untrusted::of([]),
            roster: BTreeSet::new(),
            root: crate::cast::keys::root(),
        }])
        .expect("one chain"),
        categories: std::collections::BTreeMap::new(),
        app_url: "https://example.invalid".to_owned(),
        coding: None,
        zone: crate::zone::Zone::UTC,
    };
    crate::agent::context(
        &Library,
        &deployment,
        memory.facts().as_ref(),
        memory.record().as_ref(),
        memory.budget(),
    )
}
