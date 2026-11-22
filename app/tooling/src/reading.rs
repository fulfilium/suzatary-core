//! One half of one tree, as the table's rows say.

use crate::data::Beside;
use crate::row::{Line, SetAside};
use crate::{Half, Outcome, Row};
use std::fmt;

/// The union of the outcomes the rows hold: a diamond with `Unjudged` (no row) at the bottom,
/// `Finding` (green and red both seen, so the gate read something beyond its key) at the top, and
/// the two agreements between them, neither above the other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reading {
    Unjudged,
    Agreed(Outcome),
    Finding,
}

impl Reading {
    /// The least upper bound in the diamond.
    #[must_use]
    pub fn join(self, other: Reading) -> Reading {
        match (self, other) {
            (Reading::Unjudged, reading) | (reading, Reading::Unjudged) => reading,
            (Reading::Agreed(a), Reading::Agreed(b)) if a == b => Reading::Agreed(a),
            (Reading::Finding | Reading::Agreed(_), Reading::Finding | Reading::Agreed(_)) => {
                Reading::Finding
            }
        }
    }

    pub fn of_rows<'a>(rows: impl IntoIterator<Item = &'a Row>) -> Reading {
        rows.into_iter().fold(Reading::Unjudged, |reading, row| {
            reading.join(Reading::Agreed(row.outcome))
        })
    }
}

/// A half as a land reads it, from its rows alone: its key, the reading there, every set-aside of
/// the half, and the live reds at older data.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Judged {
    pub key: Option<Beside>,
    pub reading: Reading,
    pub set_aside: Vec<SetAside>,
    pub lapsed: Vec<Row>,
}

impl Judged {
    /// One file per scope; the newest live row is a file's last, then the later instant.
    #[must_use]
    pub fn of(half: Half, files: &[Vec<Line>]) -> Judged {
        let lines: Vec<&Line> = files.iter().flatten().collect();
        let rows = || {
            lines.iter().filter_map(|line| match line {
                Line::Row(row) => Some(row),
                Line::SetAside(_) => None,
            })
        };
        let asides: Vec<&SetAside> = lines
            .iter()
            .filter_map(|line| match line {
                Line::SetAside(aside) => Some(aside),
                Line::Row(_) => None,
            })
            .collect();
        let stands = |row: &Row| !asides.iter().any(|aside| aside.sets_aside(row));
        let key = match half {
            Half::Tree => Some(Beside::Nothing),
            Half::Live => files
                .iter()
                .filter_map(|file| {
                    file.iter().rev().find_map(|line| match line {
                        Line::Row(row) if matches!(row.beside, Beside::Data(_)) => Some(row),
                        Line::Row(_) | Line::SetAside(_) => None,
                    })
                })
                .fold(None, |newest: Option<&Row>, row| match newest {
                    Some(seen) if seen.at > row.at => Some(seen),
                    Some(_) | None => Some(row),
                })
                .map(|row| row.beside.clone()),
        };
        let set_aside = asides
            .iter()
            .filter(|aside| rows().any(|row| aside.sets_aside(row)))
            .map(|aside| (*aside).clone())
            .collect();
        let Some(key) = key else {
            return Judged {
                key: None,
                reading: Reading::Unjudged,
                set_aside,
                lapsed: Vec::new(),
            };
        };
        let reading = Reading::of_rows(rows().filter(|row| row.beside == key && stands(row)));
        let lapsed = rows()
            .filter(|row| {
                row.outcome == Outcome::Red
                    && matches!(row.beside, Beside::Data(_))
                    && row.beside != key
                    && stands(row)
            })
            .map(|row| (*row).clone())
            .collect();
        Judged {
            key: Some(key),
            reading,
            set_aside,
            lapsed,
        }
    }
}

/// The words train.sh's verbs read: `ok`, `failed`, `unknown` and `finding`.
impl fmt::Display for Reading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Reading::Unjudged => "unknown",
            Reading::Agreed(outcome) => outcome.word(),
            Reading::Finding => "finding",
        })
    }
}
