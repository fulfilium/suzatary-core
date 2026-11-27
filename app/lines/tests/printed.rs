//! A line is stored as it has always been: the doors that take a line by its type (the plain
//! writer, an answer, an edit, an opening) print the bytes below for the same entry. Each print is
//! the line's clear shape and how many slots of words it locks, since the ciphertext is the lock's
//! and none of these doors moves it. A change of what a line stores fails here before it reaches
//! a store. `SUZATARY_UPDATE_PRINTED=1` rewrites the file after a change that means to move them.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use prodrome_data::event::{Actor, canonical};
use suzatary_base::capability::{Datetime, Entropy};
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::outcome::Outcome;
use suzatary_conversations::secretary::store::{
    Covered, Entry, Kind, Namer, Rewind, RunId, Scope, Title,
};
use suzatary_conversations::secretary::taint::Taint;
use suzatary_lines::secretary::answer::Answer;
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_lines::secretary::spoken::{Plain, Spoken};
use suzatary_memory::secretary::words::Words;

/// Each draw the next number, so the keys are fixed.
#[derive(Default)]
struct Counting(AtomicU64);

impl Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(0);
        bytes[..8].copy_from_slice(&self.0.fetch_add(1, Ordering::Relaxed).to_le_bytes());
        Ok(())
    }
}

fn at() -> Datetime {
    Datetime::new(2026, 11, 3, 9, 30, 0, 0).expect("an instant")
}

fn conversation() -> ConversationId {
    ConversationId::new("2026-11-03-1").expect("a slug")
}

fn ada() -> Actor {
    Actor::new("assistant").expect("an actor")
}

/// The plain entries the box writes, each beside its name.
fn plain() -> Vec<(&'static str, Plain)> {
    let said = |turn| Entry::Said {
        at: at(),
        turn,
        by: Some("ada".into()),
    };
    vec![
        (
            "opened",
            Entry::Opened {
                at: at(),
                title: Words::kept("The garden".to_owned()),
                model: "a-cli/small".to_owned(),
                kind: Kind::Topic,
            },
        ),
        (
            "titled",
            Entry::Titled {
                at: at(),
                title: Words::kept(Title::new("Garden").expect("a title")),
                by: Namer::Person {
                    actor: "ada".to_owned(),
                },
            },
        ),
        (
            "person",
            said(Spoken::person(
                "Is the permit due?".to_owned(),
                Vec::new(),
                Vec::new(),
            )),
        ),
        (
            "tool",
            said(Spoken::tool(
                "weather".into(),
                "sunny".to_owned(),
                Outcome::Answered,
                Vec::new(),
            )),
        ),
        (
            "act",
            said(Spoken::act("Ada dismissed a card".to_owned(), Vec::new())),
        ),
        (
            "stopped",
            Entry::Stopped {
                at: at(),
                by: "ada".to_owned(),
            },
        ),
        (
            "rewound",
            Entry::Rewound {
                to: 2,
                at: at(),
                by: "ada".to_owned(),
            },
        ),
        (
            "scoped",
            Entry::Scoped {
                at: at(),
                scope: Scope::Transcript,
            },
        ),
        (
            "began",
            Entry::Began {
                at: at(),
                covers: vec![Covered {
                    line: prodrome_data::event::Hash::new("a".repeat(64)).expect("a name"),
                    taint: Taint::Clean,
                }],
                run: RunId::of([3; 16]),
            },
        ),
        (
            "again",
            Entry::Again {
                at: at(),
                question: prodrome_data::event::Hash::new("b".repeat(64)).expect("a name"),
                how: Rewind::Regenerate,
                by: "ada".to_owned(),
            },
        ),
    ]
}

/// `line`'s print up to its words, with each name a hash and how many slots of words follow.
fn clear(line: &LineEvent) -> String {
    let print = canonical(line);
    let shape = print.split("words=(").next().unwrap_or_default();
    let mut out = String::new();
    let mut run = String::new();
    for c in shape.chars().chain(std::iter::once('\n')) {
        if c.is_ascii_hexdigit() && !c.is_ascii_uppercase() {
            run.push(c);
            continue;
        }
        out.push_str(if run.len() >= 32 { "<hash>" } else { &run });
        run.clear();
        out.push(c);
    }
    out.pop();
    format!("{out}words={}", print.matches("Scrubbed(").count())
}

/// Every line the typed doors write, one per row: its name, then its print.
fn printed() -> String {
    let entropy = Counting::default();
    let mut out = String::new();
    for (held, (name, entry)) in (0u32..).zip(plain()) {
        let (line, _) =
            LineEvent::write(conversation(), ada(), held, &entry, &entropy).expect("a plain line");
        let _ = writeln!(out, "{name}\t{}", clear(&line));
    }
    let question = Entry::Said {
        at: at(),
        turn: Spoken::person(
            "Is the permit due, edited?".to_owned(),
            Vec::new(),
            Vec::new(),
        ),
        by: Some("ada".into()),
    };
    let (edit, _) =
        LineEvent::edited(conversation(), ada(), 20, &question, &entropy).expect("an edit");
    let _ = writeln!(out, "edited\t{}", clear(&edit));
    let (opening, _) = LineEvent::begin(
        ada(),
        &Entry::Opened {
            at: at(),
            title: Words::kept("A new topic".to_owned()),
            model: String::new(),
            kind: Kind::Topic,
        },
        &entropy,
    )
    .expect("an opening");
    let _ = writeln!(out, "begun\t{}", clear(&opening));
    for (held, (name, carries)) in (30u32..).zip([
        ("answer clean", Taint::Clean),
        ("answer tainted", Taint::Tainted),
    ]) {
        let answer = Answer::replied(at(), "It is due.".to_owned(), Vec::new(), carries);
        let (line, _) =
            LineEvent::answered(conversation(), ada(), held, &answer, &entropy).expect("an answer");
        let _ = writeln!(out, "{name}\t{}", clear(&line));
    }
    out
}

#[test]
fn every_line_the_typed_doors_write_prints_as_stored() {
    let out = printed();
    if std::env::var_os("SUZATARY_UPDATE_PRINTED").is_some() {
        std::fs::write(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/printed.golden"),
            &out,
        )
        .expect("updates");
    }
    assert_eq!(out, include_str!("printed.golden"));
}
