//! The agent's first answer leaves a fifth of its manual's cap free, so `OPERATOR.md` can grow
//! without a silent cut, and the whole answer, the secretary's memory included, stays within one
//! budget however full the memory is: 200 facts of the most characters a fact may hold and a
//! record longer than the budget. Weighed with this box's persona, categories and
//! `[agent] memory_bytes` where there is an instance file, and otherwise with a stand-in of the
//! same shape; skipped where the checkout's manual is not present (a build sandbox is handed
//! `app/` alone). Beside it, a `zoom` into the same record shows no line longer than a line may
//! be shown, however long the line was said.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;

use prodrome_core::policy::Untrusted;
use suzatary_core::agent::{CONTEXT_CAP, CONTEXT_HEADROOM, context, manual};
use suzatary_core::capability::Turn;
use suzatary_core::chains::{ChainName, ChainPolicy, Chains};
use suzatary_core::deployment::Deployment;
use suzatary_core::persona::Persona;
use suzatary_core::secretary::compaction::Reading;
use suzatary_core::secretary::conversations::InMemory as Lines;
use suzatary_core::secretary::digest_event::DigestEvent;
use suzatary_core::secretary::digest_view::LINE_BYTES;
use suzatary_core::secretary::digests::ReadDigests;
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::memory::{
    Fact, FactId, Kind, MAX_FACT_CHARS, MAX_LIVE, Memory, Origin, Remembered,
};
use suzatary_core::secretary::shown_record::OwnersRecord;
use suzatary_core::secretary::store::{ConversationId, Entry};
use suzatary_core::store::{Doc as _, InMemory, InMemoryDoc, Prodrome as _};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The scale's seven anchors and a dozen categories, about what a deployment declares.
fn stand_in() -> (Persona, Deployment) {
    let persona = Persona {
        name: "Ada".to_owned(),
        city: "London".to_owned(),
        zone: suzatary_core::zone::Zone::UTC,
        prose: suzatary_core::persona::Prose {
            news_filter: Vec::new(),
            email_voice: Vec::new(),
            anchors: [
                "10%: an emergency where life is at stake this hour",
                "20%: an irreversible legal or financial blow that lands tomorrow",
                "35-45%: a real consequence within weeks, such as a government filing",
                "50-65%: work that matters this month",
                "70-85%: errands, forms and habits",
                "90%+: parked, someday",
                "a deadline bounds when, never the stakes",
            ]
            .map(str::to_owned)
            .to_vec(),
        },
    };
    let deployment = Deployment {
        viewers: BTreeSet::new(),
        actors: BTreeMap::new(),
        chains: Chains::of(vec![ChainPolicy {
            name: ChainName::new("life").expect("a slug"),
            untrusted: Untrusted::of([]),
            roster: BTreeSet::new(),
            root: suzatary_core::cast::keys::root(),
        }])
        .expect("one chain"),
        categories: (0..12u32)
            .map(|n| (format!("category-{n:02}"), n))
            .collect(),
        app_url: "https://example.invalid/".to_owned(),
        coding: None,
        zone: suzatary_core::zone::Zone::UTC,
    };
    (persona, deployment)
}

fn ada() -> Actor {
    Actor::new("ada").expect("an actor")
}

fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 24, 9 + minute / 60, minute % 60, 0, 0).expect("an instant")
}

/// The most facts the memory keeps, each of the most characters a fact holds, every one of
/// them a quote or a two-byte letter, so escaping costs what it can.
fn full_memory() -> Memory {
    let words: String = "\"é".repeat(MAX_FACT_CHARS / 2);
    Memory {
        facts: (1..=u64::try_from(MAX_LIVE).expect("small"))
            .map(|id| Remembered {
                id: FactId(id),
                at: at(0),
                fact: Fact::new(words.clone()).expect("a fact"),
                kind: Kind::Standing,
                said: suzatary_core::secretary::provenance::Provenance::said(Origin::Turn {
                    conversation: ConversationId::new("a").expect("a slug"),
                    turn: 0,
                }),
            })
            .collect(),
        forgotten: Vec::new(),
        merged: Vec::new(),
        version: u64::try_from(MAX_LIVE).expect("small"),
    }
}

/// How many lines [`full_record`] holds.
const RECORD_LINES: u32 = 600;

/// A record of the owner's turns longer than any budget here, its last turn alone 25 times
/// longer than a line may be shown.
fn full_record(lines: &Lines) -> Reading {
    let id = ConversationId::new("long").expect("a slug");
    for minute in 0..RECORD_LINES {
        let said = if minute + 1 == RECORD_LINES {
            "x".repeat(25 * LINE_BYTES)
        } else {
            "and then some more of what I said ".repeat(4)
        };
        let entry = Entry::Said {
            at: at(minute),
            turn: Turn::Person {
                text: format!("Turn {minute}: {said}"),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        };
        let (line, keys) =
            LineEvent::write(id.clone(), ada(), minute, &entry, &lines.entropy).expect("a line");
        for key in keys {
            lines.keys.set(&key.name(), &key).expect("kept");
        }
        lines.lines.append(line).expect("said");
    }
    let owners: BTreeSet<Actor> = [ada()].into_iter().collect();
    let history = InMemory::<DigestEvent>::default();
    let texts = InMemoryDoc::default();
    Reading::of(&ReadDigests {
        history: &history,
        texts: &texts,
        lines: lines.conversations().reads(),
        owners: &owners,
        compacting: &suzatary_core::secretary::compacting::Compacting::Off,
    })
    .expect("the record reads")
}

/// `zoom` opens any part of the record, the longest line included, with no line of its answer
/// longer than [`LINE_BYTES`] and its cut note.
#[test]
fn a_zoom_answers_no_line_longer_than_a_line_may_be() {
    let lines = Lines::default();
    let record = OwnersRecord::of(full_record(&lines));
    let most = LINE_BYTES + " … (102400 bytes more)".len();
    for first in 0..RECORD_LINES {
        for count in [1, 16.min(RECORD_LINES - first), RECORD_LINES - first] {
            let answer = record
                .zoom(&format!("{first}+{count}"))
                .expect("a handle the record spans");
            for part in answer["beneath"].as_array().expect("parts") {
                let longest = part
                    .as_object()
                    .expect("a part")
                    .values()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::len)
                    .max()
                    .unwrap_or(0);
                assert!(
                    longest <= most,
                    "{first}+{count}: a line of {longest} bytes"
                );
            }
        }
    }
}

#[test]
fn the_agents_context_leaves_a_fifth_of_its_cap_for_the_manual_and_holds_a_full_memory() {
    let repo = repo_root();
    if !repo.join("OPERATOR.md").is_file() {
        eprintln!("skipped: no OPERATOR.md beside app/ (a sandbox holds app/ alone)");
        return;
    }
    let (persona, deployment, memory_bytes) = crate::this_box::instance().map_or_else(
        || {
            let (persona, deployment) = stand_in();
            (
                persona,
                deployment,
                suzatary_cli::instance::Triage::default().memory_bytes,
            )
        },
        |instance| {
            (
                instance.persona.clone(),
                instance.deployment.clone(),
                instance.agent.memory_bytes,
            )
        },
    );
    let handbook = suzatary_io::manuals::Handbook::new(&repo, persona);
    let wire =
        serde_json::to_string_pretty(&manual(&handbook, &deployment)).expect("it serialises");
    let budget = CONTEXT_CAP - CONTEXT_HEADROOM;
    assert!(
        wire.len() <= budget,
        "the manual answers {} bytes, past the {budget} that leave {CONTEXT_HEADROOM} of the \
         {CONTEXT_CAP}-byte cap free; carry less inline or tighten OPERATOR.md",
        wire.len()
    );
    assert!(!wire.contains("cut here"), "the manual was cut");

    let lines = Lines::default();
    let record = full_record(&lines);
    let answer = context(
        &handbook,
        &deployment,
        Ok(&full_memory()),
        Ok(&OwnersRecord::of(record)),
        memory_bytes,
    );
    let whole = serde_json::to_string_pretty(&answer).expect("it serialises");
    assert!(
        whole.len() <= CONTEXT_CAP + memory_bytes,
        "context answers {} bytes, past its {} with a full memory",
        whole.len(),
        CONTEXT_CAP + memory_bytes
    );
    assert!(
        !whole.contains("cut here"),
        "the manual was cut beside the memory"
    );
    let memory = answer["memory"].as_str().expect("the memory, as text");
    assert!(
        memory.contains(&format!(
            "oldest of the {MAX_LIVE} facts are not shown here: recall(query)"
        )),
        "{memory:.400}"
    );
    assert!(
        memory.contains(&format!("fact {MAX_LIVE} (standing)")),
        "the newest fact is kept"
    );
    println!(
        "context: the manual {} bytes of {CONTEXT_CAP}, {} free ({}%); with a full memory {} \
         of {}",
        wire.len(),
        CONTEXT_CAP - wire.len(),
        (CONTEXT_CAP - wire.len()) * 100 / CONTEXT_CAP,
        whole.len(),
        CONTEXT_CAP + memory_bytes
    );
}
