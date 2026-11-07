use crate::common::*;

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
}
