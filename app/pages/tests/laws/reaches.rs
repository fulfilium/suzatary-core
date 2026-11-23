use crate::common::*;
use suzatary_inbox::secretary::card::of;
use suzatary_inbox::wire::inbox::{HeldIn, ProposedAct};
use viewtree::Program;
use viewtree::dom::Seen;

fn act() -> impl Strategy<Value = ProposedAct> {
    let people = |most: usize| {
        proptest::collection::btree_set(
            (
                prop::sample::select(vec!["ada", "grace", "alan", "edsger", "barbara"]),
                0u8..40,
            ),
            0..most,
        )
        .prop_map(|set| {
            set.into_iter()
                .map(|(name, n)| format!("{name}.{n}@example.test"))
                .collect::<Vec<_>>()
        })
    };
    let subject = prop_oneof![
        "[A-Za-z' ]{0,40}",
        "(Re|RE|re): ?(re: ?)?[A-Za-z' ]{0,30}",
        Just("<b>Thursday's review</b> #strong[now] *soon*".to_owned()),
        Just("Re:".to_owned()),
        "[A-Za-z ]{200,320}",
    ];
    let account = prop::sample::select(vec!["me@example.test", "us@example.test"]);
    prop_oneof![
        4 => (account.clone(), people(9), people(6), subject.clone())
            .prop_filter("a message goes to someone", |(_, to, _, _)| !to.is_empty())
            .prop_map(|(account, to, cc, subject)| ProposedAct::Email {
                account: account.to_owned(),
                to,
                cc,
                subject,
                body: "Dear Ada, the documents are attached.".to_owned(),
                attachments: Vec::new(),
                held: HeldIn::Read { values: Vec::new() },
            }),
        2 => (account.clone(), people(9), people(6), subject)
            .prop_filter("a message goes to someone", |(_, to, _, _)| !to.is_empty())
            .prop_map(|(account, to, cc, subject)| ProposedAct::Forward {
                account: account.to_owned(),
                to,
                cc,
                subject,
                note: "See below.".to_owned(),
                attachments: Vec::new(),
                original: "The lease, from the agent".to_owned(),
                style: suzatary_inbox::secretary::proposal::Style::Inline,
                held: Vec::new(),
            }),
        1 => account.prop_map(|account| ProposedAct::Auth {
            account: account.to_owned(),
            sentence: "sign in".to_owned(),
        }),
        1 => Just(ProposedAct::Grant {
            plugin: "ics".to_owned(),
            origin: "https://cal.example.test".to_owned(),
            sentence: "let it fetch".to_owned(),
        }),
    ]
}

/// The text of `seen`; with `closed`, a closed `<details>` keeps only its summary.
fn text(seen: &[Seen], closed: bool) -> String {
    seen.iter()
        .map(|s| match s {
            Seen::Text(t) => t.clone(),
            Seen::Element {
                tag: "details",
                attrs,
                kids,
                ..
            } if closed && !attrs.contains_key("open") => text(
                &kids
                    .iter()
                    .filter(|k| matches!(k, Seen::Element { tag: "summary", .. }))
                    .cloned()
                    .collect::<Vec<_>>(),
                closed,
            ),
            Seen::Element { kids, .. } => text(kids, closed),
            Seen::Foreign { .. } | Seen::Block { .. } => String::new(),
        })
        .collect()
}

fn texts<'a>(seen: &'a [Seen], out: &mut Vec<&'a str>) {
    for s in seen {
        match s {
            Seen::Text(t) => out.push(t),
            Seen::Element { kids, .. } => texts(kids, out),
            Seen::Foreign { .. } | Seen::Block { .. } => {}
        }
    }
}

/// Every element under `seen` whose class list holds `class`, outside a closed `<details>`.
fn shown<'a>(seen: &'a [Seen], class: &str, out: &mut Vec<&'a Seen>) {
    for s in seen {
        if let Seen::Element {
            tag, attrs, kids, ..
        } = s
        {
            if *tag == "details" && !attrs.contains_key("open") {
                continue;
            }
            if attrs
                .get("class")
                .is_some_and(|c| c.split(' ').any(|c| c == class))
            {
                out.push(s);
            }
            shown(kids, class, out);
        }
    }
}

fn card_of(seen: &[Seen]) -> Option<&[Seen]> {
    seen.iter().find_map(|s| match s {
        Seen::Element { attrs, kids, .. } => {
            if attrs.get("data-field").is_some_and(|f| f == "card/s/0") {
                Some(kids.as_slice())
            } else {
                card_of(kids)
            }
        }
        Seen::Text(_) | Seen::Foreign { .. } | Seen::Block { .. } => None,
    })
}

fn part(line: &[Seen], class: &str) -> Option<String> {
    let mut found = Vec::new();
    shown(line, class, &mut found);
    found.first().map(|s| text(std::slice::from_ref(*s), true))
}

/// Each address `said` names, in order, every one of `parties`; any not named are counted, in
/// words, and a count stands for two or more.
fn names_or_counts(said: &str, parties: &[String]) -> Result<(), String> {
    let named: Vec<&str> = said
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| w.contains('@'))
        .collect();
    if named.len() > parties.len() || named.iter().zip(parties).any(|(n, p)| n != p) {
        return Err(format!("{said:?} does not name {parties:?} in order"));
    }
    let more = parties.len() - named.len();
    match more {
        0 if !said.ends_with(" more") => Ok(()),
        0 | 1 => Err(format!("{said:?} counts {more} for {parties:?}")),
        more if said.ends_with(&format!(" and {more} more")) => Ok(()),
        more => Err(format!("{said:?} hides {more} of {parties:?} uncounted")),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Law (the owner approves knowing who a message goes to): on the closed card of every email
    /// or forward, the line names each To recipient in order, or a count of the rest, likewise
    /// each copy, the subject (a reply's thread) as its exact text, and the account it is sent
    /// from; no address on the card, open or closed, is one the proposal does not hold; and a
    /// card whose act reaches nobody outside has no such line.
    #[test]
    fn a_closed_card_names_who_its_message_reaches(act in act()) {
        let card = of(&act);
        let row = json!({"index": 0, "act": act, "card": card, "why": "Because", "sources": ["todo:life/renewcard"],
            "at": "2026-10-05T04:00:00", "state": "standing", "outcome": "", "tries": 0, "turn": 1,
            "coding": null, "announced": null, "revises": null});
        let open = json!({"payload": {"at": "", "proposals": [{"id": "ps", "asked": "proposal:s/0", "serves": null,
            "headline": "", "by": {"kind": "secretary"}, "why": "", "state": {"kind": "open"}, "value": null,
            "at": "2026-10-05T04:00:00"}], "failed": []}, "held": []});
        let held = |ask, body: &Value| inbox::Msg::Host(Box::new(Input::Held {
            ask,
            reading: Reading::Val { v: Answer::Done { body: Text::printed(body) } },
            behind: None,
        }));
        let (mut m, _) = inbox::Inbox::init(inbox::Flags { context: context(true), open: String::new(), now: 1_790_000_000_000 });
        inbox::Inbox::update(&mut m, held(inbox::Hold::Rows(Watched::Proposals), &open));
        inbox::Inbox::update(&mut m, held(inbox::Hold::Rows(Watched::Cards("s".to_owned())), &json!({"title": "s", "proposals": [row]})));
        let (_, first) = viewtree::Painter::start(inbox::Inbox::view(&m));
        let mut dom = viewtree::dom::Dom::default();
        dom.apply(&first);
        let page = dom.page();
        let drawn = card_of(&page).expect("the card is drawn");
        let mut lines = Vec::new();
        shown(drawn, "proposalreaches", &mut lines);
        let (account, to, cc, subject, forward) = match &act {
            ProposedAct::Email { account, to, cc, subject, .. } => (account, to, cc, subject, false),
            ProposedAct::Forward { account, to, cc, subject, .. } => (account, to, cc, subject, true),
            other => {
                prop_assert!(lines.is_empty(), "{other:?} reaches nobody, yet its card says {lines:?}");
                return Ok(());
            }
        };
        prop_assert_eq!(lines.len(), 1, "one line on the closed card");
        let Seen::Element { kids: line, .. } = lines[0] else { unreachable!("an element") };
        let said = text(line, true);
        prop_assert!(text(drawn, true).contains(&said));

        let leads: Vec<String> = { let mut l = Vec::new(); shown(line, "reacheslead", &mut l); l.into_iter().map(|s| text(std::slice::from_ref(s), true)).collect() };
        let to_said = part(line, "reachesto").expect("a To part");
        names_or_counts(&to_said, to).map_err(TestCaseError::fail)?;
        match part(line, "reachescc") {
            Some(cc_said) => names_or_counts(&cc_said, cc).map_err(TestCaseError::fail)?,
            None => prop_assert!(cc.is_empty(), "copies to {cc:?} unsaid"),
        }

        let subject_said = part(line, "reachessubject").expect("a subject part");
        let lead = leads.first().cloned().unwrap_or_default();
        let replies = !forward && subject.trim_start().get(..3).is_some_and(|re| re.eq_ignore_ascii_case("re:"));
        let blank = |said: &str| said == suzatary_pages::card::NO_SUBJECT;
        match (forward, replies) {
            (true, _) => prop_assert_eq!(&lead, "Forward to"),
            (false, true) => prop_assert_eq!(&lead, "Reply to"),
            (false, false) => prop_assert_eq!(&lead, "To"),
        }
        if replies {
            let thread = if blank(&subject_said) { "" } else { subject_said.as_str() };
            let cut = subject.strip_suffix(thread).map(|cut| cut.replace(' ', "").to_lowercase());
            prop_assert!(cut.is_some_and(|cut| cut.split("re:").all(str::is_empty)),
                "{subject_said:?} is not the thread of {subject:?}");
        } else if subject.trim().is_empty() {
            prop_assert!(blank(&subject_said));
        } else {
            prop_assert_eq!(&subject_said, subject, "the subject as its text");
        }

        prop_assert_eq!(part(line, "reachesfrom"), Some(account.clone()));

        let known: Vec<&str> = std::iter::once(account.as_str()).chain(to.iter().map(String::as_str)).chain(cc.iter().map(String::as_str)).collect();
        let mut every = Vec::new();
        texts(drawn, &mut every);
        for word in every.iter().flat_map(|t| t.split(|c: char| c.is_whitespace() || ",;:?()".contains(c))).filter(|w| w.contains('@')) {
            prop_assert!(known.contains(&word.trim_end_matches('.')), "{word:?} is on the card but not in the proposal");
        }
    }
}
