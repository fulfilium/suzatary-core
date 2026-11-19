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
//! - any shape the box alone writes.
//!
//! The same object posted again is answered `already` and changes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU8, Ordering};

use prodrome_core::change::mk_change;
use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::genesis::mk_genesis;
use suzatary_api::sync::{Holding, Received};
use suzatary_base::capability::{Datetime, Entropy};
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Entry, Kind, Namer, Title};
use suzatary_held::redact::Rules;
use suzatary_lines::secretary::line_event::LineEvent;
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
    names: BTreeSet<Hash>,
    /// Every line the store holds: `default`'s opening, and what it took.
    lines: Vec<LineEvent>,
    entropy: Counting,
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
            &[&suzatary_secretary::stores::phone_trusted(&["device0"])],
            &suzatary_connect::cast::keys::root(),
        )
        .expect("reads")
    });

impl Door {
    fn new() -> Self {
        let genesis: Envelope<LineEvent> =
            Envelope::Genesis(mk_genesis("lines", &"0".repeat(32)).expect("a genesis"));
        let genesis = seal_hash(&genesis);
        let entropy = Counting(AtomicU8::new(1));
        let (default, _) = LineEvent::write(
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
        Self {
            names: [genesis.clone()].into(),
            genesis,
            lines: vec![default],
            entropy,
        }
    }

    fn change(&self, line: LineEvent, keys: Vec<TextKey>) -> Posted {
        let change = mk_change(self.genesis.clone(), Vec::new(), line).expect("a change");
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

    /// A line of `conversation` as a tab seals it.
    fn line(&self, conversation: &str, entry: &Entry) -> Posted {
        let id = ConversationId::new(conversation).expect("a slug");
        let (line, keys) = LineEvent::write(id, ada(), 0, entry, &self.entropy).expect("a line");
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
        let held = <LinesRule as suzatary_store::admission::Admission>::held(self.lines.iter());
        suzatary_api::sync::receive_changes::<LinesRule>(
            &Holding {
                names: &self.names,
                held: &held,
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
            self.names.insert(name.clone());
            let envelope: Envelope<LineEvent> = prodrome_core::event::parse_envelope(
                &suzatary_lines::secretary::line_event::LineVocabulary,
                print,
            )
            .expect("parses");
            if let Some(event) = envelope.into_event() {
                self.lines.push(event);
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
    let rename = door.line(
        conversation.as_str(),
        &titled(2, "device0", "The zephyr filing"),
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

    // A shape the box alone writes.
    let spoken = door.line("default", &unscrubbed);
    assert!(refused_finally(&door.post(&[&spoken]), &spoken).contains("the box alone writes"));
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
    let door = Door::new();
    let (held, _) = LineEvent::write(
        default(),
        Actor::new("assistant").expect("an actor"),
        1,
        &titled(29, "device0", "Invoices"),
        &Given(key),
    )
    .expect("a line");
    let door = Door {
        lines: [door.lines.clone(), vec![held]].concat(),
        ..door
    };
    let (line, keys) = LineEvent::write(
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
            Some(id) => LineEvent::write(
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
