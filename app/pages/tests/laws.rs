//! Each page's two laws (`viewtree::dom::page_laws`) over generated runs: whatever the box answers
//! and whatever a person does, the view is a pure function of the model, and the patched page is a
//! fresh draw of it. The box's answers are generated as the JSON it prints, so a run also steps
//! through refusals, failures, answers of the wrong shape, and answers that move under a page.

use proptest::prelude::*;
use serde_json::{Value, json};
use suzatary_pages::runtime::{Answer, Behind, Context, Effect, Gate, Input};
use suzatary_pages::zone::Offset;
use suzatary_pages::{mail, message, thread};
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
    fn message_laws(writes: bool, msgs in proptest::collection::vec(prop_oneof![
        held::<message::Hold, message::Ask>(message::Hold::Message, page()).prop_map(|i| message::Msg::Host(Box::new(i))),
        any::<bool>().prop_map(|w| message::Msg::Host(Box::new(Input::Context(context(w))))),
        Just(message::Msg::Back),
        Just(message::Msg::Retry),
    ], 0..8)) {
        page_laws::<message::Message>(
            || message::Flags { context: context(writes), id: "i1@example.test".to_owned() },
            &msgs,
        );
    }

    #[test]
    fn thread_laws(msgs in proptest::collection::vec(prop_oneof![
        held::<thread::Hold, thread::Ask>(
            thread::Hold::Thread,
            proptest::collection::vec(page(), 0..4).prop_map(|ms| json!({"thread": "t1", "messages": ms})),
        ).prop_map(|i| thread::Msg::Host(Box::new(i))),
        Just(thread::Msg::Back),
        Just(thread::Msg::Retry),
    ], 0..8)) {
        page_laws::<thread::Thread>(
            || thread::Flags { context: context(true), id: "t1".to_owned() },
            &msgs,
        );
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
