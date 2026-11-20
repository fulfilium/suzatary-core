//! A task's reading under the waits: the least fulfillment among its own and every open task
//! upstream of it by waits, so a blocker never reads less urgent than its waiter. The least is
//! idempotent, so a cycle reads at its most urgent price and is never refused. It names no
//! schema, so it can join the Prodrome.

/// A task's reading under the waits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reading {
    /// Its own price stands: nothing that waits on it is more urgent.
    Own,
    /// It takes `value`, the own price of task `by`, an open task upstream of it.
    Raised { value: f64, by: usize },
}

impl Reading {
    /// The number the list orders by: the raised value, else `own`.
    #[must_use]
    pub fn value(self, own: Option<f64>) -> Option<f64> {
        match self {
            Reading::Own => own,
            Reading::Raised { value, .. } => Some(value),
        }
    }
}

/// Each open task's reading, by its index in `own`, which holds each task's own price (`None`
/// for a task with no number, which raises nothing). `waits` holds `(waiter, blocker)` indices;
/// a pair with an end outside `own` is left out.
///
/// The tasks are visited from the most urgent price up, and each task reached is settled by the
/// first, so the whole reading costs one sort and one walk of the waits.
#[must_use]
pub fn readings(own: &[Option<f64>], waits: &[(usize, usize)]) -> Vec<Reading> {
    let mut blockers: Vec<Vec<usize>> = vec![Vec::new(); own.len()];
    for &(waiter, blocker) in waits {
        if waiter < own.len() && blocker < own.len() && waiter != blocker {
            blockers[waiter].push(blocker);
        }
    }
    let mut sources: Vec<(usize, f64)> = own
        .iter()
        .enumerate()
        .filter_map(|(task, value)| value.map(|value| (task, value)))
        .collect();
    sources.sort_by(|(a, x), (b, y)| x.total_cmp(y).then_with(|| a.cmp(b)));
    let mut settled = vec![false; own.len()];
    let mut read = vec![Reading::Own; own.len()];
    for (source, value) in sources {
        if std::mem::replace(&mut settled[source], true) {
            continue;
        }
        let mut walking = blockers[source].clone();
        while let Some(blocker) = walking.pop() {
            if std::mem::replace(&mut settled[blocker], true) {
                continue;
            }
            // A blocker of an equal price keeps its own: it is no less urgent already.
            if own[blocker].is_none_or(|own| value < own) {
                read[blocker] = Reading::Raised { value, by: source };
            }
            walking.extend(&blockers[blocker]);
        }
    }
    read
}
