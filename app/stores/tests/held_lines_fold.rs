//! The held lines' laws, over the conversations held in core's memory.

use prodrome_data::event::Hash;
use prodrome_data::event::canonical_envelope;
use prodrome_data::literal::Datetime;
use std::collections::{BTreeMap, BTreeSet};
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model;
use suzatary_conversations::secretary::store::Kind;
use suzatary_conversations::secretary::store::{Entry, Stored};
use suzatary_lines::secretary::held_lines::*;
use suzatary_lines::secretary::rescrub;
use suzatary_lines::secretary::spoken::Spoken;
use suzatary_lines::secretary::transcript;
use suzatary_memory::secretary::locked_text::KeyName;
use suzatary_memory::secretary::words::Words;
use suzatary_store::store::Prodrome as _;
use suzatary_store::store::Reads as _;
use suzatary_store::store::{Doc as _, Gets as _};
use suzatary_stores::stores::conversations::InMemory;

/// Every key `held` keeps, spelled as a replica is given it.
fn given(held: &InMemory) -> BTreeMap<KeyName, String> {
    held.keys
        .names()
        .into_iter()
        .map(|name| {
            let key = held.keys.get(&name).expect("reads").expect("held");
            (name, suzatary_memory::secretary::key_ring::spelled(&key))
        })
        .collect()
}

/// Every object `held` keeps, printed as a replica is given it.
fn prints(held: &InMemory) -> Vec<(Hash, Vec<u8>)> {
    held.lines
        .objects()
        .expect("objects")
        .into_iter()
        .map(|(name, object)| (name, canonical_envelope(&object).into_bytes()))
        .collect()
}

fn opened<T>(title: &str) -> Entry<T> {
    Entry::Opened {
        at: Datetime::new(2026, 10, 1, 9, 0, 0, 0).expect("an instant"),
        title: Words::kept(title.to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Topic,
    }
}

/// Opening held lines parses their words and scrubs none: the scrub ran once, when each line
/// was written.
#[test]
fn opening_held_lines_scrubs_nothing() {
    let held = InMemory::default();
    let id = ConversationId::new("2026-10-01-1").expect("a slug");
    transcript::start(held.conversations(), &id, &opened("Notes, café")).expect("started");
    for n in 0..40 {
        let said = Entry::Said {
            at: Datetime::new(2026, 10, 1, 9, 1, n, 0).expect("an instant"),
            turn: Spoken::person(
                format!("Merci, voilà la réponse {n} — 確認"),
                Vec::new(),
                Vec::new(),
            ),
            by: Some("ada".into()),
        };
        transcript::push(held.conversations(), &id, &said).expect("pushed");
    }
    let mut lines = HeldLines::new();
    let before = suzatary_held::redact::scrubs();
    assert!(before > 40, "each write scrubbed its words");
    lines.keys(&given(&held), &[]);
    lines.hold(prints(&held), &[]);
    let read = lines.read().0.expect("reads");
    assert_eq!(read[0].turns.len(), 40, "every line opened");
    assert_eq!(suzatary_held::redact::scrubs(), before, "and none scrubbed");
}

/// A replica reads a conversation whose stale slots were sealed again, and whose replaced
/// keys are gone, as the box reads it: each slot from the reseal that stands for it.
#[test]
fn a_resealed_conversation_reads_as_the_box_reads_it() {
    use suzatary_lines::secretary::line_event::LineEvent;
    use suzatary_memory::secretary::scrubbed_under::ScrubbedUnder;
    let held = InMemory::default();
    let id = ConversationId::new("2026-10-25-1").expect("a slug");
    let at = |second: u32| Datetime::new(2026, 10, 25, 9, 0, second, 0).expect("an instant");
    let said = Entry::Said {
        at: at(1),
        turn: model::Turn::Person {
            text: "reset at https://example.test/r?token=s3cret".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some("ada".into()),
    };
    for (count, entry) in [opened("Notes"), said].iter().enumerate() {
        let (line, keys) = LineEvent::written_under(
            &ScrubbedUnder::Unversioned,
            id.clone(),
            transcript::writer(),
            u32::try_from(count).expect("a count"),
            entry,
            &held.entropy,
        )
        .expect("a line");
        for key in &keys {
            held.keys.set(&key.name(), key).expect("kept");
        }
        held.lines.append(line).expect("appended");
    }
    let done = rescrub::run(held.conversations(), at(9), rescrub::BATCH).expect("runs");
    assert_eq!((done.resealed, done.destroyed), (2, 2));
    let boxed = transcript::read(held.conversations(), &id)
        .expect("reads")
        .stored()
        .clone();
    let mut lines = HeldLines::new();
    lines.keys(&given(&held), &[]);
    lines.hold(prints(&held), &[]);
    let read = lines.read().0.expect("reads");
    assert_eq!(*read[0], boxed);
    assert!(!format!("{boxed:?}").contains("s3cret"), "{boxed:?}");
    assert_eq!(
        lines.dead(&|_| true).len(),
        2,
        "a replica forgets each key a reseal replaced"
    );
}

/// A reader that still holds the keys from before the sweep, each replaced seal's beside its
/// reseal's, the box's or a replica's, reads every slot from its reseal and scrubs nothing; the
/// replica's sweep then leaves it the box's keys, and it reads the same.
#[test]
fn a_replica_holding_the_replaced_keys_reads_the_reseals_and_scrubs_nothing() {
    use suzatary_lines::secretary::line_event::LineEvent;
    use suzatary_memory::secretary::scrubbed_under::ScrubbedUnder;
    let held = InMemory::default();
    let at = |second: u32| Datetime::new(2026, 10, 25, 9, 0, second, 0).expect("an instant");
    for id in ["2026-10-25-1", "2026-10-25-2"] {
        let id = ConversationId::new(id).expect("a slug");
        let mut entries = vec![opened("Notes")];
        entries.extend((1..8).map(|n| Entry::Said {
            at: at(n),
            turn: model::Turn::Person {
                text: format!("reset {n} at https://example.test/r?token=s3cret — 確認"),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        }));
        for (count, entry) in entries.iter().enumerate() {
            let (line, keys) = LineEvent::written_under(
                &ScrubbedUnder::Unversioned,
                id.clone(),
                transcript::writer(),
                u32::try_from(count).expect("a count"),
                entry,
                &held.entropy,
            )
            .expect("a line");
            for key in &keys {
                held.keys.set(&key.name(), key).expect("kept");
            }
            held.lines.append(line).expect("appended");
        }
    }
    assert_eq!(
        rescrub::reseal(held.conversations(), at(9)).expect("reseals"),
        16
    );
    let before = given(&held);
    let read_by_box = || -> Vec<Stored> {
        transcript::each(held.conversations())
            .expect("reads")
            .into_iter()
            .map(|(_, read)| read.expect("folds").stored().clone())
            .collect()
    };
    let scrubs = suzatary_held::redact::scrubs();
    let unswept = read_by_box();
    assert_eq!(
        suzatary_held::redact::scrubs(),
        scrubs,
        "the box, before its sweep, opens no replaced seal"
    );
    assert_eq!(transcript::sweep(held.conversations()), Ok(16));
    let boxed: BTreeSet<KeyName> = held.keys.names().into_iter().collect();
    let read_by_box = read_by_box();
    assert_eq!(read_by_box, unswept);
    let scrubs = suzatary_held::redact::scrubs();
    let mut lines = HeldLines::new();
    lines.keys(&before, &[]);
    lines.hold(prints(&held), &[]);
    let read = |lines: &mut HeldLines| -> Vec<Stored> {
        let read = lines.read().0.expect("reads");
        read.iter().map(|stored| (**stored).clone()).collect()
    };
    assert_eq!(read(&mut lines), read_by_box);
    assert_eq!(
        suzatary_held::redact::scrubs(),
        scrubs,
        "no replaced seal is opened"
    );
    let dead: Vec<KeyName> = lines
        .dead(&|key| lines.key_ring().holds(key))
        .into_iter()
        .collect();
    lines.keys(&BTreeMap::new(), &dead);
    let kept: BTreeSet<KeyName> = before
        .into_keys()
        .filter(|key| !dead.contains(key))
        .collect();
    assert_eq!(kept, boxed, "the replica's sweep leaves it the box's keys");
    assert_eq!(read(&mut lines), read_by_box);
    assert_eq!(suzatary_held::redact::scrubs(), scrubs);
}

/// A key taken out takes what was opened with it out of the cache at once, before any reading,
/// the rounds read on their own included.
#[test]
fn a_key_taken_out_leaves_no_words_it_opened() {
    let held = InMemory::default();
    let id = ConversationId::new("2026-10-01-1").expect("a slug");
    transcript::start(
        held.conversations(),
        &id,
        &opened("a title nobody else says"),
    )
    .expect("started");
    let mut lines = HeldLines::new();
    let given = given(&held);
    lines.keys(&given, &[]);
    lines.hold(prints(&held), &[]);
    let names: BTreeSet<Hash> = held
        .lines
        .objects()
        .expect("objects")
        .into_iter()
        .filter(|(_, object)| object.event().is_some())
        .map(|(name, _)| name)
        .collect();
    assert!(matches!(
        lines.part(&id, Part::Round(0), &names),
        Held::Opened(_)
    ));
    assert!(lines.read().0.is_ok());
    let says = "a title nobody else says";
    assert!(
        suzatary_lines::secretary::held_lines::says(&mut lines, says),
        "the reading opened the title"
    );
    let gone: Vec<KeyName> = given.into_keys().collect();
    lines.keys(&BTreeMap::new(), &gone);
    assert!(
        !suzatary_lines::secretary::held_lines::says(&mut lines, says),
        "nothing cached still says it"
    );
}

/// A measurement, run by hand with `--ignored --nocapture`: reading 1,200 rounds of mail-like
/// words (30 conversations of 40), once sealed under the current rules and once under none,
/// which every reader then scrubs as it opens.
#[test]
#[ignore = "a measurement, run by hand"]
fn bench_reading_1200_rounds() {
    use suzatary_lines::secretary::line_event::LineEvent;
    use suzatary_memory::secretary::scrubbed_under::ScrubbedUnder;
    let words = "Your order #48213 shipped on 2026-10-25 and arrives by 10/28/2026. Call us at \
                 617-555-1234 between 9:00 and 17:30. Total due: $1,532.40 (tax 212.18). \
                 Le rendez-vous est confirmé pour le 12/11 à 14h30. Savings      2024      1500.\n"
        .repeat(12);
    let at = |n: u32| Datetime::new(2026, 10, 25, 9, n / 60, n % 60, 0).expect("an instant");
    for under in [
        ScrubbedUnder::Version(suzatary_held::redact::Rules::current(()).clone()),
        ScrubbedUnder::Unversioned,
    ] {
        let held = InMemory::default();
        let ids: Vec<ConversationId> = (0..30)
            .map(|n| ConversationId::new(format!("2026-10-25-{n}")).expect("a slug"))
            .collect();
        for id in &ids {
            let mut entries = vec![opened("Notes")];
            entries.extend((1..=40).map(|n| Entry::Said {
                at: at(n),
                turn: model::Turn::Person {
                    text: words.clone(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            }));
            for (count, entry) in entries.iter().enumerate() {
                let (line, keys) = LineEvent::written_under(
                    &under,
                    id.clone(),
                    transcript::writer(),
                    u32::try_from(count).expect("a count"),
                    entry,
                    &held.entropy,
                )
                .expect("a line");
                for key in &keys {
                    held.keys.set(&key.name(), key).expect("kept");
                }
                held.lines.append(line).expect("appended");
            }
        }
        let start = std::time::Instant::now();
        for id in &ids {
            transcript::read(held.conversations(), id).expect("reads");
        }
        eprintln!("read 1200 rounds sealed {under:?}: {:?}", start.elapsed());
    }
}
