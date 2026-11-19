//! A new conversation's id is a function of its opening line (`LineEvent::begun_id`): of the
//! line's clear fields alone, never of its conversation or its words, and never an id the box
//! mints by day or by name.

use std::sync::atomic::{AtomicU64, Ordering};

use prodrome_core::event::Actor;
use prodrome_core::schema::Schema as _;
use proptest::prelude::*;
use suzatary_base::capability::{Datetime, Entropy};
use suzatary_base::conversation_id::{ConversationId, DEFAULT, Daily, SETUP};
use suzatary_conversations::secretary::store::{Entry, Kind};
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_memory::secretary::words::Words;

/// Each draw the next number, so two seals of the same words differ.
#[derive(Default)]
struct Counting(AtomicU64);

impl Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(0);
        bytes[..8].copy_from_slice(&self.0.fetch_add(1, Ordering::Relaxed).to_le_bytes());
        Ok(())
    }
}

/// An opening line's clear fields, and its title.
#[derive(Debug, Clone)]
struct Opening {
    second: u32,
    micro: u32,
    actor: String,
    model: String,
    kind: u8,
    title: String,
}

fn opening() -> impl Strategy<Value = Opening> {
    (
        0u32..60,
        0u32..1_000_000,
        "[a-z]{1,8}",
        "[a-z]{1,8}",
        0u8..3,
        "[A-Za-z ]{1,20}",
    )
        .prop_map(|(second, micro, actor, model, kind, title)| Opening {
            second,
            micro,
            actor,
            model,
            kind,
            title,
        })
}

fn entry(o: &Opening) -> Entry {
    let kind = match o.kind {
        0 => Kind::Default,
        1 => Kind::Topic,
        _ => Kind::Review,
    };
    Entry::Opened {
        at: Datetime::new(2026, 11, 15, 8, 0, o.second, o.micro).expect("an instant"),
        title: Words::kept(o.title.clone()),
        model: o.model.clone(),
        kind,
    }
}

fn written(o: &Opening, conversation: &ConversationId) -> LineEvent {
    let actor = Actor::new(o.actor.clone()).expect("an actor");
    LineEvent::write(
        conversation.clone(),
        actor,
        0,
        &entry(o),
        &Counting::default(),
    )
    .expect("a line")
    .0
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// The id is a function of the clear fields alone: written under any conversation, sealed
    /// under any keys, with any title, one opening has one id; `begin` seals the line under
    /// that id; and a change to any clear field changes it.
    #[test]
    fn law_a_conversation_s_id_is_its_opening_line_s_clear_fields(o in opening(), other in "[a-z]{1,8}") {
        let a = written(&o, &DEFAULT);
        let b = written(&Opening { title: format!("{} again", o.title), ..o.clone() }, &SETUP);
        prop_assert_eq!(a.begun_id(), b.begun_id(), "not of the conversation, nor of the words");
        let actor = Actor::new(o.actor.clone()).expect("an actor");
        let (begun, _) = LineEvent::begin(actor, &entry(&o), &Counting::default()).expect("a line");
        prop_assert_eq!(begun.key(), &a.begun_id(), "the line is sealed under its own id");
        prop_assert_eq!(begun.begun_id(), a.begun_id());
        let moved = [
            Opening { micro: (o.micro + 1) % 1_000_000, ..o.clone() },
            Opening { actor: format!("{}{other}", o.actor), ..o.clone() },
            Opening { model: format!("{}{other}", o.model), ..o.clone() },
            Opening { kind: (o.kind + 1) % 3, ..o.clone() },
        ];
        for m in &moved {
            prop_assert_ne!(written(m, &DEFAULT).begun_id(), a.begun_id(), "{:?}", m);
        }
    }

    /// No opening line's id is one the box mints: a day's daily rounds, a dated id of the form
    /// the box once minted, `default` or `setup`. An id is 32 lowercase hex digits with no `-`.
    #[test]
    fn law_no_conversation_s_id_is_one_the_box_mints(o in opening(), day in 0u32..400, n in 1u32..50) {
        let id = written(&o, &DEFAULT).begun_id();
        let id = id.as_str();
        prop_assert!(id.len() == 32 && id.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)), "{}", id);
        let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("a day") + chrono::Days::new(u64::from(day));
        let minted = [
            ConversationId::daily(Daily::Review, date),
            ConversationId::daily(Daily::Sweep, date),
            ConversationId::new(format!("{}-{n}", date.format("%Y-%m-%d"))).expect("a dated id"),
            DEFAULT.clone(),
            SETUP.clone(),
        ];
        for box_id in &minted {
            prop_assert_ne!(id, box_id.as_str());
        }
    }
}
