//! The lines an act writes into a conversation: a note of who acted and why, and the line that
//! ends an act a restart left running.

use crate::conversation_id::ConversationId;
use crate::reference::{Reference, TodoRef};
use crate::refusal::Refusal;
use crate::secretary::responded::{Performed, Tried};
use crate::secretary::stamp::Stamp;
use crate::secretary::store::Entry;
use crate::secretary::{conversations, transcript};

/// The words a restart leaves behind, since the device code is never persisted.
const RESTARTED: &str = "the box restarted while this act was waiting; the device code is gone \
                          and was never written down — Try again asks for a fresh one";

/// At serve startup, end every running act nobody owns with [`Tried::NotSent`], idempotently.
///
/// # Errors
/// The stores' own failures.
pub fn reconcile(
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox: &dyn crate::store::Prodrome<super::inbox_event::InboxEvent>,
    now: crate::capability::Datetime,
) -> Result<usize, Refusal> {
    let restarted = Performed::Tried(Tried::NotSent {
        reason: RESTARTED.to_owned(),
    });
    let fold = super::inbox::fold(inbox)?;
    let mut reconciled = 0usize;
    for object in fold.objects() {
        if !object.reading.running() {
            continue;
        }
        super::inbox::end(
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

/// One act turn, built: what [`note_act`] appends and [`heard_from_objects`] computes.
#[derive(Debug, Clone)]
pub struct ActLine {
    /// The event's own instant, never the clock's.
    pub at: crate::capability::Datetime,
    /// The actor the gate admitted.
    pub actor: String,
    /// Past tense, such as "dismissed" or "completed".
    pub verb: &'static str,
    /// What the turn's `references` array carries.
    pub reference: Reference,
    /// The write's own why, untrimmed; [`note_act`] trims and gates on it.
    pub note: String,
}

/// Append an act's note as a [`Turn::Act`](crate::capability::Turn::Act) stamped `by`, or
/// nothing when the note is empty.
///
/// # Errors
/// The store's own failures.
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
            turn: crate::capability::Turn::Act {
                text: describe_act(&crate::spelling::capitalised(&line.actor), line.verb, note),
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
    events: &[crate::record::TodoEvent],
    actor: &prodrome_core::event::Actor,
    chain: &crate::chains::ChainName,
    before: &dyn Fn(&crate::record::Authored) -> Option<crate::record::Authored>,
) -> Vec<ActLine> {
    use crate::record::TodoEvent;
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
                if crate::authoring::Reversal::of_note(&r.payload.note).is_some() =>
            {
                None
            }
            TodoEvent::Authored(r) => {
                let shown = before(r)?;
                let same = crate::record::Record {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{Datetime, Turn};
    use crate::conversation_id::ConversationId;
    use crate::mail::address::EmailAddress;
    use crate::reference::ProposalRef;
    use crate::secretary::acts::Act;
    use crate::secretary::proposal::{Proposal, Reasons};
    use crate::secretary::responded::{Done, Responded};
    use crate::secretary::store::Entry;

    use crate::store::Reads as _;

    /// An in-memory conversation store, since this crate reaches no filesystem.
    #[derive(Default)]
    struct InMemory {
        lines: crate::secretary::conversations::InMemory,
        inbox: crate::store::InMemory<crate::secretary::inbox_event::InboxEvent>,
    }

    impl crate::capability::Clock for InMemory {
        fn now(&self) -> Datetime {
            now()
        }
    }

    fn now() -> Datetime {
        Datetime::new(2026, 9, 14, 9, 0, 0, 0).expect("a real instant")
    }

    fn reasons() -> Reasons {
        Reasons::new(
            "the sync has failed for want of a grant",
            vec!["briefing:2026-09-13".parse().expect("a reference")],
        )
        .expect("a reason with a source")
    }

    fn opened_at(store: &InMemory, id: &ConversationId) {
        crate::secretary::transcript::start(
            store.lines.conversations(),
            id,
            &Entry::Opened {
                at: crate::capability::Datetime::new(2026, 9, 14, 8, 0, 0, 0).expect("an instant"),
                title: crate::secretary::words::Words::kept("A grant".to_owned()),
                model: "a fake model".to_owned(),
                kind: crate::secretary::store::Kind::Topic,
            },
        )
        .expect("opens");
    }

    /// A proposal of `act` in `id`, tapped once to `performed`.
    fn tapped(store: &InMemory, id: &ConversationId, act: Act, performed: Performed) {
        opened_at(store, id);
        let asked = crate::secretary::inbox::Asked {
            proposer: crate::secretary::proposer::Proposer::Secretary,
            proposal: Proposal::Act(act),
            reasons: reasons(),
            asked: ProposalRef {
                conversation: id.clone(),
                index: 0,
            },
            price: crate::secretary::inbox::Price::unpriced(),
        };
        let object =
            crate::secretary::inbox::ask(&store.inbox, asked, &transcript::writer(), now())
                .expect("filed");
        crate::secretary::transcript::push(
            store.lines.conversations(),
            id,
            &Entry::Proposed {
                at: crate::capability::Datetime::new(2026, 9, 14, 8, 59, 0, 0).expect("an instant"),
                object: object.clone(),
                revises: None,
            },
        )
        .expect("proposes");
        let owner = prodrome_core::event::Actor::new("ada").expect("an actor");
        crate::secretary::inbox::submit(&store.inbox, &object, &owner, now(), |_| Ok(performed))
            .expect("taps");
    }

    fn with_one_started(store: &InMemory) -> ConversationId {
        let id = ConversationId::new("2026-09-14-1").expect("a slug");
        tapped(
            store,
            &id,
            Act::Auth {
                account: EmailAddress::new("ada@example.com").expect("a shaped address"),
            },
            Performed::Tried(Tried::Running {
                began: "2026-09-14T09:00:00".to_owned(),
                expires_at: "2026-09-14T09:15:00".to_owned(),
            }),
        );
        id
    }

    /// What proposal 0 of `id` reads as answered.
    fn shown(
        store: &InMemory,
        id: &ConversationId,
    ) -> Option<Responded<crate::secretary::inbox::Check>> {
        let fold = crate::secretary::inbox::fold(&store.inbox).expect("the inbox reads");
        conversations::read(store.lines.conversations(), id)
            .expect("reads")
            .standing(0, &fold)
            .expect("proposal 0")
            .object
            .reading
            .shown()
            .map(|answer| answer.response.clone())
    }

    /// A running act ends as sent-nothing, with a matching `Turn::Act`, and counts once.
    #[test]
    fn a_standing_started_is_reconciled_once() {
        let store = InMemory::default();
        let id = with_one_started(&store);

        assert_eq!(
            reconcile(store.lines.conversations(), &store.inbox, now()).expect("reconciles"),
            1
        );

        match shown(&store, &id) {
            Some(Responded::Tried {
                tried: Tried::NotSent { reason },
                ..
            }) => {
                assert!(reason.contains("restarted"), "{reason}");
            }
            other => panic!("expected a try that sent nothing, got {other:?}"),
        }
        let stored = conversations::read(store.lines.conversations(), &id).expect("reads");
        let last = stored.turns.last().expect("a turn");
        match &last.turn {
            Turn::Act { text, .. } => assert!(text.contains("restarted"), "{text}"),
            other => panic!("expected an Act turn, got {other:?}"),
        }
    }

    #[test]
    fn running_reconcile_twice_appends_nothing_the_second_time() {
        let store = InMemory::default();
        let id = with_one_started(&store);

        assert_eq!(
            reconcile(store.lines.conversations(), &store.inbox, now()).expect("reconciles"),
            1
        );
        let after_first = conversations::read(store.lines.conversations(), &id)
            .expect("reads")
            .turns
            .len();

        assert_eq!(
            reconcile(store.lines.conversations(), &store.inbox, now()).expect("reconciles again"),
            0
        );
        let after_second = conversations::read(store.lines.conversations(), &id)
            .expect("reads again")
            .turns
            .len();
        assert_eq!(after_first, after_second, "nothing new was appended");
    }

    #[test]
    fn a_store_with_no_standing_started_is_untouched() {
        let store = InMemory::default();
        let acted_id = ConversationId::new("2026-09-14-2").expect("a slug");
        tapped(
            &store,
            &acted_id,
            Act::Auth {
                account: EmailAddress::new("ada@example.com").expect("a shaped address"),
            },
            Performed::Done(Done::Acted {
                what: "imap grant stored for ada@example.com".to_owned(),
            }),
        );
        let before = store.inbox.objects().expect("reads").len();

        assert_eq!(
            reconcile(store.lines.conversations(), &store.inbox, now()).expect("reconciles"),
            0
        );

        assert_eq!(store.inbox.objects().expect("reads").len(), before);
        assert!(matches!(
            shown(&store, &acted_id),
            Some(Responded::Done(Done::Acted { .. }))
        ));
    }

    fn a_todo_reference() -> Reference {
        Reference::Todo(TodoRef {
            chain: crate::chains::ChainName::new("life").expect("a slug"),
            id: prodrome_core::event::TodoId::new("x").expect("a todo id"),
        })
    }

    fn opened(store: &InMemory, id: &ConversationId) {
        crate::secretary::transcript::start(
            store.lines.conversations(),
            id,
            &Entry::Opened {
                at: crate::capability::Datetime::new(2026, 9, 15, 8, 0, 0, 0).expect("an instant"),
                title: crate::secretary::words::Words::kept("A chain act".to_owned()),
                model: "a fake model".to_owned(),
                kind: crate::secretary::store::Kind::Topic,
            },
        )
        .expect("opens");
    }

    /// Through the real writer, in the default conversation: a noted act is stamped by its actor,
    /// so a clean round the owner opens after it still writes the memory, and a mail read in
    /// the window still makes it a card.
    #[test]
    fn a_noted_act_in_the_default_conversation_leaves_his_round_clean() {
        use crate::capability::ToolCall;
        use crate::conversation_id::DEFAULT;
        let said = |turn: Turn, by: Option<&str>| Entry::Said {
            at: now(),
            turn,
            by: by.map(Into::into),
        };
        let person = |text: &str| Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        };
        let calling = |verb: &str| Turn::Model {
            text: String::new(),
            calls: vec![ToolCall {
                verb: verb.into(),
                args: std::collections::BTreeMap::new(),
            }],
        };
        let note = |store: &InMemory| {
            note_act(
                store.lines.conversations(),
                &DEFAULT,
                &ActLine {
                    at: now(),
                    actor: "ada".to_owned(),
                    verb: "dismissed",
                    reference: a_todo_reference(),
                    note: "not now".to_owned(),
                },
                Stamp::Actor(crate::secretary::stamp::Speaker::device("ada")),
            )
            .expect("appends");
        };
        let ask = |store: &InMemory| {
            for line in [
                said(person("remember I moved"), Some("ada")),
                said(calling("remember"), None),
            ] {
                conversations::append(store.lines.conversations(), &DEFAULT, &line).expect("said");
            }
        };
        let clean = InMemory::default();
        opened(&clean, &DEFAULT);
        note(&clean);
        ask(&clean);
        let stored = conversations::read(clean.lines.conversations(), &DEFAULT).expect("reads");
        assert!(matches!(
            &stored.turns[0].by,
            Some(Stamp::Actor(speaker)) if speaker.actor == "ada"
        ));
        assert!(!crate::secretary::taint::tainted(&stored));
        assert!(crate::secretary::taint::opener(&stored).is_some());

        let read = InMemory::default();
        opened(&read, &DEFAULT);
        conversations::append(
            read.lines.conversations(),
            &DEFAULT,
            &said(calling("search"), None),
        )
        .expect("said");
        conversations::append(
            read.lines.conversations(),
            &DEFAULT,
            &said(
                Turn::Tool {
                    verb: "search".into(),
                    answer: "an email".to_owned(),
                    outcome: crate::secretary::outcome::Outcome::Answered,
                    files: Vec::new(),
                },
                None,
            ),
        )
        .expect("said");
        note(&read);
        ask(&read);
        let stored = conversations::read(read.lines.conversations(), &DEFAULT).expect("reads");
        assert!(
            crate::secretary::taint::tainted(&stored),
            "a read in the window"
        );
    }

    /// Only the poster's own lifecycle acts with a note are heard: an empty note, an event naming a
    /// different actor and a non-lifecycle event are each skipped.
    #[test]
    fn heard_from_objects_skips_what_carries_no_note_of_the_posters() {
        let actor = prodrome_core::event::Actor::new("ada").expect("an actor");
        let chain = crate::chains::ChainName::new("life").expect("a slug");
        for event in [
            prodrome_core::event::mk_cancelled::<crate::record::Record>("x", now(), "ada", "")
                .expect("a cancelled event"),
            prodrome_core::event::mk_reopened::<crate::record::Record>(
                "x",
                now(),
                "triage",
                "wrong call",
            )
            .expect("a reopened event"),
            prodrome_core::event::mk_created::<crate::record::Record>(
                "x",
                now(),
                "ada",
                "text",
                "a note",
            )
            .expect("a created event"),
        ] {
            assert!(
                heard_from_objects(std::slice::from_ref(&event), &actor, &chain, &|_| None)
                    .is_empty(),
                "{event:?}"
            );
        }
    }

    /// An adopted record with the poster's note confirms the record shown before it when its
    /// content is the same, and updates it otherwise; a creation and an undo are not noted.
    #[test]
    fn an_adopted_record_is_noted_by_what_it_replaces() {
        let actor = prodrome_core::event::Actor::new("ada").expect("an actor");
        let chain = crate::chains::ChainName::new("life").expect("a slug");
        let body = |text: &str| crate::record::MarkupSource::from_chain(text);
        let Ok(crate::record::TodoEvent::Authored(shown)) = crate::record::mk_authored(
            "x",
            now(),
            "triage",
            "todo",
            now(),
            body("File it"),
            prodrome_core::fpl::mk_flat(0.5).expect("a term"),
            vec!["why".to_owned()],
            "",
            "",
            crate::record::MarkupSource::empty(),
            None,
            Vec::new(),
            Vec::new(),
            "",
        ) else {
            unreachable!("mk_authored answers an Authored")
        };
        let posted = |text: &str, note: &str| {
            let mut record = (*shown).clone();
            record.actor = actor.clone();
            record.payload.body = body(text);
            record.payload.note = note.to_owned();
            crate::record::TodoEvent::Authored(Box::new(record))
        };
        let verbs = |event, known: bool| {
            let before = |_: &crate::record::Authored| known.then(|| (*shown).clone());
            heard_from_objects(&[event], &actor, &chain, &before)
                .into_iter()
                .map(|line| line.verb)
                .collect::<Vec<_>>()
        };
        assert_eq!(verbs(posted("File it", "seen"), true), ["confirmed"]);
        assert_eq!(verbs(posted("File it now", "seen"), true), ["updated"]);
        assert!(verbs(posted("File it", "seen"), false).is_empty());
        let of = prodrome_core::event::Hash::new("a".repeat(64)).expect("a hash");
        let undo = crate::authoring::Reversal::Undo { of }.note();
        assert!(verbs(posted("File it now", &undo), true).is_empty());
    }
}
