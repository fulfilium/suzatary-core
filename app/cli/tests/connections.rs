//! A connection's cards over a real server and a scratch instance file.

use crate::common;

use std::sync::Arc;

use common::{DeclaredStore, Fakes, SecretRecorder, SettingsFixture};
use suzatary_base::conversation_id::SETUP;
use suzatary_connect::connect::Connection;
use suzatary_inbox::secretary::acts::Act;

/// The scratch instance: every required table, one account and nothing connected beside it.
const INSTANCE: &str = r#"# Ada's box, a scratch copy.
repo = "/tmp/suzatary-connections-tap"
data = "/tmp/suzatary-connections-tap"

[signatures]
root = "836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818"

[[chains]]
name = "life"
events = "events"
untrusted = []
actors = ["ada", "triage"]
owner = "ada"

[web]
viewers = ["ada@example.com"]
app_url = "https://example.test"

[web.act_as]
"ada@example.com" = "ada"

[web.serving]
host = "127.0.0.1"
port = 8787
tls_dir = "/var/lib/suzatary/tls"
dist = "web/dist"
briefings = "briefings"

[categories]
work = 0

[mail]
root = "Mail"
state = "state.json"
notmuch = "notmuch"
mbsyncrc = "mbsyncrc"
last_sync = "/tmp/suzatary-connections-tap-last-sync.json"
heartbeat = ".local/state/triage-heartbeat"

[[mail.accounts]]
email = "ada@example.test"
subdir = "ada-at-example-test"
sync = "gmail"
"#;

/// A link served from an origin the calendar's template does not allow, with a secret path.
const LINK: &str = "https://cal.example.test/private-7f3a9c/basic.ics";

struct Box {
    base: String,
    path: std::path::PathBuf,
    fakes: Arc<Fakes>,
    secrets: Arc<SecretRecorder>,
}

/// A server writing as `ada` over a scratch instance file, with `act` filed as the box's card.
fn serving(tag: &str, act: Act) -> Box {
    serving_with(tag, act, |server, _| server)
}

fn serving_with(
    tag: &str,
    act: Act,
    with: impl FnOnce(common::Server, &Fakes) -> common::Server,
) -> Box {
    serving_answering(tag, Some(act), None, with)
}

fn serving_answering(
    tag: &str,
    act: Option<Act>,
    model: Option<common::FakeModel>,
    with: impl FnOnce(common::Server, &Fakes) -> common::Server,
) -> Box {
    let path = scratch(tag);
    std::fs::write(&path, INSTANCE).expect("a scratch instance");
    let ada = common::login("ada@example.com");
    let whois = common::FakeWhois::new().naming(common::v4(), ada.clone());
    let fakes = Fakes::new(tag).whois(whois).configuration(Arc::new(
        suzatary_cli::instance::InstanceEdits::new(path.clone()),
    ));
    let fakes = Arc::new(match model {
        Some(model) => fakes.model(model),
        None => fakes,
    });
    if let Some(act) = act {
        asked(&fakes, act);
    }
    let secrets = Arc::new(SecretRecorder::new());
    let deployment = common::deployment([ada.clone()], [(ada, common::actor("ada"))]);
    // The GitHub tokens, which every deployment declares, are held, so each scene's cards are its
    // own connection's; a scene about them swaps this back.
    let base = with(
        common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
            .effects(|effects| suzatary_server::server::Server {
                secretary: suzatary_secretary::secretary::Secretary {
                    secrets: &GitHubHeld,
                    ..effects.secretary
                },
                ..effects
            })
            .settings(SettingsFixture {
                edits: Arc::new(suzatary_cli::instance::InstanceEdits::new(path.clone())) as _,
                secrets: Arc::clone(&secrets) as _,
                ..SettingsFixture::empty()
            }),
        &fakes,
    )
    .start()
    .base;
    Box {
        base,
        path,
        fakes,
        secrets,
    }
}

/// The scratch instance file the scene `tag` is served over, the same for each call in a test.
fn scratch(tag: &str) -> std::path::PathBuf {
    suzatary_testkit::this_test().join(format!("connections-{tag}.toml"))
}

/// A store holding the GitHub tokens and nothing else.
struct GitHubHeld;

impl suzatary_connect::secrets::Secrets for GitHubHeld {
    fn secret_state(
        &self,
        settable: &suzatary_connect::settings::Settable,
    ) -> suzatary_connect::secrets::SecretState {
        if [
            suzatary_connect::settings::DATA_PUSH_TOKEN,
            suzatary_services::repos::TOKEN,
        ]
        .contains(&settable.name())
        {
            suzatary_connect::secrets::SecretState::Set
        } else {
            suzatary_connect::secrets::SecretState::Missing
        }
    }

    fn mail_grants(
        &self,
        _: &suzatary_mail::mail::account::MailAccount,
    ) -> suzatary_connect::secrets::MailGrants {
        suzatary_connect::secrets::MailGrants::default()
    }

    fn google_grant(
        &self,
        _: &suzatary_mail::mail::address::EmailAddress,
    ) -> suzatary_google::google::Held {
        suzatary_google::google::Held::NotSignedIn
    }
}

fn tap(
    base: &str,
    conversation: &str,
    index: usize,
    body: &serde_json::Value,
) -> (u16, serde_json::Value) {
    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{conversation}/proposals/{index}/tap"),
        body,
    );
    (common::status(&tapped), common::body_json(&mut tapped))
}

fn calendar() -> Connection {
    Connection::parse("ics", &["Work".to_owned()]).expect("a calendar")
}

fn file(served: &Box) -> String {
    std::fs::read_to_string(&served.path).expect("the file")
}

const CONNECT: &str = "connect";

/// `act` filed as the secretary's in the `connect` conversation, as the setup agent files it.
fn asked(fakes: &Fakes, act: Act) {
    common::asked(fakes, CONNECT, act);
}

/// A link from an origin the plugin may not reach is stored nowhere and its card says only the
/// origin, and Disconnect leaves the plugin, granted an origin its template does not name.
#[test]
fn a_calendar_is_connected_granted_and_removed_by_its_cards() {
    let served = serving(
        "calendar",
        Act::Connect {
            connection: calendar(),
        },
    );
    let (base, setup) = (&served.base, SETUP.as_str());

    let (status, after) = tap(base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");
    assert!(file(&served).starts_with(INSTANCE), "{}", file(&served));
    assert!(file(&served).contains("credentials = [\"PLUGIN_ICS_WORK\"]"));

    let asked = common::reading(base, setup, &[]);
    assert_eq!(
        asked["proposals"][0]["act"]["kind"], "secret",
        "the box asks for the link: {asked}"
    );
    assert_eq!(
        asked["proposals"][0]["act"]["label"],
        "your Work calendar's secret link"
    );

    let (status, after) = tap(base, setup, 0, &serde_json::json!({ "value": LINK }));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "failed", "{after}");
    assert!(served.secrets.calls().is_empty(), "nothing was stored");
    let grant = &after["proposals"][1];
    assert_eq!(grant["act"]["kind"], "grant", "{after}");
    assert_eq!(grant["act"]["origin"], "https://cal.example.test");
    assert!(
        !after.to_string().contains("private-7f3a9c"),
        "no answer carries the link: {after}"
    );

    let (status, after) = tap(base, setup, 1, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][1]["state"], "acted", "{after}");
    assert!(file(&served).contains("\"https://cal.example.test\""));

    let (status, after) = tap(base, setup, 0, &serde_json::json!({ "value": LINK }));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");
    assert_eq!(
        served.secrets.calls(),
        vec![("PLUGIN_ICS_WORK".to_owned(), LINK.to_owned())]
    );

    asked_of(
        &served,
        Act::Disconnect {
            connection: calendar(),
        },
    );
    let (status, after) = tap(base, CONNECT, 1, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][1]["state"], "acted", "{after}");
    assert!(
        !file(&served).contains("PLUGIN_ICS_WORK"),
        "{}",
        file(&served)
    );
    assert!(file(&served).contains("\"https://cal.example.test\""));
    assert_eq!(
        served.secrets.cleared(),
        ["PLUGIN_ICS_WORK"],
        "its link goes too"
    );

    // Connected again, its link is asked for anew, since the one stored went with it.
    asked_of(
        &served,
        Act::Connect {
            connection: calendar(),
        },
    );
    let (status, after) = tap(base, CONNECT, 2, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    let asked = common::reading(base, setup, &[]);
    let again = asked["proposals"]
        .as_array()
        .and_then(|proposals| proposals.last())
        .expect("a card");
    assert_eq!(again["act"]["kind"], "secret", "{asked}");
    assert_eq!(again["state"], "standing", "{asked}");
}

/// Over a store that refuses, as `secretspec` does, an undeclared name, the link is declared
/// before its card is filed and cleared while still declared, and only then is the name dropped.
#[test]
fn a_calendar_s_link_is_declared_before_its_card_and_cleared_before_it_is_dropped() {
    let store = Arc::new(DeclaredStore::new());
    let declared = Arc::clone(&store);
    let served = serving_with(
        "declared",
        Act::Connect {
            connection: calendar(),
        },
        move |server, _| {
            server.settings(SettingsFixture {
                edits: Arc::new(suzatary_cli::instance::InstanceEdits::new(scratch(
                    "declared",
                ))) as _,
                secrets: Arc::clone(&declared) as _,
                declares: Arc::new(declared.declarations.clone()) as _,
                ..SettingsFixture::empty()
            })
        },
    );
    let (base, setup) = (&served.base, SETUP.as_str());
    assert!(!store.declares("PLUGIN_ICS_WORK"));

    let (status, after) = tap(base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert!(
        store.declares("PLUGIN_ICS_WORK"),
        "declared as it is connected"
    );
    let asked = common::reading(base, setup, &[]);
    assert_eq!(asked["proposals"][0]["act"]["kind"], "secret", "{asked}");

    let link = "https://outlook.office365.com/owa/calendar/a1b2/c3d4/calendar.ics";
    let (status, after) = tap(base, setup, 0, &serde_json::json!({ "value": link }));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");
    assert_eq!(
        store.recorded.calls(),
        vec![("PLUGIN_ICS_WORK".to_owned(), link.to_owned())]
    );

    asked_of(
        &served,
        Act::Disconnect {
            connection: calendar(),
        },
    );
    let (status, after) = tap(base, CONNECT, 1, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][1]["state"], "acted", "{after}");
    assert_eq!(
        store.recorded.cleared(),
        ["PLUGIN_ICS_WORK"],
        "cleared while still declared: {after}"
    );
    assert!(!store.declares("PLUGIN_ICS_WORK"), "dropped once cleared");
}

/// Storing a link syncs its plugin once at the tap, not at the next timer, and a refused link
/// syncs nothing.
#[test]
fn a_stored_link_is_checked_at_its_tap() {
    let checks = Arc::new(common::CheckRecorder::default());
    let recorder = Arc::clone(&checks);
    let served = serving_with(
        "checked",
        Act::Connect {
            connection: calendar(),
        },
        move |server, _| server.checks(recorder),
    );
    let (base, setup) = (&served.base, SETUP.as_str());
    let (status, after) = tap(base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");

    let (status, after) = tap(base, setup, 0, &serde_json::json!({ "value": LINK }));
    assert_eq!(status, 200, "{after}");
    assert!(
        checks.checked().is_empty(),
        "nothing was stored, so nothing was synced"
    );

    let (status, after) = tap(base, setup, 1, &serde_json::json!({}));
    assert_eq!(after["proposals"][1]["state"], "acted", "{status} {after}");
    assert!(
        checks.checked().is_empty(),
        "allowing an origin stores no link"
    );

    let (status, after) = tap(base, setup, 0, &serde_json::json!({ "value": LINK }));
    assert_eq!(after["proposals"][0]["state"], "acted", "{status} {after}");
    assert_eq!(checks.checked(), ["ics"], "synced once, at the tap");
}

/// Reading the same unmoved file again files nothing more.
#[test]
fn a_calendar_added_by_hand_is_seen_without_a_restart() {
    let served = serving_with(
        "byhand",
        Act::Connect {
            connection: calendar(),
        },
        |server, _| server.watching(scratch("byhand")),
    );
    let (base, setup) = (&served.base, SETUP.as_str());
    let line = || {
        let mut page = common::get(&format!("{base}/api/overview"));
        common::body_json(&mut page)["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|group| group["lines"].as_array().cloned().unwrap_or_default())
            .find(|line| line["key"] == "credential:ics/home")
    };
    let asked_for = || {
        let (_, conversations) = common::replicas(base);
        if !conversations
            .iter()
            .any(|stored| stored.id.as_str() == setup)
        {
            return 0;
        }
        common::reading(base, setup, &[])["proposals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|proposal| {
                proposal["act"]["label"] == "your Home calendar's secret link"
                    && proposal["state"] == "standing"
            })
            .count()
    };
    assert!(line().is_none(), "nothing is connected yet");
    let edited = format!(
        "{}\n[[plugins]]\nname = \"ics\"\ncomponent = \"ics.wasm\"\nschema = \"ics.schema\"\n\
         origins = [\"https://calendar.example.test\"]\ncredentials = [\"PLUGIN_ICS_HOME\"]\n",
        file(&served)
    );
    std::fs::write(&served.path, edited).expect("a hand edit");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while asked_for() == 0 && std::time::Instant::now() < deadline {
        std::thread::park_timeout(std::time::Duration::from_millis(100));
    }
    assert!(line().is_some(), "the line is drawn with no restart");
    assert_eq!(asked_for(), 1, "the box asks for the link once");
    // The watcher reads the file every second; two more reads of the unmoved file file nothing.
    std::thread::park_timeout(std::time::Duration::from_millis(2500));
    assert_eq!(asked_for(), 1, "an unmoved file asks nothing more");
}

fn asked_of(served: &Box, act: Act) {
    asked(&served.fakes, act);
}

/// The secretary proposes against the names the instance declares now, with no restart.
#[test]
fn a_connect_tap_lets_the_secretary_ask_for_the_new_calendar_link() {
    let propose = r#"{"say": "I will ask for the link.",
        "calls": [{"verb": "propose_act", "args": {
          "act": "secret",
          "name": "PLUGIN_ICS_WORK",
          "why": "The Work calendar needs its link.",
          "sources": "briefing:2026-09-02"
        }}]}"#;
    let served = serving_answering(
        "propose-new-link",
        Some(Act::Connect {
            connection: calendar(),
        }),
        Some(common::FakeModel::saying([
            propose,
            r#"{"say": "That name is not declared yet."}"#,
            propose,
            r#"{"say": "Proposed."}"#,
        ])),
        |server, _| server,
    );
    let base = &served.base;
    let id = common::opened(base, "The calendar");

    let before = common::said(base, &id, "ask for the Work calendar's link");
    assert!(
        before["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "nothing is proposed for a name not yet declared: {before}"
    );

    let (status, after) = tap(base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");

    let after = common::said(base, &id, "ask again");
    let proposals = after["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 1, "{after}");
    assert_eq!(proposals[0]["act"]["kind"], "secret", "{after}");
    assert_eq!(
        proposals[0]["act"]["label"],
        "your Work calendar's secret link"
    );
}

/// Its source is the round asked in, which the model does not name, and a second in the same
/// conversation is filed too, since neither answer reads anything from outside the round.
#[test]
fn a_connect_in_the_owners_words_is_filed_on_the_first_try() {
    let connect = |label: &str| {
        format!(
            r#"{{"say": "I will ask to add it.", "calls": [{{"verb": "propose_act", "args": {{
              "act": "connect", "template": "ics", "params": ["{label}"],
              "why": "Ada asked to add the {label} calendar.", "sources": ""}}}}]}}"#
        )
    };
    let served = serving_answering(
        "connect-first-try",
        None,
        Some(common::FakeModel::saying_owned([
            connect("Work"),
            r#"{"say": "Proposed."}"#.to_owned(),
            r#"{"say": "Asked.", "calls": [{"verb": "propose_act", "args": {
              "act": "connect", "template": "no-such-template", "params": [],
              "why": "a mistake", "sources": ""}}]}"#
                .to_owned(),
            r#"{"say": "That was refused."}"#.to_owned(),
            connect("Family"),
            r#"{"say": "Proposed."}"#.to_owned(),
        ])),
        |server, _| server,
    );
    let base = &served.base;
    let id = common::opened(base, "Calendars");
    let cards = |read: &serde_json::Value| read["proposals"].as_array().map_or(0, Vec::len);

    let first = common::said(base, &id, "add my Work calendar");
    assert_eq!(cards(&first), 1, "filed on the first try: {first}");
    assert_eq!(
        first["proposals"][0]["sources"],
        serde_json::json!([format!("round:{id}/0")]),
        "its source is the round the owner asked in"
    );
    common::said(base, &id, "add something");
    let second = common::said(base, &id, "add my Family calendar too");
    assert_eq!(
        cards(&second),
        2,
        "a second connect in the same conversation: {second}"
    );
}

/// A connection added and removed leaves the file byte for byte but for the calendar's plugin.
#[test]
fn connecting_twice_writes_once_and_removing_takes_out_what_it_wrote() {
    let served = serving(
        "twice",
        Act::Connect {
            connection: calendar(),
        },
    );
    assert_eq!(tap(&served.base, CONNECT, 0, &serde_json::json!({})).0, 200);
    let once = file(&served);
    asked_of(
        &served,
        Act::Connect {
            connection: calendar(),
        },
    );
    let (_, after) = tap(&served.base, CONNECT, 1, &serde_json::json!({}));
    assert_eq!(after["proposals"][1]["state"], "failed", "{after}");
    assert!(after.to_string().contains("already connected"), "{after}");
    assert_eq!(file(&served), once);

    asked_of(
        &served,
        Act::Disconnect {
            connection: calendar(),
        },
    );
    let (_, after) = tap(&served.base, CONNECT, 2, &serde_json::json!({}));
    assert_eq!(after["proposals"][2]["state"], "acted", "{after}");
    let left = file(&served);
    assert!(left.starts_with(INSTANCE), "{left}");
    assert!(left.contains("name = \"ics\""), "{left}");
    assert!(!left.contains("credentials"), "{left}");
}

/// A calendar's removal has no wider yes, and a tap claiming one is refused before anything runs.
#[test]
fn only_a_mail_account_s_removal_may_delete_its_mail() {
    let account = Connection::parse("lieer", &["ada@example.test".to_owned()]).expect("an account");
    let served = serving(
        "mail",
        Act::Disconnect {
            connection: account,
        },
    );
    let payload = common::reading(&served.base, CONNECT, &[]);
    let answers = &payload["proposals"][0]["card"]["answers"];
    assert_eq!(answers[1]["asks"], "always", "{payload}");
    let (status, after) = tap(
        &served.base,
        CONNECT,
        0,
        &serde_json::json!({ "always": true }),
    );
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");
    assert!(file(&served).contains("accounts = []"), "{}", file(&served));
    // The test's server keeps no mail, and the outcome says so rather than claiming a delete.
    assert!(
        after.to_string().contains("this server keeps no mail"),
        "{after}"
    );

    let calendar = serving(
        "mail-calendar",
        Act::Disconnect {
            connection: calendar(),
        },
    );
    let (status, _) = tap(
        &calendar.base,
        CONNECT,
        0,
        &serde_json::json!({ "always": true }),
    );
    assert_eq!(status, 400);
}

/// A tap carries at most one answer beside its note.
#[test]
fn a_tap_naming_two_answers_is_refused_and_the_card_stands() {
    let account = Connection::parse("lieer", &["ada@example.test".to_owned()]).expect("an account");
    let served = serving(
        "two-answers",
        Act::Disconnect {
            connection: account,
        },
    );
    let before = file(&served);
    let endorsement = serde_json::json!({
        "key": suzatary_connect::cast::keys::root().as_str(),
        "signature": "00".repeat(64),
    });
    for body in [
        serde_json::json!({ "always": true, "value": "a value" }),
        serde_json::json!({ "value": "a value", "endorsement": endorsement }),
        serde_json::json!({ "always": true, "endorsement": endorsement }),
    ] {
        let (status, refused) = tap(&served.base, CONNECT, 0, &body);
        assert_eq!(status, 400, "{refused}");
        assert_eq!(file(&served), before);
    }
    let (status, after) = tap(
        &served.base,
        CONNECT,
        0,
        &serde_json::json!({ "always": true }),
    );
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");
}

/// Each paste is the owner asking, so the box's card has an episode, never a final no.
#[test]
fn a_dismissed_grant_is_asked_again_when_the_link_is_pasted_again() {
    let served = serving(
        "regrant",
        Act::Connect {
            connection: calendar(),
        },
    );
    let (base, setup) = (&served.base, SETUP.as_str());
    let (status, after) = tap(base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    let (status, after) = tap(base, setup, 0, &serde_json::json!({ "value": LINK }));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][1]["act"]["kind"], "grant", "{after}");

    let mut dismissed = common::act(
        &format!("{base}/api/secretary/conversations/{setup}/proposals/1/dismiss"),
        &serde_json::json!({ "reason": "not that calendar" }),
    );
    assert_eq!(common::status(&dismissed), 200);
    let after = common::body_json(&mut dismissed);
    assert_eq!(after["proposals"][1]["state"], "dismissed", "{after}");

    let (status, after) = tap(base, setup, 0, &serde_json::json!({ "value": LINK }));
    assert_eq!(status, 200, "{after}");
    let again = &after["proposals"][2];
    assert_eq!(again["act"]["kind"], "grant", "{after}");
    assert_eq!(again["act"]["origin"], "https://cal.example.test");
    assert_eq!(again["state"], "standing", "{after}");
}

/// A dismissed sign-in card stays dismissed while the account stays; a reconnect is a new episode.
#[test]
fn an_account_connected_again_is_asked_for_its_sign_in_again() {
    let account = || {
        Connection::parse(
            "imap-device-code",
            &["bo@example.test".to_owned(), "consumers".to_owned()],
        )
        .expect("an account")
    };
    let served = serving(
        "resign",
        Act::Connect {
            connection: account(),
        },
    );
    let (base, setup) = (&served.base, SETUP.as_str());
    let (status, after) = tap(base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    let mut dismissed = common::act(
        &format!("{base}/api/secretary/conversations/{setup}/proposals/0/dismiss"),
        &serde_json::json!({ "reason": "later" }),
    );
    assert_eq!(common::status(&dismissed), 200);
    let after = common::body_json(&mut dismissed);
    assert_eq!(after["proposals"][0]["state"], "dismissed", "{after}");

    asked_of(
        &served,
        Act::Disconnect {
            connection: account(),
        },
    );
    let (status, after) = tap(base, CONNECT, 1, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    asked_of(
        &served,
        Act::Connect {
            connection: account(),
        },
    );
    let (status, after) = tap(base, CONNECT, 2, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");

    let asked = common::reading(base, setup, &[]);
    let proposals = asked["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 2, "{asked}");
    assert_eq!(proposals[1]["act"]["kind"], "auth", "{asked}");
    assert_eq!(proposals[1]["state"], "standing", "{asked}");
}

/// The plugin's fetch never sends a secret link over plain http, so allowing its origin would
/// allow nothing.
#[test]
fn a_link_in_the_clear_is_refused_before_any_grant() {
    let served = serving(
        "clear",
        Act::Connect {
            connection: calendar(),
        },
    );
    let (base, setup) = (&served.base, SETUP.as_str());
    let (status, after) = tap(base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    let (status, after) = tap(
        base,
        setup,
        0,
        &serde_json::json!({ "value": "http://cal.example.test/private-7f3a9c/basic.ics" }),
    );
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "failed", "{after}");
    assert_eq!(
        after["proposals"].as_array().map(Vec::len),
        Some(1),
        "no grant is asked for: {after}"
    );
    assert!(served.secrets.calls().is_empty(), "nothing was stored");
}

/// A Google calendar's link is stored nowhere, since the box reads it through the account's
/// sign-in with Google, and never asks for a grant of Google's origin.
#[test]
fn a_google_calendar_s_link_asks_for_its_google_account() {
    let served = serving(
        "google",
        Act::Connect {
            connection: calendar(),
        },
    );
    let (base, setup) = (&served.base, SETUP.as_str());
    let (status, after) = tap(base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    let google = |id: &str| {
        format!("https://calendar.google.com/calendar/ical/{id}/private-7f3a9c/basic.ics")
    };
    let (status, after) = tap(
        base,
        setup,
        0,
        &serde_json::json!({ "value": google("c_1f2e%40group.calendar.google.com") }),
    );
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "failed", "{after}");
    assert_eq!(
        after["proposals"].as_array().map(Vec::len),
        Some(1),
        "{after}"
    );
    for (account, card) in [("ada", 1), ("bea", 2)] {
        let (status, after) = tap(
            base,
            setup,
            0,
            &serde_json::json!({ "value": google(&format!("{account}%40example.test")) }),
        );
        assert_eq!(status, 200, "{after}");
        assert_eq!(after["proposals"][0]["state"], "failed", "{after}");
        let asked = &after["proposals"][card]["act"];
        assert_eq!(asked["kind"], "connect", "{after}");
        assert!(
            asked["connection"]
                .as_str()
                .is_some_and(|called| called.contains(&format!("{account}@example.test"))),
            "{after}"
        );
    }
    assert!(served.secrets.calls().is_empty(), "nothing was stored");

    // The tap writes nothing: lieer reads the account until its sign-in with Google is kept and
    // has read its mail (`google::a_lieer_account_moves_onto_the_sign_in_only_once_its_grant_reads_its_mail`).
    let before = file(&served);
    let (status, after) = tap(base, setup, 1, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][1]["state"], "acted", "{after}");
    assert_eq!(file(&served), before, "the instance file is as it was");
}

/// The template's origins for a first calendar, and the declared plugin's once there is one.
#[test]
fn a_connect_card_names_the_origins_its_plugin_may_reach_now() {
    let served = serving(
        "reach",
        Act::Connect {
            connection: Connection::parse("ics", &["Travel".to_owned()]).expect("a calendar"),
        },
    );
    let first = common::reading(&served.base, CONNECT, &[]);
    assert_eq!(
        first["proposals"][0]["act"]["reach"],
        serde_json::json!(["https://outlook.office365.com"]),
        "{first}"
    );
    std::fs::write(
        &served.path,
        format!(
            "{INSTANCE}\n[[plugins]]\nname = \"ics\"\ncomponent = \"plugins/mine/ics.wasm\"\n\
             schema = \"plugins/mine/ics.schema\"\n\
             origins = [\"https://calendar.example.test\"]\n"
        ),
    )
    .expect("the plugin declared by hand");
    let mut drawn = common::get(&format!(
        "{}/api/secretary/conversations/{CONNECT}/drawn?indices=0",
        served.base
    ));
    let now = common::body_json(&mut drawn)["proposals"][0].clone();
    assert_eq!(
        now["act"]["reach"],
        serde_json::json!(["https://calendar.example.test"]),
        "{now}"
    );
}

/// Removing the account deletes both its sign-ins, reading and sending, whatever the card's yes.
#[test]
fn removing_a_microsoft_account_forgets_its_sign_in() {
    let account = || {
        Connection::parse(
            "imap-device-code",
            &["bo@example.test".to_owned(), "consumers".to_owned()],
        )
        .expect("an account")
    };
    let tokens = common::scratch("connections-grants");
    std::fs::create_dir_all(&tokens).expect("a token directory");
    let grant = tokens.join("bo-at-example-test.json");
    let other = tokens.join("ada-at-example-test.json");
    let served = serving_with(
        "grants",
        Act::Connect {
            connection: account(),
        },
        |server, _| server.grants(suzatary_io::oauth::TokenStore::new(tokens.clone())),
    );
    let (status, after) = tap(&served.base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    std::fs::write(&grant, "{}").expect("a grant");
    std::fs::write(&other, "{}").expect("another account's grant");
    asked_of(
        &served,
        Act::Disconnect {
            connection: account(),
        },
    );
    let card = common::reading(&served.base, CONNECT, &[]);
    assert_eq!(
        card["proposals"][1]["act"]["forgets"],
        serde_json::json!(["its sign-in to read and send mail"]),
        "{card}"
    );
    let (status, after) = tap(&served.base, CONNECT, 1, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][1]["state"], "acted", "{after}");
    assert!(after.to_string().contains("forgot its sign-in"), "{after}");
    assert!(!grant.exists(), "its grants are deleted");
    assert!(other.exists(), "another account's are not");
}

/// A link is never stored unchecked: whether it may be fetched is the file's to say.
#[test]
fn a_link_is_refused_while_the_instance_file_does_not_read() {
    let served = serving(
        "unread",
        Act::Connect {
            connection: calendar(),
        },
    );
    let (base, setup) = (&served.base, SETUP.as_str());
    let (status, after) = tap(base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    std::fs::write(&served.path, "this is not an instance file").expect("broken");
    let (status, after) = tap(base, setup, 0, &serde_json::json!({ "value": LINK }));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "failed", "{after}");
    assert!(served.secrets.calls().is_empty(), "nothing was stored");
}

/// A server whose data repository pushes to `origin`, if any, holding every name the deployment
/// declares, with the calendar's Connect filed.
fn serving_pushing(tag: &str, origin: Option<&'static str>) -> Box {
    serving_with(
        tag,
        Act::Connect {
            connection: calendar(),
        },
        move |server, fakes| {
            if let Some(url) = origin {
                std::fs::create_dir_all(fakes.root.join(".git")).expect("a git directory");
                std::fs::write(
                    fakes.root.join(".git/config"),
                    format!("[remote \"origin\"]\n\turl = {url}\n"),
                )
                .expect("a git configuration");
            }
            server.effects(|effects| effects).settings_names(
                suzatary_connect::settings::Settables::of_deployment(
                    suzatary_io::model::TOKEN,
                    std::iter::empty(),
                    None,
                    (&[], &[]),
                    &suzatary_services::repos::Declared::default(),
                ),
            )
        },
    )
}

/// The setup conversation's card asking for the data repository's push token, if one stands.
fn push_token_card(served: &Box) -> Option<serde_json::Value> {
    let label =
        suzatary_connect::settings::Help::label(suzatary_connect::settings::DATA_PUSH_TOKEN);
    common::reading(&served.base, SETUP.as_str(), &[])["proposals"]
        .as_array()
        .expect("proposals")
        .iter()
        .find(|card| card["act"]["label"] == label)
        .cloned()
}

/// A tap that moves the instance file derives the settable names with the function the server
/// started with, so the push token, which no connection declares, stays settable.
#[test]
fn a_connect_tap_leaves_the_push_token_settable_and_owed() {
    let served = serving_pushing("pushtoken", Some("https://github.com/ada/data.git"));
    let (status, after) = tap(&served.base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    let owed = push_token_card(&served).expect("the push token's card");
    assert_eq!(owed["state"], "standing", "{owed}");
}

/// A repository pushing nowhere or not to GitHub is asked for no push token, as no push happens.
#[test]
fn a_repository_pushing_nowhere_is_asked_for_no_push_token() {
    for (tag, origin) in [
        ("pushnowhere", None),
        ("pushforge", Some("git@forge.example.test:ada/data.git")),
    ] {
        let served = serving_pushing(tag, origin);
        let (status, after) = tap(&served.base, CONNECT, 0, &serde_json::json!({}));
        assert_eq!(status, 200, "{after}");
        assert_eq!(push_token_card(&served), None, "{tag}");
    }
}

/// The tap begins its device flow against the instance file as it reads now, though the server
/// started before the account was there.
#[test]
fn a_new_microsoft_account_is_asked_for_its_sign_in() {
    let account = Connection::parse(
        "imap-device-code",
        &["bo@example.test".to_owned(), "consumers".to_owned()],
    )
    .expect("an account");
    // A pending poll before the grant, so the tap's answer is read while the flow still runs.
    let oauth = common::ScriptedOauth::new(vec![
        Ok(common::answered(common::ScriptedOauth::device_code(
            "WXYZ-1234",
            1,
            900,
        ))),
        Ok(common::answered(
            serde_json::json!({"error": "authorization_pending"}),
        )),
        Ok(common::answered(common::ScriptedOauth::granted(
            "refresh-for-bo",
        ))),
    ]);
    let served = serving_with(
        "signin",
        Act::Connect {
            connection: account,
        },
        |server, fakes| {
            server.real_acts_over_instance(
                "connections-signin",
                common::again(&fakes.conversations),
                oauth,
            )
        },
    );
    let (status, after) = tap(&served.base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert!(
        file(&served).contains("sent_folder = \"Sent\""),
        "{}",
        file(&served)
    );
    let asked = common::reading(&served.base, SETUP.as_str(), &[]);
    let card = &asked["proposals"][0]["act"];
    assert_eq!(card["kind"], "auth", "{asked}");
    assert_eq!(card["account"], "bo@example.test");

    let (status, signing) = tap(&served.base, SETUP.as_str(), 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{signing}");
    assert_eq!(signing["proposals"][0]["state"], "started", "{signing}");
    let announced = signing["turns"]
        .as_array()
        .and_then(|turns| turns.iter().rev().find(|turn| turn["who"] == "box"))
        .and_then(|turn| turn["text"].as_str())
        .expect("the flow's announced turn");
    assert!(announced.contains("WXYZ-1234"), "{announced}");
    assert!(announced.contains("bo@example.test"), "{announced}");
    // The card that was tapped carries the link and the code while the flow waits, so a person
    // who tapped it in Settings reads them there.
    let card = &signing["proposals"][0]["announced"];
    assert_eq!(card.as_str(), Some(announced), "{signing}");
    let granted = common::poll_conversation(&served.base, SETUP.as_str(), |body| {
        body["proposals"][0]["state"] != "started"
    });
    assert_eq!(
        granted["proposals"][0]["announced"],
        serde_json::Value::Null,
        "once the flow ends the card says nothing more of it: {granted}"
    );
}

/// The Notifications lines, as `GET /api/overview` reads them now.
fn notifications(base: &str) -> Vec<serde_json::Value> {
    let mut page = common::get(&format!("{base}/api/overview"));
    assert_eq!(common::status(&page), 200);
    let page = common::body_json(&mut page);
    page["groups"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|group| group["group"] == "notifications")
        .and_then(|group| group["lines"].as_array().cloned())
        .unwrap_or_default()
}

/// The overview reads the channels under the lock the tap swaps, not as the server started.
#[test]
fn a_connected_channel_is_on_the_overview_with_no_restart() {
    let ntfy = Connection::parse("ntfy", &[]).expect("a channel");
    let served = serving("channel", Act::Connect { connection: ntfy });
    let names_ntfy = |lines: &[serde_json::Value]| {
        lines
            .iter()
            .any(|line| line["key"].as_str().is_some_and(|key| key.contains("ntfy")))
    };
    assert!(!names_ntfy(&notifications(&served.base)));
    let (status, after) = tap(&served.base, CONNECT, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert!(file(&served).contains("\"ntfy\""), "{}", file(&served));
    let lines = notifications(&served.base);
    assert!(names_ntfy(&lines), "{lines:?}");
}

fn topic(served: &Box) -> String {
    common::opened(&served.base, "Setup")
}

/// The secretary's `propose_act` that files a Connect card for the Work calendar.
const PROPOSE_WORK: &str = r#"{"say": "I will ask.",
    "calls": [{"verb": "propose_act", "args": {
      "act": "connect",
      "template": "ics",
      "params": ["Work"],
      "why": "Ada asked to add her Work calendar.",
      "sources": "briefing:2026-09-02"
    }}]}"#;

/// The secretary's verb turn in `payload` for `propose_act`, the last one.
fn proposing(payload: &serde_json::Value) -> serde_json::Value {
    payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .rev()
        .find(|turn| turn["who"] == "verb" && turn["verb"] == "propose_act")
        .cloned()
        .unwrap_or_else(|| panic!("the verb's turn: {payload}"))
}

/// Nothing is written until the owner taps, and the same calendar asked again is refused, since
/// the file already holds it.
#[test]
fn the_secretary_files_a_connect_card_and_the_tap_adds_the_calendar() {
    let served = serving_answering(
        "secretary-connect",
        None,
        Some(common::FakeModel::saying([
            PROPOSE_WORK,
            r#"{"say": "Proposed."}"#,
            PROPOSE_WORK,
            r#"{"say": "It is already there."}"#,
        ])),
        |server, _| server,
    );
    let id = topic(&served);
    let payload = common::said(&served.base, &id, "add my Work calendar");
    let card = &payload["proposals"][0];
    assert_eq!(card["act"]["kind"], "connect", "{payload}");
    assert!(
        !file(&served).contains("PLUGIN_ICS_WORK"),
        "the secretary proposes, and nothing is written"
    );

    let (status, after) = tap(&served.base, &id, 0, &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");
    assert!(file(&served).contains("credentials = [\"PLUGIN_ICS_WORK\"]"));
    let setup = common::reading(&served.base, SETUP.as_str(), &[]);
    assert!(
        setup["proposals"]
            .as_array()
            .expect("cards")
            .iter()
            .any(|card| {
                card["act"]["kind"] == "secret"
                    && card["act"]["label"] == "your Work calendar's secret link"
            }),
        "the box asks for the link: {setup}"
    );

    // A round of its own, since a round holding an earlier proposal's answer proposes none.
    let again = common::said(&served.base, &topic(&served), "add my Work calendar again");
    let refused = proposing(&again);
    assert_eq!(refused["outcome"]["is"], "refused", "{refused}");
    assert!(
        refused["text"]
            .as_str()
            .expect("text")
            .contains("already connected"),
        "{refused}"
    );
    assert!(
        again["proposals"].as_array().expect("cards").is_empty(),
        "no second card: {again}"
    );
}

/// An account, a calendar or a folder is the owner's to name, so mail cannot name one.
#[test]
fn a_round_that_read_mail_proposes_no_connect_card() {
    let served = serving_answering(
        "secretary-connect-tainted",
        None,
        Some(common::FakeModel::saying([
            r#"{"say": "", "calls": [{"verb": "search", "args": {"query": "calendar"}}]}"#,
            PROPOSE_WORK,
            r#"{"say": "Ask me again on its own."}"#,
        ])),
        |server, _| server,
    );
    let id = topic(&served);
    let payload = common::said(&served.base, &id, "what did the calendar mail say?");
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "{payload}"
    );
    let refused = proposing(&payload);
    assert_eq!(refused["outcome"]["is"], "refused", "{refused}");
    let text = refused["text"].as_str().expect("text");
    assert!(text.contains("not the owner's own words"), "{refused}");
    assert!(
        !text.contains("himself") && !text.contains("herself"),
        "{text}"
    );
    assert!(
        !file(&served).contains("PLUGIN_ICS_WORK"),
        "nothing was written"
    );
}
