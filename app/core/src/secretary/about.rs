//! The conversation about a thing: a query over the one transcript (the rounds that touch it),
//! not a second chat.
//!
//! A person's turn or an act opens a round, the same partition `web/src/chat/rounds.ts` draws;
//! `app/core/tests/about-fixture.json` is read by both sides' tests so the two cannot drift.

use crate::secretary::model::Turn;
use crate::secretary::store::Said;
use suzatary_base::reference::{self, Reference};

/// Inclusive `[start, end]` turn-index spans partitioning the whole transcript with no gaps.
///
/// An empty transcript has no spans; the last span may be unfinished (no answer yet).
#[must_use]
pub fn round_spans(turns: &[Said]) -> Vec<(usize, usize)> {
    if turns.is_empty() {
        return Vec::new();
    }
    let mut spans = Vec::new();
    let mut start = 0usize;
    for (index, said) in turns.iter().enumerate().skip(1) {
        if matches!(&said.turn, Turn::Person { .. } | Turn::Act { .. }) {
            spans.push((start, index - 1));
            start = index;
        }
    }
    spans.push((start, turns.len() - 1));
    spans
}

/// Whether the round at `span` touches `reference`: its opening turn carries it, or a model
/// turn in it names it in prose (so a round with no opening, like the brief, can still touch).
#[must_use]
pub fn round_touches(turns: &[Said], span: (usize, usize), reference: &Reference) -> bool {
    let (start, end) = span;
    let opens_on_it = matches!(
        &turns[start].turn,
        Turn::Person { references, .. } | Turn::Act { references, .. }
            if references.contains(reference)
    );
    opens_on_it
        || turns[start..=end].iter().any(|said| match &said.turn {
            Turn::Model { text, .. } => reference::mentions(text).contains(reference),
            _ => false,
        })
}

/// The span owning turn index `target`, clamped to the nearest end; mirrors the page's proposal
/// placement, so a proposal kept for `about` is the one the page draws in a touched round.
#[must_use]
pub fn owning_span(spans: &[(usize, usize)], target: usize) -> usize {
    match spans
        .iter()
        .position(|&(start, end)| target >= start && target <= end)
    {
        Some(index) => index,
        None if spans.is_empty() => 0,
        None => usize::from(target >= spans[0].0) * (spans.len() - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use suzatary_base::reference::Reference;

    /// One row of `app/core/tests/about-fixture.json`, the file `web/test/rounds.test.ts` also
    /// reads.
    #[derive(Deserialize)]
    struct FixtureTurn {
        who: String,
        text: String,
        references: Vec<String>,
        at: String,
    }

    fn said(row: &FixtureTurn) -> Said {
        let references: Vec<Reference> = row
            .references
            .iter()
            .map(|text| text.parse().expect("a reference in the fixture"))
            .collect();
        let turn = match row.who.as_str() {
            "person" => Turn::Person {
                text: row.text.clone(),
                references,
                attachments: Vec::new(),
            },
            "act" => Turn::Act {
                text: row.text.clone(),
                references,
            },
            "model" => Turn::Model {
                text: row.text.clone(),
                calls: Vec::new(),
            },
            "verb" => Turn::Tool {
                verb: "todo_show".into(),
                answer: row.text.clone(),
                outcome: crate::secretary::outcome::Outcome::Answered,
                files: Vec::new(),
            },
            other => panic!("the fixture named an unknown turn kind {other:?}"),
        };
        Said {
            at: crate::secretary::store::instant(&row.at),
            turn,
            by: None,
        }
    }

    /// Three rounds: the first opens on `todo:life/x`, the second names `mail:a1b2c3d4e5f6` only in
    /// prose, the third touches neither.
    fn fixture() -> Vec<Said> {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/about-fixture.json"),
        )
        .expect("app/core/tests/about-fixture.json reads");
        let rows: Vec<FixtureTurn> = serde_json::from_str(&text).expect("the fixture parses");
        rows.iter().map(said).collect()
    }

    #[test]
    fn round_spans_partitions_on_person_and_act_openings() {
        let spans = round_spans(&fixture());
        assert_eq!(spans, vec![(0, 3), (4, 5), (6, 7)]);
    }

    /// A round touches a reference its opening carries or a model turn names in prose, and only
    /// that round; a reference no round was asked about touches none.
    #[test]
    fn a_round_touches_what_its_opening_carries_or_its_prose_names() {
        let turns = fixture();
        let spans = round_spans(&turns);
        for (reference, touched) in [
            ("todo:life/x", [true, false, false]),
            ("mail:a1b2c3d4e5f6", [false, true, false]),
            ("todo:life/somethingelse", [false, false, false]),
        ] {
            let parsed: Reference = reference.parse().expect("a reference");
            for (span, touches) in spans.iter().zip(touched) {
                assert_eq!(
                    round_touches(&turns, *span, &parsed),
                    touches,
                    "{reference} in {span:?}"
                );
            }
        }
    }
}
