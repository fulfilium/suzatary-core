//! Which rows a publish makes, decided before anything is checked or pushed.

use crate::core_rows::Rows;
use crate::oid::Oid;
use crate::refusal::Refusal;

/// What a publish of `rev` does.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Plan {
    /// Core's row `0` publishes it already.
    Published(Oid),
    /// The landed commits after the newest published, oldest first, through `rev`.
    Rows(Vec<Source>),
}

/// One landed commit a publish reaches.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Source {
    /// Its public tree becomes a row, naming the skips just before it.
    Row { source: Oid, skipped: Vec<Oid> },
    /// Passed over, as `--skip` asked: the next row names it.
    Skip(Oid),
}

impl Plan {
    /// Law 5: the rows are master's first-parent commits after the newest `Source:`, oldest first,
    /// or `rev` alone where core publishes nothing yet; a skip is taken only for one of them that
    /// is not the last. `chain` is master's first-parent chain, newest first.
    pub fn of(chain: &[Oid], rows: &Rows, rev: &Oid, skips: &[Oid]) -> Result<Plan, Refusal> {
        let below = chain
            .iter()
            .position(|commit| commit == rev)
            .map(|at| &chain[at..])
            .ok_or_else(|| Refusal::NotLanded(rev.clone()))?;
        if let Some(row) = rows.published(rev) {
            return Ok(Plan::Published(row.clone()));
        }
        if let Some(by) = rows.skipped_by(rev) {
            return Err(Refusal::Skipped {
                rev: rev.clone(),
                by: by.clone(),
            });
        }
        let mut sources: Vec<Oid> = match rows.newest() {
            None => vec![rev.clone()],
            Some(newest) => below
                .iter()
                .position(|commit| commit == newest)
                .map(|at| below[..at].to_vec())
                .ok_or_else(|| Refusal::Climbs {
                    newest: newest.clone(),
                    rev: rev.clone(),
                })?,
        };
        sources.reverse();
        for skip in skips {
            if !sources.contains(skip) {
                return Err(Refusal::SkipNotARow(skip.clone()));
            }
            if sources.last() == Some(skip) {
                return Err(Refusal::SkipIsLast(skip.clone()));
            }
        }
        let mut skipped = Vec::new();
        let mut plan = Vec::new();
        for source in sources {
            if skips.contains(&source) {
                skipped.push(source.clone());
                plan.push(Source::Skip(source));
            } else {
                plan.push(Source::Row {
                    source,
                    skipped: std::mem::take(&mut skipped),
                });
            }
        }
        Ok(Plan::Rows(plan))
    }
}
