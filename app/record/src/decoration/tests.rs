//! The block as a property over generated rows, which draw the awkward cases on
//! purpose: every markup special, notes on every site, empty and full optional
//! fields, lines at the renderer's 88-column boundary.

use std::collections::BTreeMap;

use proptest::prelude::*;

use crate::record::{MarkupSource, Note, NoteSite, Source, StringSource, SubTodo, mk_subtodo};
use prodrome_core::event::{Actor, Name, TodoId};
use prodrome_core::fpl;
use prodrome_core::fpl::Outcome;
use prodrome_core::literal::Datetime;

use super::*;
use suzatary_base::escape::{escape_markup, escape_string};

fn day(year: i32, month: u32, of: u32) -> Datetime {
    Datetime::new(year, month, of, 0, 0, 0, 0).expect("a real day")
}

// --- generators ----------------------------------------------------------------

/// Prose an attacker might have written; markup is drawn as its `escape_markup`
/// image, because that is the only shape stored markup has.
fn prose() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-zA-Z0-9 .,;:!?()\\[\\]#$@*_<>`\\\\\"'—–…\n]{1,120}")
        .expect("a fixed pattern")
}

fn markup() -> impl Strategy<Value = MarkupSource> {
    prose().prop_map(|text| MarkupSource::escaped(&text))
}

fn quoted() -> impl Strategy<Value = StringSource> {
    prose().prop_map(|text| StringSource::new(escape_string(&text).as_str()))
}

/// A `//` run, trimmed, as the parser always hands a note's lines over.
fn lines() -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec(
        proptest::string::string_regex("[a-zA-Z0-9 .,;:()-]{0,60}")
            .expect("a fixed pattern")
            .prop_map(|line| line.trim().to_owned()),
        1..4,
    )
}

fn datetime() -> impl Strategy<Value = Datetime> {
    (2024i32..2028, 1u32..13, 1u32..29).prop_map(|(y, m, d)| day(y, m, d))
}

fn subtodos() -> impl Strategy<Value = Vec<SubTodo>> {
    proptest::collection::vec(
        (prose(), any::<bool>()).prop_map(|(text, done)| {
            mk_subtodo(&format!("[{}]", escape_markup(&text)), done).expect("a nonempty body")
        }),
        0..4,
    )
}

fn source() -> impl Strategy<Value = Option<Source>> {
    proptest::option::of(
        (quoted(), quoted(), datetime(), quoted(), quoted(), quoted()).prop_map(
            |(sender, subject, date, hash, thread_id, message_id)| Source {
                sender,
                subject,
                date,
                hash,
                thread_id,
                message_id,
            },
        ),
    )
}

fn notes() -> impl Strategy<Value = Vec<Note>> {
    proptest::collection::vec(
        (proptest::sample::select(NoteSite::ALL.to_vec()), lines())
            .prop_map(|(on, lines)| Note { on, lines }),
        0..4,
    )
    .prop_map(|drawn| {
        // One note per site: `raise_block` keeps the last of duplicates.
        let mut seen: BTreeMap<NoteSite, Note> = BTreeMap::new();
        for note in drawn {
            seen.insert(note.on, note);
        }
        let mut notes: Vec<Note> = seen.into_values().collect();
        // In the block's canonical order.
        notes.sort_by_key(|note| written_order(note.on));
        notes
    })
}

/// Where a note's field sits in the block's canonical argument order.
fn written_order(site: NoteSite) -> usize {
    match site {
        NoteSite::Block => 0,
        NoteSite::Todo => 1,
        NoteSite::Created => 2,
        NoteSite::Category => 3,
        NoteSite::WaitingOn => 4,
        NoteSite::Source => 5,
        NoteSite::Detail => 6,
        NoteSite::Subtodos => 7,
        NoteSite::Tail => 8,
    }
}

fn outcome() -> impl Strategy<Value = Option<Outcome>> {
    proptest::option::of((datetime(), any::<bool>()).prop_map(|(at, completed)| {
        let instant = fpl::instant_of(at);
        if completed {
            Outcome::Completed(instant)
        } else {
            Outcome::Cancelled(instant)
        }
    }))
}

prop_compose! {
    /// One row, drawn wide: every optional field present or absent, prose that
    /// carries every special, and a price that lands on the renderer's own
    /// two-decimal rounding.
    fn row()(
        id in proptest::string::string_regex("[a-z][a-z0-9]{2,20}").expect("a fixed pattern"),
        actor in proptest::sample::select(vec!["ada", "triage", "assistant", "backfill"]),
        kind in proptest::sample::select(KINDS.to_vec()),
        created in datetime(),
        body in markup(),
        price in proptest::option::of(0.0f64..1.0),
        rationale in proptest::option::of(lines()),
        category in proptest::option::of(proptest::sample::select(vec![
            "work", "study", "project", "home", "other",
        ])),
        waiting_on in proptest::option::of(quoted()),
        detail in proptest::option::of(markup()),
        source in source(),
        subtodos in subtodos(),
        notes in notes(),
        outcome in outcome(),
    ) -> Shown {
        // A category on `ekaTodo` is its kind's.
        let category = if kind == "ekaTodo" { None } else { category };
        // `raise_block` refuses a note on a field the row does not carry.
        let notes: Vec<Note> = notes
            .into_iter()
            .filter(|note| match note.on {
                NoteSite::Category => category.is_some(),
                NoteSite::Source => source.is_some(),
                NoteSite::Detail => detail.as_ref().is_some_and(|d| !d.as_str().is_empty()),
                NoteSite::Subtodos => !subtodos.is_empty(),
                _ => true,
            })
            .collect();
        Shown {
            todo: TodoId::new(id).expect("a fixed pattern"),
            actor: Actor::new(actor).expect("a roster name"),
            kind: Name::new("Authored.kind", kind).expect("a constructor name"),
            created,
            body,
            price,
            // `raise_block` refuses a rationale with no priority, so none is drawn.
            rationale: if price.is_some() { rationale.unwrap_or_default() } else { Vec::new() },
            category: category.map(|name| Name::new("Authored.category", name).expect("a name")),
            waiting_on: waiting_on.unwrap_or_else(|| StringSource::new("")),
            detail: detail.unwrap_or_else(MarkupSource::empty),
            source,
            subtodos,
            notes,
            outcome,
        }
    }
}

/// The block a row shows, as `todos::render_current` prints it.
fn shown(row: &Shown) -> Result<String, String> {
    let (call, leading) = block::raise_block(row).map_err(|e| e.to_string())?;
    render_verified(&call, &leading).map_err(|e| e.message().to_owned())
}

// --- the law -------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Every row the record can hold renders as a block typst reads back as the
    /// block built: the containment check refuses no honest record, whatever
    /// specials its prose carries.
    #[test]
    fn every_row_renders_verified(row in row()) {
        prop_assert!(shown(&row).is_ok(), "{:?}", shown(&row));
    }

    /// Every `//` line is printed; a closing run with no argument after it is
    /// the most at risk.
    #[test]
    fn no_comment_is_dropped(row in row()) {
        let printed = shown(&row).expect("a representable row");
        let mut wanted: Vec<String> = row.rationale.clone();
        for note in &row.notes {
            wanted.extend(note.lines.iter().cloned());
        }
        for line in wanted {
            prop_assert!(
                line.is_empty() || printed.contains(&format!("// {line}")),
                "the run {line:?} is not in the block"
            );
        }
    }
}
