//! A Microsoft account's calendars through the real routes and the real sync, against a fake
//! Microsoft (`common::fake_graph`): the calendar consent, the read, a lapse, an administrator's
//! refusal, and an approved event written.

use crate::common;
use crate::google::{event_proposed, setup};

use std::sync::Arc;

use common::fake_graph::{FakeGraph, Tenant};
use common::{EditsRecorder, Fakes, SecretRecorder, SettingsFixture};
use suzatary_google::calendar::effect::Calendar as _;
use suzatary_io_mail::oauth::TokenStore;
use suzatary_mail::mail::account::{Authority, MailAccount, MsImap, Sync};
use suzatary_mail::mail::oauth::Endpoints;

const ADA: &str = "ada@example.test";

fn account() -> MailAccount {
    MailAccount {
        email: suzatary_mail::mail::account_email::AccountEmail::new(ADA)
            .expect("a lower-case address"),
        subdir: "ada-at-example-test".to_owned(),
        sync: Sync::MsImap(MsImap {
            authority: Authority::Organizations,
            inbox_folders: suzatary_mail::mail::folders::Folders::inbox(),
            sent_folder: "Sent Items".to_owned(),
        }),
    }
}

/// A server declaring one Microsoft account, its sign-in and Graph both the fake's.
struct Serving {
    base: String,
    fake: FakeGraph,
    tokens: TokenStore,
}

fn serving(tag: &str) -> Serving {
    serving_for(tag, "microsoft")
}

/// The same, the model proposing the event at `provider`.
fn serving_for(tag: &str, provider: &str) -> Serving {
    let fake = FakeGraph::start(3);
    let tokens = TokenStore::new(common::scratch(&format!("{tag}-tokens")).join("oauth"))
        .at(Endpoints::at(&fake.base));
    let secrets = Arc::new(SecretRecorder::new().with_tokens(tokens.clone()));
    let login = common::login("ada@example.com");
    let whois = common::FakeWhois::new().naming(common::v4(), login.clone());
    let deployment = common::deployment([login.clone()], [(login, common::actor("ada"))]);
    let declared = suzatary_connect::connect::Declared {
        accounts: vec![account()],
        ..suzatary_connect::connect::Declared::default()
    };
    let settables = suzatary_connect::settings::Settables::derive(
        suzatary_io_model::model::TOKEN,
        [],
        None,
        &[],
    );
    let empty = SettingsFixture::empty();
    let accounts = suzatary_mail::mail::account::Roster::new(declared.accounts.clone())
        .expect("one account is a roster");
    let settings = SettingsFixture {
        live: suzatary_server::serve::Live {
            settables,
            accounts,
            ..empty.live.clone()
        },
        secrets: Arc::clone(&secrets) as _,
        edits: Arc::new(EditsRecorder::declaring(declared)) as _,
        ..empty
    };
    let model = common::FakeModel::saying_owned([
        format!(
            r#"{{"say": "Here is the event.", "calls": [{{"verb": "propose_act", "args": {{
              "act": "calendar_event", "account": "{ADA}", "provider": "{provider}", "title": "Seminar",
              "start": "2026-10-20T14:00:00Z", "end": "2026-10-20T15:00:00Z",
              "place": "Hall 2",
              "why": "the owner asked for the seminar in his university calendar",
              "sources": "briefing:2026-10-17"}}}}]}}"#
        ),
        r#"{"say": "Proposed. Tap it to add it."}"#.to_owned(),
    ]);
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(whois)
            .secrets(Arc::clone(&secrets) as _)
            .mailbox(common::FakeMailbox::new().with_accounts(vec![ADA.to_owned()]))
            .model(model),
    );
    let running = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .settings(settings)
        .real_acts_with_tokens(tag, common::again(&fakes.conversations), tokens.clone())
        .start();
    Serving {
        base: running.base,
        fake,
        tokens,
    }
}

fn tap_event(serving: &Serving, id: &str, body: &serde_json::Value) -> serde_json::Value {
    let mut tapped = common::act(
        &format!(
            "{}/api/secretary/conversations/{id}/proposals/0/tap",
            serving.base
        ),
        body,
    );
    assert_eq!(common::status(&tapped), 200);
    common::body_json(&mut tapped)["proposals"][0].clone()
}

/// Tap the event with no calendar consent held, then its fix, and wait for the consent's card to
/// end; the card as it ended.
fn consenting(serving: &Serving, id: &str) -> serde_json::Value {
    let lacked = tap_event(serving, id, &serde_json::json!({}));
    assert_eq!(lacked["state"], "failed", "{lacked}");
    assert_eq!(
        lacked["card"]["link"],
        serde_json::json!({"kind": "fix", "label": format!("Connect {ADA}"),
            "then": {"kind": "place", "href": "#/settings"}}),
        "the fix is the calendar consent, which stands on Settings"
    );
    tap_event(serving, id, &serde_json::json!({"fix": true}));
    let started = std::time::Instant::now();
    loop {
        let conversation = setup(&serving.base);
        let ended = conversation["proposals"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["act"]["kind"] == "auth_calendar" && row["state"] != "running");
        if let Some(ended) = ended {
            return ended.clone();
        }
        assert!(
            started.elapsed() < common::ROUND_WAIT,
            "the consent did not end: {conversation}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A calendar consent stores a grant in its own slot and widens neither mail consent; the tap
/// that follows it writes the event through Graph, once, as the card showed it.
#[test]
fn an_event_is_written_through_graph_once_the_account_consents_to_calendars() {
    let serving = serving("ms-calendar-event");
    let id = event_proposed(&serving.base);
    let ended = consenting(&serving, &id);
    assert_eq!(ended["state"], "acted", "{ended}");
    assert_eq!(
        serving.tokens.granted(&account()).expect("reads"),
        ["calendar"],
        "reading and sending are not granted by it"
    );
    assert!(
        serving.fake.inserted().is_empty(),
        "a consent writes no event"
    );

    let written = tap_event(&serving, &id, &serde_json::json!({}));
    assert_eq!(written["state"], "acted", "{written}");
    assert_eq!(
        serving.fake.inserted(),
        vec![serde_json::json!({
            "subject": "Seminar",
            "isAllDay": false,
            "location": {"displayName": "Hall 2"},
            "start": {"dateTime": "2026-10-20T14:00:00", "timeZone": "UTC"},
            "end": {"dateTime": "2026-10-20T15:00:00", "timeZone": "UTC"},
        })]
    );
}

/// An organisation that lets only an administrator approve the app ends the consent with
/// Microsoft's own words and no retry, and stores no grant.
#[test]
fn an_administrators_approval_is_shown_as_it_is_and_offers_no_retry() {
    let serving = serving("ms-calendar-admin");
    serving.fake.tenant(Tenant::AdminOnly);
    let id = event_proposed(&serving.base);
    let ended = consenting(&serving, &id);
    assert_eq!(ended["state"], "refused", "{ended}");
    let said = ended["outcome"].as_str().expect("its words");
    assert!(said.contains("administrator"), "{said}");
    assert!(said.contains("AADSTS90094"), "{said}");
    assert_eq!(
        ended["card"]["answers"],
        serde_json::json!([]),
        "a tap again meets the same refusal, so none is offered"
    );
    assert!(
        serving
            .tokens
            .granted(&account())
            .expect("reads")
            .is_empty()
    );
    assert_eq!(serving.fake.grants(), 0);
}

/// A grant is read into the same calendar store a Google account's calendars are, its health under
/// the same key, and a lapse asks for the calendar consent again by its own slot's name.
#[test]
fn calendars_are_read_into_the_one_store_and_a_lapse_asks_for_the_consent() {
    use suzatary_cli::commands::calendars::{Providers, read_calendars};
    let serving = serving("ms-calendar-read");
    let id = event_proposed(&serving.base);
    assert_eq!(consenting(&serving, &id)["state"], "acted");
    let grants = suzatary_io_mail::google::Grants::new(
        suzatary_io_mail::google::Google::new(suzatary_google::google::Endpoints::google()),
        Arc::new(suzatary_server::serve::Unresolved),
    );
    let scratch = suzatary_testkit::scratch("ms-calendar-store");
    let root = scratch.path();
    let at = prodrome_data::literal::Datetime::new(2026, 10, 17, 9, 0, 0, 0).expect("an instant");
    let read = || {
        read_calendars(
            &[account()],
            Providers {
                google: &grants,
                microsoft: &serving.tokens,
            },
            |account| root.join(&account.subdir),
            at,
        )
    };
    let key = suzatary_health::health::SourceKey::GoogleCalendar(
        suzatary_mail::mail::account_email::AccountEmail::folded(
            &suzatary_mail::mail::address::EmailAddress::new(ADA).expect("an address"),
        ),
    );

    let (lines, healths, stored) = read();
    assert!(stored, "{lines:?}");
    assert_eq!(
        lines,
        [format!(
            "microsoft {ADA}: 6 occurrences read, 6 learned, 0 known"
        )]
    );
    assert!(
        healths
            .health(&key)
            .is_some_and(|health| health.succeeded().is_some())
    );
    let days = suzatary_io_chains::calendar::Clocked::new(
        [root.join(account().subdir)],
        suzatary_base::zone::Zone::UTC,
    )
    .days(
        chrono::NaiveDate::from_ymd_opt(2026, 10, 19).expect("a day"),
        1,
    )
    .expect("the calendar reads");
    assert_eq!(days[0].entries.len(), 6, "every reader shows them");
    assert!(
        serving
            .fake
            .asked()
            .iter()
            .any(|asked| asked.ends_with("/calendarView")),
        "{:?}",
        serving.fake.asked()
    );

    serving.fake.ending_every_grant();
    let (lines, healths, stored) = read();
    assert!(!stored, "{lines:?}");
    assert!(lines[0].contains("--scope calendar"), "{lines:?}");
    assert_eq!(
        healths
            .health(&key)
            .and_then(suzatary_health::health::Health::remedial)
            .map(|(class, _)| class),
        Some(suzatary_health::health::Failure::NeedsLogin)
    );
}

/// An event names its provider, and no provider's refusal falls back to the other: an event for
/// the Google calendar of an address signed in for Microsoft calendars only asks for Google, and
/// nothing is written to Graph. This is also the test of an account that lacks a calendar grant:
/// `consenting` taps an event with no consent held and reads the Microsoft consent as its fix.
#[test]
fn an_event_at_google_is_never_written_through_graph() {
    let serving = serving_for("ms-calendar-google", "google");
    let id = event_proposed(&serving.base);
    let lacked = tap_event(&serving, &id, &serde_json::json!({}));
    assert_eq!(lacked["state"], "failed", "{lacked}");
    assert!(
        lacked["outcome"]
            .as_str()
            .is_some_and(|said| said.contains("Google")),
        "{lacked}"
    );
    assert!(serving.fake.inserted().is_empty());
}
