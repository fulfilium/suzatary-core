//! A synthetic memory of the live shape, for the tests and benches that measure reading it:
//! lines over conversations, one in four of the owner's notes alone, and the digest summarised.
//! Its shape is its size, not a box's order: each conversation is one block in time, with no
//! claims, verbs, verbs' answers, acts or notes, and nothing tainted, where a box interleaves
//! conversations and writes all of those. Every figure measured over it is of that shape.

use std::collections::BTreeSet;

use prodrome_data::event::Actor;
use prodrome_data::literal::Datetime;

use suzatary_base::capability::Entropy as _;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Entry, Kind, Said};
use suzatary_conversations::secretary::taint::{self, Taint};
use suzatary_digest::model_name::ModelName;
use suzatary_digest::secretary::compact_ask::Beside;
use suzatary_digest::secretary::compacting::Compacting;
use suzatary_digest::secretary::digest_event::{self, Attempt, DigestEvent};
use suzatary_digest::secretary::digest_line::Line;
use suzatary_digest::secretary::digests::{Digests, compactor};
use suzatary_digest::secretary::memories::ReadMemories;
use suzatary_digest::secretary::recollection::Recollection;
use suzatary_lines::nothing::Counting;
use suzatary_lines::secretary::answer::{Answer, BoxLine};
use suzatary_lines::secretary::line_event::{LineEvent, NotALine};
use suzatary_lines::secretary::spoken::{Spoken, try_map_turn};
use suzatary_memory::secretary::memory::Fact;
use suzatary_memory::secretary::memory_event::MemoryEvent;
use suzatary_memory::secretary::salted_text::{Text, TextName};
use suzatary_memory::secretary::words::Words;
use suzatary_store::store::{Doc as _, InMemoryDoc, Prodrome as _};
use suzatary_stores::stores::InMemory;
use suzatary_stores::stores::conversations::InMemory as Lines;

#[derive(Debug, Clone, Copy)]
pub struct Shape {
    /// How many lines, shared evenly over the conversations.
    pub lines: usize,
    pub conversations: usize,
    /// How many bytes each of the secretary's answers holds; every other line is one of them in
    /// three conversations of four.
    pub answer_bytes: usize,
    pub summaries: Summaries,
}

/// How much of the digest's tree is summarised: what a compactor that runs at a budget's pace,
/// and finds some nodes unfit, has written so far.
#[derive(Debug, Clone, Copy)]
pub enum Summaries {
    Every,
    /// The first this many nodes the compactor takes: the smallest first, level by level.
    First(usize),
}

#[derive(Default)]
pub struct Stores {
    pub lines: Lines,
    /// Whose retraction the digest believes.
    pub owners: BTreeSet<Actor>,
    pub digests: InMemory<DigestEvent>,
    pub summaries: InMemoryDoc<TextName, Text<Line>>,
    pub facts: InMemory<MemoryEvent>,
    pub words: InMemoryDoc<TextName, Text<Fact>>,
    /// What each conversation's turns so far carry, joined: what a model's turn appended next
    /// carries, every turn before it, as the round's join is at its widest.
    pub held: std::sync::Mutex<std::collections::BTreeMap<ConversationId, Taint>>,
}

impl Stores {
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
#[must_use]
pub fn at(second: usize) -> Datetime {
    let start = prodrome_data::instant::instant_of(
        Datetime::new(2026, 1, 1, 0, 0, 0, 0).expect("a constant instant"),
    );
    let offset = chrono::Duration::seconds(i64::try_from(second).expect("a year of seconds"));
    prodrome_data::instant::datetime_of(start + offset).expect("an instant in 2026")
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

    /// The memory of this shape: its lines, then the digest's tree summarised as
    /// [`Shape::summaries`] says, level by level, each summary after the call that asked for it.
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
                stores.append(&id, held, &self.entry(held, second, conversation % 4 != 0));
            }
        }
        stores.summarise(
            match self.summaries {
                Summaries::Every => usize::MAX,
                Summaries::First(most) => most,
            },
            second + 1,
        );
        stores
    }
}

/// `entry` as the box writes it: a model's turn carrying `carries`, and any other turn as it is;
/// a turn whose words are gone has none to write.
fn lined(entry: &Entry, carries: Taint) -> Result<BoxLine, NotALine> {
    let spoken = |turn: Turn| match turn {
        Turn::Person {
            text,
            references,
            attachments,
        } => Ok(Spoken::person(text, references, attachments)),
        Turn::Tool {
            verb,
            answer,
            outcome,
            files,
        } => Ok(Spoken::tool(verb, answer, outcome, files)),
        Turn::Act { text, references } => Ok(Spoken::act(text, references)),
        Turn::Model { .. } | Turn::Erased { .. } => Err(turn),
    };
    match entry.clone() {
        Entry::Said {
            at,
            turn: Turn::Model { text, calls },
            ..
        } => Ok(BoxLine::Answer(Answer::replied(at, text, calls, carries))),
        other => try_map_turn(other, spoken)
            .map(BoxLine::Plain)
            .map_err(|_| NotALine::Gone),
    }
}

impl Stores {
    /// `entry` as line `held` of conversation `id`, written as the box writes it: under its own
    /// actor, so the owner's stamp stands with no claim to judge it (a tab's line no claim judged
    /// reads unconfirmed, and so tainted).
    /// A model's turn carries every turn before it in its conversation.
    pub fn append(&self, id: &ConversationId, held: usize, entry: &Entry) {
        let carries = {
            let mut joined = self.held.lock().expect("not poisoned");
            let before = joined.get(id).copied().unwrap_or_default();
            let own = Said::of(entry).map_or(Taint::Clean, |said| match said.turn {
                Turn::Model { .. } => before,
                _ => taint::written(&said),
            });
            joined.insert(id.clone(), before.join(own));
            before
        };
        let (writer, held) = (
            suzatary_lines::secretary::transcript::writer(),
            u32::try_from(held).expect("few lines"),
        );
        let (line, keys) = match lined(entry, carries).expect("a line") {
            BoxLine::Answer(answer) => {
                LineEvent::answered(id.clone(), writer, held, &answer, &self.lines.entropy)
            }
            BoxLine::Plain(entry) => {
                LineEvent::write(id.clone(), writer, held, &entry, &self.lines.entropy)
            }
        }
        .expect("a line");
        for key in keys {
            self.lines.keys.set(&key.name(), &key).expect("kept");
        }
        self.lines.lines.append(line).expect("said");
    }

    /// Up to `most` of the nodes owed summarised at `second`, smallest first and level by level,
    /// each after the call that asked for it: what a compactor running behind has written.
    pub fn summarise(&self, most: usize, second: usize) {
        self.summarise_saying(most, second, "The owner asked about the permit");
    }

    /// [`Stores::summarise`], each summary `words` and its node's name.
    pub fn summarise_saying(&self, most: usize, second: usize, words: &str) {
        self.summarise_with(most, second, &|node| format!("{words}, node {node}."));
    }

    /// [`Stores::summarise`], each summary the words `words` gives its node.
    pub fn summarise_with(
        &self,
        most: usize,
        second: usize,
        words: &dyn Fn(&prodrome_data::event::Hash) -> String,
    ) {
        let salts = Counting::default();
        let mut left = most;
        loop {
            let read = self.digests().reads().read().expect("the digest reads");
            let pending = read.pending();
            if pending.is_empty() || left == 0 {
                return;
            }
            let taken = pending.len().min(left);
            left -= taken;
            for node in pending.into_iter().take(taken) {
                let label = *read.node(&node).expect("in the tree").measure();
                let text = Text::new(
                    Line::new(&words(&node)).expect("a line"),
                    salts.salt().expect("a salt"),
                );
                self.summaries.set(&text.name(), &text).expect("kept");
                let asked = digest_event::asked(node.clone(), at(second), compactor(), 4_096)
                    .expect("a call");
                self.digests.append(asked).expect("appended");
                let summary = digest_event::summarized(
                    node.clone(),
                    at(second),
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
                self.digests.append(summary).expect("appended");
            }
        }
    }
}

struct Library;

impl suzatary_base::capability::Manuals for Library {
    fn document(&self, _: suzatary_base::capability::Manual) -> Option<String> {
        Some("The operator's manual.".to_owned())
    }
}

/// The agent's `context` answer over `memory`, as its verb makes it, under a manual of one line
/// and a deployment of one chain.
#[must_use]
pub fn context(memory: &Recollection<'_>) -> serde_json::Value {
    use suzatary_base::chains::{ChainName, ChainPolicy, Chains};
    let deployment = suzatary_connect::deployment::Deployment {
        viewers: BTreeSet::new(),
        actors: std::collections::BTreeMap::new(),
        chains: Chains::of(vec![ChainPolicy {
            name: ChainName::new("life").expect("a constant slug"),
            untrusted: prodrome_data::policy::Untrusted::of([]),
            roster: BTreeSet::new(),
            root: suzatary_connect::cast::keys::root(),
            naming: suzatary_base::chains::Naming::AsActs,
        }])
        .expect("one chain"),
        categories: std::collections::BTreeMap::new(),
        app_url: "https://example.invalid".to_owned(),
        coding: None,
        upkeep: None,
        zone: suzatary_base::zone::Zone::UTC,
    };
    crate::agent::context(
        &Library,
        &deployment,
        memory.facts().as_ref(),
        memory.record().as_ref(),
        memory.budget(),
    )
}
