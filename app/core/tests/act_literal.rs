//! A proposal's act as the inbox stores it, over every arm of every sum it holds: it reads back
//! what it prints, through the walker that reads every other field of an `Asked`; and no
//! constructor of an act is an inbox event's or a term's.

use prodrome_core::event::EventVocabulary;
use prodrome_core::literal::codec::Literal;
use prodrome_core::literal::{Table, Vocabulary as _, parse_literal, print_literal};
use suzatary_core::secretary::act_vocabulary::SIGNATURES;
use suzatary_core::secretary::inbox_event::{InboxEvent, InboxVocabulary};
use suzatary_core::secretary::store::Proposal;

const DIFF: &str = "--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-old\n+new\n";
const SHA: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
const TEXT: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

/// Every arm of every sum an act holds, at least once, each written as its JSON.
fn every_act() -> Vec<String> {
    let source = |source: &str| {
        format!(
            r#"{{"source":{source},"name":"notice.pdf","media":"application/pdf","size":2048,"sha256":"{SHA}"}}"#
        )
    };
    let attachments = [
        source(r#"{"from":"mail","message":"m1@example.test","name":"notice.pdf"}"#),
        source(r#"{"from":"drive","path":"/my-files/Docs/notice.pdf"}"#),
        source(&format!(r#"{{"from":"upload","id":"{SHA}"}}"#)),
        source(&format!(r#"{{"from":"kept","id":"{SHA}"}}"#)),
        source(r#"{"from":"repo","repo":"ada/notes","path":"docs/plan.md","at":"HEAD"}"#),
    ]
    .join(",");
    let prices = [
        r#"{"how":"flat","percent":40}"#,
        r#"{"how":"deadline","end_date":"2026-10-20","start_percent":70,"end_percent":10,"lead_up_weeks":3,"start_date":null}"#,
        r#"{"how":"deadline","end_date":"2026-10-20","start_percent":70,"end_percent":10,"lead_up_weeks":null,"lead_up_days":10,"start_date":"2026-10-01"}"#,
        r#"{"how":"recur","days":60,"from_percent":100,"to_percent":30}"#,
        r#"{"how":"periodic","days":7,"from_percent":90,"to_percent":50}"#,
    ];
    let adds: Vec<String> = prices
        .iter()
        .map(|price| {
            format!(
                r#"{{"act":"add","todo":"lease","body":"Renew the *lease*","rationale":"it lapses","kind":"todo","category":"home","priority":{price},"detail":"","waiting_on":""}}"#
            )
        })
        .collect();
    let mut acts = vec![
        r#"{"act":"email","account":"her@example.test","to":["counsel@example.test"],"cc":[],"subject":"Thursday","body":"Thursday works."}"#.to_owned(),
        format!(
            r#"{{"act":"email","account":"her@example.test","to":["a@example.test","b@example.test"],"cc":["c@example.test"],"subject":"Files","body":"Attached.","attachments":[{attachments}]}}"#
        ),
        format!(
            r#"{{"act":"forward","account":"her@example.test","to":["counsel@example.test"],"cc":[],"subject":"Fwd: Notice","note":"","message":{{"file":{{"id":"m1@example.test","size":4096,"sha256":"{SHA}","held":[{{"kind":"ssn","hint":"3456"}},{{"kind":"code","hint":""}}],"parts":[{{"name":"image.png","media":"image/png","size":5,"sha256":"{SHA}"}},{{"name":"image.png","media":"image/png","size":6,"sha256":"{SHA}"}}]}},"style":"inline","from":"notices@example.test","subject":"Notice","date":"2026-10-20T09:00:00"}}}}"#
        ),
        format!(
            r#"{{"act":"forward","account":"her@example.test","to":["counsel@example.test"],"cc":["c@example.test"],"subject":"Fwd: Notice","note":"See below.","message":{{"file":{{"id":"m1@example.test","size":4096,"sha256":"{SHA}","held":[],"parts":[]}},"style":"attached","from":"notices@example.test","subject":"Notice","date":null}}}}"#
        ),
        format!(
            r#"{{"act":"changes","chain":"life","acts":[{},
              {{"act":"update","todo":"lease","body":null,"detail":null,"detail_append":null,"rationale":null,"priority":null,"waiting_on":null,"clear_waiting_on":false}},
              {{"act":"update","todo":"lease","body":"Renew it","detail":"","detail_append":"And the deposit.","rationale":"sooner","priority":{{"how":"flat","percent":30}},"waiting_on":"the landlord","clear_waiting_on":true,"category":"home","subtodos":[{{"body":"call","done":true}},{{"body":"sign","done":false}}],"defer_to":"2026-10-12"}},
              {{"act":"complete","todo":"lease","note":"signed"}},
              {{"act":"cancel","todo":"gym","note":""}},
              {{"act":"tend","todo":"plants","note":"watered"}},
              {{"act":"reprice","todo":"lease","priority":{{"how":"flat","percent":20}},"rationale":"it is tomorrow"}}]}}"#,
            adds.join(",")
        ),
        r#"{"act":"act","kind":"auth","account":"her@example.test"}"#.to_owned(),
        r#"{"act":"act","kind":"secret","name":"NTFY_TOPIC","why":"the secretary needs a channel"}"#.to_owned(),
        r#"{"act":"act","kind":"web","request":{"kind":"search","query":"train times"}}"#.to_owned(),
        r#"{"act":"act","kind":"web","request":{"kind":"fetch","url":"https://docs.rs/serde"},"rule":"docs.rs"}"#.to_owned(),
        format!(
            r#"{{"act":"act","kind":"patch","repo":"ada/notes","visibility":"private","base":"{}","patch":{},"message":"Fix the readme"}}"#,
            "0".repeat(40),
            serde_json::to_string(DIFF).expect("a string")
        ),
        r#"{"act":"act","kind":"drive_read","path":"/my-files/Taxes/return.pdf","size":4096}"#.to_owned(),
        r#"{"act":"act","kind":"drive_read","path":"/my-files/Taxes/return.pdf"}"#.to_owned(),
        r#"{"act":"act","kind":"coding_session","task":"Make the inbox's act a literal."}"#.to_owned(),
        r#"{"act":"act","kind":"connect","connection":{"template":"ics","params":["WORK"]}}"#.to_owned(),
        r#"{"act":"act","kind":"connect","connection":{"template":"imap-device-code","params":["ada@example.test","consumers"]}}"#.to_owned(),
        r#"{"act":"act","kind":"connect","connection":{"template":"git-remote","params":[]}}"#.to_owned(),
        r#"{"act":"act","kind":"disconnect","connection":{"template":"lieer","params":["ada@example.test"]}}"#.to_owned(),
        r#"{"act":"act","kind":"grant","plugin":"ics","origin":"https://cal.example.test"}"#.to_owned(),
        r#"{"act":"decide","id":"literalacts","title":"Acts are literals","body":"The inbox stores *literals*.","rationale":"one walker","supersedes":["jsonacts"]}"#.to_owned(),
    ];
    for kind in ["owner", "person", "preference", "standing"] {
        acts.push(format!(
            r#"{{"act":"remember","text":"{TEXT}","kind":"{kind}"}}"#
        ));
    }
    acts
}

fn read(json: &str) -> Proposal {
    serde_json::from_str(json).unwrap_or_else(|failed| panic!("{json}: {failed}"))
}

#[test]
fn every_act_reads_back_what_it_prints_through_the_inbox_walker() {
    let words = EventVocabulary::<InboxEvent>::new(&InboxVocabulary);
    for json in every_act() {
        let proposal = read(&json);
        let value = proposal.to_value();
        assert_eq!(
            Proposal::from_value(&value).as_ref(),
            Ok(&proposal),
            "{json}"
        );
        let printed = print_literal(&value);
        let parsed = parse_literal(&printed, &words).unwrap_or_else(|failed| {
            panic!("{printed}: {failed}");
        });
        assert_eq!(parsed, value, "the print is canonical: {printed}");
    }
}

#[test]
fn no_constructor_of_an_act_is_an_events_or_a_terms() {
    let terms = prodrome_core::term::schema::signatures();
    let events = Table(suzatary_core::secretary::inbox_event::SIGNATURES);
    for (name, _) in SIGNATURES {
        assert!(terms.signature(name).is_none(), "{name} is a term's");
        assert!(events.find(name).is_none(), "{name} is an inbox event's");
    }
    let mut names: Vec<&str> = SIGNATURES.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), SIGNATURES.len(), "no constructor twice");
}
