use crate::common::*;

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
                    body: Text::printed(&node(json!({"type": "All"}), &p[0], vec![home, work])),
                },
            }
        })
}

/// What the tab has not handed the box: nothing, or one act on `a`.
fn list_unsent() -> impl Strategy<Value = Value> {
    prop::sample::select(vec!["", "a"]).prop_map(|t| {
        let (todos, acts) = if t.is_empty() {
            (json!({}), json!([]))
        } else {
            (json!({t: "Sending"}), json!([{"todo": t, "name": "n", "at": "2026-09-04T12:00:00"}]))
        };
        json!({"todos": todos, "acts": acts, "refused": [], "blocked": 0, "stranded": 0, "losses": [], "waiting": 0})
    })
}

fn list_host() -> impl Strategy<Value = todos::Msg> {
    use suzatary_pages::todoparts::{Acting, Phase};
    use todos::{Ask, Hold, Kept, Listen, Msg};
    type In = Input<Hold, Ask, Listen>;
    let host = |i: In| Msg::Host(Box::new(i));
    let done = |ask: Ask, body: Value| Input::Answered {
        ask,
        answer: Answer::Done {
            body: Text::printed(&body),
        },
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
        held::<Hold, Ask, Listen>(Hold::Unsent, list_unsent()).prop_map(host),
        held::<Hold, Ask, Listen>(Hold::Route, route).prop_map(host),
        kept.prop_map(move |(k, v)| host(Input::Held {
            ask: Hold::Kept(k),
            reading: Reading::Val {
                v: Answer::Done { body: Text::printed(&v) }
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
        Just(Msg::Retry),
    ]
}

fn list_msg() -> impl Strategy<Value = todos::Msg> {
    prop_oneof![list_host(), list_person()]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn list_laws(writes: bool, beside: bool, msgs in proptest::collection::vec(list_msg(), 0..24)) {
        let opens = if beside { suzatary_pages::runtime::Opens::Beside } else { suzatary_pages::runtime::Opens::Over };
        page_laws::<todos::Todos>(|| todos::Flags {
            context: Context { opens, ..context(writes) },
            chain: "life".to_owned(),
            title: "Todos".to_owned(),
            now: 1_788_000_000_000,
        }, &msgs);
    }
}

proptest! {
    /// Law: the cursor is focus, so no answer that only adds a line above the list may draw the
    /// list again. A briefing for today, or setup's notice, answers after the list is drawn and a
    /// key has moved the cursor; the step that draws it removes nothing and makes no row anew.
    #[test]
    fn a_notice_above_the_list_never_draws_the_list_again(entry in list_entry("c", &[])) {
        let held = |ask: todos::Hold, v: Value| todos::Msg::Host(Box::new(Input::Held {
            ask,
            reading: Reading::Val { v: Answer::Done { body: Text::printed(&v) } },
            behind: None,
        }));
        let (mut page, _) = Runtime::<todos::Todos>::start(todos::Flags {
            context: context(true),
            chain: "life".to_owned(),
            title: "Todos".to_owned(),
            now: 1_788_000_000_000,
        });
        let (_, asked) = page.step(held(todos::Hold::Fold, json!({"entries": [entry], "at": "2026-08-29T10:00:00"})));
        let n = asked
            .iter()
            .find_map(|e| match e {
                Effect::Core { ask: todos::Ask::Tree { n }, .. } => Some(*n),
                _ => None,
            })
            .expect("the page asks for its tree");
        let node = |place: Value, children: Vec<Value>| {
            json!({"place": place, "term": {"kind": "ref", "todo": "c"}, "priced": {"type": "Value", "v": 0.4}, "children": children})
        };
        let tree = node(json!({"type": "All"}), vec![node(
            json!({"type": "Category", "v": "work"}),
            vec![node(json!({"type": "Todo", "v": "c"}), Vec::new())],
        )]);
        page.step(todos::Msg::Host(Box::new(Input::Answered {
            ask: todos::Ask::Tree { n },
            answer: Answer::Done { body: Text::printed(&tree) },
        })));
        let (_, moved) = page.step(todos::Msg::Host(Box::new(Input::Heard {
            ask: todos::Listen::Keys,
            frame: json!({"type": "Move", "by": 1}).to_string(),
        })));
        let field = moved
            .iter()
            .find_map(|e| match e {
                Effect::Focus { field } => Some(field.clone()),
                _ => None,
            })
            .expect("j moves the cursor");
        let days: Vec<Value> = ["2026-08-28", "2026-08-29", "2026-08-30"]
            .map(|date| json!({"date": date, "title": "Today", "words": 10}))
            .into();
        let briefed = json!({"briefings": days, "briefs": []});
        let (patches, _) = page.step(held(todos::Hold::Briefings, briefed));
        let drawn = format!("{patches:?}");
        prop_assert!(drawn.contains("todaybriefing"), "the notice is drawn: {drawn}");
        prop_assert!(!drawn.contains("Remove"), "nothing is removed: {drawn}");
        prop_assert!(!drawn.contains(&format!("{field:?}")), "the cursor's row is not made anew: {drawn}");
    }
}

/// One step of the world one todo's line lives in: this tab seals its next act, the box takes the
/// oldest act it has not taken, the tab drops the newest act it holds (a reset, or a store the box
/// no longer serves), a reading of the tab's replica arrives `later` steps of five seconds after
/// the last, or the worker answers the last tree asked.
#[derive(Debug, Clone, Copy)]
enum World {
    Seal,
    Take,
    Drop,
    Read { later: u32 },
    Grow,
}

#[derive(Debug, Clone, Copy, Default)]
struct Act {
    taken: bool,
    dropped: bool,
}

fn drawn(page: &Runtime<todos::Todos>, todo: &str) -> bool {
    fn row(seen: &viewtree::dom::Seen, todo: &str) -> bool {
        match seen {
            viewtree::dom::Seen::Element { attrs, kids, .. } => {
                (attrs
                    .get("class")
                    .is_some_and(|c| c.split(' ').any(|w| w == "row"))
                    && attrs.get("data-todo").is_some_and(|t| t == todo))
                    || kids.iter().any(|k| row(k, todo))
            }
            _ => false,
        }
    }
    viewtree::dom::fresh(&<todos::Todos as viewtree::program::Program>::view(
        page.model(),
    ))
    .iter()
    .any(|s| row(s, todo))
}

/// The price a tree question's members give `a`, printed as the page asks it; `None` when it does not
/// ask for `a`.
fn priced_a(asked: &str) -> Option<f64> {
    let member = &asked[asked.find("todo: \"a\"")?..];
    let worth =
        &member[member.find("worth: Value { v: Some(")? + "worth: Value { v: Some(".len()..];
    worth[..worth.find(')')?].parse().ok()
}

/// The instant `step` five-second steps after the first reading.
fn instant(step: u32) -> String {
    let t = 5 * step;
    format!("2026-09-04T12:{:02}:{:02}.000000", t / 60, t % 60)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Law: this tab's acts on todo `a` alternate Done and Undo by their index, each stamped ten
    /// seconds after the one before, so a reading can name an act it does not yet believe. A reading
    /// is folded from the tab's replica, so it names every act the tab holds and none it dropped.
    /// Whatever order the seals, the box's takings, the drops, the readings and the trees come in,
    /// `a`'s line is in sight from the seal of an act that leaves it open until the next seal or
    /// drop, and out of sight once the box has every act the tab holds and a current tree drawn from
    /// a reading that believes the newest of them, a Done, whatever was dropped after it.
    #[test]
    fn a_line_an_act_leaves_open_never_leaves_the_list(
        entry in list_entry("a", &[]),
        world in proptest::collection::vec(prop_oneof![
            Just(World::Seal),
            Just(World::Take),
            Just(World::Drop),
            Just(World::Grow),
            (0u32..4).prop_map(|later| World::Read { later }),
        ], 1..32),
    ) {
        let act_at = |k: usize| instant(2 * u32::try_from(k).expect("few acts") + 1);
        let opens = |k: usize| k % 2 == 1;
        let held = |ask: todos::Hold, v: Value| todos::Msg::Host(Box::new(Input::Held {
            ask,
            reading: Reading::Val { v: Answer::Done { body: Text::printed(&v) } },
            behind: None,
        }));
        let live = |acts: &[Act]| (0..acts.len()).filter(|k| !acts[*k].dropped).collect::<Vec<_>>();
        let price = |open: bool| if open { 0.4 } else { 1.0 };
        let open_at = |acts: &[Act], step: u32| {
            live(acts).into_iter().rev().find(|k| act_at(*k) <= instant(step)).is_none_or(opens)
        };
        let reading = |acts: &[Act], step: u32| {
            let open = open_at(acts, step);
            let state = if open { "open" } else { "completed" };
            let mut e = entry.clone();
            e["value"] = json!(price(open));
            e["state"] = json!(state);
            e["states"] = json!([state]);
            e["stream"] = json!(live(acts).into_iter().map(|k| format!("n{k}")).collect::<Vec<_>>());
            held(todos::Hold::Fold, json!({"entries": [e], "at": instant(step)}))
        };
        let unsent = |acts: &[Act]| {
            let acts: Vec<Value> = live(acts).into_iter().filter(|k| !acts[*k].taken).map(|k| json!({"todo": "a", "name": format!("n{k}"), "at": act_at(k)})).collect();
            let todos = if acts.is_empty() { json!({}) } else { json!({"a": "Sending"}) };
            held(todos::Hold::Unsent, json!({"todos": todos, "acts": acts, "refused": [], "blocked": 0, "stranded": 0, "losses": [], "waiting": 0}))
        };
        let (mut page, _) = Runtime::<todos::Todos>::start(todos::Flags { context: context(true), chain: "life".to_owned(), title: "Todos".to_owned(), now: 1_788_000_000_000 });
        // The worker answers each tree asked: `a` under its category when asked for, priced as the
        // question's members price it, so a line a Done prices whole sits in a card folded by default.
        let node = |place: Value, v: f64, children: Vec<Value>| {
            json!({"place": place, "term": {"kind": "ref", "todo": "a"}, "priced": {"type": "Value", "v": v}, "children": children})
        };
        let grown = |a: Option<f64>| {
            let lines = a.map_or_else(Vec::new, |v| {
                vec![node(json!({"type": "Category", "v": "home"}), v, vec![node(json!({"type": "Todo", "v": "a"}), v, Vec::new())])]
            });
            Text::printed(&node(json!({"type": "All"}), a.unwrap_or(1.0), lines))
        };
        let mut acts: Vec<Act> = Vec::new();
        let mut step = 0u32;
        // The last tree asked and not yet answered: its number, and the price its members give `a`
        // when it asks for `a`, which is all the worker prices it from.
        let mut asked: Option<(u32, Option<f64>)> = None;
        let tree = |effects: Vec<Effect<todos::Hold, todos::Ask, todos::Listen>>| {
            effects.into_iter().filter_map(|e| match e {
                Effect::Core { ask: todos::Ask::Tree { n }, .. } => Some((n, priced_a(&format!("{e:?}")))),
                _ => None,
            }).next_back()
        };
        let grow = |page: &mut Runtime<todos::Todos>, (n, a): (u32, Option<f64>)| {
            page.send(todos::Msg::Host(Box::new(Input::Answered { ask: todos::Ask::Tree { n }, answer: Answer::Done { body: grown(a) } }))).effects
        };
        let first = page.send(reading(&acts, 0)).effects;
        if let Some(t) = tree(first) {
            prop_assert!(tree(grow(&mut page, t)).is_none());
        }
        // Whether the line has been in sight since the last seal or drop, whether a reading came
        // since then, and whether a drop waits for the reading of the replica it moved, which the
        // tab always folds: until it comes, the reading drawn still names the dropped act.
        let (mut back, mut read, mut dropping) = (true, true, false);
        for w in world {
            let effects = match w {
                World::Seal => {
                    acts.push(Act::default());
                    (back, read) = (false, false);
                    page.send(unsent(&acts)).effects
                }
                World::Take => match live(&acts).into_iter().find(|k| !acts[*k].taken) {
                    Some(k) => {
                        acts[k].taken = true;
                        page.send(unsent(&acts)).effects
                    }
                    None => Vec::new(),
                },
                World::Drop => match live(&acts).last() {
                    Some(&k) => {
                        acts[k].dropped = true;
                        (back, read, dropping) = (false, false, true);
                        page.send(unsent(&acts)).effects
                    }
                    None => Vec::new(),
                },
                World::Read { later } => {
                    step += later;
                    (read, dropping) = (true, false);
                    page.send(reading(&acts, step)).effects
                }
                World::Grow => asked.take().map(|t| grow(&mut page, t)).unwrap_or_default(),
            };
            asked = tree(effects).or(asked);
            let pending = asked;
            let shown = drawn(&page, "a");
            let held_acts = live(&acts);
            let last = held_acts.last().copied();
            let said = format!("{w:?} acts {acts:?} at {step}");
            if dropping {
                back = false;
            } else if last.is_none_or(opens) {
                prop_assert!(shown || !back, "once back, a's line stays in sight until the next seal or drop: {said}");
                prop_assert!(shown || pending.is_some(), "a tree drawn after an act that leaves a open shows its line: {said}");
                back = shown;
            } else if let Some(k) = last.filter(|k| read && pending.is_none() && held_acts.iter().all(|k| acts[*k].taken) && act_at(*k) <= instant(step)) {
                prop_assert!(!shown, "a Done the box took and the reading believes takes {k}'s line out of sight, whatever was dropped: {said}");
            }
        }
    }
}
