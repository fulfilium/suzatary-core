use crate::cards::*;
use crate::common::*;

fn windowed() -> impl Strategy<Value = Value> {
    prop_oneof![
        3 => (proptest::collection::vec(any::<bool>(), 0..5), conversation_cards("talk"), prop::sample::select(vec!["idle", "thinking"]), prop::sample::select(vec!["Asked", "Quiet", "Unheard"])).prop_flat_map(|(shape, cards, state, about)| {
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
                json!({"type": "Read", "about": about, "payload": {"read": c, "html": {}, "drawn": []}})
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

fn uploaded() -> impl Strategy<Value = Answer> {
    prop::sample::select(vec![
        Answer::Done {
            body: Text::printed(
                &json!({"kind": "kept", "file": {"id": "f1", "media_type": "application/pdf", "name": "w2.pdf", "private": ""}}),
            ),
        },
        Answer::Done {
            body: Text::printed(
                &json!({"kind": "sensitive", "why": "its text names an identity number", "folder": "/my-files/Suzatary/Private"}),
            ),
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
        (reading(windowed()), behind()).prop_map(move |(reading, behind)| host(Input::Held {
            ask: Hold::Window,
            reading,
            behind
        })),
        frame().prop_map(move |frame| host(Input::Heard {
            ask: conversation::Stream::Conversation,
            frame
        })),
        held(
            Hold::Draft,
            prop::sample::select(vec![
                json!({"text": "", "others": []}),
                json!({"text": "draft", "others": ["elsewhere"]})
            ])
        )
        .prop_map(host),
        held(
            Hold::Scroll,
            prop::sample::select(vec![
                json!({"by": "reader", "at": "above"}),
                json!({"by": "reader", "at": "bottom"}),
                json!({"by": "page", "at": "above"})
            ])
        )
        .prop_map(host),
        held(
            Hold::Block("ab".to_owned()),
            Just(json!({"html": "<p>Hi</p>", "key": "k"}))
        )
        .prop_map(host),
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
        outcome().prop_map(move |answer| host(Input::Answered {
            ask: Ask::Said(Box::new(conversation::Spoken {
                text: "hello".to_owned(),
                references: Vec::new(),
                files: Vec::new(),
                mode: composer::Mode::Asking
            })),
            answer
        })),
        (prop::sample::select(vec!["A name", "Another"]), outcome()).prop_map(
            move |(title, answer)| host(Input::Answered {
                ask: Ask::Rename {
                    title: title.to_owned()
                },
                answer
            })
        ),
        (1u32..3, uploaded()).prop_map(move |(n, answer)| host(Input::Answered {
            ask: Ask::Composer(composer::Asking::Upload(n)),
            answer
        })),
        (1u32..3, uploaded()).prop_map(move |(n, answer)| host(Input::Answered {
            ask: Ask::Composer(composer::Asking::Keep(n)),
            answer
        })),
        Just(host(Input::Answered {
            ask: Ask::Slice,
            answer: Answer::Done {
                body: Text::printed(&json!(1_790_000_005_000_i64))
            }
        })),
        Just(host(Input::Opened {
            ask: conversation::Stream::Conversation
        })),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn conversation_laws(writes: bool, panel: bool, msgs in proptest::collection::vec(conversation_msg(), 0..16)) {
        let at = if panel {
            conversation::At::Panel { about: "todo:life/renewcard".to_owned() }
        } else {
            conversation::At::Page { id: "talk".to_owned(), refs: vec!["todo:life/renewcard".to_owned()], focus: true, at: None }
        };
        page_laws::<conversation::Conversation>(|| conversation::Flags { context: context(writes), at: at.clone() }, &msgs);
    }
}

/// The panel's reference, said in one round of the default conversation and not in another.
const ABOUT: &str = "todo:life/renewcard";

/// What happens to the default conversation on a device that holds part of it.
#[derive(Debug, Clone)]
enum Arrival {
    /// These rounds' lines land.
    Lines(Vec<usize>),
    /// The round's turns about the reference are rewound away: it touches it no more.
    Rewound(usize),
    /// A pull ends whole.
    Whole,
}

/// A conversation of rounds, each touching the reference or not, and its rounds landing in any
/// order and grouping, a rewind among them, the pull ending whole with the last or after it.
fn arrivals() -> impl Strategy<Value = (Vec<bool>, Vec<Arrival>)> {
    proptest::collection::vec(any::<bool>(), 1..7).prop_flat_map(|touches| {
        let n = touches.len();
        (
            Just(touches),
            Just((0..n).collect::<Vec<_>>()).prop_shuffle(),
            proptest::collection::vec(any::<bool>(), n),
            proptest::collection::vec((0..n, 0..n), 0..3),
            any::<bool>(),
        )
            .prop_map(|(touches, order, cuts, rewinds, whole_last)| {
                let mut steps = Vec::new();
                let mut batch = Vec::new();
                for (k, round) in order.into_iter().enumerate() {
                    batch.push(round);
                    if cuts[k] || k + 1 == touches.len() {
                        steps.push(Arrival::Lines(std::mem::take(&mut batch)));
                        let after = steps.len() - 1;
                        steps.extend(
                            rewinds
                                .iter()
                                .filter(|(at, _)| *at == after)
                                .map(|(_, r)| Arrival::Rewound(*r)),
                        );
                    }
                }
                if whole_last {
                    steps.push(Arrival::Whole);
                } else {
                    let at = steps
                        .iter()
                        .rposition(|s| matches!(s, Arrival::Lines(_)))
                        .unwrap_or(0);
                    steps.insert(at + 1, Arrival::Whole);
                }
                (touches, steps)
            })
    })
}

/// The device's lines at one step: the rounds held, which still touch the reference, and whether
/// a pull has ended whole.
#[derive(Debug, Clone, Default)]
struct Device {
    held: std::collections::BTreeSet<usize>,
    touches: Vec<bool>,
    rewound: usize,
    whole: bool,
}

impl Device {
    fn step(&mut self, a: &Arrival) {
        match a {
            Arrival::Lines(rounds) => self.held.extend(rounds),
            Arrival::Rewound(round) => {
                if self.held.contains(round) && self.touches[*round] {
                    self.touches[*round] = false;
                    self.rewound += 1;
                }
            }
            Arrival::Whole => self.whole = true,
        }
    }

    fn talks(&self) -> bool {
        self.held.iter().any(|r| self.touches[*r])
    }

    /// The worker's answer to the panel's window about `ABOUT` (`questions.ts`, "Window").
    fn answer(&self) -> Value {
        let (about, rounds): (&str, Vec<usize>) = if self.talks() {
            (
                "Asked",
                self.held
                    .iter()
                    .copied()
                    .filter(|r| self.touches[*r])
                    .collect(),
            )
        } else if self.whole {
            ("Quiet", self.held.iter().copied().collect())
        } else {
            ("Unheard", self.held.iter().copied().collect())
        };
        let turns: Vec<Value> = rounds
            .iter()
            .map(|&i| {
                let refs = if self.touches[i] { json!([ABOUT]) } else { json!([]) };
                json!({"index": i, "text": format!("question {i}"), "at": "2026-10-05T04:00:00", "attempt": "current", "gone": null,
                    "who": "person", "references": refs, "attachments": [], "queued": false})
            })
            .collect();
        let shapes: Vec<Value> = rounds
            .iter()
            .enumerate()
            .map(|(k, &i)| json!({"key": i, "startIndex": i, "endIndex": i, "opening": i, "trace": [], "answer": null,
                "proposals": [], "earlier": [], "day": "2026-10-05", "label": "Monday 5 October", "marked": k == 0}))
            .collect();
        // As the core's window counts it: the turns of the rounds about the reference, else every turn.
        let total = match about {
            "Asked" => rounds.len(),
            _ => self.held.len() + self.rewound,
        };
        json!({"type": "Read", "about": about, "payload": {"html": {}, "drawn": [], "read": {
            "id": "default", "title": "", "model": "", "opened": "2026-10-05T04:00:00",
            "lines": rounds.iter().map(|i| format!("l{i}")).collect::<Vec<_>>(), "turns": turns, "waiting": [], "proposals": [],
            "unplaced": [], "state": "idle", "first": rounds.first().copied().unwrap_or(total), "total": total, "again": null,
            "rounds": shapes}}})
    }
}

/// The page given `answer`, with every slice it asks for drawn; what it draws.
fn drawn_after(rt: &mut Runtime<conversation::Conversation>, answer: &Value) -> String {
    use conversation::{Ask, Hold, Msg};
    let held = Input::Held {
        ask: Hold::Window,
        reading: Reading::Val {
            v: Answer::Done {
                body: Text::printed(answer),
            },
        },
        behind: None,
    };
    rt.step(Msg::Host(Box::new(held)));
    while rt.active().get(&Slot::Asked(Ask::Slice)).is_some() {
        let slice = Input::Answered {
            ask: Ask::Slice,
            answer: Answer::Done {
                body: Text::printed(&json!(1_790_000_005_000_i64)),
            },
        };
        rt.step(Msg::Host(Box::new(slice)));
    }
    format!(
        "{:?}",
        viewtree::dom::fresh(&<conversation::Conversation as viewtree::Program>::view(
            rt.model()
        ))
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The secretary beside a page is a function of the lines the device holds now: after any run
    /// of arrivals, rewinds and a pull ending whole, it draws what a page opened cold on the last
    /// answer draws, and its heading says "nothing" only when the lines are whole and none touches
    /// the reference.
    #[test]
    fn the_panel_draws_what_a_page_opened_on_the_lines_held_now_draws((touches, steps) in arrivals()) {
        let flags = || conversation::Flags { context: context(true), at: conversation::At::Panel { about: ABOUT.to_owned() } };
        let (mut long, _) = Runtime::<conversation::Conversation>::start(flags());
        let mut device = Device { touches, ..Device::default() };
        for step in &steps {
            device.step(step);
            let answer = device.answer();
            let lived = drawn_after(&mut long, &answer);
            let (mut cold, _) = Runtime::<conversation::Conversation>::start(flags());
            prop_assert_eq!(&lived, &drawn_after(&mut cold, &answer), "after {:?}", step);
            let heading = [
                (format!("About {ABOUT} · "), device.talks()),
                (format!("Nothing about {ABOUT} yet · "), !device.talks() && device.whole),
                (format!("Talk about {ABOUT} may not be on this device yet · "), !device.talks() && !device.whole),
            ];
            for (said, now) in heading {
                prop_assert_eq!(lived.contains(&said), now, "{} after {:?}: {}", said, step, lived);
            }
        }
    }
}
