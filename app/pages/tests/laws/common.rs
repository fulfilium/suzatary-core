pub(crate) use proptest::prelude::*;
pub(crate) use serde_json::{Value, json};
pub(crate) use suzatary_base::answer::Kind as BlockKind;
pub(crate) use suzatary_core::release::{Lot, Named, Place};
pub(crate) use suzatary_core::secretary::memory::{FactId, Kind};
pub(crate) use suzatary_core::wire::cards::{AboutPayload, HeldTodo};
pub(crate) use suzatary_core::wire::todo::{EntryState, Fulfillment, Neighbour};
pub(crate) use suzatary_core::wire::topics::{BlockRow, FactRow, FactSaid, MemoryPayload, SaidOn};
pub(crate) use suzatary_core::wire::{
    BriefRow, BriefingDay, BriefingPayload, BriefingRow, BriefingsPayload, CalendarPayload,
    RecordPayload, RecordSummary, ReleaseItem, ReleaseRow, ReleasesPayload,
};
pub(crate) use suzatary_pages::reading::{Nothing, One, Opened};
pub(crate) use suzatary_pages::runtime::{
    Answer, Behind, Body, Context, Effect, Gate, Input, Reach, Untrusted,
};
pub(crate) use suzatary_pages::todoparts::{Seal, Verb};
pub(crate) use suzatary_pages::waiting::Watched;
pub(crate) use suzatary_pages::zone::Offset;
pub(crate) use suzatary_pages::{
    about, briefing, briefings, calendar, card, chats, composer, conversation, draft, inbox, mail,
    memory, message, needsyou, reading as one, secretary, settings, thread, todo, todos, typstlist,
};
pub(crate) use viewtree::Runtime;
pub(crate) use viewtree::dom::page_laws;
pub(crate) use viewtree::kit::Reading;

pub(crate) fn context(writes: bool) -> Context {
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

pub(crate) fn row() -> impl Strategy<Value = Value> {
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

pub(crate) fn page() -> impl Strategy<Value = Value> {
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

pub(crate) fn behind() -> impl Strategy<Value = Option<Behind>> {
    proptest::option::of(
        proptest::option::of(Just("offline".to_owned())).prop_map(|why| Behind {
            why,
            since: 1_780_000_000_000,
        }),
    )
}

pub(crate) fn reading(
    answer: impl Strategy<Value = Value>,
) -> impl Strategy<Value = Reading<Answer>> {
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

pub(crate) fn held<H: Clone + std::fmt::Debug + 'static, A: std::fmt::Debug, S: std::fmt::Debug>(
    ask: H,
    answer: impl Strategy<Value = Value>,
) -> impl Strategy<Value = Input<H, A, S>> {
    (reading(answer), behind()).prop_map(move |(reading, behind)| Input::Held {
        ask: ask.clone(),
        reading,
        behind,
    })
}

// The small pages: answers built from the core's types and printed as the box prints them.

pub(crate) fn words() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("one".to_owned()),
        Just("two words".to_owned()),
        Just("<b>three</b> & more".to_owned()),
    ]
}

pub(crate) fn printed<T: serde::Serialize>(t: &T) -> Value {
    serde_json::to_value(t).expect("a payload prints")
}

pub(crate) fn todo_write() -> impl Strategy<Value = todo::Write> {
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

pub(crate) fn todo_asked() -> impl Strategy<Value = (todo::Ask, Answer)> {
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

pub(crate) fn list_entry(
    id: &'static str,
    refs: &'static [&'static str],
) -> impl Strategy<Value = Value> {
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

pub(crate) fn list_fold() -> impl Strategy<Value = Value> {
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

pub(crate) fn one_msgs(
    answer: impl Strategy<Value = Value>,
) -> impl Strategy<Value = Vec<one::Msg>> {
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

pub(crate) fn answered<H: std::fmt::Debug, A: Clone + std::fmt::Debug + 'static>(
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

pub(crate) fn opened<A>(writes: bool, at: A) -> Opened<A> {
    Opened {
        context: context(writes),
        at,
    }
}

pub(crate) fn turn_row(index: usize) -> impl Strategy<Value = Value> {
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

pub(crate) fn outcome() -> impl Strategy<Value = Answer> {
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

pub(crate) fn composer_ev() -> impl Strategy<Value = composer::Ev> {
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
