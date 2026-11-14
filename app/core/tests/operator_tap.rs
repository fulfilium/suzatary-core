//! The operator's tap at the box's terminal performs every card but the ones that send mail or
//! grant a way to, over one proposal of every kind; the device's tap performs them all.

use suzatary_core::calendar::When;
use suzatary_core::google::{NewEvent, Scope};
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::secretary::acts::Act;
use suzatary_core::secretary::proposal::Proposal;
use suzatary_core::secretary::sending::performs;
use suzatary_signatures::signatures::ActHeaders;
use suzatary_signatures::signatures::Seat;
use suzatary_signatures::signatures::Tap;

const SHA: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

fn read(json: &str) -> Proposal {
    serde_json::from_str(json).unwrap_or_else(|failed| panic!("{json}: {failed}"))
}

/// One proposal of every kind, each act's arm among them.
fn every_kind() -> Vec<Proposal> {
    let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 11, d).expect("a day");
    let mut every: Vec<Proposal> = [
        r#"{"act":"email","account":"her@example.test","to":["counsel@example.test"],"cc":[],"subject":"Thursday","body":"Thursday works."}"#.to_owned(),
        format!(
            r#"{{"act":"forward","account":"her@example.test","to":["counsel@example.test"],"cc":[],"subject":"Fwd: Notice","note":"","message":{{"file":{{"id":"m1@example.test","size":4096,"sha256":"{SHA}","held":[],"parts":[]}},"style":"attached","from":"notices@example.test","subject":"Notice","date":null}}}}"#
        ),
        r#"{"act":"changes","chain":"life","acts":[{"act":"complete","todo":"lease","note":"signed"}]}"#.to_owned(),
        format!(r#"{{"act":"remember","text":"{SHA}","kind":"person","replaces":"nothing"}}"#),
        r#"{"act":"decide","id":"operatortaps","title":"The operator taps","body":"Not sends.","rationale":"testing","supersedes":[]}"#.to_owned(),
        r#"{"act":"act","kind":"auth","account":"her@example.test"}"#.to_owned(),
        r#"{"act":"act","kind":"auth_send","account":"her@example.test"}"#.to_owned(),
        r#"{"act":"act","kind":"secret","name":"NTFY_TOPIC","why":"a channel"}"#.to_owned(),
        r#"{"act":"act","kind":"web","request":{"kind":"search","query":"train times"}}"#.to_owned(),
        format!(
            r#"{{"act":"act","kind":"patch","repo":"ada/notes","visibility":"private","base":"{}","patch":"--- a/R\n+++ b/R\n@@ -1 +1 @@\n-a\n+b\n","message":"Fix"}}"#,
            "0".repeat(40)
        ),
        r#"{"act":"act","kind":"drive_read","path":"/my-files/Taxes/return.pdf"}"#.to_owned(),
        r#"{"act":"act","kind":"coding_session","task":"Run the gates."}"#.to_owned(),
        format!(r#"{{"act":"act","kind":"device","actor":"ada","key":"{}"}}"#, "1".repeat(64)),
        r#"{"act":"act","kind":"sign_in"}"#.to_owned(),
        r#"{"act":"act","kind":"sign_in_code","url":"https://claude.ai/oauth/authorize"}"#.to_owned(),
        r#"{"act":"act","kind":"unlock_store"}"#.to_owned(),
        r#"{"act":"act","kind":"connect","connection":{"template":"ics","params":["WORK"]}}"#.to_owned(),
        r#"{"act":"act","kind":"disconnect","connection":{"template":"lieer","params":["ada@example.test"]}}"#.to_owned(),
        r#"{"act":"act","kind":"grant","plugin":"ics","origin":"https://cal.example.test"}"#.to_owned(),
        r#"{"act":"act","kind":"google_client","redirect":"https://box.example.test/api/oauth/google/callback"}"#.to_owned(),
        r#"{"act":"act","kind":"google_sign_in","account":"her@example.test","scopes":["mail"]}"#.to_owned(),
        r#"{"act":"act","kind":"erase","fact":3,"keys":[]}"#.to_owned(),
        r#"{"act":"act","kind":"afresh","conversation":"default","turn":0}"#.to_owned(),
        format!(r#"{{"act":"act","kind":"retract","node":"{SHA}"}}"#),
        format!(r#"{{"act":"act","kind":"noise","sender":"alerts@example.test","message":"{SHA}"}}"#),
    ]
    .iter()
    .map(|json| read(json))
    .collect();
    every.push(Proposal::Act(Act::CalendarEvent {
        account: EmailAddress::new("her@example.test").expect("an address"),
        event: NewEvent::new("Dentist", When::all_day(day(3), day(4)).expect("a day"), "")
            .expect("an event"),
    }));
    every
}

/// The kinds a tap sends mail on, named by hand rather than read from the code that decides, and
/// complete because the match names every arm.
fn sends(proposal: &Proposal) -> bool {
    match proposal {
        Proposal::Email(_) | Proposal::Forward(_) | Proposal::Act(Act::AuthSend { .. }) => true,
        // Gmail's grant to read is `gmail.modify`, which sends too.
        Proposal::Act(Act::GoogleSignIn { scopes, .. }) => scopes.contains(Scope::Mail),
        Proposal::Changes(_)
        | Proposal::Remember { .. }
        | Proposal::Decide(_)
        | Proposal::Act(
            Act::Auth { .. }
            | Act::Secret { .. }
            | Act::Web { .. }
            | Act::Patch { .. }
            | Act::DriveRead { .. }
            | Act::CodingSession { .. }
            | Act::Device(_)
            | Act::SignIn { .. }
            | Act::SignInCode { .. }
            | Act::UnlockStore { .. }
            | Act::Connect { .. }
            | Act::Disconnect { .. }
            | Act::Grant { .. }
            | Act::Retract { .. }
            | Act::GoogleClient { .. }
            | Act::CalendarEvent { .. }
            | Act::Erase { .. }
            | Act::Afresh { .. }
            | Act::Noise { .. }
            | Act::Restore { .. },
        ) => false,
    }
}

fn tap(seat: Seat) -> Tap {
    Tap::of(
        &suzatary_core::stores::scene("ada"),
        &ActHeaders {
            act: Some("1"),
            client: None,
            content_type: Some("application/json"),
            site: None,
        },
        seat,
    )
    .expect("a tap")
}

#[test]
fn the_operators_tap_performs_every_card_but_the_ones_that_send() {
    let every = every_kind();
    let kinds: std::collections::BTreeSet<String> = every
        .iter()
        .map(|proposal| match proposal {
            Proposal::Act(act) => act.kind().to_owned(),
            other => format!("{other:?}")
                .split(['(', ' '])
                .next()
                .unwrap_or_default()
                .to_owned(),
        })
        .collect();
    assert_eq!(
        kinds.len(),
        every.len(),
        "one proposal of each kind: {kinds:?}"
    );
    let (device, operator) = (tap(Seat::Device), tap(Seat::Operator));
    for proposal in &every {
        assert!(performs(&device, proposal).is_ok(), "{proposal:?}");
        match performs(&operator, proposal) {
            Ok(()) => assert!(!sends(proposal), "the operator sends: {proposal:?}"),
            Err(refused) => {
                assert!(
                    sends(proposal),
                    "refused a card that sends nothing: {proposal:?}"
                );
                assert_eq!(refused.status().code(), 403);
                assert!(refused.message().contains("sending stays the owner's tap"));
            }
        }
    }
    assert_eq!(every.iter().filter(|p| sends(p)).count(), 4);
}
