use crate::common::*;

/// A link to store (a typed value), a grant (a confirmed yes), or an email (a confirmed send),
/// standing, failed, or answered.
pub(crate) fn proposal(index: usize) -> impl Strategy<Value = Value> {
    (
        prop::sample::select(vec!["secret", "grant", "email"]),
        prop::sample::select(vec![
            "standing",
            "failed",
            "refused",
            "unfinished",
            "unknown",
            "sent",
            "doneelsewhere",
        ]),
        prop::sample::select(vec![json!({"kind": "setup"}), json!({"kind": "secretary"}), json!({"kind": "triage"})]),
        prop::sample::select(vec!["", "This changeset no longer applies"]),
        prop::sample::select(vec![
            json!(null),
            json!({"kind": "page", "label": "Where to get it", "href": "https://example.test/token"}),
            json!({"kind": "setup", "label": "Set it up", "template": "microsoft"}),
        ]),
    )
        .prop_map(move |(kind, state, by, problem, link)| {
            let (act, answer) = match kind {
                "secret" => (
                    json!({"kind": "secret", "label": "the calendar's link", "found": null, "link": null}),
                    json!({"verb": "store", "label": "Store", "done": "Stored", "asks": "value", "input": "The link"}),
                ),
                "grant" => (
                    json!({"kind": "grant", "plugin": "ics", "origin": "https://cal.example.test", "sentence": "Allow it."}),
                    json!({"verb": "allow", "label": "Allow", "done": "Allowed", "asks": "confirm", "question": "Allow it?", "yes": "Allow"}),
                ),
                _ => (
                    json!({"kind": "email", "account": "me@example.test", "to": ["ada@example.test"], "cc": [], "subject": "Hello", "body": "Dear Ada", "reply_to": null, "attachments": [], "held": {"type": "Read", "values": []}, "sentence": "Send it."}),
                    json!({"verb": "send", "label": "Send", "done": "Sent", "asks": "confirm", "question": "Send to ada@example.test?", "yes": "Send"}),
                ),
            };
            json!({"index": index, "act": act, "why": "Because", "sources": ["todo:life/renewcard"], "at": "2026-10-05T04:00:00",
                "state": state, "outcome": "", "tries": 0, "turn": 1, "coding": null, "announced": null, "revises": null,
                "card": {"title": "A card", "by": by, "sentence": "Do it.", "fields": [{"label": "To", "value": "ada@example.test"}],
                    "body": "", "rows": [], "files": [], "problem": problem, "answers": [answer], "uncertain": "Check Sent.", "check": "I checked Sent", "failed": "Not sent",
                    "readonly": "You may read this.", "link": link}})
        })
}

pub(crate) fn conversation_cards(id: &'static str) -> impl Strategy<Value = Value> {
    proptest::collection::vec(any::<bool>(), 0..3).prop_flat_map(move |shown| {
        let rows: Vec<_> = (0..shown.len()).map(proposal).collect();
        rows.prop_map(move |proposals| {
            json!({"id": id, "title": "Setup", "model": "", "opened": "", "turns": [], "proposals": proposals,
                "unplaced": [], "state": "idle", "first": 0, "total": 0, "again": null})
        })
    })
}

pub(crate) fn inbox_read() -> impl Strategy<Value = Value> {
    proptest::collection::vec(
        (
            prop::sample::select(vec!["setup", "talk"]),
            0usize..3,
            prop::sample::select(vec![json!({"kind": "open"}), json!({"kind": "completed", "at": "2026-10-05T04:00:00"}), json!({"kind": "unreadable", "reason": "garbled"})]),
            prop::sample::select(vec![json!(0.4), json!(null), json!(0.8)]),
        ),
        0..4,
    )
    .prop_map(|ps| {
        let proposals: Vec<Value> = ps
            .into_iter()
            .enumerate()
            .map(|(n, (c, i, state, value))| {
                json!({"id": format!("p{n}"), "asked": format!("proposal:{c}/{i}"), "serves": "todo:life/renewcard",
                    "headline": "A card", "by": {"kind": "setup"}, "why": "Because", "state": state, "value": value,
                    "at": "2026-10-05T04:00:00"})
            })
            .collect();
        json!({"payload": {"at": "2026-10-05T04:00:00", "proposals": proposals, "failed": []}, "held": ["life"]})
    })
}

pub(crate) fn watched() -> impl Strategy<Value = (Watched, Value)> {
    prop_oneof![
        inbox_read().prop_map(|v| (Watched::Proposals, v)),
        conversation_cards("setup").prop_map(|v| (Watched::Cards("setup".to_owned()), v)),
        conversation_cards("talk").prop_map(|v| (Watched::Cards("talk".to_owned()), v)),
        Just((Watched::Conversations, json!({"conversations": []}))),
    ]
}

pub(crate) fn card_ev() -> impl Strategy<Value = (card::Key, card::Ev)> {
    (
        prop::sample::select(vec!["setup", "talk"]),
        0u32..3,
        prop_oneof![
            Just(card::Ev::Open(card::Step::Confirming {
                verb: "allow".to_owned()
            })),
            Just(card::Ev::Open(card::Step::Confirming {
                verb: "send".to_owned()
            })),
            Just(card::Ev::Open(card::Step::Saying {
                verb: "dismiss".to_owned()
            })),
            prop::sample::select(vec!["", "not yet"]).prop_map(|t| card::Ev::Typed(t.to_owned())),
            Just(card::Ev::Close),
            any::<bool>().prop_map(card::Ev::Details),
            (0usize..2).prop_map(card::Ev::Yes),
            any::<bool>().prop_map(card::Ev::Value),
            Just(card::Ev::Dismiss),
            Just(card::Ev::Acknowledge),
            Just(card::Ev::Reply),
            Just(card::Ev::Setup),
        ],
    )
        .prop_map(|(c, index, ev)| {
            (
                card::Key {
                    conversation: c.to_owned(),
                    index,
                },
                ev,
            )
        })
}
