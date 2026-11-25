//! The lines' admission rule ([`LinesRule`]), as the box runs it on a tab's post
//! (`sync::receive_changes`): it takes a tab's rename, opening and setup sealed as the box seals
//! them, and refuses, each for good unless said:
//! - a slot whose scrub under the current rules changes its words;
//! - a slot sealed under other rules (outdated, so a newer page seals it again);
//! - a slot that does not open with the key posted beside it;
//! - a rename naming anyone but its poster, the actor the signature proves;
//! - an opening under any conversation but its own id, or of one the store holds;
//! - a rename of a conversation neither held nor begun in the post, so no tab names an id the
//!   box mints first;
//! - a line carried from another store;
//! - a slot sealed under a key a held line names, or another slot of the same post names;
//! - any shape the box alone writes;
//! - a person's turn not stamped as its poster where the post came from, naming an upload the
//!   box does not hold, or past a full queue;
//! - a stop of a conversation with nothing being answered;
//! - an `Again` that does not rest on every live line descending from its question, asks again
//!   of a question a running round covers, or regenerates a question with no answer yet.
//!
//! The same object posted again is answered `already` and changes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU8, Ordering};

use prodrome_dag::dag::Dag;
use prodrome_data::change::mk_change;
use prodrome_data::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_data::genesis::mk_genesis;
use suzatary_api::sync::{Holding, Received};
use suzatary_base::capability::{Datetime, Entropy};
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Covered, Entry, Kind, Namer, Rewind, RunId, Title};
use suzatary_conversations::secretary::taint::Taint;
use suzatary_held::redact::Rules;
use suzatary_inbox::secretary::stamp::{Speaker, Stamp};
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_lines::secretary::line_shape::LineShape;
use suzatary_lines::secretary::lines_rule::{LinesRule, SlotKeys};
use suzatary_memory::secretary::locked_text::TextKey;
use suzatary_memory::secretary::scrubbed_under::ScrubbedUnder;
use suzatary_memory::secretary::words::Words;
use suzatary_signatures::wire::signatures::RefusalKind;
use suzatary_store::admission::Beside as _;

/// Each key one more than the last, from `start`.
struct Counting(AtomicU8);

impl Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(self.0.fetch_add(1, Ordering::Relaxed));
        Ok(())
    }
}

fn at(second: u32) -> Datetime {
    Datetime::new(2026, 11, 17, 9, 0, second, 0).expect("an instant")
}

fn ada() -> Actor {
    Actor::new("device0").expect("an actor")
}

fn opened(second: u32) -> Entry {
    Entry::Opened {
        at: at(second),
        title: Words::kept("The garden".to_owned()),
        model: String::new(),
        kind: Kind::Topic,
    }
}

fn titled(second: u32, by: &str, title: &str) -> Entry {
    Entry::Titled {
        at: at(second),
        title: Words::kept(Title::new(title).expect("a title")),
        by: Namer::Person {
            actor: by.to_owned(),
        },
    }
}

/// The lines' store at the door: its genesis, and the conversations it holds.
struct Door {
    genesis: Hash,
    /// Every object the store holds, by name.
    history: Dag<LineEvent>,
    /// Every object of a line the store holds: `default`'s opening, and what it took.
    lines: Vec<(Hash, Envelope<LineEvent>)>,
    entropy: Counting,
    /// What the box reads beyond the lines: its clock among it.
    world: &'static dyn suzatary_lines::secretary::lines_rule::World,
}

/// One line posted: its object, its signature by the device's key, and its slots' keys.
struct Posted {
    change: Envelope<LineEvent>,
    keys: Vec<TextKey>,
}

impl Posted {
    fn name(&self) -> Hash {
        seal_hash(&self.change)
    }

    fn prints(&self) -> [String; 2] {
        let key = suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE);
        let signed: Envelope<LineEvent> = Envelope::Signed(key.sign(&self.name()));
        [
            canonical_envelope(&self.change),
            canonical_envelope(&signed),
        ]
    }

    fn hex(&self) -> Vec<String> {
        self.keys
            .iter()
            .map(|key| hex::encode(key.bytes()))
            .collect()
    }
}

static TRUSTING: std::sync::LazyLock<suzatary_signatures::signatures::Trusting> =
    std::sync::LazyLock::new(|| {
        suzatary_signatures::signatures::Trusting::held(
            &suzatary_signatures::signatures::Keeping::stores(&[
                &suzatary_stores::stores::phone_trusted(&["device0"]),
            ]),
            &suzatary_connect::cast::keys::root(),
        )
        .expect("reads")
    });

impl Door {
    fn new() -> Self {
        let begun: Envelope<LineEvent> =
            Envelope::Genesis(mk_genesis("lines", &"0".repeat(32)).expect("a genesis"));
        let genesis = seal_hash(&begun);
        let entropy = Counting(AtomicU8::new(1));
        let (default, _) = LineEvent::older(
            ConversationId::new("default").expect("a slug"),
            Actor::new("assistant").expect("an actor"),
            0,
            &Entry::Opened {
                at: at(0),
                title: Words::kept("Secretary".to_owned()),
                model: "a-cli/small".to_owned(),
                kind: Kind::Default,
            },
            &entropy,
        )
        .expect("a line");
        let default = stored(&genesis, default, Vec::new());
        Self {
            history: [(genesis.clone(), begun), default.clone()]
                .into_iter()
                .collect(),
            genesis,
            lines: vec![default],
            entropy,
            world: &OnADevice,
        }
    }

    /// `line` as a tab seals it: a line of `default` rests on its opening, as a tab's line rests on
    /// its conversation's heads.
    fn change(&self, line: LineEvent, keys: Vec<TextKey>) -> Posted {
        let deps = if line.conversation().as_str() == "default" {
            vec![self.default_opening()]
        } else {
            Vec::new()
        };
        let change = mk_change(self.genesis.clone(), deps, line).expect("a change");
        Posted {
            change: Envelope::Change(change),
            keys,
        }
    }

    /// An opening as a tab seals it: its id its own.
    fn opening(&self, entry: &Entry) -> Posted {
        let (line, keys) = LineEvent::begin(ada(), entry, &self.entropy).expect("a line");
        self.change(line, keys)
    }

    /// A line of `conversation` as a tab seals it, resting on `deps`.
    fn line_on(&self, conversation: &str, entry: &Entry, deps: &[&Hash]) -> Posted {
        let id = ConversationId::new(conversation).expect("a slug");
        let (line, keys) = LineEvent::older(id, ada(), 0, entry, &self.entropy).expect("a line");
        let deps = deps.iter().map(|dep| (*dep).clone()).collect();
        let change = mk_change(self.genesis.clone(), deps, line).expect("a change");
        Posted {
            change: Envelope::Change(change),
            keys,
        }
    }

    /// A line the box wrote to `default`, resting on `deps`: its name.
    fn box_wrote(&mut self, entry: &Entry, deps: &[&Hash]) -> Hash {
        let (line, _) = LineEvent::older(
            ConversationId::new("default").expect("a slug"),
            Actor::new("assistant").expect("an actor"),
            0,
            entry,
            &self.entropy,
        )
        .expect("a line");
        let held = stored(
            &self.genesis,
            line,
            deps.iter().map(|dep| (*dep).clone()).collect(),
        );
        let name = held.0.clone();
        self.history.extend([held.clone()]);
        self.lines.push(held);
        name
    }

    /// A line of `default` as a tab seals it, under the shared key `key` (one of four; the first
    /// is a held line's), resting on `on`.
    fn sealed_under(&self, entry: &Entry, key: u8, on: Hash) -> Posted {
        let (line, keys) = LineEvent::older(
            ConversationId::new("default").expect("a slug"),
            ada(),
            0,
            entry,
            &Given([200 + key; 32]),
        )
        .expect("a line");
        Posted {
            change: Envelope::Change(
                mk_change(self.genesis.clone(), vec![on], line).expect("a change"),
            ),
            keys,
        }
    }

    /// `line` held as the store holds a line, resting on `deps`: its name.
    fn hold(&mut self, line: LineEvent, deps: &[&Hash]) -> Hash {
        let held = stored(
            &self.genesis,
            line,
            deps.iter().map(|dep| (*dep).clone()).collect(),
        );
        let name = held.0.clone();
        self.history.extend([held.clone()]);
        self.lines.push(held);
        name
    }

    /// The name of `default`'s opening.
    fn default_opening(&self) -> Hash {
        self.lines[0].0.clone()
    }

    /// A line of `conversation` as a tab seals it.
    fn line(&self, conversation: &str, entry: &Entry) -> Posted {
        let id = ConversationId::new(conversation).expect("a slug");
        let (line, keys) = LineEvent::older(id, ada(), 0, entry, &self.entropy).expect("a line");
        self.change(line, keys)
    }

    /// `posted` handed over, with each slot's key beside its line.
    fn post(&self, posted: &[&Posted]) -> Received {
        let prints: Vec<String> = posted.iter().flat_map(|p| p.prints()).collect();
        let keys: BTreeMap<String, Vec<String>> = posted
            .iter()
            .map(|p| (p.name().as_str().to_owned(), p.hex()))
            .collect();
        let keys = SlotKeys::parse(&keys).expect("the keys parse");
        suzatary_api::sync::receive_changes::<LinesRule>(
            Holding {
                history: &self.history,
                world: self.world,
            },
            &prints,
            &keys,
            &ada(),
            &TRUSTING,
        )
    }

    /// `posted` handed over beside the extra objects `beside`, read by `world`.
    fn post_in(
        &self,
        posted: &[&Posted],
        beside: &[String],
        world: &'static dyn suzatary_lines::secretary::lines_rule::World,
    ) -> Received {
        let prints: Vec<String> = posted
            .iter()
            .flat_map(|p| p.prints())
            .chain(beside.iter().cloned())
            .collect();
        let keys: BTreeMap<String, Vec<String>> = posted
            .iter()
            .map(|p| (p.name().as_str().to_owned(), p.hex()))
            .collect();
        let keys = SlotKeys::parse(&keys).expect("the keys parse");
        suzatary_api::sync::receive_changes::<LinesRule>(
            Holding {
                history: &self.history,
                world,
            },
            &prints,
            &keys,
            &ada(),
            &TRUSTING,
        )
    }

    /// The post taken in, as the store's accept does.
    fn take(&mut self, received: &Received) {
        for (name, print) in &received.outcome.taken {
            let envelope: Envelope<LineEvent> = prodrome_data::event::parse_envelope(
                &suzatary_lines::secretary::line_event::LineVocabulary,
                print,
            )
            .expect("parses");
            self.history.extend([(name.clone(), envelope.clone())]);
            if matches!(envelope, Envelope::Change(_)) {
                self.lines.push((name.clone(), envelope));
            }
        }
    }
}

/// What the door said of `posted`'s line: taken, or refused on which kind of grounds.
fn said(received: &Received, posted: &Posted) -> Result<(), (RefusalKind, String)> {
    let name = posted.name();
    if received.outcome.taken.iter().any(|(n, _)| *n == name) {
        return Ok(());
    }
    let refused = received
        .outcome
        .refused
        .iter()
        .find(|r| r.name.as_ref() == Some(&name))
        .expect("taken or refused");
    Err((refused.reason.kind(), refused.reason.said()))
}

fn refused_finally(received: &Received, posted: &Posted) -> String {
    match said(received, posted) {
        Err((RefusalKind::Final, why)) => why,
        other => panic!("refused for good, got {other:?}"),
    }
}

/// A tab's opening, its rename in the same post, a setup and a rename of a held conversation
/// are taken; each again is `already`, and changes nothing.
#[test]
fn a_tab_s_lines_sealed_as_the_box_seals_them_are_taken_once() {
    use suzatary_connect::connect::TemplateId;
    let mut door = Door::new();
    let open = door.opening(&opened(1));
    let conversation = open.change.event().expect("a line").conversation().clone();
    let rename = door.line_on(
        conversation.as_str(),
        &titled(2, "device0", "The zephyr filing"),
        &[&open.name()],
    );
    let setup = door.opening(&Entry::SetUp {
        at: at(3),
        title: Words::kept(TemplateId::Gmail.template().called()),
        template: TemplateId::Gmail,
    });
    let renamed = door.line("default", &titled(4, "device0", "Secretary"));
    let received = door.post(&[&open, &rename, &setup, &renamed]);
    for posted in [&open, &rename, &setup, &renamed] {
        assert_eq!(said(&received, posted), Ok(()));
    }
    assert!(received.recorded.is_empty());
    door.take(&received);
    let again = door.post(&[&open, &rename, &setup, &renamed]);
    assert!(again.outcome.taken.is_empty() && again.outcome.refused.is_empty());
    assert_eq!(
        again.outcome.already.len(),
        8,
        "four lines, four signatures"
    );
}

/// The refusals, each of a line alone in its post.
#[test]
fn the_rule_refuses_what_the_signer_does_not_prove_and_words_the_box_would_not_seal() {
    let door = Door::new();

    // A slot whose words the current rules would change: sealed as if scrubbed, and not.
    let unscrubbed = Entry::Said {
        at: at(5),
        turn: Turn::Person {
            text: "Your verification code is 482913.".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: None,
    };
    let title_of = |text: &str| Entry::Titled {
        at: at(5),
        title: Words::kept(Title::new(text).expect("a title")),
        by: Namer::Person {
            actor: "device0".to_owned(),
        },
    };
    let current = ScrubbedUnder::Version(Rules::current(()).clone());
    let (line, keys) = LineEvent::written_under(
        &current,
        ConversationId::new("default").expect("a slug"),
        ada(),
        0,
        &title_of("Your verification code is 482913."),
        &door.entropy,
    )
    .expect("a line");
    let dirty = door.change(line, keys);
    assert!(refused_finally(&door.post(&[&dirty]), &dirty).contains("as the box would seal them"));

    // A slot under rules older than the box's: outdated, not final.
    let older = ScrubbedUnder::Version(
        Rules::new("67d7f8df2fd4d019fd90b6d59f17f44b046e102ea2d8ad4da122d6e7517c4f4a")
            .expect("a rules name"),
    );
    let (line, keys) = LineEvent::written_under(
        &older,
        ConversationId::new("default").expect("a slug"),
        ada(),
        0,
        &title_of("Garden"),
        &door.entropy,
    )
    .expect("a line");
    let stale = door.change(line, keys);
    let received = door.post(&[&stale]);
    assert!(matches!(
        said(&received, &stale),
        Err((RefusalKind::Outdated, _))
    ));
    assert_eq!(received.recorded.len(), 1, "an outdated line is recorded");

    // A slot posted with another line's key.
    let mut wrong = door.line("default", &titled(6, "device0", "Garden"));
    let other = door.line("default", &titled(7, "device0", "Invoices"));
    wrong.keys = other.keys;
    assert!(refused_finally(&door.post(&[&wrong]), &wrong).contains("does not open"));

    // A rename naming someone else.
    let forged = door.line("default", &titled(8, "bob", "Garden"));
    assert!(refused_finally(&door.post(&[&forged]), &forged).contains("someone else"));

    // An opening under an id that is not its own, and one the box mints.
    for id in [
        "0123456789abcdef0123456789abcdef",
        "2026-11-18-review",
        "2026-11-17-3",
        "setup",
    ] {
        let misnamed = door.line(id, &opened(9));
        assert!(
            refused_finally(&door.post(&[&misnamed]), &misnamed).contains("named by its opening"),
            "{id}"
        );
    }

    // A rename of a conversation the store does not hold and the post does not begin: no tab
    // names tomorrow's round, a dated id or a fixed one before the box does.
    for id in [
        "2026-11-18-review",
        "2026-11-18-sweep",
        "2026-11-17-3",
        "setup",
    ] {
        let early = door.line(id, &titled(10, "device0", "Mine"));
        assert!(
            refused_finally(&door.post(&[&early]), &early).contains("to rename"),
            "{id}"
        );
    }

    // An opening of a kind the box opens.
    let review = door.opening(&Entry::Opened {
        at: at(11),
        title: Words::kept("Review".to_owned()),
        model: "claude".to_owned(),
        kind: Kind::Review,
    });
    assert!(refused_finally(&door.post(&[&review]), &review).contains("a tab opens a topic"));

    // A person's turn the signer does not stamp: a tab's turn names its poster.
    let spoken = door.line("default", &unscrubbed);
    assert!(refused_finally(&door.post(&[&spoken]), &spoken).contains("stamped as its poster"));

    // A shape the box alone writes: the model's turn.
    let answered = door.line(
        "default",
        &Entry::Said {
            at: at(5),
            turn: Turn::Model {
                text: "Done.".to_owned(),
                calls: Vec::new(),
            },
            by: None,
        },
    );
    assert!(refused_finally(&door.post(&[&answered]), &answered).contains("the box alone writes"));
}

/// Only the box claims a round, so no tab can say a turn of its own was answered or judged: a
/// posted `Began` is refused for good, whatever it names.
#[test]
fn a_posted_claim_is_refused_for_good() {
    let door = Door::new();
    let claim = door.line(
        "default",
        &Entry::Began {
            at: at(12),
            covers: vec![suzatary_conversations::secretary::store::Covered {
                line: Hash::new("a".repeat(64)).expect("a name"),
                taint: suzatary_conversations::secretary::taint::Taint::Clean,
            }],
            run: suzatary_conversations::secretary::store::RunId::of([7; 16]),
        },
    );
    assert!(
        refused_finally(&door.post(&[&claim]), &claim).contains("the box alone writes a Began")
    );
}

/// A second opening of one conversation is refused once the first is held: two tabs never join
/// one conversation, and an id is never taken twice.
#[test]
fn a_conversation_is_opened_once() {
    let mut door = Door::new();
    let first = door.opening(&opened(12));
    let received = door.post(&[&first]);
    door.take(&received);
    let (line, keys) = LineEvent::begin(ada(), &opened(12), &door.entropy).expect("a line");
    let twin = door.change(line, keys);
    assert_ne!(twin.name(), first.name(), "other keys, another object");
    assert!(refused_finally(&door.post(&[&twin]), &twin).contains("already exists"));
}

/// Keys that do not parse refuse the whole post at the door, and a store that takes no keys
/// refuses any.
#[test]
fn keys_are_parsed_at_the_door() {
    let bad: BTreeMap<String, Vec<String>> = [("a".repeat(64), vec!["not hex".to_owned()])].into();
    assert!(SlotKeys::parse(&bad).is_err());
    assert!(<() as suzatary_store::admission::Beside>::parse(&bad).is_err());
    assert!(<() as suzatary_store::admission::Beside>::parse(&BTreeMap::new()).is_ok());
}

/// The rule reads the words it opened as the box reads them: a rename's are a title (one line,
/// at most 80 characters, held as it reads); an opening's are empty or a title; a setup's are its
/// template's own name. Each other title, and an opening naming a model, is refused for good;
/// an empty opening is taken.
#[test]
fn the_rule_takes_a_title_only_as_the_box_reads_one() {
    use suzatary_connect::connect::TemplateId;
    use suzatary_lines::secretary::line_shape::LineShape;
    use suzatary_memory::secretary::line_words::LineWords;
    let door = Door::new();
    let words = |title: &str| {
        [LineWords::Title {
            title: title.to_owned(),
        }]
    };
    let renamed = |title: &str, second: u32| {
        let (line, keys) = LineEvent::sealed_as(
            ConversationId::new("default").expect("a slug"),
            at(second),
            ada(),
            1,
            LineShape::Titled {
                by: Namer::Person {
                    actor: "device0".to_owned(),
                },
            },
            &words(title),
            &door.entropy,
        )
        .expect("a line");
        door.change(line, keys)
    };
    let long = "a".repeat(81);
    for (second, title) in [
        (20, long.as_str()),
        (21, "two\nlines"),
        (22, ""),
        (23, " padded "),
    ] {
        let posted = renamed(title, second);
        assert!(
            refused_finally(&door.post(&[&posted]), &posted).contains("title"),
            "{title:?}"
        );
    }

    let opening = |title: &str, model: &str, second: u32| {
        door.opening(&Entry::Opened {
            at: at(second),
            title: Words::kept(title.to_owned()),
            model: model.to_owned(),
            kind: Kind::Topic,
        })
    };
    for (second, title) in [(24, long.as_str()), (25, "two\nlines")] {
        let posted = opening(title, "", second);
        assert!(
            refused_finally(&door.post(&[&posted]), &posted).contains("title"),
            "{title:?}"
        );
    }
    let empty = opening("", "", 26);
    assert_eq!(
        said(&door.post(&[&empty]), &empty),
        Ok(()),
        "an untitled topic is taken"
    );
    let modelled = opening("Garden", "a-cli/small", 27);
    assert!(refused_finally(&door.post(&[&modelled]), &modelled).contains("names no model"));

    let elsewhere = door.opening(&Entry::SetUp {
        at: at(28),
        title: Words::kept("Garden".to_owned()),
        template: TemplateId::Gmail,
    });
    assert!(
        refused_finally(&door.post(&[&elsewhere]), &elsewhere).contains("titled by its template")
    );
}

/// No post brings back a key a line the store holds names, whether the box keeps it for that
/// line or an erasure destroyed it: a line sealed under one is refused for good, so the store's
/// key store never takes it again and the words it opened stay closed.
#[test]
fn a_key_a_held_line_names_is_never_taken_again() {
    let key = [7u8; 32];
    let default = || ConversationId::new("default").expect("a slug");
    let mut door = Door::new();
    let (held, _) = LineEvent::older(
        default(),
        Actor::new("assistant").expect("an actor"),
        1,
        &titled(29, "device0", "Invoices"),
        &Given(key),
    )
    .expect("a line");
    let held = stored(&door.genesis, held, Vec::new());
    door.history.extend([held.clone()]);
    door.lines.push(held);
    let (line, keys) = LineEvent::older(
        default(),
        ada(),
        2,
        &titled(31, "device0", "Garden"),
        &Given(key),
    )
    .expect("a line");
    let reused = door.change(line, keys);
    assert!(refused_finally(&door.post(&[&reused]), &reused).contains("a key of its own"));
}

/// No two lines the box takes share a key: of a new topic's opening and its first question sealed
/// under one key, the opening is judged first and taken, and the question is refused for good as
/// naming a key a held line names; a line whose own two slots share one is refused, whatever its
/// shape.
#[test]
fn no_two_slots_of_one_post_share_a_key() {
    let door = Door::new();
    let (opening, keys) = LineEvent::begin(ada(), &opened(40), &Given([9; 32])).expect("a line");
    let id = opening.conversation().clone();
    let opening = door.change(opening, keys);
    let (first, keys) = LineEvent::older(
        id,
        ada(),
        1,
        &question(41, "Is the permit due?"),
        &Given([9; 32]),
    )
    .expect("a line");
    let first = Posted {
        change: Envelope::Change(
            mk_change(door.genesis.clone(), vec![opening.name()], first).expect("a change"),
        ),
        keys,
    };
    let received = door.post(&[&opening, &first]);
    assert_eq!(said(&received, &opening), Ok(()));
    assert!(refused_finally(&received, &first).contains("already names"));

    let (called, keys) = LineEvent::older(
        ConversationId::new("default").expect("a slug"),
        ada(),
        0,
        &Entry::Said {
            at: at(42),
            turn: Turn::Model {
                text: "Looking.".to_owned(),
                calls: vec![suzatary_conversations::secretary::model::ToolCall {
                    verb: "your_memory".into(),
                    args: BTreeMap::new(),
                }],
            },
            by: None,
        },
        &Given([11; 32]),
    )
    .expect("a line");
    let called = door.change(called, keys);
    assert!(refused_finally(&door.post(&[&called]), &called).contains("two of its slots"));
}

proptest::proptest! {
    /// Over a generated post of renames, each sealed under a key drawn from a small pool: of the
    /// lines under one key, the one judged first (the least name, since all are ready together) is
    /// taken, and every other is refused for good as naming a key a held line names.
    #[test]
    fn a_line_is_refused_for_a_shared_key_exactly_when_another_names_it(
        drawn in proptest::collection::vec(20u8..24, 1..6),
    ) {
        let door = Door::new();
        let lines: Vec<Posted> = drawn
            .iter()
            .zip(0u32..)
            .map(|(byte, n)| {
                let (line, keys) = LineEvent::older(
                    ConversationId::new("default").expect("a slug"),
                    ada(),
                    0,
                    &titled(50 + n, "device0", "Garden"),
                    &Given([*byte; 32]),
                )
                .expect("a line");
                door.change(line, keys)
            })
            .collect();
        let received = door.post(&lines.iter().collect::<Vec<_>>());
        for (posted, byte) in lines.iter().zip(&drawn) {
            let first = lines
                .iter()
                .zip(&drawn)
                .filter(|(_, other)| *other == byte)
                .map(|(one, _)| one.name())
                .min()
                .expect("itself");
            match said(&received, posted) {
                Ok(()) => proptest::prop_assert_eq!(posted.name(), first),
                Err((RefusalKind::Final, why)) => {
                    proptest::prop_assert!(posted.name() != first && why.contains("already names"), "{why}");
                }
                Err(other) => proptest::prop_assert!(false, "{other:?}"),
            }
        }
    }
}

/// The same bytes every time.
struct Given([u8; 32]);

impl Entropy for Given {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.copy_from_slice(&self.0[..bytes.len()]);
        Ok(())
    }
}

/// A line a tab writes is written here: each shape a tab posts (an opening, a setup, a rename) is
/// refused for good when it says it was carried from another store, whose object the digest would
/// read as its leaf.
#[test]
fn a_tab_s_line_carried_from_elsewhere_is_refused() {
    use suzatary_connect::connect::TemplateId;
    let door = Door::new();
    let elsewhere = Hash::new("a".repeat(64)).expect("a name");
    let carried = |entry: &Entry, conversation: Option<&str>| {
        let (line, keys) = match conversation {
            Some(id) => LineEvent::older(
                ConversationId::new(id).expect("a slug"),
                ada(),
                0,
                entry,
                &door.entropy,
            ),
            None => LineEvent::begin(ada(), entry, &door.entropy),
        }
        .expect("a line");
        door.change(line.carried_from(elsewhere.clone()), keys)
    };
    let setup = Entry::SetUp {
        at: at(21),
        title: Words::kept(TemplateId::Gmail.template().called()),
        template: TemplateId::Gmail,
    };
    for posted in [
        carried(&opened(20), None),
        carried(&setup, None),
        carried(&titled(22, "device0", "Garden"), Some("default")),
    ] {
        assert!(
            refused_finally(&door.post(&[&posted]), &posted).contains("carried from another store")
        );
    }
}

/// `line` as the store holds it: a change over `deps`, beside its name.
fn stored(genesis: &Hash, line: LineEvent, deps: Vec<Hash>) -> (Hash, Envelope<LineEvent>) {
    let change = Envelope::Change(mk_change(genesis.clone(), deps, line).expect("a change"));
    (seal_hash(&change), change)
}

/// A post from a device, of a box that keeps no uploads, once the box's clock has moved past
/// every line the laws date ahead.
struct Moved;

impl suzatary_lines::secretary::lines_rule::World for Moved {
    fn now(&self) -> prodrome_data::literal::Datetime {
        prodrome_data::literal::Datetime::new(2026, 11, 17, 9, 20, 0, 0).expect("an instant")
    }

    fn seat(&self, key: &prodrome_data::sign::PublicKey) -> suzatary_signatures::signatures::Seat {
        OnADevice.seat(key)
    }

    fn upload(
        &self,
        id: &suzatary_held::attachment::AttachmentId,
    ) -> Result<Option<suzatary_lines::secretary::lines_rule::Upload>, String> {
        OnADevice.upload(id)
    }
}

/// A post from a device, of a box that keeps no uploads.
struct OnADevice;

impl suzatary_lines::secretary::lines_rule::World for OnADevice {
    fn now(&self) -> prodrome_data::literal::Datetime {
        prodrome_data::literal::Datetime::new(2026, 11, 17, 9, 1, 0, 0).expect("an instant")
    }

    fn seat(&self, _: &prodrome_data::sign::PublicKey) -> suzatary_signatures::signatures::Seat {
        suzatary_signatures::signatures::Seat::Device
    }

    fn upload(
        &self,
        _: &suzatary_held::attachment::AttachmentId,
    ) -> Result<Option<suzatary_lines::secretary::lines_rule::Upload>, String> {
        Ok(None)
    }
}

fn question(second: u32, text: &str) -> Entry {
    Entry::Said {
        at: at(second),
        turn: Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some(Stamp::Actor(Speaker::device("device0"))),
    }
}

fn answer(second: u32) -> Entry {
    Entry::Said {
        at: at(second),
        turn: Turn::Model {
            text: "Here it is.".to_owned(),
            calls: Vec::new(),
        },
        by: None,
    }
}

fn claim(second: u32, covers: &Hash) -> Entry {
    Entry::Began {
        at: at(second),
        covers: vec![Covered {
            line: covers.clone(),
            taint: Taint::Clean,
        }],
        run: RunId::of([3; 16]),
    }
}

fn again(second: u32, question: &Hash, how: Rewind) -> Entry {
    Entry::Again {
        at: at(second),
        question: question.clone(),
        how,
        by: "device0".to_owned(),
    }
}

/// A tab's question stamped as its poster is taken; one past a full queue, or naming an upload
/// the box does not hold, is refused for good.
#[test]
fn a_tab_s_question_is_taken_within_the_queue_and_with_held_uploads() {
    let mut door = Door::new();
    let opening = door.default_opening();
    let asked = door.line_on("default", &question(1, "Is the permit due?"), &[&opening]);
    let received = door.post(&[&asked]);
    assert_eq!(said(&received, &asked), Ok(()));
    door.take(&received);

    let mut last = asked.name();
    for n in 0..15 {
        let more = door.line_on("default", &question(2 + n, "and this?"), &[&last]);
        let received = door.post(&[&more]);
        assert_eq!(said(&received, &more), Ok(()), "turn {n}");
        door.take(&received);
        last = more.name();
    }
    let seventeenth = door.line_on("default", &question(20, "one more"), &[&last]);
    assert!(refused_finally(&door.post(&[&seventeenth]), &seventeenth).contains("already wait"));

    let photo = suzatary_held::attachment::Attachment::accept(PNG).expect("a photo");
    let pictured = door.line_on(
        "default",
        &Entry::Said {
            at: at(21),
            turn: Turn::Person {
                text: String::new(),
                references: Vec::new(),
                attachments: vec![photo],
            },
            by: Some(Stamp::Actor(Speaker::device("device0"))),
        },
        &[&opening],
    );
    assert!(refused_finally(&door.post(&[&pictured]), &pictured).contains("no upload"));
}

/// A stop is taken only where something beneath it is being answered.
#[test]
fn a_stop_stops_only_what_is_being_answered() {
    let mut door = Door::new();
    let opening = door.default_opening();
    let idle = door.line_on(
        "default",
        &Entry::Stopped {
            at: at(1),
            by: "device0".to_owned(),
        },
        &[&opening],
    );
    assert!(refused_finally(&door.post(&[&idle]), &idle).contains("nothing stops"));

    let asked = door.line_on("default", &question(2, "Is the permit due?"), &[&opening]);
    let received = door.post(&[&asked]);
    door.take(&received);
    let stop = door.line_on(
        "default",
        &Entry::Stopped {
            at: at(3),
            by: "device0".to_owned(),
        },
        &[&asked.name()],
    );
    assert_eq!(said(&door.post(&[&stop]), &stop), Ok(()));

    let forged = door.line_on(
        "default",
        &Entry::Stopped {
            at: at(4),
            by: "bob".to_owned(),
        },
        &[&asked.name()],
    );
    assert!(refused_finally(&door.post(&[&forged]), &forged).contains("someone else"));
}

/// Law 11: an `Again` is taken only when it rests on every live line descending from its
/// question, and refused while a running round covers its question; a regenerate asks again
/// only of an answered question.
#[test]
fn an_again_is_refused_unless_it_saw_the_whole_answer_and_no_round_runs() {
    let mut door = Door::new();
    let opening = door.default_opening();
    let asked = door.line_on("default", &question(1, "Is the permit due?"), &[&opening]);
    let received = door.post(&[&asked]);
    door.take(&received);
    let q = asked.name();

    let early = door.line_on("default", &again(2, &q, Rewind::Regenerate), &[&q]);
    let why = refused_finally(&door.post(&[&early]), &early);
    assert!(why.contains("no answer yet"), "{why}");

    let began = door.box_wrote(&claim(3, &q), &[&q]);
    let running = door.line_on("default", &again(4, &q, Rewind::Edit), &[&began]);
    assert!(refused_finally(&door.post(&[&running]), &running).contains("stop it first"));

    let answered = door.box_wrote(&answer(5), &[&began]);
    let unseen = door.line_on("default", &again(6, &q, Rewind::Regenerate), &[&began]);
    assert!(refused_finally(&door.post(&[&unseen]), &unseen).contains("moved on"));

    for how in [Rewind::Regenerate, Rewind::Edit] {
        let seen = door.line_on("default", &again(7, &q, how), &[&answered]);
        assert_eq!(said(&door.post(&[&seen]), &seen), Ok(()), "{how:?}");
    }

    let answer_line = door.line_on("default", &again(8, &answered, Rewind::Edit), &[&answered]);
    assert!(refused_finally(&door.post(&[&answer_line]), &answer_line).contains("live question"));
}

/// A regenerate makes its answered question owed again, so it asks the queue's bound as a
/// question does: beside fifteen owed questions, asked beside it, it is taken; beside sixteen it
/// is refused.
#[test]
fn an_again_that_makes_a_question_owed_asks_the_queue() {
    for (owed, taken) in [(15, true), (16, false)] {
        let mut door = Door::new();
        let opening = door.default_opening();
        let asked = door.line_on("default", &question(1, "Is the permit due?"), &[&opening]);
        let received = door.post(&[&asked]);
        door.take(&received);
        let q = asked.name();
        let began = door.box_wrote(&claim(2, &q), &[&q]);
        let answered = door.box_wrote(&answer(3), &[&began]);
        let mut last = opening.clone();
        for n in 0..owed {
            let more = door.line_on("default", &question(4 + n, "and this?"), &[&last]);
            let received = door.post(&[&more]);
            assert_eq!(said(&received, &more), Ok(()), "question {n}");
            door.take(&received);
            last = more.name();
        }
        let regenerate = door.line_on(
            "default",
            &again(30, &q, Rewind::Regenerate),
            &[&answered, &last],
        );
        let received = door.post(&[&regenerate]);
        if taken {
            assert_eq!(said(&received, &regenerate), Ok(()), "beside {owed} owed");
        } else {
            let why = refused_finally(&received, &regenerate);
            assert!(why.contains("already wait"), "beside {owed} owed: {why}");
        }
    }
}

/// The smallest PNG: one transparent pixel.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// The box's seats by key: the root key is the operator's, any other a device's.
struct ByKey;

impl suzatary_lines::secretary::lines_rule::World for ByKey {
    fn now(&self) -> prodrome_data::literal::Datetime {
        prodrome_data::literal::Datetime::new(2026, 11, 17, 9, 1, 0, 0).expect("an instant")
    }

    fn seat(&self, key: &prodrome_data::sign::PublicKey) -> suzatary_signatures::signatures::Seat {
        if *key == suzatary_connect::cast::keys::root() {
            suzatary_signatures::signatures::Seat::Operator
        } else {
            suzatary_signatures::signatures::Seat::Device
        }
    }

    fn upload(
        &self,
        _: &suzatary_held::attachment::AttachmentId,
    ) -> Result<Option<suzatary_lines::secretary::lines_rule::Upload>, String> {
        Ok(None)
    }
}

/// A seat is proven by a signature the trust admits, never by one merely posted: a device's
/// question stamped at the operator's seat, beside a worthless signature naming the root key, is
/// refused for good, and the same question at the device's seat is taken.
#[test]
fn a_seat_is_proven_only_by_an_admitted_signature() {
    let door = Door::new();
    let opening = door.default_opening();
    let at_seat = |seat, second| Entry::Said {
        at: at(second),
        turn: Turn::Person {
            text: "Is the permit due?".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some(Stamp::Actor(Speaker::at("device0", seat))),
    };
    let claimed = door.line_on(
        "default",
        &at_seat(suzatary_signatures::signatures::Seat::Operator, 1),
        &[&opening],
    );
    // A signature that names the root key over the line, made with another key: it never verifies.
    let phone = suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE);
    let mut worthless = phone.sign(&claimed.name());
    worthless.key = suzatary_connect::cast::keys::root();
    let worthless: Envelope<LineEvent> = Envelope::Signed(worthless);
    let received = door.post_in(&[&claimed], &[canonical_envelope(&worthless)], &ByKey);
    assert!(
        refused_finally(&received, &claimed).contains("where the post came from"),
        "{:?}",
        received.outcome
    );

    let honest = door.line_on(
        "default",
        &at_seat(suzatary_signatures::signatures::Seat::Device, 2),
        &[&opening],
    );
    assert_eq!(
        said(&door.post_in(&[&honest], &[], &ByKey), &honest),
        Ok(())
    );
}

/// The queue's bound counts what a question rests on, held or posted beneath it: of a tab's
/// seventeen questions, each resting on the one before, sixteen are taken and the seventeenth is
/// refused for good, whether they come in one post or in two.
#[test]
fn the_queue_counts_what_a_question_rests_on() {
    let door = Door::new();
    let chain = asked_in_a_row(&door, 17);
    let taken = |posts: &[&[Posted]]| -> Vec<bool> {
        let mut door = Door::new();
        let mut verdicts = BTreeMap::new();
        for post in posts {
            let received = door.post(&post.iter().collect::<Vec<_>>());
            for q in *post {
                verdicts.insert(q.name(), said(&received, q).is_ok());
            }
            door.take(&received);
        }
        chain.iter().map(|q| verdicts[&q.name()]).collect()
    };
    let expected: Vec<bool> = (0..17).map(|k| k < 16).collect();
    assert_eq!(taken(&[&chain]), expected, "one post");
    assert_eq!(
        taken(&[&chain[..16], &chain[16..]]),
        expected,
        "sixteen, then one"
    );
    let held = Door::new();
    let mut door = held;
    door.take(&door.post(&chain[..16].iter().collect::<Vec<_>>()));
    assert!(refused_finally(&door.post(&[&chain[16]]), &chain[16]).contains("already wait"));
}

/// `n` questions of `default` a tab seals one after another, each resting on the one before.
fn asked_in_a_row(door: &Door, n: u32) -> Vec<Posted> {
    let mut head = door.default_opening();
    (0..n)
        .map(|k| {
            let q = door.line_on(
                "default",
                &question(30 + k, &format!("question {k}")),
                &[&head],
            );
            head = q.name();
            q
        })
        .collect()
}

/// One line a tab writes in `default`, for the split law.
#[derive(Debug, Clone, Copy)]
enum Written {
    Ask,
    /// A question with no words, which the rule refuses for good.
    Empty,
    Stop,
}

/// The lines a tab writes, each resting on the one before it, over a store holding `owed` of the
/// box's questions in `default`: the door that holds them, and the lines.
fn written_in_a_row(owed: u32, script: &[Written]) -> (Door, Vec<Posted>) {
    let mut door = Door::new();
    let mut head = door.default_opening();
    for k in 0..owed {
        head = door.box_wrote(&question(10 + k, "asked before"), &[&head]);
    }
    let mut lines = Vec::new();
    for (k, written) in script.iter().enumerate() {
        let second = 30 + u32::try_from(k).expect("few lines");
        let entry = match written {
            Written::Ask => question(second, "Is the permit due?"),
            Written::Empty => question(second, " "),
            Written::Stop => Entry::Stopped {
                at: at(second),
                by: "device0".to_owned(),
            },
        };
        let line = door.line_on("default", &entry, &[&head]);
        head = line.name();
        lines.push(line);
    }
    (door, lines)
}

/// The names of `lines` taken by a door holding what `written_in_a_row` held, posted in `posts`
/// (each a list of indices into `lines`), as a tab hands them over: what the box holds back for
/// now (a line whose parent it does not hold yet) goes again with the next post, and once more at
/// the end until nothing moves.
fn taken_in(
    owed: u32,
    script: &[Written],
    lines: &[Posted],
    posts: &[Vec<usize>],
) -> BTreeSet<Hash> {
    let (mut door, _) = written_in_a_row(owed, script);
    let mut taken = BTreeSet::new();
    let mut again: Vec<usize> = Vec::new();
    let mut hand = |door: &mut Door, these: Vec<usize>| -> Vec<usize> {
        if these.is_empty() {
            return these;
        }
        let received = door.post(&these.iter().map(|&k| &lines[k]).collect::<Vec<_>>());
        door.take(&received);
        these
            .into_iter()
            .filter(|&k| match said(&received, &lines[k]) {
                Ok(()) => {
                    taken.insert(lines[k].name());
                    false
                }
                Err((RefusalKind::Final | RefusalKind::Outdated, _)) => false,
                Err(_) => true,
            })
            .collect()
    };
    for post in posts {
        let these: Vec<usize> = again.iter().copied().chain(post.iter().copied()).collect();
        again = hand(&mut door, these);
    }
    while !again.is_empty() {
        let before = again.len();
        again = hand(&mut door, again);
        if again.len() == before {
            break;
        }
    }
    taken
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(96))]

    /// THE SPLIT LAW: the lines the box takes of what a tab writes are the same however the tab
    /// splits them across posts and orders them within each and among the posts. A line's verdict
    /// reads the store and what the line rests on, never what else its post carries.
    #[test]
    fn the_lines_taken_do_not_depend_on_how_they_are_posted(
        owed in 0u32..18,
        script in proptest::collection::vec(
            proptest::prop_oneof![
                4 => proptest::strategy::Just(Written::Ask),
                1 => proptest::strategy::Just(Written::Empty),
                2 => proptest::strategy::Just(Written::Stop),
            ],
            1..12,
        ),
        cuts in proptest::collection::vec(proptest::prelude::any::<proptest::sample::Index>(), 0..4),
        order in proptest::prelude::any::<proptest::sample::Index>(),
        shuffle in proptest::prelude::any::<u64>(),
    ) {
        let (_, lines) = written_in_a_row(owed, &script);
        let whole = taken_in(owed, &script, &lines, &[(0..lines.len()).collect()]);
        // The lines cut into pieces, each piece shuffled, and the pieces turned.
        let mut at: Vec<usize> = cuts.iter().map(|cut| cut.index(lines.len() + 1)).collect();
        at.extend([0, lines.len()]);
        at.sort_unstable();
        at.dedup();
        let mut posts: Vec<Vec<usize>> = at.windows(2).map(|w| (w[0]..w[1]).collect()).collect();
        let mut seed = shuffle;
        for post in &mut posts {
            for k in (1..post.len()).rev() {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                post.swap(k, usize::try_from(seed >> 33).expect("fits") % (k + 1));
            }
        }
        let turn = order.index(posts.len());
        posts.rotate_left(turn);
        let split = taken_in(owed, &script, &lines, &posts);
        proptest::prop_assert_eq!(whole, split, "posts {:?}", posts);
    }
}

/// A tab's line rests on its conversation's opening, held or posted beside it: a question resting
/// on nothing is refused for good, and so is one beside an opening the box refused, and one resting
/// on an opening this post's rule refuses is refused with it.
#[test]
fn a_tab_s_line_rests_on_its_conversation_s_opening() {
    let door = Door::new();
    let loose = door.line_on("default", &question(40, "Is the permit due?"), &[]);
    assert!(refused_finally(&door.post(&[&loose]), &loose).contains("opening"));

    // An opening the box refuses (it names a model), and a question of it resting on nothing.
    let refused_opening = door.opening(&Entry::Opened {
        at: at(41),
        title: Words::kept("The garden".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Topic,
    });
    let LineShape::Opened { .. } = refused_opening_shape(&refused_opening) else {
        panic!("an opening")
    };
    let id = conversation_of(&refused_opening);
    let orphan = door.line(&id, &question(42, "and this?"));
    let received = door.post(&[&refused_opening, &orphan]);
    assert!(refused_finally(&received, &refused_opening).contains("names no model"));
    assert!(said(&received, &orphan).is_err(), "{:?}", received.outcome);

    let resting = door.line_on(&id, &question(43, "and on it?"), &[&refused_opening.name()]);
    let received = door.post(&[&refused_opening, &resting]);
    assert!(refused_finally(&received, &refused_opening).contains("names no model"));
    let why = refused_finally(&received, &resting);
    assert!(why.contains("which this box refused"), "{why}");
}

/// A stop posted with the question it stops, as a tab sends both while the box is away, stops it.
#[test]
fn a_stop_posted_with_its_question_is_taken() {
    let door = Door::new();
    let opening = door.default_opening();
    let asked = door.line_on("default", &question(50, "Is the permit due?"), &[&opening]);
    let stop = door.line_on(
        "default",
        &Entry::Stopped {
            at: at(51),
            by: "device0".to_owned(),
        },
        &[&asked.name()],
    );
    let received = door.post(&[&asked, &stop]);
    assert_eq!(said(&received, &asked), Ok(()));
    assert_eq!(said(&received, &stop), Ok(()), "{:?}", received.outcome);
}

fn refused_opening_shape(posted: &Posted) -> LineShape {
    match &posted.change {
        Envelope::Change(change) => change.event.shape().clone(),
        _ => panic!("a change"),
    }
}

fn conversation_of(posted: &Posted) -> String {
    match &posted.change {
        Envelope::Change(change) => change.event.conversation().as_str().to_owned(),
        _ => panic!("a change"),
    }
}

/// A tab's line dated more than `MAX_AHEAD_SECONDS` ahead of the box's clock is refused for now,
/// to be posted again then, never for good; one a little ahead is taken.
#[test]
fn a_line_dated_ahead_of_the_box_is_posted_again_later() {
    let door = Door::new();
    let opening = door.default_opening();
    // The box's clock reads 09:01:00 (`OnADevice`).
    let dated = |minute: u32| Entry::Said {
        at: Datetime::new(2026, 11, 17, 9, minute, 0, 0).expect("an instant"),
        turn: Turn::Person {
            text: format!("asked at {minute}"),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some(Stamp::Actor(Speaker::device("device0"))),
    };
    let far = door.line_on("default", &dated(11), &[&opening]);
    match said(&door.post(&[&far]), &far) {
        Err((RefusalKind::Later, why)) => {
            assert!(why.contains("ahead of the box's clock"), "{why}");
        }
        other => panic!("refused for now, got {other:?}"),
    }
    let near = door.line_on("default", &dated(5), &[&opening]);
    assert_eq!(said(&door.post(&[&near]), &near), Ok(()));

    // A line resting on one that waits waits with it, and says so: never "refused".
    let after = door.line_on("default", &question(50, "and then?"), &[&far.name()]);
    match said(&door.post(&[&far, &after]), &after) {
        Err((RefusalKind::Later, why)) => {
            assert!(
                why.contains("has not taken yet, and waits with it"),
                "{why}"
            );
            assert!(!why.contains("refused"), "{why}");
        }
        other => panic!("waiting with what it rests on, got {other:?}"),
    }
}

/// A conversation is opened once however its openings arrive: two openings of one id in one post
/// open it once, as two posts of one opening each do.
#[test]
fn a_conversation_is_opened_once_in_one_post_too() {
    let door = Door::new();
    let first = door.opening(&opened(12));
    let (line, keys) = LineEvent::begin(ada(), &opened(12), &door.entropy).expect("a line");
    let twin = door.change(line, keys);
    assert_eq!(conversation_of(&first), conversation_of(&twin), "one id");
    let received = door.post(&[&first, &twin]);
    let taken = [&first, &twin]
        .iter()
        .filter(|posted| said(&received, posted).is_ok())
        .count();
    assert_eq!(taken, 1, "{:?}", received.outcome);
}

/// The queue's bound holds over a post of questions that rest on one head beside each other: of
/// the box's fifteen owed and twenty more posted at once, one is taken, and of five hundred
/// resting on the opening alone, sixteen.
#[test]
fn sibling_questions_never_pass_the_queue() {
    let (door, _) = written_in_a_row(15, &[]);
    let head = door.lines.last().expect("a held line").0.clone();
    let siblings: Vec<Posted> = (0..20)
        .map(|k| {
            door.line_on(
                "default",
                &question(30 + k, &format!("beside {k}")),
                &[&head],
            )
        })
        .collect();
    let received = door.post(&siblings.iter().collect::<Vec<_>>());
    let taken = siblings
        .iter()
        .filter(|q| said(&received, q).is_ok())
        .count();
    assert_eq!(taken, 1, "{:?}", received.outcome.refused.first());

    let door = Door::new();
    let opening = door.default_opening();
    let many: Vec<Posted> = (0..500)
        .map(|k| {
            door.line_on(
                "default",
                &question(k % 59, &format!("many {k}")),
                &[&opening],
            )
        })
        .collect();
    let received = door.post(&many.iter().collect::<Vec<_>>());
    let taken = many.iter().filter(|q| said(&received, q).is_ok()).count();
    assert_eq!(taken, suzatary_conversations::secretary::queue::MAX_PENDING);
}

/// One line a tab writes, for the post laws: where each rests is an index into what came before
/// it (the store's own lines first), read modulo how many there are.
#[derive(Debug, Clone, Copy)]
enum Wrote {
    /// A question, sealed under one of the renames' four keys where `key` says so (the first is
    /// a held line's).
    Ask {
        on: usize,
        key: Option<u8>,
    },
    Stop {
        on: usize,
    },
    /// An edit, or a regenerate, which makes an answered question owed again.
    Again {
        of: usize,
        on: usize,
        regenerate: bool,
    },
    /// A rename sealed under one of four keys, so lines share them.
    Rename {
        key: u8,
        on: usize,
    },
    /// An opening of the one topic every opening here names, in one of two forms.
    Open {
        form: bool,
    },
    AskTopic {
        on: usize,
    },
    /// A question dated ten minutes past the box's clock.
    Ahead {
        on: usize,
    },
    /// A question resting on a line no one holds.
    Orphan,
    /// An opening of the topic the store already holds, in one of two forms.
    Reopen {
        form: bool,
    },
    /// A setup's opening: every one here names one id.
    SetUp,
    /// A regenerate of the store's answered question, resting on its answer and on another line.
    Regenerate {
        on: usize,
    },
}

fn wrote() -> impl proptest::strategy::Strategy<Value = Wrote> {
    use proptest::prelude::*;
    prop_oneof![
        4 => (any::<usize>(), proptest::option::of(0u8..4)).prop_map(|(on, key)| Wrote::Ask { on, key }),
        2 => any::<usize>().prop_map(|on| Wrote::Stop { on }),
        2 => (any::<usize>(), any::<usize>(), any::<bool>())
            .prop_map(|(of, on, regenerate)| Wrote::Again { of, on, regenerate }),
        2 => (0u8..4, any::<usize>()).prop_map(|(key, on)| Wrote::Rename { key, on }),
        2 => any::<bool>().prop_map(|form| Wrote::Open { form }),
        2 => any::<usize>().prop_map(|on| Wrote::AskTopic { on }),
        1 => any::<usize>().prop_map(|on| Wrote::Ahead { on }),
        1 => Just(Wrote::Orphan),
        1 => any::<bool>().prop_map(|form| Wrote::Reopen { form }),
        1 => Just(Wrote::SetUp),
        2 => any::<usize>().prop_map(|on| Wrote::Regenerate { on }),
    ]
}

/// The lines `script` writes over [`law_store`]: its door, and the lines, siblings and chains
/// alike.
fn written_beside(owed: u32, script: &[Wrote]) -> (Door, Vec<Posted>) {
    let (door, asked, answered) = law_store(owed);
    let mut default: Vec<Hash> = door.lines.iter().map(|(name, _)| name.clone()).collect();
    let mut questions: Vec<Hash> = door
        .lines
        .iter()
        .skip(1)
        .map(|(name, _)| name.clone())
        .collect();
    let mut topic: Vec<Hash> = Vec::new();
    let mut lines = Vec::new();
    for (k, wrote) in script.iter().enumerate() {
        let second = 30 + u32::try_from(k).expect("few lines");
        let pick = |from: &[Hash], at: usize| from[at % from.len()].clone();
        let line = match *wrote {
            Wrote::Ask { on, key: None } => door.line_on(
                "default",
                &question(second, "asked"),
                &[&pick(&default, on)],
            ),
            Wrote::Ask { on, key: Some(key) } => door.sealed_under(
                &question(second, "asked under a shared key"),
                key,
                pick(&default, on),
            ),
            Wrote::Stop { on } => door.line_on(
                "default",
                &Entry::Stopped {
                    at: at(second),
                    by: "device0".to_owned(),
                },
                &[&pick(&default, on)],
            ),
            Wrote::Again { of, on, regenerate } if !questions.is_empty() => door.line_on(
                "default",
                &again(
                    second,
                    &pick(&questions, of),
                    [Rewind::Edit, Rewind::Regenerate][usize::from(regenerate)],
                ),
                &[&pick(&default, on)],
            ),
            Wrote::Again { .. } => continue,
            Wrote::Rename { key, on } => door.sealed_under(
                &titled(second, "device0", "Garden"),
                key,
                pick(&default, on),
            ),
            Wrote::Open { form } => door.opening(&topic_opening(form)),
            Wrote::AskTopic { .. } if topic.is_empty() => continue,
            Wrote::AskTopic { on } => {
                let id = topic_id(&door);
                door.line_on(
                    &id,
                    &question(second, "and the trip?"),
                    &[&pick(&topic, on)],
                )
            }
            Wrote::Ahead { on } => door.line_on("default", &ahead(), &[&pick(&default, on)]),
            Wrote::Orphan => door.line_on(
                "default",
                &question(second, "on nothing held"),
                &[&Hash::new("f".repeat(64)).expect("a name")],
            ),
            Wrote::Reopen { form } => door.opening(&held_topic(form)),
            Wrote::Regenerate { on } => {
                let beside = pick(&default, on);
                let deps: Vec<&Hash> = if beside == answered {
                    vec![&answered]
                } else {
                    vec![&answered, &beside]
                };
                door.line_on("default", &again(second, &asked, Rewind::Regenerate), &deps)
            }
            Wrote::SetUp => door.opening(&gmail_setup()),
        };
        let name = line.name();
        match wrote {
            Wrote::Open { .. } | Wrote::AskTopic { .. } => topic.push(name),
            Wrote::Ask { .. } | Wrote::Ahead { .. } => {
                questions.push(name.clone());
                default.push(name);
            }
            Wrote::Stop { .. }
            | Wrote::Again { .. }
            | Wrote::Regenerate { .. }
            | Wrote::Rename { .. }
            | Wrote::Orphan => {
                default.push(name);
            }
            Wrote::Reopen { .. } | Wrote::SetUp => {}
        }
        lines.push(line);
    }
    (door, lines)
}

/// The store the post laws write over: `owed` of the box's questions in `default`, an answered
/// question beside them sealed under the renames' first key, and a tab's topic. The door, the
/// answered question, and its answer.
fn law_store(owed: u32) -> (Door, Hash, Hash) {
    let (mut door, _) = written_in_a_row(owed, &[]);
    let opening = door.default_opening();
    let (asked, _) = LineEvent::older(
        ConversationId::new("default").expect("a slug"),
        Actor::new("assistant").expect("an actor"),
        0,
        &question(8, "answered before"),
        &Given([200; 32]),
    )
    .expect("a line");
    let asked = door.hold(asked, &[&opening]);
    let began = door.box_wrote(&claim(8, &asked), &[&asked]);
    let answered = door.box_wrote(&answer(9), &[&began]);
    let (topic, _) = LineEvent::begin(ada(), &held_topic(true), &door.entropy).expect("a line");
    door.hold(topic, &[]);
    (door, asked, answered)
}

/// A question dated ten minutes past the box's clock.
fn ahead() -> Entry {
    Entry::Said {
        at: Datetime::new(2026, 11, 17, 9, 11, 0, 0).expect("an instant"),
        turn: Turn::Person {
            text: "ahead".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some(Stamp::Actor(Speaker::device("device0"))),
    }
}

/// An opening of the one topic every such opening of [`written_beside`] names, in one of two
/// forms.
fn topic_opening(form: bool) -> Entry {
    Entry::Opened {
        at: at(5),
        title: Words::kept(if form { "The garden" } else { "The trip" }.to_owned()),
        model: String::new(),
        kind: Kind::Topic,
    }
}

/// A Gmail setup's opening: every one of them at one instant names one id.
fn gmail_setup() -> Entry {
    use suzatary_connect::connect::TemplateId;
    Entry::SetUp {
        at: at(6),
        title: Words::kept(TemplateId::Gmail.template().called()),
        template: TemplateId::Gmail,
    }
}

/// The opening of the topic [`written_beside`]'s store holds, in one of two forms: one id.
fn held_topic(form: bool) -> Entry {
    Entry::Opened {
        at: at(7),
        title: Words::kept(if form { "The permit" } else { "The lease" }.to_owned()),
        model: String::new(),
        kind: Kind::Topic,
    }
}

/// The id every opening of [`written_beside`] names.
fn topic_id(door: &Door) -> String {
    let (line, _) = LineEvent::begin(ada(), &opened(5), &Given([1; 32])).expect("a line");
    let _ = door;
    line.conversation().as_str().to_owned()
}

/// `lines` in the order the box judges one post of them: parents first, ties by name.
fn canonical_order(lines: &[Posted]) -> Vec<usize> {
    let names: Vec<Hash> = lines.iter().map(Posted::name).collect();
    let parents = |k: usize| -> Vec<usize> {
        let Envelope::Change(change) = &lines[k].change else {
            return Vec::new();
        };
        change
            .deps
            .iter()
            .filter_map(|dep| names.iter().position(|n| n == dep))
            .collect()
    };
    let mut done: Vec<usize> = Vec::new();
    while done.len() < lines.len() {
        let next = (0..lines.len())
            .filter(|k| !done.contains(k) && parents(*k).iter().all(|p| done.contains(p)))
            .min_by_key(|k| names[*k].clone())
            .expect("a dag");
        done.push(next);
    }
    done
}

/// What a door holding `owed` of the box's questions takes of `lines` handed over in `posts`, as
/// a tab does: what is held back for now goes again with the next post and at the end. The door
/// after, and the names taken.
fn handed_over(
    owed: u32,
    script: &[Wrote],
    lines: &[Posted],
    posts: &[Vec<usize>],
) -> (Door, BTreeSet<Hash>) {
    let (mut door, _) = written_beside(owed, script);
    let mut taken = BTreeSet::new();
    let mut again: Vec<usize> = Vec::new();
    let mut hand = |door: &mut Door, these: Vec<usize>| -> Vec<usize> {
        if these.is_empty() {
            return these;
        }
        let received = door.post(&these.iter().map(|&k| &lines[k]).collect::<Vec<_>>());
        door.take(&received);
        these
            .into_iter()
            .filter(|&k| match said(&received, &lines[k]) {
                Ok(()) => {
                    taken.insert(lines[k].name());
                    false
                }
                Err((RefusalKind::Final | RefusalKind::Outdated, _)) => false,
                Err(_) => true,
            })
            .collect()
    };
    for post in posts {
        let these: Vec<usize> = again.iter().copied().chain(post.iter().copied()).collect();
        again = hand(&mut door, these);
    }
    // The box's clock moves past the lines dated ahead, and what waited goes again.
    door.world = &Moved;
    loop {
        let before = again.len();
        again = hand(&mut door, again);
        if again.is_empty() || again.len() == before {
            break;
        }
    }
    (door, taken)
}

/// The store's invariants: each conversation opened once, no key named by two lines, and no
/// conversation owing more than the bound, or than the box owed before (`owed`).
fn invariants(door: &Door, owed: u32) -> Result<(), String> {
    let mut openings: BTreeMap<String, usize> = BTreeMap::new();
    let mut keys: BTreeSet<suzatary_memory::secretary::locked_text::KeyName> = BTreeSet::new();
    for (_, object) in &door.lines {
        let Envelope::Change(change) = object else {
            continue;
        };
        if matches!(
            change.event.shape(),
            LineShape::Opened { .. } | LineShape::SetUp { .. }
        ) {
            *openings
                .entry(change.event.conversation().as_str().to_owned())
                .or_default() += 1;
        }
        for key in change.event.keys() {
            if !keys.insert(key.clone()) {
                return Err(format!("the key {key:?} is named twice"));
            }
        }
    }
    if let Some((id, n)) = openings.iter().find(|(_, n)| **n > 1) {
        return Err(format!("{id} is opened {n} times"));
    }
    let keyless = |_: &suzatary_memory::secretary::locked_text::KeyName| -> Words<TextKey> {
        suzatary_memory::secretary::words::Gone::Missing.into()
    };
    let genesis: Envelope<LineEvent> =
        Envelope::Genesis(mk_genesis("lines", &"0".repeat(32)).expect("a genesis"));
    let objects = door
        .lines
        .iter()
        .cloned()
        .chain([(seal_hash(&genesis), genesis)]);
    let read = suzatary_lines::secretary::transcript::of_objects(objects, &keyless)
        .map_err(|failed| failed.to_string())?;
    let bound = suzatary_conversations::secretary::queue::MAX_PENDING
        .max(usize::try_from(owed).expect("few"));
    for (id, transcript) in &read {
        let now = transcript.owed().len();
        if now > bound {
            return Err(format!("{id} owes {now}, past {bound}"));
        }
    }
    Ok(())
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(96))]

    /// THE POST LAWS, over generated posts of siblings and chains, openings of one id and of an id
    /// the store holds, setups, stops, edits and regenerates, renames and questions under a shared
    /// key and under a held line's, lines dated ahead (taken once the box's clock moves) and lines
    /// resting on nothing held: (i) a post takes exactly what its lines posted one by one in the
    /// box's order take; (ii) however they are cut into posts and the posts ordered, the store
    /// keeps its invariants. (iii), a writer's own chain taken the same however it is split, is
    /// [`the_lines_taken_do_not_depend_on_how_they_are_posted`]: over these siblings it does not
    /// hold, since of two conflicting lines the one judged first is taken.
    #[test]
    fn a_post_is_its_lines_one_at_a_time(
        owed in 0u32..18,
        script in proptest::collection::vec(wrote(), 1..14),
        cuts in proptest::collection::vec(proptest::prelude::any::<proptest::sample::Index>(), 0..4),
        turn in proptest::prelude::any::<proptest::sample::Index>(),
    ) {
        let (_, lines) = written_beside(owed, &script);
        proptest::prop_assume!(!lines.is_empty());
        let whole = handed_over(owed, &script, &lines, &[(0..lines.len()).collect()]);
        let one_by_one: Vec<Vec<usize>> = canonical_order(&lines).into_iter().map(|k| vec![k]).collect();
        let single = handed_over(owed, &script, &lines, &one_by_one);
        proptest::prop_assert_eq!(&whole.1, &single.1, "(i)");
        proptest::prop_assert_eq!(invariants(&whole.0, owed), Ok(()), "(ii) in one post");

        let mut at: Vec<usize> = cuts.iter().map(|cut| cut.index(lines.len() + 1)).collect();
        at.extend([0, lines.len()]);
        at.sort_unstable();
        at.dedup();
        let mut posts: Vec<Vec<usize>> = at.windows(2).map(|w| (w[0]..w[1]).rev().collect()).collect();
        let first = turn.index(posts.len());
        posts.rotate_left(first);
        let (door, _) = handed_over(owed, &script, &lines, &posts);
        proptest::prop_assert_eq!(invariants(&door, owed), Ok(()), "(ii) in {:?}", posts);
    }
}

/// A rename of `conversation` by the device, under keys of its own drawn from `seed`, resting on
/// `deps` under the genesis `genesis`.
fn renamed(conversation: &ConversationId, n: u8, genesis: &Hash, deps: Vec<Hash>) -> Posted {
    let (line, keys) = LineEvent::older(
        conversation.clone(),
        ada(),
        0,
        &titled(50 + u32::from(n), "device0", "Garden"),
        &Given([100 + n; 32]),
    )
    .expect("a line");
    Posted {
        change: Envelope::Change(mk_change(genesis.clone(), deps, line).expect("a change")),
        keys,
    }
}

/// What the store's own check reports of `history`, as a store a tab writes verifies.
fn findings(history: &Dag<LineEvent>) -> Vec<String> {
    suzatary_signatures::signatures::findings_of(history, &prodrome_data::policy::Untrusted::none())
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// `default` held three deep (its opening, then two lines of the box's, each on the one before),
/// and the opening of a second conversation: the door the laws below post to, and the names held.
fn deep() -> (Door, ConversationId, [Hash; 4]) {
    let mut door = Door::new();
    let opening = door.default_opening();
    let first = door.box_wrote(&titled(10, "assistant", "One"), &[&opening]);
    let second = door.box_wrote(&titled(11, "assistant", "Two"), &[&first]);
    let (begun, _) = LineEvent::begin(
        Actor::new("assistant").expect("an actor"),
        &opened(12),
        &door.entropy,
    )
    .expect("a line");
    let other = begun.begun_id();
    let garden = door.hold(begun, &[]);
    (door, other, [opening, first, second, garden])
}

/// Each finding the store's check makes of one change against the store it joins, and the door
/// refuses: a dep beneath another dep, a dep that is not a write to its entity, a genesis that is
/// no genesis the store holds, and a parent of another genesis. Each is refused for good in the
/// check's sentence, and the line the door takes beside them leaves the store reporting nothing.
#[test]
fn the_door_refuses_each_finding_of_one_change_in_the_check_s_words() {
    let (mut door, other, [opening, first, second, garden]) = deep();
    let default = ConversationId::new("default").expect("a slug");
    let genesis = door.genesis.clone();
    // A second prodrome in the store, with a line of its own.
    let elsewhere: Envelope<LineEvent> =
        Envelope::Genesis(mk_genesis("lines", &"1".repeat(32)).expect("a genesis"));
    let elsewhere_name = seal_hash(&elsewhere);
    door.history.extend([(elsewhere_name.clone(), elsewhere)]);
    let (line, _) = LineEvent::older(
        default.clone(),
        Actor::new("assistant").expect("an actor"),
        0,
        &titled(13, "assistant", "Three"),
        &door.entropy,
    )
    .expect("a line");
    let theirs = stored(&elsewhere_name, line, Vec::new());
    door.history.extend([theirs.clone()]);
    door.lines.push(theirs.clone());
    let before = findings(&door.history);
    assert_eq!(before, Vec::<String>::new(), "the store reports nothing");

    let redundant = renamed(&default, 0, &genesis, vec![second.clone(), first.clone()]);
    let foreign = renamed(&other, 1, &genesis, vec![garden.clone(), opening.clone()]);
    let stranger = renamed(&default, 2, &second, vec![second.clone()]);
    let crossing = renamed(
        &default,
        3,
        &genesis,
        vec![second.clone(), theirs.0.clone()],
    );
    let whole = renamed(&default, 4, &genesis, vec![second.clone()]);
    let received = door.post(&[&redundant, &foreign, &stranger, &crossing, &whole]);
    let found = |posted: &Posted, finding: &str| {
        let why = refused_finally(&received, posted);
        assert!(
            why.starts_with("the store's check would report it for ever once held: ")
                && why.contains(finding),
            "{why}"
        );
    };
    found(
        &redundant,
        &format!("depends on {first}, which its dep {second} already rests on"),
    );
    found(
        &foreign,
        &format!("depends on {opening}, which is not a write to its entity"),
    );
    found(
        &stranger,
        &format!("names genesis {second}, which the store does not hold"),
    );
    found(
        &crossing,
        &format!("rests on {}, of another genesis", theirs.0),
    );
    assert_eq!(said(&received, &whole), Ok(()), "{received:?}");
    door.take(&received);
    assert_eq!(
        findings(&door.history),
        before,
        "the store reports what it reported"
    );
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(128))]

    /// For any post the door takes, the store's findings after it are its findings before it,
    /// and each line it refuses as the check's the check reports once the store holds it: over
    /// generated posts of renames of either conversation, each resting on any set of the lines
    /// held and of those posted before it, under the store's genesis or a held line's name.
    #[test]
    fn the_store_reports_after_any_post_what_it_reported_before(
        drawn in proptest::collection::vec(
            (
                proptest::prelude::any::<bool>(),
                proptest::collection::btree_set(0usize..10, 0..4),
                proptest::prelude::any::<bool>(),
            ),
            1..6,
        ),
    ) {
        let (mut door, other, held) = deep();
        let default = ConversationId::new("default").expect("a slug");
        let before = findings(&door.history);
        let mut pool: Vec<Hash> = held.to_vec();
        let mut posted = Vec::new();
        for (n, (of_default, deps, stranger)) in drawn.into_iter().enumerate() {
            let n = u8::try_from(n).expect("few lines");
            let deps = deps.into_iter().filter_map(|i| pool.get(i).cloned()).collect();
            let genesis = if stranger { held[1].clone() } else { door.genesis.clone() };
            let line = renamed(if of_default { &default } else { &other }, n, &genesis, deps);
            pool.push(line.name());
            posted.push(line);
        }
        let received = door.post(&posted.iter().collect::<Vec<_>>());
        door.take(&received);
        proptest::prop_assert_eq!(findings(&door.history), before.clone(), "{:?}", received);
        for refused in &received.outcome.refused {
            let (Some(name), suzatary_api::sync::Reason::Reported(said)) = (&refused.name, &refused.reason) else {
                continue;
            };
            let line = posted.iter().find(|line| line.name() == *name).expect("posted");
            let mut held = door.history.clone();
            held.extend([(name.clone(), line.change.clone())]);
            proptest::prop_assert!(
                findings(&held).iter().any(|finding| finding.contains(name.as_str()) && !before.contains(finding)),
                "refused as {:?}, and the check reports nothing of it once held", said
            );
        }
    }
}
