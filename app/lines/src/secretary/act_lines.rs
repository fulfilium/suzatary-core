//! The lines an act writes into a conversation: a note of who acted and why, and the line that
//! ends an act a restart left running.

use crate::secretary::{conversations, transcript};
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::{Reference, TodoRef};
use suzatary_base::refusal::Refusal;
use suzatary_conversations::secretary::store::Entry;
use suzatary_inbox::secretary::responded::{Performed, Tried};
use suzatary_inbox::secretary::stamp::Stamp;

/// The words a restart leaves behind, since the device code is never persisted.
const RESTARTED: &str = "the box restarted while this act was waiting; the device code is gone \
                          and was never written down — Try again asks for a fresh one";

/// At serve startup, end every running act nobody owns with [`Tried::NotSent`], idempotently.
pub fn reconcile(
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox: &dyn suzatary_store::store::Prodrome<suzatary_inbox::secretary::inbox_event::InboxEvent>,
    now: suzatary_base::capability::Datetime,
) -> Result<usize, Refusal> {
    let restarted = Performed::Tried(Tried::NotSent {
        reason: RESTARTED.to_owned(),
    });
    let fold = suzatary_inbox::secretary::inbox::fold(inbox)?;
    let mut reconciled = 0usize;
    for object in fold.objects() {
        if !object.reading.running() {
            continue;
        }
        suzatary_inbox::secretary::inbox::end(
            inbox,
            &object.id,
            &transcript::writer(),
            now,
            restarted.clone(),
        )?;
        conversations::append(
            conversations,
            &object.asked.asked.conversation,
            &Entry::noted(now, RESTARTED.to_owned(), Vec::new()),
        )?;
        reconciled += 1;
    }
    Ok(reconciled)
}

/// The sentence an act turn prints. What it acted on travels only in `references`, never here: a
/// todo's id can be chosen by mail, and an act line stamped by its actor reads clean.
#[must_use]
pub fn describe_act(actor: &str, verb: &str, note: &str) -> String {
    format!("{actor} {verb} it: {note}")
}

/// One act turn, built.
#[derive(Debug, Clone)]
pub struct ActLine {
    /// The event's own instant, never the clock's.
    pub at: suzatary_base::capability::Datetime,
    /// The actor the gate admitted.
    pub actor: String,
    /// Past tense, such as "dismissed" or "completed".
    pub verb: &'static str,
    /// What the turn's `references` array carries.
    pub reference: Reference,
    /// The write's own why, untrimmed; [`note_act`] trims and gates on it.
    pub note: String,
}

/// Append an act's note as a [`Turn::Act`](suzatary_conversations::secretary::model::Turn::Act) stamped `by`, or
/// nothing when the note is empty.
pub fn note_act(
    conversations: crate::secretary::conversations::Conversations<'_>,
    id: &ConversationId,
    line: &ActLine,
    by: Stamp,
) -> Result<(), Refusal> {
    let note = line.note.trim();
    if note.is_empty() {
        return Ok(());
    }
    conversations::append(
        conversations,
        id,
        &Entry::Said {
            at: line.at,
            turn: suzatary_conversations::secretary::model::Turn::Act {
                text: describe_act(
                    &suzatary_base::spelling::capitalised(&line.actor),
                    line.verb,
                    note,
                ),
                references: vec![line.reference.clone()],
            },
            by: Some(by),
        },
    )
}

/// The act lines that adopted events with a note carry; only `actor`'s. `before` answers the
/// record the store showed where a posted one now stands: the same content confirms it, other
/// content updates it, and a creation or an undo or redo is not noted.
#[must_use]
pub fn heard_from_objects(
    events: &[suzatary_record::record::TodoEvent],
    actor: &prodrome_core::event::Actor,
    chain: &suzatary_base::chains::ChainName,
    before: &dyn Fn(
        &suzatary_record::record::Authored,
    ) -> Option<suzatary_record::record::Authored>,
) -> Vec<ActLine> {
    use suzatary_record::record::TodoEvent;
    let line = |todo: &prodrome_core::event::TodoId,
                at,
                by: &prodrome_core::event::Actor,
                note: &str,
                verb| {
        (by == actor && !note.trim().is_empty()).then(|| ActLine {
            at,
            actor: by.as_str().to_owned(),
            verb,
            reference: Reference::Todo(TodoRef {
                chain: chain.clone(),
                id: todo.clone(),
            }),
            note: note.to_owned(),
        })
    };
    let moved =
        |l: &prodrome_core::event::Lifecycle, verb| line(&l.todo, l.at, &l.actor, &l.note, verb);
    events
        .iter()
        .filter_map(|event| match event {
            TodoEvent::Completed(l) => moved(l, "completed"),
            TodoEvent::Cancelled(l) => moved(l, "cancelled"),
            TodoEvent::Reopened(l) => moved(l, "reopened"),
            TodoEvent::Tended(l) => moved(l, "tended"),
            TodoEvent::Created(_) | TodoEvent::SpecRevised(_) => None,
            TodoEvent::Authored(r)
                if suzatary_todo::authoring::Reversal::of_note(&r.payload.note).is_some() =>
            {
                None
            }
            TodoEvent::Authored(r) => {
                let shown = before(r)?;
                let same = suzatary_record::record::Record {
                    note: shown.payload.note.clone(),
                    ..r.payload.clone()
                } == shown.payload;
                line(
                    &r.todo,
                    r.at,
                    &r.actor,
                    &r.payload.note,
                    if same { "confirmed" } else { "updated" },
                )
            }
        })
        .collect()
}
