//! Each page's two laws (`viewtree::dom::page_laws`) over generated runs, with the box's answers
//! generated as the JSON it prints: refusals, failures, wrong shapes, answers that move.

use proptest::prelude::*;
use serde_json::{Value, json};
use suzatary_core::release::{Lot, Named, Place};
use suzatary_core::secretary::answer::Kind as BlockKind;
use suzatary_core::secretary::memory::{FactId, Kind};
use suzatary_core::wire::{
    AboutPayload, BlockRow, BriefRow, BriefingDay, BriefingPayload, BriefingRow, BriefingsPayload,
    CalendarPayload, EntryState, FactRow, FactSaid, Fulfillment, HeldTodo, MemoryPayload,
    Neighbour, RecordPayload, RecordSummary, ReleaseItem, ReleaseRow, ReleasesPayload, SaidOn,
};
use suzatary_pages::reading::{Nothing, One, Opened};
use suzatary_pages::runtime::{
    Answer, Behind, Body, Context, Effect, Gate, Input, Reach, Untrusted,
};
use suzatary_pages::todoparts::{Seal, Verb};
use suzatary_pages::waiting::Watched;
use suzatary_pages::zone::Offset;
use suzatary_pages::{
    about, briefing, briefings, calendar, card, chats, composer, conversation, draft, inbox, mail,
    memory, message, needsyou, reading as one, secretary, settings, thread, todo, todos, typstlist,
};
use viewtree::Runtime;
use viewtree::dom::page_laws;
use viewtree::kit::Reading;

fn context(writes: bool) -> Context {
    Context {
        gate: if writes { Gate::Writes } else { Gate::Reads },
        reach: if writes {
            Reach::Writes
        } else {
            Reach::Reads {
                said: "This device is not trusted to write yet.".to_owned(),
                settings: true,
                key: Untrusted::Untrusted,
            }
        },
        offsets: vec![
            Offset {
                from: 0,
                minutes: 60,
            },
            Offset {
                from: 1_790_000_000_000,
                minutes: 120,
            },
        ],
        wide: writes,
        categories: vec!["home".to_owned(), "work".to_owned()],
    }
}

fn row() -> impl Strategy<Value = Value> {
    (
        0u8..6,
        prop::sample::select(vec!["ada@example.test", "@odd", "bob"]),
        prop::sample::select(vec!["", "  ", "hello"]),
        prop::sample::select(vec!["", "2026-09-01T08:30:00", "2026-11-01T08:30:00"]),
        prop::sample::select(vec!["inbox", "archive"]),
        prop::sample::select(vec!["", "me@example.test"]),
        prop::sample::select(vec![
            json!({"kind": "undecided"}),
            json!({"kind": "todo", "todo": "renewcard"}),
            json!({"kind": "noise"}),
            json!({"kind": "skip"}),
        ]),
        prop::sample::select(vec![json!([]), json!(["ssn", "code"])]),
    )
        .prop_map(|(n, sender, subject, date, container, account, disposition, held)| {
            json!({"hash": format!("h{n}"), "id": format!("i{n}"), "sender": sender, "subject": subject,
                "date": date, "container": container, "account": account,
                "disposition": disposition, "held": held})
        })
}

fn page() -> impl Strategy<Value = Value> {
    (
        row(),
        prop::sample::select(vec!["", "t1"]),
        prop::sample::select(vec!["", "Dear Ada, [SSN ending 3456 · withheld]"]),
        any::<bool>(),
        prop::sample::select(vec![
            json!([]),
            json!([{"url": "https://example.test/a", "label": "a"}, {"url": "mailto:x@y", "label": ""}]),
        ]),
    )
        .prop_map(|(row, thread, body, truncated, links)| {
            json!({"row": row, "thread": thread, "body": body, "truncated": truncated,
                "links": links, "headers": [{"name": "From", "value": "ada@example.test"}], "todos": []})
        })
}

fn behind() -> impl Strategy<Value = Option<Behind>> {
    proptest::option::of(
        proptest::option::of(Just("offline".to_owned())).prop_map(|why| Behind {
            why,
            since: 1_780_000_000_000,
        }),
    )
}

fn reading(answer: impl Strategy<Value = Value>) -> impl Strategy<Value = Reading<Answer>> {
    prop_oneof![
        Just(Reading::Loading),
        Just(Reading::Error {
            e: "no answer".to_owned()
        }),
        (prop::sample::select(vec![400u16, 404])).prop_map(|status| Reading::Val {
            v: Answer::Refused {
                status,
                words: "no such message".to_owned()
            }
        }),
        Just(Reading::Val {
            v: Answer::Done {
                body: json!({"shape": "another"})
            }
        }),
        answer.prop_map(|body| Reading::Val {
            v: Answer::Done { body }
        }),
    ]
}

fn listing() -> impl Strategy<Value = Value> {
    (
        proptest::collection::vec(row(), 0..6),
        proptest::collection::vec(prop::sample::select(vec!["me@example.test", "her@example.test"]), 0..3),
    )
        .prop_map(|(messages, accounts)| json!({"q": "from:ada", "accounts": accounts, "messages": messages}))
}

fn watches() -> impl Strategy<Value = Value> {
    proptest::collection::vec(
        (0u8..3, any::<bool>()).prop_map(|(n, sender)| {
            json!({"id": format!("w{n}"), "on": if sender { "sender" } else { "subject" },
                "text": "ada", "set": "2026-10-01T00:00:00Z", "until": "2026-12-01T00:00:00Z"})
        }),
        0..3,
    )
    .prop_map(|ws| json!({"watches": ws}))
}

fn mail_msg() -> impl Strategy<Value = mail::Msg> {
    let host = |i: Input<mail::Hold, mail::Ask>| mail::Msg::Host(Box::new(i));
    prop_oneof![
        (reading(listing()), behind()).prop_map(move |(reading, behind)| host(Input::Held {
            ask: mail::Hold::First,
            reading,
            behind
        })),
        (reading(watches()), behind()).prop_map(move |(reading, behind)| host(Input::Held {
            ask: mail::Hold::Watches,
            reading,
            behind
        })),
        (
            prop::sample::select(vec!["", "i0/inbox", "i3/archive"]),
            reading(listing())
        )
            .prop_map(move |(after, r)| host(Input::Answered {
                ask: mail::Ask::More {
                    after: after.to_owned()
                },
                answer: match r {
                    Reading::Val { v } => v,
                    Reading::Loading | Reading::Error { .. } => Answer::Refused {
                        status: 0,
                        words: "offline".to_owned()
                    },
                }
            })),
        (0u8..3, reading(watches())).prop_map(move |(n, r)| host(Input::Answered {
            ask: mail::Ask::Cancel {
                id: format!("w{n}")
            },
            answer: match r {
                Reading::Val { v } => v,
                Reading::Loading | Reading::Error { .. } => Answer::Refused {
                    status: 403,
                    words: "this login only reads".to_owned()
                },
            }
        })),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
        prop::sample::select(vec!["", "from:ada", "subject:\"ünï\""])
            .prop_map(|s| mail::Msg::Typed(s.to_owned())),
        Just(mail::Msg::Search),
        Just(mail::Msg::More),
        Just(mail::Msg::Retry),
        (0u8..3).prop_map(|n| mail::Msg::Cancel(format!("w{n}"))),
    ]
}

fn held<H: Clone + std::fmt::Debug + 'static, A: std::fmt::Debug, S: std::fmt::Debug>(
    ask: H,
    answer: impl Strategy<Value = Value>,
) -> impl Strategy<Value = Input<H, A, S>> {
    (reading(answer), behind()).prop_map(move |(reading, behind)| Input::Held {
        ask: ask.clone(),
        reading,
        behind,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn mail_laws(writes: bool, searching: bool, msgs in proptest::collection::vec(mail_msg(), 0..12)) {
        page_laws::<mail::Mail>(
            || mail::Flags {
                context: context(writes),
                query: if searching { "from:ada".to_owned() } else { String::new() },
                account: String::new(),
            },
            &msgs,
        );
    }

    #[test]
    fn message_laws(writes: bool, msgs in one_msgs(page())) {
        page_laws::<One<message::Message>>(|| opened(writes, message::At { id: "i1@example.test".to_owned() }), &msgs);
    }

    #[test]
    fn thread_laws(msgs in one_msgs(
        proptest::collection::vec(page(), 0..4).prop_map(|ms| json!({"thread": "t1", "messages": ms})),
    )) {
        page_laws::<One<thread::Thread>>(|| opened(true, thread::At { id: "t1".to_owned() }), &msgs);
    }
}

/// A second Cancel tap asks nothing until the first is answered; a refusal lets it be tapped again.
#[test]
fn a_cancel_runs_once_until_it_is_answered() {
    let (mut rt, _) = Runtime::<mail::Mail>::start(mail::Flags {
        context: context(true),
        query: String::new(),
        account: String::new(),
    });
    let cancel = || mail::Msg::Cancel("w0".to_owned());
    let (_, first) = rt.step(cancel());
    let (_, second) = rt.step(cancel());
    assert_eq!(
        first,
        vec![Effect::Post {
            ask: mail::Ask::Cancel {
                id: "w0".to_owned()
            },
            path: "/api/mail/watches/w0/cancel".to_owned(),
            body: Body::Empty {},
        }]
    );
    assert_eq!(second, Vec::new());
    let refused = Input::Answered {
        ask: mail::Ask::Cancel {
            id: "w0".to_owned(),
        },
        answer: Answer::Refused {
            status: 403,
            words: "This login only reads.".to_owned(),
        },
    };
    let (_, said) = rt.step(mail::Msg::Host(Box::new(refused)));
    assert_eq!(
        said,
        vec![Effect::Toast {
            words: "This login only reads.".to_owned()
        }]
    );
    assert_eq!(
        rt.step(cancel()).1.len(),
        1,
        "a refused act may be tapped again"
    );
}

/// The address carries the query trimmed and the listing's account, encoded as the router reads.
#[test]
fn a_search_goes_to_its_address() {
    let (mut rt, first) = Runtime::<mail::Mail>::start(mail::Flags {
        context: context(true),
        query: String::new(),
        account: "me@example.test".to_owned(),
    });
    assert_eq!(
        first.effects.first(),
        Some(&Effect::Hold {
            ask: mail::Hold::First,
            path: "/api/mail/inbox?limit=50&account=me%40example.test".to_owned(),
        })
    );
    rt.step(mail::Msg::Typed(" from:ada subject:\"a/b\" ".to_owned()));
    assert_eq!(
        rt.step(mail::Msg::Search).1,
        vec![Effect::Go {
            href: "#/mail?q=from%3Aada%20subject%3A%22a%2Fb%22&account=me%40example.test"
                .to_owned()
        }]
    );
}

// The small pages: answers built from the core's types and printed as the box prints them.

fn words() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("one".to_owned()),
        Just("two words".to_owned()),
        Just("<b>three</b> & more".to_owned()),
    ]
}

fn printed<T: serde::Serialize>(t: &T) -> Value {
    serde_json::to_value(t).expect("a payload prints")
}

fn said() -> impl Strategy<Value = SaidOn> {
    prop_oneof![
        (0u32..3).prop_map(|turn| SaidOn::Turn {
            conversation: "c1".to_owned(),
            turn
        }),
        (0u32..3).prop_map(|proposal| SaidOn::Tapped {
            conversation: "c 2".to_owned(),
            proposal
        }),
    ]
}

fn memory_payload() -> impl Strategy<Value = Value> {
    let kind = prop_oneof![
        Just(Kind::Owner),
        Just(Kind::Person),
        Just(Kind::Preference),
        Just(Kind::Standing)
    ];
    let source = prop_oneof![
        said().prop_map(|on| FactSaid::Said { on }),
        (said(), said()).prop_map(|(o, round)| FactSaid::Merged {
            from: vec![FactId(1), FactId(2)],
            origins: vec![o],
            round
        }),
    ];
    let fact = (0u64..5, words(), kind, source).prop_map(|(id, fact, kind, said)| FactRow {
        id: FactId(id),
        at: "2026-09-01T00:00:00".to_owned(),
        fact,
        kind,
        said,
    });
    proptest::collection::vec(fact, 0..5).prop_map(|facts| {
        printed(&MemoryPayload {
            facts,
            cap: 40,
            forgotten: Vec::new(),
            merged: Vec::new(),
            cards: Vec::new(),
        })
    })
}

fn record_payload() -> impl Strategy<Value = Value> {
    let summary = (0u8..3, 1u32..3, words()).prop_map(|(n, lines, text)| RecordSummary {
        node: format!("n{n}"),
        handle: "1+2".to_owned(),
        lines,
        text,
    });
    proptest::collection::vec(summary, 0..3)
        .prop_map(|summaries| printed(&RecordPayload { summaries }))
}

fn calendar_payload() -> impl Strategy<Value = Value> {
    (words(), proptest::option::of(words())).prop_map(|(text, rich)| {
        printed(&CalendarPayload {
            from: "2026-09-07".to_owned(),
            days: 7,
            text,
            rich,
        })
    })
}

fn briefings_payload() -> impl Strategy<Value = Value> {
    let saved = (1u8..4, words()).prop_map(|(d, title)| BriefingRow {
        date: format!("2026-08-0{d}"),
        title,
        words: 120,
    });
    let brief = (1u8..4, words()).prop_map(|(d, title)| BriefRow {
        date: format!("2026-09-1{d}"),
        conversation: "c1".to_owned(),
        turn: 1,
        title,
        words: 1,
        text: String::new(),
    });
    (
        proptest::collection::vec(saved, 0..3),
        proptest::collection::vec(brief, 0..3),
    )
        .prop_map(|(briefings, briefs)| printed(&BriefingsPayload { briefings, briefs }))
}

fn briefing_payload() -> impl Strategy<Value = Value> {
    let block = (words(), proptest::option::of(words())).prop_map(|(source, html)| BlockRow {
        plain: source.clone(),
        source,
        kind: BlockKind::Prose,
        html,
    });
    let day = prop_oneof![
        (words(), proptest::collection::vec(block, 0..3)).prop_map(|(text, blocks)| {
            BriefingDay::Brief {
                conversation: "c1".to_owned(),
                turn: 2,
                text,
                blocks,
            }
        }),
        (words(), proptest::option::of(words()))
            .prop_map(|(source, rich)| BriefingDay::Saved { source, rich }),
    ];
    day.prop_map(|day| {
        printed(&BriefingPayload {
            date: "2026-09-04".to_owned(),
            day,
        })
    })
}

fn held_todo() -> impl Strategy<Value = Option<HeldTodo>> {
    let value = prop_oneof![
        Just(None),
        (0u8..=10).prop_map(|n| Some(Fulfillment::Number(f64::from(n) / 10.0))),
        Just(Some(Fulfillment::Absent(
            suzatary_core::wire::Absent::Absent
        ))),
    ];
    proptest::option::of((value, words()).prop_map(|(value, title)| HeldTodo {
        row: Neighbour {
            todo: "t".to_owned(),
            state: EntryState::Open,
            value,
            unconfirmed: false,
        },
        title,
    }))
}

fn lot() -> impl Strategy<Value = Lot> {
    prop_oneof![Just(Lot::Open), Just(Lot::Cancelled), Just(Lot::Completed)]
}

fn releases_payload() -> impl Strategy<Value = Value> {
    let item =
        (0u8..4, held_todo(), lot(), any::<bool>()).prop_map(|(n, held, lot, now)| ReleaseItem {
            todo: format!("i{n}"),
            held,
            lot,
            named: if now { Named::Current } else { Named::Dropped },
        });
    let place = prop_oneof![Just(Place::Earlier), Just(Place::Next), Just(Place::Later)];
    let release = (
        0u8..3,
        held_todo(),
        lot(),
        place,
        proptest::collection::vec(item, 0..4),
    )
        .prop_map(|(n, held, lot, place, items)| ReleaseRow {
            todo: format!("r{n}"),
            held,
            version: "2.3".to_owned(),
            lot,
            place,
            items,
        });
    proptest::collection::vec(release, 0..3).prop_map(|releases| {
        printed(&ReleasesPayload {
            chain: "suzatary".to_owned(),
            reads: "2.2".to_owned(),
            releases,
        })
    })
}

fn todo_entry() -> impl Strategy<Value = Value> {
    (
        prop::sample::select(vec!["open", "completed"]),
        any::<bool>(),
        any::<bool>(),
        prop::sample::select(vec![json!(0.3), json!("absent"), Value::Null]),
        0usize..3,
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(state, unconfirmed, rendered, value, subs, waiting, conflicted)| {
            let html = |v: &str| if rendered { json!(format!("<p>{v}</p>")) } else { Value::Null };
            let key = |v: &str| if unconfirmed { Value::Null } else { json!(format!("k-{v}")) };
            json!({
                "todo": "renewcard", "state": state, "states": if conflicted { json!([state, "open"]) } else { json!([state]) },
                "at": if state == "open" { "" } else { "2026-09-02T10:00:00" }, "claimed": "", "value": value,
                "recurring": false, "unconfirmed": unconfirmed,
                "conflicts": if conflicted { json!({"Authored": ["a1", "a2"]}) } else { json!({}) },
                "authored": {
                    "rich": {"body": html("Renew"), "detail": html("Soon"), "rationale": Value::Null, "waiting_on": Value::Null,
                        "keys": {"body": key("body"), "detail": key("detail"), "rationale": Value::Null, "waiting_on": Value::Null}},
                    "block": {"kind": "todo", "name": "todo"}, "created": "2026-08-01", "body": "*Renew* the card", "text": "Renew the card",
                    "detail": "Soon", "category": "home", "waiting_on": if waiting { "the bank" } else { "" }, "rationale": ["it expires"],
                    "spec": {"kind": "recur", "todo": "renewcard", "term": {"kind": "curve", "points": [
                        {"at": "2026-09-01T00:00:00", "value": 0.7}, {"at": "2026-09-15T00:00:00", "value": 0.3}]}},
                    "actor": "ada", "at": "2026-08-01T09:00:00", "source": Value::Null,
                    "subtodos": (0..subs).map(|i| json!({"body": format!("step {i}"), "done": i == 0})).collect::<Vec<_>>(),
                },
                "candidates": [], "stream": ["a1"],
            })
        })
}

fn todo_payload() -> impl Strategy<Value = Value> {
    (todo_entry(), any::<bool>(), any::<bool>()).prop_map(|(entry, priced, builder)| {
        json!({
            "todo": "renewcard", "at": "2026-09-04T12:00:00", "entry": entry,
            "why": if priced {
                json!({"type": "Priced", "heading": "Why this number — 30.0%", "says": "30.0%: a ramp.", "passes": ["Last pass: never."],
                    "parts": [{"says": "a: 30.0%", "parts": []}]})
            } else {
                json!({"type": "Unpriced", "says": "The chain records no priority spec for this todo."})
            },
            "stream": [
                {"hash": "a1", "parents": [], "literal": "Created(...)", "holds": "event", "kind": "Created", "todo": "renewcard",
                    "actor": "ada", "at": "2026-08-01T09:00:00", "note": "", "reverses": {"kind": "nothing"}},
                {"hash": "a2", "parents": ["a1"], "literal": "Completed(...)", "holds": "event", "kind": "Completed", "todo": "renewcard",
                    "actor": "ada", "at": "2026-08-02T09:00:00", "note": "", "reverses": {"kind": "undo", "of": "a1"}},
            ],
            "diffs": {"a1": [{"field": "body", "before": "", "after": "'Renew'"}]},
            "links": {"out": [{"todo": "other", "state": "open", "value": 0.5, "unconfirmed": false}], "in": []},
            "builder": if builder { json!("build") } else { Value::Null },
        })
    })
}

fn todo_write() -> impl Strategy<Value = todo::Write> {
    prop_oneof![
        prop::sample::select(vec![Verb::Done, Verb::Cancel, Verb::Reopen, Verb::Confirm]).prop_map(
            |verb| todo::Write::Act {
                acting: suzatary_pages::todoparts::Acting::Verb(verb)
            }
        ),
        Just(todo::Write::Act {
            acting: suzatary_pages::todoparts::Acting::Undo
        }),
        Just(todo::Write::Save),
        Just(todo::Write::Defer),
        (0usize..3).prop_map(|line| todo::Write::Tick { line }),
    ]
}

fn todo_answer() -> impl Strategy<Value = Input<todo::Hold, todo::Ask>> {
    todo_asked().prop_map(|(ask, answer)| Input::Answered { ask, answer })
}

fn todo_asked() -> impl Strategy<Value = (todo::Ask, Answer)> {
    let printed = json!({"todo": "renewcard", "event": "Completed(...)", "revision": null});
    let taken = json!({"type": "Answered", "answer": {"type": "Taken"}, "held": false});
    let refused = Answer::Refused {
        status: 409,
        words: "refused".to_owned(),
    };
    (todo_write(), 0u8..5, any::<bool>()).prop_map(move |(write, which, ok)| {
        let (ask, done) = match which {
            0 => (todo::Ask::Printed { write }, printed.clone()),
            1 => (
                todo::Ask::Sealed {
                    write,
                    seal: Seal::Event,
                },
                taken.clone(),
            ),
            2 => (
                todo::Ask::Sealed {
                    write,
                    seal: Seal::Parent,
                },
                taken.clone(),
            ),
            3 => (
                todo::Ask::Thirty,
                json!({"type": "Drawn", "span": {"type": "Unsampled"}}),
            ),
            _ => (todo::Ask::Undo, Value::Null),
        };
        let answer = if ok {
            Answer::Done { body: done }
        } else {
            refused.clone()
        };
        (ask, answer)
    })
}

fn todo_msg() -> impl Strategy<Value = todo::Msg> {
    use todo::{DeferMsg, EditMsg, Msg};
    let fold = (todo_entry(), any::<bool>()).prop_map(|(e, held)| {
        if held {
            json!({"entries": [e], "at": "2026-09-04T12:00:00"})
        } else {
            Value::Null
        }
    });
    let unsent = (0usize..3, 0u32..2).prop_map(|(refused, waiting)| {
        json!({"todos": {}, "refused": (0..refused).map(|i| format!("no {i}")).collect::<Vec<_>>(), "blocked": refused,
            "stranded": 0, "losses": ["h1"], "waiting": waiting})
    });
    prop_oneof![
        held::<todo::Hold, todo::Ask, suzatary_pages::runtime::Never>(
            todo::Hold::Page,
            todo_payload()
        )
        .prop_map(|i| Msg::Host(Box::new(i))),
        held::<todo::Hold, todo::Ask, suzatary_pages::runtime::Never>(todo::Hold::Fold, fold)
            .prop_map(|i| Msg::Host(Box::new(i))),
        held::<todo::Hold, todo::Ask, suzatary_pages::runtime::Never>(todo::Hold::Unsent, unsent)
            .prop_map(|i| Msg::Host(Box::new(i))),
        todo_answer().prop_map(|i| Msg::Host(Box::new(i))),
        any::<bool>().prop_map(|w| Msg::Host(Box::new(Input::Context(context(w))))),
        prop::sample::select(vec![Verb::Done, Verb::Cancel, Verb::Reopen, Verb::Confirm])
            .prop_map(Msg::Run),
        words().prop_map(Msg::Note),
        any::<bool>().prop_map(|o| Msg::Edit(EditMsg::Toggle(o))),
        words().prop_map(|w| Msg::Edit(EditMsg::Body(w))),
        words().prop_map(|w| Msg::Edit(EditMsg::Waiting(w))),
        Just(Msg::Edit(EditMsg::Reprice)),
        Just(Msg::Edit(EditMsg::Price(
            suzatary_pages::form::PriceMsg::Kind(suzatary_pages::form::Kind::Deadline)
        ))),
        Just(Msg::Edit(EditMsg::Save)),
        Just(Msg::Edit(EditMsg::Cancel)),
        any::<bool>().prop_map(|o| Msg::Defer(DeferMsg::Toggle(o))),
        prop::sample::select(vec!["2026-10-01", "not a day"])
            .prop_map(|d| Msg::Defer(DeferMsg::Day(d.to_owned()))),
        Just(Msg::Defer(DeferMsg::Send)),
        (0usize..3).prop_map(Msg::Tick),
        any::<bool>().prop_map(Msg::Provenance),
        any::<bool>().prop_map(Msg::History),
        Just(Msg::Retry),
        Just(Msg::Build),
    ]
}

fn list_entry(id: &'static str, refs: &'static [&'static str]) -> impl Strategy<Value = Value> {
    (
        prop::sample::select(vec![json!(0.3), json!(0.6), json!(0.9), json!("absent"), Value::Null]),
        prop::sample::select(vec!["home", "work", ""]),
        any::<bool>(),
        prop::sample::select(vec!["open", "completed"]),
    )
        .prop_map(move |(value, category, unconfirmed, state)| {
            let spec = if refs.is_empty() {
                json!({"kind": "flat", "value": 0.5})
            } else {
                json!({"kind": "conj", "p": -4.0, "terms": refs.iter().map(|r| json!({"kind": "ref", "todo": r})).collect::<Vec<_>>()})
            };
            json!({
                "todo": id, "state": state, "states": [state], "at": "", "claimed": "", "value": value,
                "recurring": false, "unconfirmed": unconfirmed, "conflicts": {},
                "authored": {
                    "rich": {"body": "<p>B</p>", "detail": Value::Null, "rationale": Value::Null, "waiting_on": Value::Null,
                        "keys": {"body": format!("k-{id}"), "detail": Value::Null, "rationale": Value::Null, "waiting_on": Value::Null}},
                    "block": {"kind": "todo", "name": "todo"}, "created": "2026-08-01", "body": format!("Do {id}"), "text": format!("Do {id}"),
                    "detail": "", "category": category, "waiting_on": "", "rationale": [], "spec": spec,
                    "actor": "ada", "at": "2026-08-01T09:00:00", "source": Value::Null, "subtodos": [],
                },
                "candidates": [], "stream": [],
            })
        })
}

fn list_fold() -> impl Strategy<Value = Value> {
    (
        list_entry("a", &[]),
        list_entry("b", &["a"]),
        list_entry("c", &[]),
        0usize..4,
        any::<bool>(),
    )
        .prop_map(|(a, b, c, n, held)| {
            let entries: Vec<Value> = [a, b, c].into_iter().take(n).collect();
            if held {
                json!({"entries": entries, "at": "2026-09-04T12:00:00"})
            } else {
                Value::Null
            }
        })
}

/// The worker's prices, answering one of the page's first asks.
/// The worker's tree, answering one of the page's first asks: the generated fold's todos under
/// their categories, each node with one of the generated prices.
fn list_prices() -> impl Strategy<Value = Input<todos::Hold, todos::Ask, todos::Listen>> {
    (
        proptest::collection::vec(
            prop_oneof![
                Just(json!({"type": "Value", "v": 0.4})),
                Just(json!({"type": "Value", "v": null})),
                Just(json!({"type": "Refused", "why": "no"}))
            ],
            6,
        ),
        1u32..4,
    )
        .prop_map(|(p, n)| {
            let node = |place: Value, priced: &Value, children: Vec<Value>| {
                json!({"place": place, "term": {"kind": "ref", "todo": "a"}, "priced": priced, "children": children})
            };
            let todo = |t: &str, i: usize| node(json!({"type": "Todo", "v": t}), &p[i], Vec::new());
            let home = node(
                json!({"type": "Category", "v": "home"}),
                &p[1],
                vec![todo("a", 2), node(json!({"type": "Todo", "v": "b"}), &p[3], vec![todo("a", 4)])],
            );
            let work = node(json!({"type": "Category", "v": "work"}), &p[5], vec![todo("c", 0)]);
            Input::Answered {
                ask: todos::Ask::Tree { n },
                answer: Answer::Done {
                    body: node(json!({"type": "All"}), &p[0], vec![home, work]),
                },
            }
        })
}

fn list_host() -> impl Strategy<Value = todos::Msg> {
    use suzatary_pages::todoparts::{Acting, Phase};
    use todos::{Ask, Hold, Kept, Listen, Msg};
    type In = Input<Hold, Ask, Listen>;
    let host = |i: In| Msg::Host(Box::new(i));
    let done = |ask: Ask, body: Value| Input::Answered {
        ask,
        answer: Answer::Done { body },
    };
    let route = (prop::sample::select(vec!["", "a", "b"]), prop::sample::select(vec!["", "Shared words"]))
        .prop_map(|(selected, shared)| json!({"page": "todos", "conversation": "", "selected": selected, "shared": shared}));
    let kept = prop_oneof![
        Just((Kept::Filter, json!({"state": "all", "query": "do"}))),
        Just((Kept::Filter, json!("{not json"))),
        Just((Kept::By, json!("urgency"))),
        Just((Kept::By, json!("flat"))),
        Just((Kept::Folds, json!({"c:home": false, "t:b": true}))),
        Just((Kept::Opened, json!("a"))),
        Just((Kept::Sheet, json!(true))),
        Just((Kept::Category, json!("work"))),
    ];
    let unsent = prop::sample::select(vec!["", "a"]).prop_map(|t| {
        let todos = if t.is_empty() { json!({}) } else { json!({t: "Sending"}) };
        json!({"todos": todos, "refused": [], "blocked": 0, "stranded": 0, "losses": [], "waiting": 0})
    });
    let read = prop_oneof![
        Just(json!({"held": ["life"], "read": {"type": "Unheld"}})),
        Just(
            json!({"held": ["life"], "read": {"type": "Read", "reading": {"chain": "life", "set": "s", "at": "2026-09-04T12:00:00"},
            "at": "2026-09-04T12:00:00", "specs": {"b": [{"kind": "ref", "todo": "a"}]}, "functions": {"a": {"kind": "flat", "value": 0.4}}}})
        ),
    ];

    let keys = prop_oneof![
        (-2i32..3).prop_map(|by| json!({"type": "Move", "by": by})),
        Just(json!({"type": "Open"})),
        Just(json!({"type": "Close"})),
        Just(json!({"type": "Act", "name": "done"})),
        Just(json!({"type": "Search"})),
        Just(json!({"type": "Add"})),
        Just(json!({"type": "Escape"})),
    ];
    prop_oneof![
        held::<Hold, Ask, Listen>(Hold::Fold, list_fold()).prop_map(host),
        held::<Hold, Ask, Listen>(Hold::Unsent, unsent).prop_map(host),
        held::<Hold, Ask, Listen>(Hold::Route, route).prop_map(host),
        kept.prop_map(move |(k, v)| host(Input::Held {
            ask: Hold::Kept(k),
            reading: Reading::Val {
                v: Answer::Done { body: v }
            },
            behind: None
        })),
        read.prop_map(move |r| host(done(
            Ask::Reading {
                at: "2026-09-04T12:00:00".to_owned()
            },
            r
        ))),
        list_prices().prop_map(host),
        prop::sample::select(vec![Value::Null, json!("Revised(...)")]).prop_map(move |revision| host(done(
            Ask::Added,
            json!({"todo": "x", "event": "Created(...)", "revision": revision})
        ))),
        (prop::sample::select(vec!["x", "y"]), prop::sample::select(vec![Seal::Event, Seal::Parent]), any::<bool>()).prop_map(
            move |(todo, seal, taken)| host(done(
                Ask::Sealed { todo: todo.to_owned(), seal },
                if taken {
                    json!({"type": "Answered", "answer": {"type": "Taken"}, "held": false})
                } else {
                    json!({"type": "Answered", "answer": {"type": "Refused", "e": "no"}, "held": false})
                }
            ))
        ),
        Just(host(done(
            Ask::Line {
                todo: "a".to_owned(),
                acting: Acting::Verb(Verb::Done),
                phase: Phase::Sealed(Seal::Event)
            },
            json!({"type": "Answered", "answer": {"type": "Taken"}, "held": false})
        ))),
        keys.prop_map(move |k| host(Input::Heard {
            ask: Listen::Keys,
            frame: k.to_string()
        })),
        (prop::sample::select(vec!["a", "b"]), todo_asked()).prop_map(
            move |(todo, (ask, answer))| host(Input::Answered {
                ask: Ask::Pane {
                    todo: todo.to_owned(),
                    ask
                },
                answer
            })
        ),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

fn list_person() -> impl Strategy<Value = todos::Msg> {
    use todos::{Choice, Grouping, Msg, Narrow, SheetMsg, States};
    prop_oneof![
        (
            prop::sample::select(vec!["c:home", "t:b", "all"]),
            any::<bool>()
        )
            .prop_map(|(k, o)| Msg::Fold(k.to_owned(), o)),
        prop::sample::select(vec!["a", "b"])
            .prop_map(|t| Msg::Tap(t.to_owned(), format!("c:home/t:{t}"))),
        Just(Msg::Find),
        prop::sample::select(vec![Grouping::Category, Grouping::Urgency, Grouping::Flat])
            .prop_map(Msg::By),
        words().prop_map(|q| Msg::Narrow(Narrow::Query(q))),
        prop::sample::select(vec![States::Open, States::Done, States::All])
            .prop_map(|s| Msg::Narrow(Narrow::States(s))),
        Just(Msg::Narrow(Narrow::Attention)),
        Just(Msg::Narrow(Narrow::Category(Choice::Is("work".to_owned())))),
        Just(Msg::Narrow(Narrow::Category(Choice::Any))),
        Just(Msg::Act("a".to_owned(), Verb::Done)),
        Just(Msg::Under("b".to_owned())),
        Just(Msg::Sheet(SheetMsg::Handle)),
        words().prop_map(|w| Msg::Sheet(SheetMsg::Body(w))),
        Just(Msg::Sheet(SheetMsg::Category("home".to_owned()))),
        Just(Msg::Sheet(SheetMsg::Add)),
        Just(Msg::Pane(todo::Msg::Provenance(true))),
        Just(Msg::Retry),
    ]
}

fn list_msg() -> impl Strategy<Value = todos::Msg> {
    prop_oneof![list_host(), list_person()]
}

fn typst_msg() -> impl Strategy<Value = typstlist::Msg> {
    use typstlist::{Ask, Hold, Msg};
    let host = |i: Input<Hold, Ask>| Msg::Host(Box::new(i));
    let route = prop::sample::select(vec!["", "a"]).prop_map(
        |selected| json!({"page": "typst", "conversation": "", "selected": selected, "shared": ""}),
    );
    prop_oneof![
        held::<Hold, Ask, suzatary_pages::runtime::Never>(Hold::Fold, list_fold()).prop_map(host),
        held::<Hold, Ask, suzatary_pages::runtime::Never>(Hold::Route, route).prop_map(host),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
        prop::sample::select(vec![
            "open todo:life/a",
            "complete todo:life/a",
            "complete todo:work/a",
            "rm -rf"
        ])
        .prop_map(|w| Msg::Intent(w.to_owned())),
        Just(host(Input::Answered {
            ask: Ask::Act {
                todo: "a".to_owned(),
                acting: suzatary_pages::todoparts::Acting::Verb(Verb::Done),
                phase: suzatary_pages::todoparts::Phase::Sealed(Seal::Event)
            },
            answer: Answer::Done {
                body: json!({"type": "Answered", "answer": {"type": "Taken"}, "held": false})
            },
        })),
    ]
}

fn about_payload() -> impl Strategy<Value = Value> {
    prop_oneof![Just("dev"), Just("abc1234")].prop_map(|build| {
        printed(&AboutPayload {
            version: "2.1.59".to_owned(),
            build: build.to_owned(),
            completes: Vec::new(),
        })
    })
}

fn one_msgs(answer: impl Strategy<Value = Value>) -> impl Strategy<Value = Vec<one::Msg>> {
    proptest::collection::vec(
        prop_oneof![
            held::<one::Hold, one::Ask, _>(one::Hold::Page, answer)
                .prop_map(|i| one::Msg::Host(Box::new(i))),
            any::<bool>().prop_map(|w| one::Msg::Host(Box::new(Input::Context(context(w))))),
            Just(one::Msg::Back),
            Just(one::Msg::Retry),
        ],
        0..8,
    )
}

fn answered<H: std::fmt::Debug, A: Clone + std::fmt::Debug + 'static>(
    ask: impl Strategy<Value = A>,
) -> impl Strategy<Value = Input<H, A>> {
    (ask, any::<bool>()).prop_map(|(ask, done)| Input::Answered {
        ask,
        answer: if done {
            Answer::Done { body: json!({}) }
        } else {
            Answer::Refused {
                status: 409,
                words: "refused".to_owned(),
            }
        },
    })
}

fn opened<A>(writes: bool, at: A) -> Opened<A> {
    Opened {
        context: context(writes),
        at,
    }
}

fn act() -> impl Strategy<Value = memory::Act> {
    prop_oneof![
        (0u64..5).prop_map(|id| memory::Act::Forget(FactId(id))),
        (0u8..3).prop_map(|n| memory::Act::Retract(format!("n{n}"))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn memory_laws(writes: bool, msgs in proptest::collection::vec(prop_oneof![
        held::<memory::Hold, memory::Ask, _>(memory::Hold::Memory, memory_payload()).prop_map(|i| memory::Msg::Host(Box::new(i))),
        held::<memory::Hold, memory::Ask, _>(memory::Hold::Record, record_payload()).prop_map(|i| memory::Msg::Host(Box::new(i))),
        answered::<memory::Hold, memory::Act>(act()).prop_map(|i| memory::Msg::Host(Box::new(i))),
        any::<bool>().prop_map(|w| memory::Msg::Host(Box::new(Input::Context(context(w))))),
        act().prop_map(memory::Msg::Run),
        Just(memory::Msg::Back),
        Just(memory::Msg::Retry),
    ], 0..10)) {
        page_laws::<memory::Memory>(|| opened(writes, Nothing {}), &msgs);
    }

    #[test]
    fn about_laws(msgs in proptest::collection::vec(prop_oneof![
        held::<about::Hold, about::Ask, _>(about::Hold::About, about_payload()).prop_map(|i| about::Msg::Host(Box::new(i))),
        held::<about::Hold, about::Ask, _>(about::Hold::Releases, releases_payload()).prop_map(|i| about::Msg::Host(Box::new(i))),
        Just(about::Msg::Retry),
    ], 0..8)) {
        page_laws::<about::About>(|| opened(true, Nothing {}), &msgs);
    }

    #[test]
    fn todo_laws(writes: bool, pushed: bool, msgs in proptest::collection::vec(todo_msg(), 0..16)) {
        page_laws::<todo::Todo>(|| todo::Flags {
            context: context(writes),
            chain: "life".to_owned(),
            id: "renewcard".to_owned(),
            pushed,
            now: 1_788_000_000_000,
        }, &msgs);
    }

    #[test]
    fn list_laws(writes: bool, wide: bool, msgs in proptest::collection::vec(list_msg(), 0..24)) {
        page_laws::<todos::Todos>(|| todos::Flags {
            context: Context { wide, ..context(writes) },
            chain: "life".to_owned(),
            now: 1_788_000_000_000,
        }, &msgs);
    }

    #[test]
    fn a_pane_answer_never_acts_on_the_todo_shown_now(asked in proptest::collection::vec(todo_asked(), 1..6)) {
        let route = |selected: &str| todos::Msg::Host(Box::new(Input::Held {
            ask: todos::Hold::Route,
            reading: Reading::Val { v: Answer::Done { body: json!({"page": "todos", "conversation": "", "selected": selected, "shared": ""}) } },
            behind: None,
        }));
        let (mut page, _) = Runtime::<todos::Todos>::start(todos::Flags {
            context: Context { wide: true, ..context(true) },
            chain: "life".to_owned(),
            now: 1_788_000_000_000,
        });
        page.send(route("shownnow"));
        for (ask, answer) in asked {
            let said = page.send(todos::Msg::Host(Box::new(Input::Answered {
                ask: todos::Ask::Pane { todo: "askedfirst".to_owned(), ask },
                answer,
            })));
            let effects = format!("{:?}", said.effects);
            prop_assert!(!effects.contains("shownnow"), "{effects}");
        }
    }

    #[test]
    fn typst_laws(writes: bool, msgs in proptest::collection::vec(typst_msg(), 0..12)) {
        page_laws::<typstlist::Typst>(|| typstlist::Flags { context: context(writes), chain: "life".to_owned() }, &msgs);
    }

    #[test]
    fn calendar_laws(msgs in one_msgs(calendar_payload())) {
        page_laws::<One<calendar::Calendar>>(|| opened(true, calendar::At { from: "2026-09-07".to_owned() }), &msgs);
    }

    #[test]
    fn briefings_laws(msgs in one_msgs(briefings_payload())) {
        page_laws::<One<briefings::Briefings>>(|| opened(true, Nothing {}), &msgs);
    }

    #[test]
    fn briefing_laws(msgs in one_msgs(briefing_payload())) {
        page_laws::<One<briefing::Briefing>>(|| opened(true, briefing::At { date: "2026-09-04".to_owned() }), &msgs);
    }
}

/// A second Forget of the same fact sends nothing until the first is answered; an answer rereads
/// the memory.
#[test]
fn a_forget_runs_once_until_it_is_answered() {
    let (mut rt, _) = Runtime::<memory::Memory>::start(opened(true, Nothing {}));
    let tap = || memory::Msg::Run(memory::Act::Forget(FactId(3)));
    let sent = rt.step(tap()).1;
    assert_eq!(
        sent,
        vec![Effect::Post {
            ask: memory::Act::Forget(FactId(3)),
            path: "/api/secretary/memory/3/forget".to_owned(),
            body: Body::Empty {},
        }]
    );
    assert_eq!(rt.step(tap()).1, Vec::new());
    let done = Input::Answered {
        ask: memory::Act::Forget(FactId(3)),
        answer: Answer::Done { body: json!({}) },
    };
    assert_eq!(
        rt.step(memory::Msg::Host(Box::new(done))).1,
        vec![Effect::Again {
            path: "/api/secretary/memory".to_owned()
        }]
    );
    assert_eq!(
        rt.step(tap()).1.len(),
        1,
        "an answered act may be tapped again"
    );
}

fn overview() -> impl Strategy<Value = Value> {
    let line = |n: u8, standing: Value, cards: Value| {
        let replaces = if n.is_multiple_of(2) {
            json!([{"label": "its token", "handle": format!("h{n}")}])
        } else {
            json!([])
        };
        json!({"key": format!("l{n}"), "label": format!("Line {n}"), "detail": null,
            "standing": standing, "cards": cards, "replaces": replaces})
    };
    (
        proptest::collection::vec(
            (
                0u8..4,
                prop::sample::select(vec![
                    json!({"kind": "kept"}),
                    json!({"kind": "synced", "at": "2026-10-05T04:00:00"}),
                    json!({"kind": "locked"}),
                    json!({"kind": "missing", "what": ["a key", "a link"]}),
                    json!({"kind": "by_hand", "command": "suzatary auth", "why": "sign in"}),
                ]),
                prop::sample::select(vec![json!([]), json!(["proposal:setup/1"])]),
            ),
            0..4,
        ),
        any::<bool>(),
    )
        .prop_map(move |(lines, offers)| {
            let lines: Vec<Value> = lines.into_iter().map(|(n, s, c)| line(n, s, c)).collect();
            json!({"groups": [{"group": "models", "lines": lines}, {"group": "mail", "lines": []}],
                "ask": "setup",
                "offers": if offers { json!([{"template": "google", "group": "mail", "title": "a Google account"}]) } else { json!([]) }})
        })
}

fn device() -> impl Strategy<Value = Value> {
    prop::sample::select(vec![
        json!(null),
        json!({"persist": {"type": "Granted"}, "usage": {"bytes": 1024.0, "count": 1}, "budget": 1e6,
            "signing": {"type": "Keyed", "key": "ab".repeat(32), "standing": "untrusted"}}),
        json!({"persist": {"type": "Denied"}, "usage": {"bytes": 0.0, "count": 0}, "budget": 1e6,
            "signing": {"type": "Unreadable", "e": "it does not read"}}),
    ])
}

/// A link to store (a typed value), a grant (a confirmed yes), or an email (a confirmed send),
/// standing, failed, or answered.
fn proposal(index: usize) -> impl Strategy<Value = Value> {
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

fn conversation_cards(id: &'static str) -> impl Strategy<Value = Value> {
    proptest::collection::vec(any::<bool>(), 0..3).prop_flat_map(move |shown| {
        let rows: Vec<_> = (0..shown.len()).map(proposal).collect();
        rows.prop_map(move |proposals| {
            json!({"id": id, "title": "Setup", "model": "", "opened": "", "turns": [], "proposals": proposals,
                "unplaced": [], "state": "idle", "first": 0, "total": 0, "again": null})
        })
    })
}

fn inbox_read() -> impl Strategy<Value = Value> {
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

fn watched() -> impl Strategy<Value = (Watched, Value)> {
    prop_oneof![
        inbox_read().prop_map(|v| (Watched::Proposals, v)),
        conversation_cards("setup").prop_map(|v| (Watched::Cards("setup".to_owned()), v)),
        conversation_cards("talk").prop_map(|v| (Watched::Cards("talk".to_owned()), v)),
        Just((Watched::Conversations, json!({"conversations": []}))),
    ]
}

fn card_ev() -> impl Strategy<Value = (card::Key, card::Ev)> {
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

fn settings_act() -> impl Strategy<Value = settings::Act> {
    prop_oneof![
        Just(settings::Act::Begin {
            template: "google".to_owned()
        }),
        Just(settings::Act::AskTrust {
            key: "ab".repeat(32)
        }),
        Just(settings::Act::Renew),
        Just(settings::Act::Pair {
            code: "BCDF-GHJK".to_owned()
        }),
        Just(settings::Act::Replace {
            handle: "h0".to_owned()
        }),
    ]
}

fn settings_msg() -> impl Strategy<Value = settings::Msg> {
    let host = |i: Input<settings::Hold, settings::Ask>| settings::Msg::Host(Box::new(i));
    prop_oneof![
        held(settings::Hold::Overview, overview()).prop_map(host),
        held(settings::Hold::Device, device()).prop_map(host),
        watched()
            .prop_flat_map(move |(w, v)| held(settings::Hold::Rows(w), Just(v)))
            .prop_map(host),
        card_ev().prop_map(|(k, ev)| settings::Msg::Card(k, ev)),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
        settings_act().prop_map(settings::Msg::Run),
        (settings_act(), any::<bool>()).prop_map(move |(ask, done)| host(Input::Answered {
            ask: settings::Ask::Run(ask),
            answer: if done {
                Answer::Done {
                    body: json!({"id": "setup-1"}),
                }
            } else {
                Answer::Refused {
                    status: 403,
                    words: "no".to_owned(),
                }
            }
        })),
        Just(settings::Msg::Pairing(settings::Pairing::Closed)),
        prop::sample::select(vec!["", "BCDF"])
            .prop_map(|c| settings::Msg::Pairing(settings::Pairing::Open { code: c.to_owned() })),
        Just(settings::Msg::Retry),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn settings_laws(writes: bool, msgs in proptest::collection::vec(settings_msg(), 0..12)) {
        page_laws::<settings::Settings>(|| settings::Flags { context: context(writes) }, &msgs);
    }
}

fn inbox_msg() -> impl Strategy<Value = inbox::Msg> {
    let host = |i: Input<inbox::Hold, inbox::Ask>| inbox::Msg::Host(Box::new(i));
    prop_oneof![
        watched().prop_flat_map(move |(w, v)| held(inbox::Hold::Rows(w), Just(v))).prop_map(host),
        held(inbox::Hold::Rows(Watched::Decisions("life".to_owned())), Just(json!({"at": "2026-10-05T04:00:00", "heads": [], "entries": [], "failed": [], "next": null}))).prop_map(host),
        held(inbox::Hold::Intents, Just(json!({"asked": [{"act": {"type": "Answer", "at": {"conversation": "setup", "index": 0, "from": "standing:0:"}, "answer": {"type": "Dismiss", "reason": "no"}}, "answer": null, "retracted": false}]}))).prop_map(host),
        card_ev().prop_map(|(k, ev)| inbox::Msg::Card(k, ev)),
        any::<bool>().prop_map(inbox::Msg::Setup),
        any::<bool>().prop_map(inbox::Msg::Settled),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn inbox_laws(writes: bool, opened: bool, msgs in proptest::collection::vec(inbox_msg(), 0..12)) {
        page_laws::<inbox::Inbox>(
            || inbox::Flags { context: context(writes), open: if opened { "p0".to_owned() } else { String::new() }, now: 1_790_000_000_000 },
            &msgs,
        );
    }
}

fn turn_row(index: usize) -> impl Strategy<Value = Value> {
    (
        prop::sample::select(vec!["person", "model", "verb", "act", "gone"]),
        prop::sample::select(vec!["", "see todo:life/renewcard", "https://example.test/x"]),
    )
        .prop_map(move |(who, text)| {
            let base = json!({"index": index, "text": text, "at": "2026-10-05T04:00:00", "attempt": "current", "gone": null});
            let mut t = base.as_object().cloned().unwrap_or_default();
            let extra = match who {
                "person" => json!({"who": "person", "references": [], "attachments": [{"id": "a1", "media_type": "image/jpeg", "name": "", "private": ""}], "queued": false}),
                "model" => json!({"who": "model", "blocks": [{"source": "Hi\n\n", "plain": "Hi", "kind": "prose", "html": null}]}),
                "verb" => json!({"who": "verb", "verb": "search", "outcome": {"is": "remembered", "receipt": {"state": "standing", "fact": 3, "text": "calls after ten"}}}),
                "act" => json!({"who": "act", "references": ["todo:life/renewcard"]}),
                _ => json!({"who": "model", "blocks": [], "gone": {"why": "asked", "by": "ada"}}),
            };
            for (k, v) in extra.as_object().cloned().unwrap_or_default() {
                t.insert(k, v);
            }
            Value::Object(t)
        })
}

fn stream_read() -> impl Strategy<Value = Value> {
    proptest::collection::vec((turn_row(0), prop::sample::select(vec![json!([]), json!([{"topic": "garden", "by": "Owner"}]), json!([{"topic": "garden", "by": "Owner"}, {"topic": "trip", "by": "Inferred"}])]), any::<bool>()), 0..4).prop_map(|rs| {
        let rounds: Vec<Value> = rs
            .into_iter()
            .enumerate()
            .map(|(n, (opening, topics, boxed))| {
                json!({"target": format!("round:c{n}/0"), "conversation": format!("c{n}"), "title": "", "turn": 0,
                    "opening": opening, "asked": if boxed { json!({"by": "box", "title": "The day's brief"}) } else { json!({"by": "person"}) },
                    "answer": null, "topics": topics, "room": 2})
            })
            .collect();
        let total = rounds.len() + 1;
        json!({"stream": {"rounds": rounds, "total": total}, "bar": {"topics": [{"topic": "garden", "rounds": 2, "disputed": []}, {"topic": "trip", "rounds": 1, "disputed": ["travel"]}]}})
    })
}

fn outcome() -> impl Strategy<Value = Answer> {
    prop::sample::select(vec![
        Answer::Done {
            body: json!({"type": "Answered", "answer": {"type": "Taken"}, "held": true}),
        },
        Answer::Done {
            body: json!({"type": "Answered", "answer": {"type": "Refused", "e": "the box refused this topic"}, "held": false}),
        },
        Answer::Done {
            body: json!({"type": "Answered", "answer": {"type": "Lost", "e": "offline"}, "held": true}),
        },
        Answer::Done {
            body: json!({"type": "Already"}),
        },
        Answer::Refused {
            status: 0,
            words: "no answer".to_owned(),
        },
    ])
}

fn secretary_msg() -> impl Strategy<Value = secretary::Msg> {
    let host = |i: Input<secretary::Hold, secretary::Ask>| secretary::Msg::Host(Box::new(i));
    prop_oneof![
        held(secretary::Hold::Stream, stream_read()).prop_map(host),
        held(secretary::Hold::Found, Just(json!({"query": "zephyr", "hits": [{"conversation": "c0", "title": "", "kind": "topic", "turn": 0, "at": "2026-10-05T04:00:00", "side": "person", "excerpt": "when"}]}))).prop_map(host),
        held(secretary::Hold::Scroll, prop::sample::select(vec![json!({"by": "reader", "at": "above"}), json!({"by": "page", "at": "bottom"})])).prop_map(host),
        Just(secretary::Msg::Earlier),
        prop::sample::select(vec!["garden", "trip"]).prop_map(|t| secretary::Msg::Rename(t.to_owned())),
        Just(secretary::Msg::Add("round:c0/0".to_owned())),
        Just(secretary::Msg::Chip(suzatary_pages::runtime::TopicWrite::Remove { target: "round:c0/0".to_owned(), topic: "garden".to_owned() })),
        Just(secretary::Msg::Chip(suzatary_pages::runtime::TopicWrite::Add { target: "round:c0/0".to_owned(), topic: "trip".to_owned() })),
        prop::sample::select(vec!["", "Trip", "trip", "garden"]).prop_map(|t| secretary::Msg::Typed(t.to_owned())),
        Just(secretary::Msg::Write("trip".to_owned())),
        Just(secretary::Msg::Submit),
        Just(secretary::Msg::Close),
        prop::sample::select(vec!["", "zephyr", " zephyr "]).prop_map(|t| secretary::Msg::Searched(t.to_owned())),
        (0u32..4).prop_map(move |n| host(Input::Answered { ask: secretary::Ask::Search(chats::Asking::Rested(n)), answer: Answer::Done { body: json!(1_790_000_000_000_i64) } })),
        (prop::sample::select(vec![
            secretary::Closes::Nothing,
            secretary::Closes::Renaming { topic: "garden".to_owned() },
            secretary::Closes::Adding { target: "round:talk/1".to_owned() },
        ]), outcome()).prop_map(move |(closes, answer)| host(Input::Answered { ask: secretary::Ask::Topic(secretary::Wrote { renamed: Some(("garden".to_owned(), "yard".to_owned())), closes }), answer })),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

fn windowed() -> impl Strategy<Value = Value> {
    prop_oneof![
        3 => (proptest::collection::vec(any::<bool>(), 0..5), conversation_cards("talk"), prop::sample::select(vec!["idle", "thinking"])).prop_flat_map(|(shape, cards, state)| {
            let rows: Vec<_> = (0..shape.len()).map(turn_row).collect();
            rows.prop_map(move |turns| {
                let rounds: Vec<Value> = (0..turns.len())
                    .map(|i| json!({"key": i, "startIndex": i, "endIndex": i, "opening": i, "trace": [], "answer": null,
                        "proposals": if i == 0 { json!([0, 1]) } else { json!([]) }, "earlier": [], "day": "2026-10-05", "label": "Monday 5 October", "marked": i == 0}))
                    .collect();
                let total = turns.len();
                let mut c = cards.clone();
                c["turns"] = json!(turns);
                c["rounds"] = json!(rounds);
                c["total"] = json!(total);
                c["state"] = json!(state);
                c["again"] = if total > 0 { json!(0) } else { json!(null) };
                json!({"type": "Read", "payload": c})
            })
        }),
        1 => Just(json!({"type": "NotHere", "lines": 3, "why": "offline"})),
        1 => Just(json!({"type": "Unheard"})),
    ]
}

fn frame() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        r#"{"kind":"state","state":"thinking"}"#,
        r#"{"kind":"working","verb":"search"}"#,
        r#"{"kind":"delta","text":"Hi\n\nthere"}"#,
        r#"{"kind":"block","n":0,"sha":"ab","source":"Hi\n\n"}"#,
        r#"{"kind":"turn"}"#,
        r#"{"kind":"state","state":"idle"}"#,
        "not a frame",
    ])
    .prop_map(str::to_owned)
}

fn composer_ev() -> impl Strategy<Value = composer::Ev> {
    prop_oneof![
        prop::sample::select(vec!["", "hello", "  "])
            .prop_map(|t| composer::Ev::Typed(t.to_owned())),
        Just(composer::Ev::Picked(vec![viewtree::Picked {
            media: "application/pdf".to_owned(),
            base64: "JVBE".to_owned(),
            name: "w2.pdf".to_owned()
        }])),
        Just(composer::Ev::Detached("todo:life/renewcard".to_owned())),
        Just(composer::Ev::Unfiled("a1".to_owned())),
        (1u32..3).prop_map(composer::Ev::KeepPrivate),
        (1u32..3).prop_map(composer::Ev::Drop),
        Just(composer::Ev::KeepMine),
        Just(composer::Ev::Take("other".to_owned())),
        Just(composer::Ev::Cancel),
        Just(composer::Ev::Send),
        Just(composer::Ev::Stop),
        Just(composer::Ev::Up),
    ]
}

fn uploaded() -> impl Strategy<Value = Answer> {
    prop::sample::select(vec![
        Answer::Done {
            body: json!({"kind": "kept", "file": {"id": "f1", "media_type": "application/pdf", "name": "w2.pdf", "private": ""}}),
        },
        Answer::Done {
            body: json!({"kind": "sensitive", "why": "its text names an identity number", "folder": "/my-files/Suzatary/Private"}),
        },
        Answer::Refused {
            status: 413,
            words: "too big".to_owned(),
        },
    ])
}

fn conversation_msg() -> impl Strategy<Value = conversation::Msg> {
    use conversation::{Ask, Hold, Msg};
    let host = |i: conversation::Input| Msg::Host(Box::new(i));
    prop_oneof![
        (reading(windowed()), behind()).prop_map(move |(reading, behind)| host(Input::Held { ask: Hold::Window, reading, behind })),
        frame().prop_map(move |frame| host(Input::Heard { ask: conversation::Stream::Conversation, frame })),
        held(Hold::Draft, prop::sample::select(vec![json!({"text": "", "others": []}), json!({"text": "draft", "others": ["elsewhere"]})])).prop_map(host),
        held(Hold::Scroll, prop::sample::select(vec![json!({"by": "reader", "at": "above"}), json!({"by": "reader", "at": "bottom"}), json!({"by": "page", "at": "above"})])).prop_map(host),
        held(Hold::Block("ab".to_owned()), Just(json!({"html": "<p>Hi</p>", "key": "k"}))).prop_map(host),
        held(Hold::Chats, Just(json!({"conversations": [{"id": "talk", "title": "", "kind": "topic", "opened": "", "turns": 2, "standing": 1, "unplaced": [], "last": "hi", "last_at": "2026-10-05T04:00:00"}]}))).prop_map(host),
        composer_ev().prop_map(Msg::Composer),
        card_ev().prop_map(|(k, ev)| Msg::Card(k, ev)),
        Just(Msg::Quote("a line".to_owned())),
        Just(Msg::Earlier),
        Just(Msg::Pill),
        Just(Msg::Stop),
        Just(Msg::Regenerate),
        Just(Msg::Edit),
        (0usize..3).prop_map(|t| Msg::Erasing(Some(t))),
        Just(Msg::Erasing(None)),
        (0usize..3).prop_map(Msg::Erase),
        Just(Msg::Forget(3)),
        Just(Msg::Rename),
        Just(Msg::Renamed("A name".to_owned())),
        prop::option::of(Just("A name".to_owned())).prop_map(Msg::Named),
        prop::sample::select(vec!["", "zephyr"]).prop_map(|t| Msg::Searched(t.to_owned())),
        outcome().prop_map(move |answer| host(Input::Answered { ask: Ask::Said(Box::new(conversation::Spoken { text: "hello".to_owned(), references: Vec::new(), files: Vec::new(), edit: false })), answer })),
        (prop::sample::select(vec!["A name", "Another"]), outcome()).prop_map(move |(title, answer)| host(Input::Answered { ask: Ask::Rename { title: title.to_owned() }, answer })),
        (1u32..3, uploaded()).prop_map(move |(n, answer)| host(Input::Answered { ask: Ask::Composer(composer::Asking::Upload(n)), answer })),
        (1u32..3, uploaded()).prop_map(move |(n, answer)| host(Input::Answered { ask: Ask::Composer(composer::Asking::Keep(n)), answer })),
        Just(host(Input::Answered { ask: Ask::Slice, answer: Answer::Done { body: json!(1_790_000_005_000_i64) } })),
        Just(host(Input::Opened { ask: conversation::Stream::Conversation })),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

fn draft_msg() -> impl Strategy<Value = draft::Msg> {
    let host = |i: draft::Input| draft::Msg::Host(Box::new(i));
    prop_oneof![
        composer_ev().prop_map(draft::Msg::Composer),
        prop::sample::select(vec!["", "The zephyr filing"])
            .prop_map(|t| draft::Msg::Named(t.to_owned())),
        prop::sample::select(vec![
            Answer::Done {
                body: json!({"id": "2026-10-05-1"})
            },
            Answer::Refused {
                status: 403,
                words: "no".to_owned()
            }
        ])
        .prop_map(move |answer| host(Input::Answered {
            ask: draft::Ask::Open,
            answer
        })),
        outcome().prop_map(move |answer| host(Input::Answered {
            ask: draft::Ask::Said("2026-10-05-1".to_owned()),
            answer
        })),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

fn needsyou_msg() -> impl Strategy<Value = needsyou::Msg> {
    let host = |i: needsyou::Input| needsyou::Msg::Host(Box::new(i));
    prop_oneof![
        inbox_read()
            .prop_flat_map(move |v| held(needsyou::Hold::Rows(Watched::Proposals), Just(v)))
            .prop_map(host),
        held(
            needsyou::Hold::Route,
            prop::sample::select(vec![
                json!({"page": "inbox", "conversation": ""}),
                json!({"page": "todos", "conversation": ""}),
                json!({"page": "conversation", "conversation": "talk"})
            ])
        )
        .prop_map(host),
        (0u8..4).prop_map(|n| needsyou::Msg::Close(format!("p{n}"))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn secretary_laws(writes: bool, chosen: bool, msgs in proptest::collection::vec(secretary_msg(), 0..14)) {
        page_laws::<secretary::Secretary>(
            || secretary::Flags { context: context(writes), topics: if chosen { vec!["trip".to_owned(), "garden".to_owned()] } else { Vec::new() }, now: 1_790_000_000_000 },
            &msgs,
        );
    }

    #[test]
    fn conversation_laws(writes: bool, panel: bool, msgs in proptest::collection::vec(conversation_msg(), 0..16)) {
        let at = if panel {
            conversation::At::Panel { about: "todo:life/renewcard".to_owned() }
        } else {
            conversation::At::Page { id: "talk".to_owned(), refs: vec!["todo:life/renewcard".to_owned()], focus: true, at: None }
        };
        page_laws::<conversation::Conversation>(|| conversation::Flags { context: context(writes), at: at.clone(), now: 1_790_000_000_000 }, &msgs);
    }

    #[test]
    fn draft_laws(writes: bool, msgs in proptest::collection::vec(draft_msg(), 0..12)) {
        page_laws::<draft::Draft>(|| draft::Flags { context: context(writes) }, &msgs);
    }

    #[test]
    fn needsyou_laws(msgs in proptest::collection::vec(needsyou_msg(), 0..12)) {
        page_laws::<needsyou::NeedsYou>(|| needsyou::Flags { context: context(true) }, &msgs);
    }
}

/// A card Settings reads that changes in any field, its outcome after the box's check say, asks
/// the overview again, waiting or settled: the line's health is in the overview, and nothing
/// else moves when a check lands after the tap's answer.
#[test]
fn a_card_that_changes_asks_the_overview_again() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    let mut runner = TestRunner::deterministic();
    let row = proposal(0).new_tree(&mut runner).expect("a row").current();
    let with = |state: &str, outcome: &str| {
        let mut r = row.clone();
        r["state"] = json!(state);
        r["outcome"] = json!(outcome);
        let body = json!({"id": "setup", "title": "Setup", "model": "", "opened": "", "turns": [], "proposals": [r],
            "unplaced": [], "state": "idle", "first": 0, "total": 0, "again": null});
        settings::Msg::Host(Box::new(Input::Held {
            ask: settings::Hold::Rows(Watched::Cards("setup".to_owned())),
            reading: Reading::Val {
                v: Answer::Done { body },
            },
            behind: None,
        }))
    };
    let again = |effects: &[Effect<settings::Hold, settings::Ask>]| {
        effects
            .iter()
            .any(|e| matches!(e, Effect::Again { path } if path == "/api/overview"))
    };
    let (mut rt, _) = Runtime::<settings::Settings>::start(settings::Flags {
        context: context(true),
    });
    rt.step(with("sent", ""));
    assert!(
        again(&rt.step(with("sent", "its address could not be reached")).1),
        "a settled card's new outcome"
    );
    rt.step(with("standing", ""));
    assert!(
        again(&rt.step(with("standing", "checked")).1),
        "a waiting card's new outcome"
    );
    assert!(
        !again(&rt.step(with("standing", "checked")).1),
        "the same reading asks nothing"
    );
}

/// A card's tap answered asks the overview again: the box checks what the tap stored before it
/// answers, so the answer is when the line's health has moved.
#[test]
fn a_card_tap_answered_asks_the_overview_again() {
    let (mut rt, _) = Runtime::<settings::Settings>::start(settings::Flags {
        context: context(true),
    });
    let answered = settings::Msg::Host(Box::new(Input::Answered {
        ask: settings::Ask::Card(card::Asking::Act {
            key: card::Key {
                conversation: "setup".to_owned(),
                index: 0,
            },
            then: card::Then {
                done: "Stored".to_owned(),
                yes: false,
            },
            signed: "standing:0:".to_owned(),
        }),
        answer: Answer::Done {
            body: json!({"type": "Answered", "answer": {"type": "Taken"}, "held": true}),
        },
    }));
    let effects = rt.step(answered).1;
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Again { path } if path == "/api/overview")),
        "{effects:?}"
    );
}

/// THE LINK A GRANT'S YES SENDS IS THE ONE ITS LINE DREW BESIDE IT: a link's card refused for
/// its origin files a grant on the same line, and the grant's yes taps the link's card again
/// with the link the tab kept. Once the grant is performed the box's overview names it no more,
/// so an overview read during the tap, landing before the tap's answer, must not lose the link.
/// `allow.json` is what the links scene's Settings page held before the yes was answered, the
/// overview read after it, and the answer (train 229 lost the resend in that order).
#[test]
fn a_grant_answered_after_its_line_moved_still_sends_its_link() {
    type In = suzatary_pages::runtime::Input<settings::Hold, settings::Ask>;
    let recorded: Value = serde_json::from_str(include_str!("allow.json")).expect("the recording");
    let input = |v: &Value| -> settings::Msg {
        let read: In = serde_json::from_value(v.clone()).expect("a Settings input");
        settings::Msg::Host(Box::new(read))
    };
    let link = card::Key {
        conversation: "setup".to_owned(),
        index: 0,
    };
    let resends = |effects: &[Effect<settings::Hold, settings::Ask>]| {
        effects.iter().any(|e| {
            matches!(e, Effect::Act { ask: settings::Ask::Card(card::Asking::Act { key, .. }), .. } if *key == link)
        })
    };
    for overview_first in [false, true] {
        let (mut rt, _) = Runtime::<settings::Settings>::start(settings::Flags {
            context: context(true),
        });
        for one in recorded["before"].as_array().expect("inputs") {
            rt.step(input(one));
        }
        let effects = if overview_first {
            rt.step(input(&recorded["overview_after"]));
            rt.step(input(&recorded["answer"])).1
        } else {
            rt.step(input(&recorded["answer"])).1
        };
        assert!(
            resends(&effects),
            "the grant's yes sends its link again (the overview read after it landing first: {overview_first})"
        );
    }
}

/// Each page's rehearsal (`app/pages/tests/rehearsal/`) is flags and inputs its page takes, so the
/// worker that runs it never meets a refused input, and a wire change that would refuse one fails
/// here.
#[test]
fn every_rehearsal_runs_as_its_page_would() {
    use viewtree::host::Host;
    Host::<secretary::Secretary>::rehearse().expect("the stream's");
    Host::<conversation::Conversation>::rehearse().expect("a conversation's");
    Host::<inbox::Inbox>::rehearse().expect("the Inbox's");
    Host::<needsyou::NeedsYou>::rehearse().expect("the popup's");
    Host::<settings::Settings>::rehearse().expect("Settings'");
}
