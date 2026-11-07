use crate::common::*;

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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn list_laws(writes: bool, wide: bool, msgs in proptest::collection::vec(list_msg(), 0..24)) {
        page_laws::<todos::Todos>(|| todos::Flags {
            context: Context { wide, ..context(writes) },
            chain: "life".to_owned(),
            now: 1_788_000_000_000,
        }, &msgs);
    }

    #[test]
    fn a_row_whose_act_is_taken_stays_until_the_fold_holds_it(entry in list_entry("a", &[])) {
        let mut done = entry;
        done["state"] = json!("completed");
        done["states"] = json!(["completed"]);
        let held = |ask: todos::Hold, v: Value| todos::Msg::Host(Box::new(Input::Held {
            ask,
            reading: Reading::Val { v: Answer::Done { body: v } },
            behind: None,
        }));
        let unsent = |todos: Value| held(todos::Hold::Unsent, json!({"todos": todos, "refused": [], "blocked": 0, "stranded": 0, "losses": [], "waiting": 0}));
        let (mut page, _) = Runtime::<todos::Todos>::start(todos::Flags { context: context(true), chain: "life".to_owned(), now: 1_788_000_000_000 });
        page.send(held(todos::Hold::Fold, json!({"entries": [done], "at": "2026-09-04T12:00:00"})));
        page.send(unsent(json!({"a": "Sending"})));
        let taken = format!("{:?}", page.send(unsent(json!({}))).effects);
        prop_assert!(!taken.contains("Tree {") || taken.contains("todo: \"a\""), "{taken}");
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
}
