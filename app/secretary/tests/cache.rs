//! THE CACHE LAW: consecutive turns of one conversation, asked as the box asks them, against a
//! fake provider with the prompt cache's rules and a fake of each CLI a deployment may run.
//!
//! The provider (platform.claude.com/docs/en/build-with-claude/prompt-caching, read 2026-11-24):
//! at most 4 marks a request; a mark writes the prefix through its block, and reads the longest
//! earlier write among the 20 positions ending at it; an entry lives an hour from its last use.
//! The CLIs (captured requests): 2.1.321 puts the project file first and its fourth mark on a
//! trailing block of its own; 2.1.331 puts its environment first and its fourth mark on the
//! message's last block; each marks its system text twice.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use suzatary_base::conversation_id::ConversationId;
use suzatary_base::topic_target::Target;
use suzatary_conversations::secretary::model::{Conversation, Earlier, ToolCall, Turn};
use suzatary_conversations::secretary::outcome::Outcome;
use suzatary_conversations::secretary::store::Entry;
use suzatary_digest::secretary::memories::Memories;
use suzatary_io_model::blocks::Blocks;
use suzatary_io_model::model::stdin_for;
use suzatary_io_model::prompt::{Reads, prompt_of};
use suzatary_lines::nothing::Counting;
use suzatary_lines::secretary::run::Runs as _;
use suzatary_lines::secretary::transcript::{self, Retry};
use suzatary_memory::secretary::memory::{Fact, Kind, Origin};
use suzatary_secretary::conversation::ask::Ask;
use suzatary_secretary::secretary::live_shape::{self, Shape, Stores, Summaries};
use suzatary_store::store::Prodrome as _;
use suzatary_stores::stores::InMemory;
use suzatary_topics::secretary::topics::stores::TopicStores;
use suzatary_topics::secretary::topics::topic::Topic;

const LOOK_BACK: usize = 20;

const LIFETIME: usize = 3_600;

const MOST_MARKS: usize = 4;

/// Which CLI the call goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cli {
    Deployed,
    Newer,
}

/// What a block of a request is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    System,
    /// What the CLI sends of its own: a project file, the environment.
    Cli,
    Facts,
    Record,
    Head,
    Turn,
    After,
    Tail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Ours,
    /// One of the CLI's: the system text's (which stands for its two there), and its last.
    Cli,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    text: String,
    part: Part,
    mark: Mark,
}

/// The prompt cache, by the prefix each entry was written for, with when it was last used.
#[derive(Default)]
struct Provider {
    entries: BTreeMap<Vec<u8>, usize>,
    last: Vec<Block>,
    /// The first turn of the window the call before was sent.
    window: Option<usize>,
}

/// One call's input, in bytes: read from the cache, written to it and neither, and the bytes not
/// read by why.
#[derive(Debug, Default, Clone)]
struct Served {
    read: usize,
    written: usize,
    fresh: usize,
    unread: BTreeMap<Cause, usize>,
}

/// Why a block was not read from the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Cause {
    First,
    /// A fact was remembered, so the facts and all after them are new.
    Facts,
    /// The record changed: a summary came, a part closed, a line of any conversation landed.
    Record,
    /// The window's start stepped: its oldest rounds left it for the record.
    Window,
    /// Turns said since the call before: the conversation's own growth.
    New,
    /// The call before's entry stood more than the look-back before this call's mark.
    LookBack,
    /// After the call's mark: the topics, the tail, what the CLI sends last.
    After,
}

impl Provider {
    /// `blocks` served at `now`, a call whose window starts at turn `window`.
    fn serve(&mut self, blocks: &[Block], now: usize, window: Option<usize>) -> Served {
        let marks: Vec<usize> = (0..blocks.len())
            .filter(|at| blocks[*at].mark != Mark::None)
            .collect();
        let ours = blocks
            .iter()
            .filter(|block| block.mark == Mark::Ours)
            .count();
        // The system text's one mark here stands for the CLI's two there.
        assert!(ours <= 1, "{ours} marks of the call's");
        assert!(
            marks.len() < MOST_MARKS,
            "{} marks: the provider refuses the request",
            marks.len() + 1
        );
        let prefix = |through: usize| -> Vec<u8> {
            blocks[..=through]
                .iter()
                .flat_map(|block| block.text.bytes().chain([0]))
                .collect()
        };
        let bytes = |through: usize| -> usize {
            blocks[..=through]
                .iter()
                .map(|block| block.text.len())
                .sum()
        };
        let live = |entries: &BTreeMap<Vec<u8>, usize>, key: &Vec<u8>| {
            entries.get(key).is_some_and(|used| now - used <= LIFETIME)
        };
        let read = marks
            .iter()
            .filter_map(|&mark| {
                (mark.saturating_sub(LOOK_BACK - 1)..=mark)
                    .rev()
                    .find(|&at| live(&self.entries, &prefix(at)))
            })
            .max();
        if let Some(at) = read {
            self.entries.insert(prefix(at), now);
        }
        for &mark in &marks {
            self.entries.insert(prefix(mark), now);
        }
        let unread = self.causes(blocks, read, window != self.window);
        self.last = blocks.to_vec();
        self.window = window;
        let marked = marks.last().map_or(0, |&mark| bytes(mark));
        let read = read.map_or(0, bytes);
        let marked = marked.max(read);
        Served {
            read,
            written: marked - read,
            fresh: bytes(blocks.len() - 1) - marked,
            unread,
        }
    }

    /// The bytes after `read` (the last position read), by why they were not read: a step of
    /// the window is told by its first turn, since the rounds it lets go reach the record in the
    /// same call.
    fn causes(
        &self,
        blocks: &[Block],
        read: Option<usize>,
        stepped: bool,
    ) -> BTreeMap<Cause, usize> {
        let ours = blocks.iter().position(|block| block.mark == Mark::Ours);
        let turns = |blocks: &[Block]| -> Vec<String> {
            blocks
                .iter()
                .filter(|block| block.part == Part::Turn)
                .map(|block| block.text.clone())
                .collect()
        };
        let (was, is) = (turns(&self.last), turns(blocks));
        let grew = is.starts_with(&was);
        let moved = (0..blocks.len())
            .find(|at| self.last.get(*at) != Some(&blocks[*at]))
            .map(|at| blocks[at].part);
        let mut unread = BTreeMap::new();
        let mut turn = blocks[..read.map_or(0, |at| at + 1)]
            .iter()
            .filter(|block| block.part == Part::Turn)
            .count();
        for (at, block) in blocks.iter().enumerate().skip(read.map_or(0, |at| at + 1)) {
            let cause = if self.last.is_empty() {
                Cause::First
            } else if ours.is_none_or(|ours| at > ours) {
                Cause::After
            } else {
                match moved {
                    Some(Part::Facts) => Cause::Facts,
                    Some(_) if stepped => Cause::Window,
                    Some(Part::Record | Part::System | Part::Cli | Part::Head) => Cause::Record,
                    Some(Part::Turn) if !grew => Cause::Window,
                    Some(Part::Turn | Part::After | Part::Tail) | None
                        if block.part == Part::Turn && grew && turn >= was.len() =>
                    {
                        Cause::New
                    }
                    Some(Part::Turn | Part::After | Part::Tail) | None => Cause::LookBack,
                }
            };
            if block.part == Part::Turn {
                turn += 1;
            }
            *unread.entry(cause).or_insert(0) += block.text.len();
        }
        unread
    }
}

fn asked(ask: &Ask<'_>, stores: &Stores, id: &ConversationId) -> Conversation {
    let read = transcript::read(stores.lines.conversations(), id).expect("the conversation reads");
    ask.call("You are the secretary.", &read)
}

/// `sent` as the provider reads it through `cli`.
fn blocks(sent: &Conversation, cli: Cli) -> Vec<Block> {
    let prompt = prompt_of(
        sent,
        &suzatary_secretary::secretary::surface::tools(),
        Reads::Files,
    );
    let stdin = stdin_for(&prompt, &Blocks::default());
    let message: serde_json::Value =
        serde_json::from_str(stdin.payload.trim()).expect("one JSON message");
    let content = message["message"]["content"].as_array().expect("blocks");
    let turns = prompt.earlier.len() + prompt.round.len() + usize::from(prompt.newest.is_some());
    let parts = prompt
        .told
        .iter()
        .map(|told| {
            if told.as_str().contains("what you remember about the owner") {
                Part::Facts
            } else {
                Part::Record
            }
        })
        .chain([Part::Head])
        .chain(std::iter::repeat_n(Part::Turn, turns))
        .chain(std::iter::repeat_n(Part::After, prompt.after.len()))
        .chain([Part::Tail]);
    let ours = content.iter().zip(parts).map(|(block, part)| Block {
        text: block["text"].as_str().expect("text").to_owned(),
        part,
        mark: if block.get("cache_control").is_some() {
            Mark::Ours
        } else {
            Mark::None
        },
    });
    let system = Block {
        text: prompt.system.as_str().to_owned(),
        part: Part::System,
        mark: Mark::Cli,
    };
    let cli_block = |text: String, mark| Block {
        text,
        part: Part::Cli,
        mark,
    };
    let environment = "The environment: the working directory, the model, the day.".to_owned();
    match cli {
        Cli::Deployed => std::iter::once(system)
            .chain([cli_block("p".repeat(36_000), Mark::None)])
            .chain(ours)
            .chain([cli_block(environment, Mark::Cli)])
            .collect(),
        Cli::Newer => {
            let mut blocks: Vec<Block> = std::iter::once(system)
                .chain([cli_block(environment, Mark::None)])
                .chain(ours)
                .collect();
            if let Some(last) = blocks.last_mut() {
                last.mark = Mark::Cli;
            }
            blocks
        }
    }
}

/// What happens in a run beside the asked conversation's own questions and answers.
#[derive(Debug, Clone, Copy)]
struct Life {
    /// Every this many turns, a round of five steps, a verb answering at each.
    steps: Option<usize>,
    remembers: Option<usize>,
    /// Another conversation writes a question and an answer each turn: plain lines, with no
    /// claim, verb or note, so the record moves at its end as a box's does at the least.
    others: bool,
    topics: bool,
}

const QUIET: Life = Life {
    steps: None,
    remembers: None,
    others: false,
    topics: false,
};

const LIVELY: Life = Life {
    steps: Some(3),
    remembers: Some(25),
    others: true,
    topics: true,
};

/// One call: its turn, its step in that turn's round, how it was served, and its bytes.
struct Call {
    turn: usize,
    step: usize,
    served: Served,
    size: usize,
}

struct Running {
    stores: Stores,
    cards: InMemory<suzatary_inbox::secretary::inbox_event::InboxEvent>,
    memberships: InMemory<suzatary_topics::secretary::topics::reading::MembershipEvent>,
    aliases: InMemory<suzatary_topics::secretary::topics::reading::AliasEvent>,
    entropy: Counting,
    id: ConversationId,
}

impl Running {
    fn of(shape: Shape) -> Self {
        Self {
            stores: shape.build(),
            cards: InMemory::default(),
            memberships: InMemory::default(),
            aliases: InMemory::default(),
            entropy: Counting::default(),
            id: ConversationId::new(if shape.conversations > 1 {
                "c-1"
            } else {
                "c-0"
            })
            .expect("a slug"),
        }
    }

    fn memories(&self) -> Memories<'_> {
        Memories {
            history: &self.stores.facts,
            texts: &self.stores.words,
            cards: &self.cards,
            entropy: &self.entropy,
            digests: self.stores.digests(),
        }
    }

    fn topics(&self) -> TopicStores<'_> {
        TopicStores {
            memberships: &self.memberships,
            aliases: &self.aliases,
        }
    }

    fn say(&self, id: &ConversationId, second: usize, turn: Turn, by: Option<&str>) {
        let next = transcript::read(self.stores.lines.conversations(), id)
            .map_or(0, |read| read.lines().len());
        self.stores.append(
            id,
            next,
            &Entry::Said {
                at: live_shape::at(second),
                turn,
                by: by.map(Into::into),
            },
        );
    }

    /// The round a box's run claims before it asks, held until it drops.
    fn claim(&self, second: usize) -> suzatary_lines::secretary::run::Held {
        let held = self.stores.lines.runs.begin().expect("a run");
        let claimed = transcript::claim(
            self.stores.lines.conversations(),
            &self.id,
            &held.run,
            live_shape::at(second),
            Retry::Allowed,
            &|_| true,
            |_| Vec::new(),
        )
        .expect("claims");
        assert_eq!(claimed, transcript::Claimed::Began, "the question is owed");
        held
    }

    fn run(&self, turns: usize, pace: usize, life: Life, cli: Cli) -> Vec<Call> {
        let memories = self.memories();
        let topics = self.topics();
        let ask = Ask::Digest {
            model: &suzatary_base::capability::Nothing,
            memories: &memories,
            topics: &topics,
        };
        let other = ConversationId::new("c-2").expect("a slug");
        let mut second = 20_000_000;
        let mut provider = Provider::default();
        let mut served = Vec::new();
        let mut serve = |turn: usize, step: usize, second: usize| {
            let blocks = blocks(&asked(&ask, &self.stores, &self.id), cli);
            let size = blocks.iter().map(|block| block.text.len()).sum();
            let read = transcript::read(self.stores.lines.conversations(), &self.id)
                .expect("the conversation reads");
            let window =
                suzatary_conversations::secretary::queue::window(read.stored(), &read.queued())
                    .first();
            served.push(Call {
                turn,
                step,
                served: provider.serve(&blocks, second, window),
                size,
            });
        };
        for turn in 0..turns {
            second += 120;
            if life.others {
                let asked = format!("Elsewhere, question {turn}.");
                self.say(&other, tick(&mut second), person(&asked), Some("ada"));
                let answered = format!("Elsewhere, answer {turn}. ");
                self.say(&other, tick(&mut second), answer(&answered, 200), None);
            }
            let question = format!("Turn {turn}: what is left to do on the permit?");
            self.say(&self.id, tick(&mut second), person(&question), Some("ada"));
            let _held = self.claim(tick(&mut second));
            let steps = if life.steps.is_some_and(|every| turn % every == 1) {
                5
            } else {
                0
            };
            for step in 0..steps {
                serve(turn, step, second);
                self.say(&self.id, tick(&mut second), calling("weather"), None);
                let forecast = format!("{{\"forecast\": \"rain at {turn}, step {step}\"}}");
                self.say(
                    &self.id,
                    tick(&mut second),
                    answered("weather", forecast),
                    None,
                );
            }
            serve(turn, steps, second);
            let answered = format!("Answer {turn}: the form, then the fee. ");
            self.say(&self.id, tick(&mut second), answer(&answered, 600), None);
            if life.topics {
                self.topic(turn, second);
            }
            if life
                .remembers
                .is_some_and(|every| turn % every == every - 1)
            {
                memories
                    .remember(
                        &Fact::new(format!("The permit's step {turn} is done")).expect("a fact"),
                        Kind::Owner,
                        &Origin::Turn {
                            conversation: self.id.clone(),
                            turn: 0,
                        },
                        live_shape::at(second),
                    )
                    .expect("remembers");
            }
            self.stores.summarise(pace, second);
        }
        served
    }

    fn topic(&self, turn: usize, second: usize) {
        let read = transcript::read(self.stores.lines.conversations(), &self.id)
            .expect("the conversation reads");
        let opening = read
            .stored()
            .turns
            .iter()
            .rposition(|said| matches!(said.turn, Turn::Person { .. }))
            .expect("a question");
        let target = Target::Round {
            conversation: self.id.clone(),
            turn: opening,
        };
        let topic = Topic::new(if turn.is_multiple_of(2) {
            "permit"
        } else {
            "fees"
        })
        .expect("a topic");
        let reading = self.topics().reading().expect("the topics read");
        if let Some(event) = reading.suggestion(&target, &topic, live_shape::at(second)) {
            self.memberships.append(event).expect("appended");
        }
    }
}

fn person(text: &str) -> Turn {
    Turn::Person {
        text: text.to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
    }
}

fn answer(sentence: &str, bytes: usize) -> Turn {
    Turn::Model {
        text: sentence.repeat(bytes.div_ceil(sentence.len())),
        calls: Vec::new(),
    }
}

fn calling(verb: &str) -> Turn {
    Turn::Model {
        text: String::new(),
        calls: vec![ToolCall {
            verb: verb.into(),
            args: BTreeMap::new(),
        }],
    }
}

fn answered(verb: &str, answer: String) -> Turn {
    Turn::Tool {
        verb: verb.into(),
        answer,
        outcome: Outcome::Answered,
        files: Vec::new(),
    }
}

/// The share of `calls`' input read from the cache, in percent.
fn share<'c>(calls: impl IntoIterator<Item = &'c Call>) -> usize {
    let (read, all) = calls.into_iter().fold((0, 0), |(read, all), call| {
        let one = &call.served;
        (read + one.read, all + one.read + one.written + one.fresh)
    });
    read * 100 / all.max(1)
}

/// What was not read, by cause, in tokens and as a share of the input.
fn causes(calls: &[Call]) -> String {
    let mut causes: BTreeMap<Cause, usize> = BTreeMap::new();
    let mut all = 0;
    for call in calls {
        let one = &call.served;
        all += one.read + one.written + one.fresh;
        for (cause, bytes) in &one.unread {
            *causes.entry(*cause).or_insert(0) += bytes;
        }
    }
    let mut out = String::new();
    for (cause, bytes) in causes {
        let _ = write!(
            out,
            "  {cause:?} {} ({}%)",
            bytes / 4,
            bytes * 100 / all.max(1)
        );
    }
    out
}

/// After the first call; within a round (a round's later steps); across rounds (a round's first).
fn shares(calls: &[Call]) -> (usize, usize, usize) {
    let after = &calls[1..];
    (
        share(after),
        share(after.iter().filter(|call| call.step > 0)),
        share(after.iter().filter(|call| call.step == 0)),
    )
}

fn quiet_shape() -> Shape {
    Shape {
        lines: 1_000,
        conversations: 1,
        answer_bytes: 600,
        summaries: Summaries::Every,
    }
}

fn lively_shape() -> Shape {
    Shape {
        lines: 2_400,
        conversations: 12,
        answer_bytes: 600,
        summaries: Summaries::Every,
    }
}

/// One conversation of 1,000 lines, nothing else moving: 60 turns through each CLI.
#[test]
fn consecutive_turns_read_most_of_their_input_from_the_cache() {
    for (cli, floor) in [(Cli::Deployed, QUIET_DEPLOYED), (Cli::Newer, QUIET_NEWER)] {
        let calls = Running::of(quiet_shape()).run(60, 4, QUIET, cli);
        let (read, _, _) = shares(&calls);
        eprintln!("{cli:?}: {read}% after the first call;{}", causes(&calls));
        assert!(
            read >= floor,
            "{cli:?}: {read}% of the input read from the cache"
        );
    }
}

/// The floors of the quiet run, by CLI: each reads 96%.
const QUIET_DEPLOYED: usize = 90;
const QUIET_NEWER: usize = 90;

/// The floors of the lively run, by CLI, over every call and within a round: the deployed CLI
/// reads 76% and 98%, the newer 80% and 98%.
const LIVELY_DEPLOYED: (usize, usize) = (70, 95);
const LIVELY_NEWER: (usize, usize) = (75, 95);

/// 2,400 lines over 12 conversations, a round of five verb steps every third turn, a remember
/// every 25th, another conversation writing and a topic each round.
#[test]
fn a_lively_store_still_reads_from_the_cache() {
    for (cli, floor) in [(Cli::Deployed, LIVELY_DEPLOYED), (Cli::Newer, LIVELY_NEWER)] {
        let calls = Running::of(lively_shape()).run(60, 4, LIVELY, cli);
        let (read, within, across) = shares(&calls);
        eprintln!(
            "{cli:?}: {read}% after the first call, {within}% within a round, {across}% \
             across rounds;{}",
            causes(&calls)
        );
        assert!(read >= floor.0, "{cli:?}: {read}% read");
        assert!(within >= floor.1, "{cli:?}: {within}% read within a round");
    }
}

/// The measures the pull request quotes, printed.
#[test]
#[ignore = "a measurement, printed: cargo test --release -p suzatary-secretary --test all cache -- --ignored --nocapture"]
fn the_cache_share_and_the_calls_size() {
    for (name, shape, life) in [
        ("one conversation, quiet", quiet_shape(), QUIET),
        ("12 conversations, lively", lively_shape(), LIVELY),
        (
            "13,000 lines over 100 conversations, lively",
            Shape {
                lines: 13_000,
                conversations: 100,
                answer_bytes: 3_000,
                summaries: Summaries::Every,
            },
            LIVELY,
        ),
    ] {
        for cli in [Cli::Deployed, Cli::Newer] {
            let running = Running::of(shape);
            if cli == Cli::Deployed {
                eprintln!("{name}: {}", shown_summaries(&running.stores));
            }
            let calls = running.run(120, 4, life, cli);
            let sizes: Vec<usize> = calls.iter().map(|call| call.size).collect();
            let at_100 = calls
                .iter()
                .find(|call| call.turn == 100)
                .map_or(0, |call| call.size);
            let (read, within, across) = shares(&calls);
            eprintln!(
                "{name}, {cli:?}: {read}% read after the first call, {within}% within a \
                 round, {across}% across rounds;{}\n  call size in bytes: first {}, at turn \
                 100 {at_100}, largest {}",
                causes(&calls),
                sizes[0],
                sizes.iter().max().copied().unwrap_or(0)
            );
        }
    }
    for every in [5, 25, 100] {
        let life = Life {
            remembers: Some(every),
            ..QUIET
        };
        let calls = Running::of(quiet_shape()).run(100, 4, life, Cli::Deployed);
        eprintln!(
            "a remember every {every} turns: {}% read;{}",
            share(&calls[1..]),
            causes(&calls)
        );
    }
}

/// Of the record's summarised nodes, how many are the owner's alone, how many more the call
/// reads in words (the owner's and the secretary's clean answers), and how many by their kinds.
fn shown_summaries(stores: &Stores) -> String {
    use suzatary_digest::secretary::shown_record::{OwnersAndAnswers, OwnersWords, Shows as _};
    let reading = suzatary_digest::secretary::compaction::Reading::of(&stores.digests().reads())
        .expect("the record reads");
    let (mut owners, mut answers, mut hidden) = (0, 0, 0);
    let mut stack: Vec<_> = reading
        .digest()
        .tree()
        .map(prodrome_digest::Tree::root)
        .into_iter()
        .collect();
    while let Some(node) = stack.pop() {
        stack.extend(node.children());
        if node.is_leaf() || reading.digest().summary(node.name()).is_none() {
            continue;
        }
        let label = node.measure();
        match (OwnersWords::shows(label), OwnersAndAnswers::shows(label)) {
            (true, _) => owners += 1,
            (false, true) => answers += 1,
            (false, false) => hidden += 1,
        }
    }
    format!(
        "summaries: {owners} the owner's alone, {answers} more of the secretary's answers, \
         {hidden} by kinds"
    )
}

/// What a call holds of something said one round back and past the window, by who said it.
#[test]
#[ignore = "a measurement, printed: cargo test --release -p suzatary-secretary --test all cache -- --ignored --nocapture"]
fn what_a_call_holds_of_what_was_said_long_ago() {
    // Two conversations, so the one asked in (`c-1`) is answered round by round.
    let running = Running::of(Shape {
        lines: 400,
        conversations: 2,
        answer_bytes: 600,
        summaries: Summaries::Every,
    });
    let id = running.id.clone();
    let mut second = 30_000_000;
    let round = |second: usize, question: Turn, verbs: &[(&str, String)], said: Turn| {
        running.say(&id, second, question, Some("ada"));
        for (verb, text) in verbs {
            running.say(&id, second, calling(verb), None);
            running.say(&id, second, answered(verb, text.clone()), None);
        }
        running.say(&id, second, said, None);
    };
    second += 60;
    round(
        second,
        person("OWNERSAID the gate code is the old one"),
        &[
            (
                "weather",
                "{\"forecast\": \"CLEANVERBSAID rain\"}".to_owned(),
            ),
            (
                "show",
                "{\"mail\": \"MAILVERBSAID the fee is due\"}".to_owned(),
            ),
        ],
        answer("SECRETARYSAID the inspector comes on the second day. ", 60),
    );
    let needles = [
        ("the owner", "OWNERSAID"),
        ("the secretary", "SECRETARYSAID"),
        ("a clean verb", "CLEANVERBSAID"),
        ("a verb reading mail", "MAILVERBSAID"),
    ];
    for (rounds, label) in [(0, "one round back"), (40, "40 rounds back")] {
        for k in 0..rounds {
            second += 60;
            round(
                second,
                person(&format!("Later question {k}")),
                &[],
                answer(&format!("Later answer {k}. "), 600),
            );
        }
        second += 60;
        running.say(
            &id,
            second,
            person("What did we say back then?"),
            Some("ada"),
        );
        let _held = running.claim(second);
        summarise_keeping_words(&running.stores, second);
        let memories = running.memories();
        let topics = running.topics();
        let ask = Ask::Digest {
            model: &suzatary_base::capability::Nothing,
            memories: &memories,
            topics: &topics,
        };
        let call = asked(&ask, &running.stores, &id);
        for (who, word) in needles {
            eprintln!("{label}, said by {who}: {}", holds(&call, word));
        }
        second += 60;
        running.say(&id, second, answer("Back then: see above. ", 60), None);
    }
}

/// Every node owed a summary summarised in words it keeps: the lines beneath it, as `zoom`
/// shows them.
fn summarise_keeping_words(stores: &Stores, second: usize) {
    let digests = stores.digests();
    stores.summarise_with(usize::MAX, second, &|node| {
        let reading = suzatary_digest::secretary::compaction::Reading::of(&digests.reads())
            .expect("the record reads");
        let span = reading.digest().node(node).expect("in the tree").span();
        let opened = suzatary_digest::secretary::told::zoom(
            &reading,
            &format!("{}+{}", span.start, span.len()),
            |_| true,
        )
        .expect("opens");
        let words: String = opened["beneath"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|part| part.as_object()?.values().find_map(|value| value.as_str()))
            .filter(|text| !text.contains('+'))
            .collect::<Vec<_>>()
            .join(" / ")
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        let words = if words.trim().is_empty() {
            "nothing in words".to_owned()
        } else {
            words
        };
        words[..words.floor_char_boundary(400)].to_owned()
    });
}

/// Where `call` holds `word`: as itself, in words in the record, or only behind a handle.
fn holds(call: &Conversation, word: &str) -> &'static str {
    let window = serde_json::to_string(
        &call
            .earlier
            .iter()
            .filter_map(Earlier::turn)
            .chain(&call.turns)
            .collect::<Vec<_>>(),
    )
    .expect("turns print");
    let record = call.context.cached.join("\n") + &call.context.rest.join("\n");
    if window.contains(word) {
        "as itself"
    } else if record.contains(word) {
        "in words in the record"
    } else {
        "only behind a handle"
    }
}

/// The next instant of the run's clock: every line a box writes takes the clock's own instant, so
/// no two of a run's lines are written at one, and the log's order of them is their order.
fn tick(second: &mut usize) -> usize {
    *second += 1;
    *second
}
