//! "Build it": a todo the owner asks to be built is filed as the coding-session card that serves
//! it. A todo is in progress while it is open and a card serving it is open, or the session that
//! card started is fired or reported: a reading of the chains, the inbox and the sessions, so
//! nothing of it is stored.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use crate::believed::Believed;
use crate::capability::Datetime;
use crate::coding::Task;
use crate::record::Authored;
use crate::reference::{ProposalRef, TodoRef};
use crate::refusal::Refusal;
use crate::secretary::Bound;
use crate::secretary::acts::Act;
use crate::secretary::coding::Advanced;
use crate::secretary::inbox::Fold;
use crate::secretary::store::{ConversationId, Proposal};

/// How many todos may be in progress at once.
pub const AT_ONCE: usize = 2;

/// The conversation build cards are drawn in.
pub static CONVERSATION: LazyLock<ConversationId> =
    LazyLock::new(|| ConversationId::new("build").expect("\"build\" is a conversation id"));

/// The one record of `todo` a session is told: refused unless the chain holds one text for it
/// and it is open.
///
/// # Errors
/// [`Refusal::not_found`] for no text, [`Refusal::conflict`] for a register in conflict or a
/// closed todo.
pub fn record<'b>(believed: &'b Believed, todo: &TodoRef) -> Result<&'b Authored, Refusal> {
    let outcomes = believed.outcome(&todo.id);
    let specs = believed.specs.get(&todo.id).map_or(0, Vec::len);
    match believed
        .content
        .get(&todo.id)
        .map_or(&[][..], Vec::as_slice)
    {
        [] => Err(Refusal::not_found(format!(
            "the chain holds no text for {todo}"
        ))),
        [record] if outcomes.len() == 1 && specs <= 1 => {
            if believed.is_open(&todo.id) {
                Ok(record)
            } else {
                Err(Refusal::conflict(format!("{todo} is closed")))
            }
        }
        _ => Err(Refusal::conflict(format!(
            "{todo} is in a register conflict: settle it first, so the session is told one text"
        ))),
    }
}

/// The task `todo`'s card carries: a fixed frame around the text the chain holds, as typst
/// source, so the card shows the session everything it will be told. `attempt` makes a retry
/// after an earlier card or session ended a request of its own.
///
/// # Errors
/// [`Refusal::bad_request`] for a text over a task's bound, or holding a control character.
pub fn task(todo: &TodoRef, record: &Authored, attempt: usize) -> Result<Task, Refusal> {
    let detail = record.payload.detail.as_str().trim();
    Task::try_from(format!(
        "Build {todo}, attempt {attempt}. Its text follows, as the owner's prodrome holds it.\n\
         When the pull request finishes it, put `Completes: {}` on a line of its description.\n\n\
         {}{}{detail}",
        todo.id.as_str(),
        record.payload.body.as_str().trim(),
        if detail.is_empty() { "" } else { "\n\n" },
    ))
    .map_err(|failed| Refusal::bad_request(failed.to_string()))
}

/// Every coding-session card serving a todo, with that todo.
fn serving(fold: &Fold) -> impl Iterator<Item = (&TodoRef, &crate::secretary::inbox::Object)> {
    fold.objects()
        .filter(|object| {
            matches!(
                object.asked.proposal,
                Proposal::Act(Act::CodingSession { .. })
            )
        })
        .filter_map(|object| Some((object.asked.price.serves()?, object)))
}

/// Every open todo in progress, each with the card that serves it; `sessions` are the ones fired
/// or reported ([`crate::secretary::coding::waiting`]).
#[must_use]
pub fn in_progress(
    fold: &Fold,
    sessions: &[Advanced],
    open: &dyn Fn(&TodoRef) -> bool,
) -> BTreeMap<TodoRef, ProposalRef> {
    serving(fold)
        .filter(|(todo, object)| {
            open(todo)
                && (object.reading.open()
                    || sessions.iter().any(|session| {
                        session.conversation == object.asked.asked.conversation
                            && session.index == object.asked.asked.index
                    }))
        })
        .map(|(todo, object)| (todo.clone(), object.asked.asked.clone()))
        .collect()
}

/// Whether `todo` reads open at `at` in `chains`; a todo of a chain not among them reads open.
fn open(chains: &[Bound<'_>], todo: &TodoRef, at: Datetime) -> Result<bool, Refusal> {
    let Some(bound) = chains
        .iter()
        .find(|bound| bound.name == todo.chain.as_str())
    else {
        return Ok(true);
    };
    Ok(
        Believed::of(&bound.caps.chain.nodes()?, at, &bound.policy.untrusted)
            .map_err(|failed| Refusal::internal(failed.to_string()))?
            .is_open(&todo.id),
    )
}

/// A coding session serving `todo` may be filed: refused while it is in progress, or while
/// [`AT_ONCE`] others are, each read open in `chains` (none read: every one counts). Asked by
/// the filing door under the inbox's hold, so every filer is held to it at once.
///
/// # Errors
/// [`Refusal::conflict`] naming what is in progress, and a chain that does not read.
pub fn admitted(
    fold: &Fold,
    sessions: &[Advanced],
    chains: &[Bound<'_>],
    at: Datetime,
    todo: &TodoRef,
) -> Result<(), Refusal> {
    let mut open = BTreeMap::new();
    for (served, _) in serving(fold) {
        if !open.contains_key(served) {
            open.insert(served.clone(), self::open(chains, served, at)?);
        }
    }
    let busy = in_progress(fold, sessions, &|todo| {
        open.get(todo).copied().unwrap_or(true)
    });
    if let Some(card) = busy.get(todo) {
        return Err(Refusal::conflict(format!(
            "{todo} is already in progress: {card}"
        )));
    }
    if busy.len() >= AT_ONCE {
        let named: Vec<String> = busy.keys().map(ToString::to_string).collect();
        return Err(Refusal::conflict(format!(
            "{} todos are in progress, {}: one must be merged, dropped or closed first",
            busy.len(),
            named.join(" and ")
        )));
    }
    Ok(())
}

/// The attempt a new card serving `todo` is, counted from the cards that served it before.
#[must_use]
pub fn attempt(fold: &Fold, todo: &TodoRef) -> usize {
    serving(fold).filter(|(served, _)| *served == todo).count() + 1
}
