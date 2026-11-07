use crate::common::*;

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

fn todo_answer() -> impl Strategy<Value = Input<todo::Hold, todo::Ask>> {
    todo_asked().prop_map(|(ask, answer)| Input::Answered { ask, answer })
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

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
}
