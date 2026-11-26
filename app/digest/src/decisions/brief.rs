//! The brief: what a session loads before it plans. A function of the ledger, the summaries and the
//! arguments, so it reads no clock, network or model.

use suzatary_todo::decisions::{Ledger, Summaries};

use std::fmt::Write as _;

use super::about::{About, reach};
use super::decision_log::{Cyclic, DecisionLog, full};
use crate::secretary::digest_view;
use crate::secretary::told;

/// How many bytes a token is taken to be.
pub const BYTES_PER_TOKEN: usize = 4;

/// The budget of a brief that names none.
pub const DEFAULT_TOKENS: usize = 6_000;

/// What the decisions that name a task may take of what the head leaves, in thirds.
const FULL_THIRDS: usize = 2;

/// The most bytes the closing line and the note that names a cut take.
const CLOSING_BYTES: usize = 160;

const HEAD: &str = "THE PROJECT'S MEMORY, read before you plan: the decisions in force, each saying what \
     was decided and why. The same store and the same --about words print the same bytes. The \
     decisions that name your paths or words stand first, whole. THE REST is every decision in \
     force as the record, oldest first, one part a line, led by its handle (first line + how \
     many): a line `id: title` is one decision, and a line after `derived:` is a summary of the \
     decisions beneath it, written by a model, so it can be wrong. `zoom(handle)` means \
     `suzatary decision zoom <handle> --store memory`: it opens a part into what is beneath it, \
     down to whole decisions. `suzatary decision show <id> --store memory` reads one decision \
     whole, and `suzatary decision list --store memory --grep <words>` finds one. Do not write to \
     the store: to propose a lesson, put `Lesson: <words>` on its own line of your pull \
     request's description.";

/// The brief of `ledger` under `summaries` for a task that names `abouts`, within `tokens`.
///
/// # Errors
/// [`Cyclic`] as [`DecisionLog::of`].
pub fn brief(
    ledger: &Ledger,
    summaries: &Summaries,
    abouts: &[About],
    tokens: usize,
) -> Result<String, Cyclic> {
    let log = DecisionLog::of(ledger, summaries)?;
    let total = tokens.saturating_mul(BYTES_PER_TOKEN);
    let room = total.saturating_sub(HEAD.len());
    let mut named: Vec<_> = ledger
        .iter()
        .filter(|(id, _)| ledger.in_force(id))
        .filter_map(|(id, decision)| Some((reach(abouts, decision)?, id, decision)))
        .collect();
    named.sort_by(|(one, first_id, first), (two, second_id, second)| {
        one.cmp(two)
            .then_with(|| second.decided.cmp(&first.decided))
            .then_with(|| first_id.cmp(second_id))
    });
    let allowed = room / 3 * FULL_THIRDS;
    let mut whole = Vec::new();
    let mut spent = 0;
    for (_, id, decision) in &named {
        let text = full(id, decision);
        if spent + text.len() + 2 > allowed {
            break;
        }
        spent += text.len() + 2;
        whole.push(text);
    }
    let mut out = format!("{HEAD}\n");
    if !abouts.is_empty() {
        out.push_str("\nTHE DECISIONS THAT NAME YOUR PATHS AND WORDS, whole:\n");
        for text in &whole {
            let _ = write!(out, "\n{text}\n");
        }
        if whole.len() < named.len() {
            let _ = write!(
                out,
                "\n({} more name them and stand in THE REST below: open one with `suzatary \
                 decision show <id> --store memory`.)\n",
                named.len() - whole.len()
            );
        }
        if named.is_empty() {
            out.push_str("\n(No decision names them: read THE REST for what is near.)\n");
        }
    }
    out.push_str("\nTHE REST, oldest first:\n");
    let left = total.saturating_sub(out.len() + CLOSING_BYTES);
    let shown = digest_view::view(&log, left, |()| true);
    for line in told::fit_latest(told::lines(&shown), left) {
        out.push_str(&line);
        out.push('\n');
    }
    let size = out.len().div_ceil(BYTES_PER_TOKEN);
    let _ = writeln!(out, "(This brief is about {size} tokens of {tokens}.)");
    Ok(out)
}

/// What lies beneath the part of the log that `handle` names, one part a line led by its handle,
/// a whole decision set apart by a blank line. `None` for a handle naming no lines.
#[must_use]
pub fn zoom(log: &DecisionLog, handle: &str) -> Option<String> {
    let ((), beneath) = digest_view::opened(log, handle, |()| true)?;
    let mut out = String::new();
    for (_, text) in told::lines(&beneath) {
        out.push_str(&text);
        out.push('\n');
        if text.contains('\n') {
            out.push('\n');
        }
    }
    Some(out)
}
