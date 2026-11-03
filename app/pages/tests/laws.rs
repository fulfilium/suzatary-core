//! Each page's two laws (`viewtree::dom::page_laws`) over generated runs: whatever the box answers
//! and whatever a person does, the view is a pure function of the model, and the patched page is a
//! fresh draw of it. The box's answers are generated as the JSON it prints, so a run also steps
//! through refusals, failures, answers of the wrong shape, and answers that move under a page.

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
use suzatary_pages::runtime::{Answer, Behind, Body, Context, Effect, Gate, Input};
use suzatary_pages::zone::Offset;
use suzatary_pages::{
    about, briefing, briefings, calendar, mail, memory, message, reading as one, settings, thread,
};
use viewtree::Runtime;
use viewtree::dom::page_laws;
use viewtree::kit::Reading;

fn context(writes: bool) -> Context {
    Context {
        gate: if writes { Gate::Writes } else { Gate::Reads },
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

/// What the tab's register of a question may hold: on its way, failed, refused, or an answer.
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
        reading(listing()).prop_map(move |r| host(Input::Answered {
            ask: mail::Ask::More,
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

fn held<H: Clone + std::fmt::Debug + 'static, A: std::fmt::Debug>(
    ask: H,
    answer: impl Strategy<Value = Value>,
) -> impl Strategy<Value = Input<H, A>> {
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

/// A double tap on Cancel is two writes unless the act is running: the second tap asks nothing
/// until the first is answered, and a refusal is said and lets it be tapped again.
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

/// A search is a navigation: the address carries the query as typed, trimmed, and the account the
/// listing had, each encoded as the router reads it.
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

// The small pages of stage 4: their answers built from the core's own types and printed as the
// box prints them.

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
            held::<one::Hold, one::Ask>(one::Hold::Page, answer)
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
        held::<memory::Hold, memory::Ask>(memory::Hold::Memory, memory_payload()).prop_map(|i| memory::Msg::Host(Box::new(i))),
        held::<memory::Hold, memory::Ask>(memory::Hold::Record, record_payload()).prop_map(|i| memory::Msg::Host(Box::new(i))),
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
        held::<about::Hold, about::Ask>(about::Hold::About, about_payload()).prop_map(|i| about::Msg::Host(Box::new(i))),
        held::<about::Hold, about::Ask>(about::Hold::Releases, releases_payload()).prop_map(|i| about::Msg::Host(Box::new(i))),
        Just(about::Msg::Retry),
    ], 0..8)) {
        page_laws::<about::About>(|| opened(true, Nothing {}), &msgs);
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

/// A double tap would be two writes: a second Forget of the same fact sends nothing until the
/// first is answered, and an answered one reads the memory again.
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
        json!({"key": format!("l{n}"), "label": format!("Line {n}"), "detail": null,
            "standing": standing, "cards": cards})
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

fn cards() -> impl Strategy<Value = Value> {
    prop::sample::select(vec![
        json!([]),
        json!([{"reference": "proposal:setup/1", "act": "secret", "stands": "open", "version": "a"}]),
        json!([{"reference": "proposal:setup/1", "act": "sign_in", "stands": "settled", "version": "b"}]),
    ])
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
    ]
}

fn settings_msg() -> impl Strategy<Value = settings::Msg> {
    let host = |i: Input<settings::Hold, settings::Ask>| settings::Msg::Host(Box::new(i));
    prop_oneof![
        held(settings::Hold::Overview, overview()).prop_map(host),
        held(settings::Hold::Device, device()).prop_map(host),
        held(settings::Hold::Cards, cards()).prop_map(host),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
        settings_act().prop_map(settings::Msg::Run),
        (settings_act(), any::<bool>()).prop_map(move |(ask, done)| host(Input::Answered {
            ask,
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
