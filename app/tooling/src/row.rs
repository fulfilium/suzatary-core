//! One line of the verdict table.

use crate::data::Beside;
use crate::instant::Instant;
use crate::oid::Oid;
use crate::{Error, Outcome};
use std::fmt;

/// One judged run of a half: `<ok|failed> <commit> <instant>`, a live half's then `data:<digest>`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub outcome: Outcome,
    /// The commit gated; its tree is the row's key.
    pub commit: Oid,
    pub at: Instant,
    pub beside: Beside,
}

/// `set-aside <commit> <instant> <-|data:<digest>> <task> <instant> <words>`: one red row.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetAside {
    pub commit: Oid,
    pub red_at: Instant,
    pub beside: Beside,
    pub task: Task,
    pub at: Instant,
    pub words: Words,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Line {
    Row(Row),
    SetAside(SetAside),
}

/// A roadmap task's id.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Task(String);

impl Task {
    pub(crate) fn parse(word: &str) -> Result<Task, Error> {
        let plain = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-';
        if word.is_empty() || !word.bytes().all(plain) {
            return Err(Error::Usage(format!(
                "\"{word}\" is no task id: lowercase letters, digits, - and _"
            )));
        }
        Ok(Task(word.to_owned()))
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Words(String);

impl Words {
    pub(crate) fn parse(text: &str) -> Result<Words, Error> {
        let text = text.trim();
        if text.is_empty() || text.contains(['\n', '\r']) {
            return Err(Error::Usage("the reason is some words, on one line".into()));
        }
        Ok(Words(text.to_owned()))
    }
}

impl fmt::Display for Words {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl SetAside {
    #[must_use]
    pub(crate) fn sets_aside(&self, row: &Row) -> bool {
        row.outcome == Outcome::Red
            && row.commit == self.commit
            && row.at == self.red_at
            && row.beside == self.beside
    }
}

impl Line {
    pub fn parse(line: &str) -> Result<Line, Error> {
        let fields: Vec<&str> = line.splitn(7, ' ').collect();
        match fields[..] {
            ["set-aside", commit, red_at, beside, task, at, words] => {
                Ok(Line::SetAside(SetAside {
                    commit: Oid::parse(commit)?,
                    red_at: Instant::parse(red_at)?,
                    beside: Beside::parse(beside)?,
                    task: Task::parse(task)?,
                    at: Instant::parse(at)?,
                    words: Words::parse(words)?,
                }))
            }
            [outcome, commit, at] => Ok(Line::Row(Row {
                outcome: Outcome::parse(outcome)?,
                commit: Oid::parse(commit)?,
                at: Instant::parse(at)?,
                beside: Beside::Nothing,
            })),
            [outcome, commit, at, data] if data.starts_with("data:") => Ok(Line::Row(Row {
                outcome: Outcome::parse(outcome)?,
                commit: Oid::parse(commit)?,
                at: Instant::parse(at)?,
                beside: Beside::parse(data)?,
            })),
            _ => Err(Error::Malformed(format!(
                "\"{line}\" is neither <ok|failed> <commit> <instant> [data:<digest>] nor \
                 set-aside <commit> <instant> <-|data:<digest>> <task> <instant> <words>"
            ))),
        }
    }

    /// A file's lines, each ending in a newline; the error names the line, counted from 1.
    pub fn parse_all(text: &str) -> Result<Vec<Line>, (usize, Error)> {
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
            .map(|(at, line)| Line::parse(line).map_err(|e| (at + 1, e)))
            .collect()
    }
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.outcome.word(), self.commit, self.at)?;
        match &self.beside {
            Beside::Nothing => Ok(()),
            data @ Beside::Data(_) => write!(f, " {data}"),
        }
    }
}

impl fmt::Display for SetAside {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "set-aside {} {} {} {} {} {}",
            self.commit, self.red_at, self.beside, self.task, self.at, self.words
        )
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Line::Row(row) => row.fmt(f),
            Line::SetAside(aside) => aside.fmt(f),
        }
    }
}
