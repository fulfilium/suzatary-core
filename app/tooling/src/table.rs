//! The verdict table: `.git/train/gates/<gate>/<half>[-full]/<tree>`, one file of lines per tree,
//! only ever appended to.

use crate::admitted::Terminal;
use crate::gate::Gate;
use crate::instant::Instant;
use crate::oid::Oid;
use crate::reading::{Judged, Reading};
use crate::row::{Line, SetAside, Task, Words};
use crate::{Error, Half, Outcome, Row};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Which gates wrote a row: a selective one (`gate`), or a full one (`gate --full`, `nightly`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Selective,
    Full,
}

/// Which rows a reading takes: every gate's, or the full gates' alone (the nightly's question).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Seen {
    Any,
    Full,
}

impl Seen {
    #[must_use]
    pub fn scopes(self) -> &'static [Scope] {
        match self {
            Seen::Any => &[Scope::Selective, Scope::Full],
            Seen::Full => &[Scope::Full],
        }
    }
}

/// One gate's rows.
#[derive(Clone, Debug)]
pub struct Table {
    dir: PathBuf,
}

impl Table {
    /// The table of `gate` under `gates`, the repository's `.git/train/gates`.
    #[must_use]
    pub fn at(gates: &Path, gate: &Gate) -> Table {
        Table {
            dir: gates.join(gate.dir()),
        }
    }

    /// The directory of one half's rows from one scope, a file per tree.
    fn key_dir(&self, half: Half, scope: Scope) -> PathBuf {
        let suffix = match scope {
            Scope::Selective => "",
            Scope::Full => "-full",
        };
        self.dir.join(format!("{}{suffix}", half.name()))
    }

    #[must_use]
    pub fn file(&self, half: Half, scope: Scope, tree: &Oid) -> PathBuf {
        self.key_dir(half, scope).join(tree.to_string())
    }

    /// A tree's lines, in the order they were written; a tree never written has none.
    pub(crate) fn lines(&self, half: Half, scope: Scope, tree: &Oid) -> Result<Vec<Line>, Error> {
        let path = self.file(half, scope, tree);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(Error::Io { path, source }),
        };
        Line::parse_all(&text).map_err(|(line, why)| Error::Unreadable {
            path,
            line,
            why: Box::new(why),
        })
    }

    /// What a land reads of `half` of `tree`, from the rows `seen` takes.
    pub fn judged(&self, half: Half, seen: Seen, tree: &Oid) -> Result<Judged, Error> {
        let files = seen
            .scopes()
            .iter()
            .map(|scope| self.lines(half, *scope, tree))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Judged::of(half, &files))
    }

    pub fn read(&self, half: Half, seen: Seen, tree: &Oid) -> Result<Reading, Error> {
        self.judged(half, seen, tree).map(|judged| judged.reading)
    }

    /// Sets aside the newest standing red row at the land's key; only at a terminal.
    pub fn set_aside(
        &self,
        half: Half,
        tree: &Oid,
        reason: (Task, Words),
        _: &Terminal,
    ) -> Result<Option<(Row, SetAside)>, Error> {
        self.aside(half, tree, reason)
    }

    /// `set_aside` behind its door, for the laws.
    pub(crate) fn aside(
        &self,
        half: Half,
        tree: &Oid,
        (task, words): (Task, Words),
    ) -> Result<Option<(Row, SetAside)>, Error> {
        let Some(key) = self.judged(half, Seen::Any, tree)?.key else {
            return Ok(None);
        };
        let mut red: Option<(Scope, Row)> = None;
        for scope in Seen::Any.scopes() {
            let lines = self.lines(half, *scope, tree)?;
            let stands = |row: &Row| {
                !lines
                    .iter()
                    .any(|line| matches!(line, Line::SetAside(aside) if aside.sets_aside(row)))
            };
            for line in &lines {
                if let Line::Row(row) = line
                    && row.outcome == Outcome::Red
                    && row.beside == key
                    && stands(row)
                    && red.as_ref().is_none_or(|(_, newest)| newest.at <= row.at)
                {
                    red = Some((*scope, row.clone()));
                }
            }
        }
        let Some((scope, row)) = red else {
            return Ok(None);
        };
        let aside = SetAside {
            commit: row.commit.clone(),
            red_at: row.at,
            beside: key,
            task,
            at: Instant::now(),
            words,
        };
        self.append(half, scope, tree, &Line::SetAside(aside.clone()))?;
        Ok(Some((row, aside)))
    }

    /// Appends a judged run's row; a set-aside goes only through `set_aside`.
    pub fn write(&self, half: Half, scope: Scope, tree: &Oid, row: &Row) -> Result<(), Error> {
        self.append(half, scope, tree, &Line::Row(row.clone()))
    }

    /// Appends `line` to its tree's file in one write, as `>>` did.
    fn append(&self, half: Half, scope: Scope, tree: &Oid, line: &Line) -> Result<(), Error> {
        let path = self.file(half, scope, tree);
        let failed = |source| Error::Io {
            path: path.clone(),
            source,
        };
        std::fs::create_dir_all(self.key_dir(half, scope)).map_err(failed)?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()))
            .map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    use super::{Scope, Seen, Table};
    use crate::data::{Beside, Data};
    use crate::gate::Gate;
    use crate::instant::Instant;
    use crate::oid::Oid;
    use crate::row::{Task, Words};
    use crate::{Half, Outcome, Reading, Row};
    use proptest::prelude::*;

    fn row() -> impl Strategy<Value = (Row, Scope)> {
        let beside = prop_oneof![
            Just(Beside::Nothing),
            Just(Beside::Data(
                Data::parse(&"a".repeat(40)).expect("a digest")
            )),
            Just(Beside::Data(
                Data::parse(&"b".repeat(40)).expect("a digest")
            )),
        ];
        let scope = prop_oneof![Just(Scope::Selective), Just(Scope::Full)];
        (any::<bool>(), 0u8..3, 0u64..4, beside, scope).prop_map(
            |(green, commit, at, beside, scope)| {
                let outcome = if green { Outcome::Green } else { Outcome::Red };
                let commit = Oid::parse(&format!("{commit:040x}")).expect("a commit");
                let row = Row {
                    outcome,
                    commit,
                    at: Instant(at),
                    beside,
                };
                (row, scope)
            },
        )
    }

    /// `set_aside` behind its door, over generated tables: it sets aside the newest red row that
    /// stands at the key a land reads, so that row no longer counts and the set-aside is what the
    /// land prints; with none, it writes nothing; and it only ever appends.
    #[test]
    fn a_set_aside_takes_the_newest_standing_red_at_the_lands_key() {
        let gates = std::env::temp_dir().join(format!("suzatary-aside-{}", std::process::id()));
        let table = Table::at(&gates, &Gate::merge_gate());
        let reason = || {
            (
                Task::parse("gateverdicts").expect("a task"),
                Words::parse("the runners' cache job failed").expect("words"),
            )
        };
        proptest!(|(rows in prop::collection::vec(row(), 0..6), half in prop_oneof![Just(Half::Tree), Just(Half::Live)], seed in "[0-9a-f]{40}")| {
            let tree = Oid::parse(&seed).expect("a tree");
            for (row, scope) in &rows {
                table.write(half, *scope, &tree, row).expect("written");
            }
            let contents = |scope| std::fs::read_to_string(table.file(half, scope, &tree)).unwrap_or_default();
            let files = (contents(Scope::Selective), contents(Scope::Full));
            let before = table.judged(half, Seen::Any, &tree).expect("it reads");
            let newest = before.key.as_ref().and_then(|key| {
                rows.iter()
                    .map(|(row, _)| row)
                    .filter(|row| row.outcome == Outcome::Red && row.beside == *key)
                    .max_by_key(|row| row.at)
                    .map(|row| row.at)
            });
            match table.aside(half, &tree, reason()).expect("it writes") {
                None => {
                    prop_assert_eq!(newest, None);
                    prop_assert_eq!((contents(Scope::Selective), contents(Scope::Full)), files);
                }
                Some((red, aside)) => {
                    prop_assert_eq!(Some(red.at), newest);
                    prop_assert_eq!(Some(&red.beside), before.key.as_ref());
                    let after = table.judged(half, Seen::Any, &tree).expect("it reads");
                    prop_assert!(after.set_aside.contains(&aside));
                    let (now_selective, now_full) = (contents(Scope::Selective), contents(Scope::Full));
                    prop_assert!(now_selective.starts_with(&files.0) && now_full.starts_with(&files.1));
                    prop_assert!(now_selective.len() + now_full.len() > files.0.len() + files.1.len());
                    if after.reading == Reading::Agreed(Outcome::Red) {
                        prop_assert!(rows.iter().filter(|(row, _)| row.outcome == Outcome::Red && Some(&row.beside) == after.key.as_ref()).count() > 1);
                    }
                }
            }
        });
    }
}
