use crate::cards::*;
use crate::common::*;

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
            "signing": {"type": "Keyed", "key": "ab".repeat(32), "standing": "untrusted"}, "earlier": []}),
        json!({"persist": {"type": "Denied"}, "usage": {"bytes": 0.0, "count": 0}, "budget": 1e6,
            "signing": {"type": "Unreadable", "e": "it does not read"}, "earlier": ["topics", "chain/life"]}),
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
                    body: Text::printed(&json!({"id": "setup-1"})),
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
                v: Answer::Done {
                    body: Text::printed(&body),
                },
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
                after: card::After::Toast {
                    done: "Stored".to_owned(),
                },
                yes: false,
            },
            signed: "standing:0:".to_owned(),
        }),
        answer: Answer::Done {
            body: Text::printed(
                &json!({"type": "Answered", "answer": {"type": "Taken"}, "held": true}),
            ),
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

/// A fix's tap answered goes on as its link says: a path the box serves outside the app (Google's
/// consent) is a document load, never a route of the app, and a place of the app is a route.
#[test]
fn a_fix_answered_leaves_the_app_or_goes_to_a_place_as_its_link_says() {
    let answered = |after: card::After| {
        let (mut rt, _) = Runtime::<settings::Settings>::start(settings::Flags {
            context: context(true),
        });
        rt.step(settings::Msg::Host(Box::new(Input::Answered {
            ask: settings::Ask::Card(card::Asking::Act {
                key: card::Key {
                    conversation: "default".to_owned(),
                    index: 0,
                },
                then: card::Then { after, yes: true },
                signed: "failed:1:".to_owned(),
            }),
            answer: Answer::Done {
                body: Text::printed(
                    &json!({"type": "Answered", "answer": {"type": "Taken"}, "held": true}),
                ),
            },
        })))
        .1
    };
    let consent = "/api/oauth/google/consent/p1".to_owned();
    let left = answered(card::After::Leave {
        href: consent.clone(),
    });
    assert!(
        left.iter()
            .any(|e| matches!(e, Effect::Leave { href } if *href == consent)),
        "{left:?}"
    );
    assert!(
        !left.iter().any(|e| matches!(e, Effect::Go { .. })),
        "{left:?}"
    );
    let went = answered(card::After::Go {
        href: "#/settings".to_owned(),
    });
    assert!(
        went.iter()
            .any(|e| matches!(e, Effect::Go { href } if href == "#/settings")),
        "{went:?}"
    );
}

/// THE LINK A GRANT'S YES SENDS IS THE ONE ITS LINE DREW BESIDE IT: a link's card refused for
/// its origin files a grant on the same line, and the grant's yes taps the link's card again
/// with the link the tab kept. Once the grant is performed the box's overview names it no more,
/// so an overview read during the tap, landing before the tap's answer, must not lose the link.
/// `allow.json` is what the links scene's Settings page held before the yes was answered, the
/// overview read after it, and the answer: the order in which the resend can be lost.
#[test]
fn a_grant_answered_after_its_line_moved_still_sends_its_link() {
    let recorded: Value = serde_json::from_str(include_str!("allow.json")).expect("the recording");
    let input = |v: &Value| -> settings::Msg { hosted::<settings::Settings>(v) };
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn settings_laws(writes: bool, msgs in proptest::collection::vec(settings_msg(), 0..12)) {
        page_laws::<settings::Settings>(|| settings::Flags { context: context(writes) }, &msgs);
    }
}

/// What happens while a setup card that asks a value stands in Settings: the overview with its
/// offer or without, the Inbox's rows read or failing (the line that says the cards are on their
/// way comes and goes), this device's reading, the card's body, and the person's steps.
#[derive(Debug, Clone)]
enum Around {
    Overview(bool),
    Rows(bool),
    Device(Value),
    Body,
    Ui(card::Ev),
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Law (the secret ask rests on it, in Settings as in the Inbox): whatever comes and goes
    /// above Settings' cards (this device's part, the line that says the cards are on their way,
    /// the offers), a setup card's value field is drawn once while it stands and never made
    /// another's.
    #[test]
    fn a_settings_card_keeps_its_field_as_the_parts_above_it_come_and_go(
        steps in proptest::collection::vec(prop_oneof![
            any::<bool>().prop_map(Around::Overview),
            any::<bool>().prop_map(Around::Rows),
            device().prop_map(Around::Device),
            Just(Around::Body),
            prop_oneof![
                Just(card::Ev::Open(card::Step::Saying { verb: "dismiss".to_owned() })),
                Just(card::Ev::Close),
                any::<bool>().prop_map(card::Ev::Details),
            ].prop_map(Around::Ui),
        ], 0..20),
    ) {
        use proptest::strategy::ValueTree as _;
        use viewtree::Program;
        const FIELD: &str = "secret/setup/1";
        let held = |ask: settings::Hold, reading: Reading<Answer>| settings::Msg::Host(Box::new(Input::Held { ask, reading, behind: None }));
        let done = |body: &Value| Reading::Val { v: Answer::Done { body: Text::printed(body) } };
        let mut secret = proposal(1).new_tree(&mut proptest::test_runner::TestRunner::deterministic()).expect("a row").current();
        secret["state"] = json!("standing");
        secret["act"] = json!({"kind": "secret", "label": "the calendar's link", "found": null, "link": null});
        secret["card"]["answers"] = json!([{"verb": "store", "label": "Store", "done": "Stored", "asks": "value", "input": "The link"}]);
        secret["card"]["problem"] = json!("");
        secret["card"]["link"] = json!(null);
        let overview = |offers: bool| json!({
            "groups": [{"group": "models", "lines": [{"key": "l0", "label": "Line 0", "detail": null,
                "standing": {"kind": "missing", "what": ["a link"]}, "cards": ["proposal:setup/1"], "replaces": []}]}],
            "ask": "setup",
            "offers": if offers { json!([{"template": "google", "group": "mail", "title": "a Google account"}]) } else { json!([]) }});
        let rows = json!({"payload": {"at": "", "failed": [], "proposals": [{"id": "p1", "asked": "proposal:setup/1", "serves": null,
            "headline": "", "by": {"kind": "setup"}, "why": "", "state": {"kind": "open"}, "value": null, "at": "2026-11-10T04:00:00"}]}, "held": []});
        let body = json!({"id": "setup", "title": "Setup", "model": "", "opened": "", "turns": [], "proposals": [secret],
            "unplaced": [], "state": "idle", "first": 0, "total": 0, "again": null});
        let (mut m, _) = settings::Settings::init(settings::Flags { context: context(true) });
        settings::Settings::update(&mut m, held(settings::Hold::Overview, done(&overview(false))));
        let (mut painter, first) = viewtree::Painter::start(settings::Settings::view(&m));
        let mut dom = viewtree::dom::Dom::default();
        dom.apply(&first);
        let (mut field, mut drawn) = (None, 0);
        for step in steps {
            let msg = match step {
                Around::Overview(offers) => held(settings::Hold::Overview, done(&overview(offers))),
                Around::Rows(true) => held(settings::Hold::Rows(Watched::Proposals), done(&rows)),
                Around::Rows(false) => held(settings::Hold::Rows(Watched::Proposals), Reading::Error { e: "no answer".to_owned() }),
                Around::Device(device) => held(settings::Hold::Device, done(&device)),
                Around::Body => held(settings::Hold::Rows(Watched::Cards("setup".to_owned())), done(&body)),
                Around::Ui(ev) => settings::Msg::Card(card::Key { conversation: "setup".to_owned(), index: 1 }, ev),
            };
            settings::Settings::update(&mut m, msg);
            let patches = painter.paint(settings::Settings::view(&m));
            for patch in &patches {
                if let viewtree::Patch::Set { id, name, value } = patch && name == "data-field" {
                    if value == FIELD {
                        drawn += 1;
                        field = Some(*id);
                    } else {
                        prop_assert_ne!(Some(*id), field, "the field became {}", value);
                    }
                }
            }
            prop_assert!(drawn <= 1, "the field was drawn {} times in one stay", drawn);
            dom.apply(&patches);
            if !format!("{:?}", dom.page()).contains(FIELD) {
                (field, drawn) = (None, 0);
            }
        }
    }
}
