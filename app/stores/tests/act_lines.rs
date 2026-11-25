//! The act lines' laws, over the conversations and the inbox held in core's memory.

use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::ProposalRef;
use suzatary_base::reference::{Reference, TodoRef};
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::Entry;
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::proposal::{Proposal, Reasons};
use suzatary_inbox::secretary::responded::{Done, Responded};
use suzatary_inbox::secretary::responded::{Performed, Tried};
use suzatary_inbox::secretary::stamp::Stamp;
use suzatary_lines::secretary::act_lines::*;
use suzatary_lines::secretary::{conversations, transcript};
use suzatary_mail::mail::address::EmailAddress;

use suzatary_conversations::secretary::taint;
use suzatary_inbox::secretary::inbox;
use suzatary_store::store::Reads as _;

/// An in-memory conversation store, since this crate reaches no filesystem.
#[derive(Default)]
struct InMemory {
    lines: suzatary_stores::stores::conversations::InMemory,
    inbox: suzatary_stores::stores::InMemory<suzatary_inbox::secretary::inbox_event::InboxEvent>,
}

impl suzatary_base::capability::Clock for InMemory {
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
    suzatary_lines::secretary::transcript::start(
        store.lines.conversations(),
        id,
        &Entry::Opened {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 8, 0, 0, 0)
                .expect("an instant"),
            title: suzatary_memory::secretary::words::Words::kept("A grant".to_owned()),
            model: "a fake model".to_owned(),
            kind: suzatary_conversations::secretary::store::Kind::Topic,
        },
    )
    .expect("opens");
}

/// A proposal of `act` in `id`, tapped once to `performed`.
fn tapped(store: &InMemory, id: &ConversationId, act: Act, performed: Performed) {
    opened_at(store, id);
    let asked = inbox::Asked {
        proposer: suzatary_inbox::secretary::proposer::Proposer::Secretary,
        proposal: Proposal::Act(act),
        reasons: reasons(),
        asked: ProposalRef {
            conversation: id.clone(),
            index: 0,
        },
        price: inbox::Price::unpriced(),
    };
    let object = inbox::ask(&store.inbox, asked, &transcript::writer(), now()).expect("filed");
    suzatary_lines::secretary::transcript::push(
        store.lines.conversations(),
        id,
        &Entry::Proposed {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 8, 59, 0, 0)
                .expect("an instant"),
            object: object.clone(),
            revises: None,
        },
    )
    .expect("proposes");
    let owner = prodrome_data::event::Actor::new("ada").expect("an actor");
    inbox::submit(&store.inbox, &object, &owner, now(), |_| Ok(performed)).expect("taps");
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
fn shown(store: &InMemory, id: &ConversationId) -> Option<Responded<inbox::Check>> {
    let fold = inbox::fold(&store.inbox).expect("the inbox reads");
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
        chain: suzatary_base::chains::ChainName::new("life").expect("a slug"),
        id: prodrome_data::event::TodoId::new("x").expect("a todo id"),
    })
}

fn opened(store: &InMemory, id: &ConversationId) {
    suzatary_lines::secretary::transcript::start(
        store.lines.conversations(),
        id,
        &Entry::Opened {
            at: suzatary_base::capability::Datetime::new(2026, 9, 15, 8, 0, 0, 0)
                .expect("an instant"),
            title: suzatary_memory::secretary::words::Words::kept("A chain act".to_owned()),
            model: "a fake model".to_owned(),
            kind: suzatary_conversations::secretary::store::Kind::Topic,
        },
    )
    .expect("opens");
}

/// Through the real writer, in the default conversation: a noted act is stamped by its actor,
/// so a clean round the owner opens after it still writes the memory, and a mail read in
/// the window still makes it a card.
#[test]
fn a_noted_act_in_the_default_conversation_leaves_his_round_clean() {
    use suzatary_base::conversation_id::DEFAULT;
    use suzatary_conversations::secretary::model::ToolCall;
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
            Stamp::Actor(suzatary_inbox::secretary::stamp::Speaker::device("ada")),
        )
        .expect("appends");
    };
    let ask = |store: &InMemory| {
        for line in [
            said(person("remember I moved"), Some("ada")),
            said(calling("remember"), None),
        ] {
            suzatary_lines::secretary::transcript::push_older(
                store.lines.conversations(),
                &DEFAULT,
                &line,
            )
            .expect("said");
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
    assert_eq!(taint::held(&stored), taint::Taint::Clean);
    assert!(taint::opener(&stored).is_some());

    let read = InMemory::default();
    opened(&read, &DEFAULT);
    suzatary_lines::secretary::transcript::push_older(
        read.lines.conversations(),
        &DEFAULT,
        &said(calling("search"), None),
    )
    .expect("said");
    suzatary_lines::secretary::transcript::push_older(
        read.lines.conversations(),
        &DEFAULT,
        &said(
            Turn::Tool {
                verb: "search".into(),
                answer: "an email".to_owned(),
                outcome: suzatary_conversations::secretary::outcome::Outcome::Answered,
                files: Vec::new(),
            },
            None,
        ),
    )
    .expect("said");
    note(&read);
    ask(&read);
    let stored = conversations::read(read.lines.conversations(), &DEFAULT).expect("reads");
    assert_eq!(
        taint::held(&stored),
        taint::Taint::Tainted,
        "a read in the window"
    );
}

/// Only the poster's own lifecycle acts with a note are heard: an empty note, an event naming a
/// different actor and a non-lifecycle event are each skipped.
#[test]
fn heard_from_objects_skips_what_carries_no_note_of_the_posters() {
    let actor = prodrome_data::event::Actor::new("ada").expect("an actor");
    let chain = suzatary_base::chains::ChainName::new("life").expect("a slug");
    for event in [
        prodrome_todo::event::mk_cancelled::<suzatary_record::record::Record>(
            "x",
            now(),
            "ada",
            "",
        )
        .expect("a cancelled event"),
        prodrome_todo::event::mk_reopened::<suzatary_record::record::Record>(
            "x",
            now(),
            "triage",
            "wrong call",
        )
        .expect("a reopened event"),
        prodrome_todo::event::mk_created::<suzatary_record::record::Record>(
            "x",
            now(),
            "ada",
            "text",
            "a note",
        )
        .expect("a created event"),
    ] {
        assert!(
            heard_from_objects(std::slice::from_ref(&event), &actor, &chain, &|_| None).is_empty(),
            "{event:?}"
        );
    }
}

/// An adopted record with the poster's note confirms the record shown before it when its
/// content is the same, and updates it otherwise; a creation and an undo are not noted.
#[test]
fn an_adopted_record_is_noted_by_what_it_replaces() {
    let actor = prodrome_data::event::Actor::new("ada").expect("an actor");
    let chain = suzatary_base::chains::ChainName::new("life").expect("a slug");
    let body = |text: &str| suzatary_record::record::MarkupSource::from_chain(text);
    let Ok(suzatary_record::record::TodoEvent::Authored(shown)) =
        suzatary_record::record::mk_authored(
            "x",
            now(),
            "triage",
            "todo",
            now(),
            body("File it"),
            prodrome_term::fpl::mk_flat(0.5).expect("a term"),
            vec!["why".to_owned()],
            "",
            "",
            suzatary_record::record::MarkupSource::empty(),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
    else {
        unreachable!("mk_authored answers an Authored")
    };
    let posted = |text: &str, note: &str| {
        let mut record = (*shown).clone();
        record.actor = actor.clone();
        record.payload.body = body(text);
        record.payload.note = note.to_owned();
        suzatary_record::record::TodoEvent::Authored(Box::new(record))
    };
    let verbs = |event, known: bool| {
        let before = |_: &suzatary_record::record::Authored| known.then(|| (*shown).clone());
        heard_from_objects(&[event], &actor, &chain, &before)
            .into_iter()
            .map(|line| line.verb)
            .collect::<Vec<_>>()
    };
    assert_eq!(verbs(posted("File it", "seen"), true), ["confirmed"]);
    assert_eq!(verbs(posted("File it now", "seen"), true), ["updated"]);
    assert!(verbs(posted("File it", "seen"), false).is_empty());
    let of = prodrome_data::event::Hash::new("a".repeat(64)).expect("a hash");
    let undo = suzatary_todo::authoring::Reversal::Undo { of }.note();
    assert!(verbs(posted("File it now", &undo), true).is_empty());
}
