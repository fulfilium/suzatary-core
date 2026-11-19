//! One line of the verdict table.

use crate::instant::Instant;
use crate::oid::Oid;
use crate::{Error, Outcome};
use std::fmt;

/// One judged run of a half: `<ok|failed> <commit> <instant>`, a line of its key's file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub outcome: Outcome,
    /// The commit gated; its tree is the row's key.
    pub commit: Oid,
    pub at: Instant,
}

impl Row {
    pub fn parse(line: &str) -> Result<Row, Error> {
        let fields: Vec<&str> = line.split(' ').collect();
        let [outcome, commit, at] = fields[..] else {
            return Err(Error::Malformed(format!(
                "\"{line}\" is not three words: <ok|failed> <commit> <instant>"
            )));
        };
        Ok(Row {
            outcome: Outcome::parse(outcome)?,
            commit: Oid::parse(commit)?,
            at: Instant::parse(at)?,
        })
    }

    /// A file's rows, each line ending in a newline as every append wrote it; the error names
    /// the line, counted from 1.
    pub fn parse_all(text: &str) -> Result<Vec<Row>, (usize, Error)> {
        let mut lines: Vec<&str> = text.split('\n').collect();
        if lines.pop() != Some("") {
            return Err((
                lines.len() + 1,
                Error::Malformed("the last line has no newline".into()),
            ));
        }
        lines
            .into_iter()
            .enumerate()
            .map(|(at, line)| Row::parse(line).map_err(|e| (at + 1, e)))
            .collect()
    }
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.outcome.word(), self.commit, self.at)
    }
}
