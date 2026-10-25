//! "Sign in with Google" through the real routes, against a fake Google
//! (`common::fake_google`): the box files the client's card, its tap keeps the client only when
//! it lists the box's exact callback, the sign-in card opens a consent bound by its state and an
//! S256 challenge, and Google's callback keeps the grant once. A callback naming a state the box
//! never drew, or one already read, exchanges nothing; a code Google already redeemed keeps
//! nothing. The calendars are read bounded, and an event is written only by a tapped card.

mod common;

use std::sync::Arc;

use common::fake_google::{self, FakeGoogle};
use common::{EditsRecorder, Fakes, SecretRecorder, SettingsFixture};
use suzatary_core::capability::Calendar as _;
use suzatary_core::mail::account::{MailAccount, Sync};
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::secretary::noticed::CONVERSATION;

const ADA: &str = "ada@example.test";

/// The deployment's callback: the common deployment's `app_url` and the fixed path.
const REDIRECT: &str = "https://example.test/api/oauth/google/callback";

/// Where a second writer's browser connects from: another loopback address, which the fake
/// daemon names as another login.
const SECOND: std::net::IpAddr = std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 2));

/// The grant's name for [`ADA`].
const GRANT: &str = "GOOGLE_GRANT_ADA_AT_EXAMPLE_TEST";

fn ada() -> EmailAddress {
    EmailAddress::new(ADA).expect("an address")
}

fn account() -> MailAccount {
    MailAccount {
        email: suzatary_core::mail::account_email::AccountEmail::new(ADA)
            .expect("a lower-case address"),
        subdir: "ada-at-example-test".to_owned(),
        sync: Sync::Google,
    }
}

/// A server where `v4` writes as `ada`, declaring one Google account, asking `fake` for Google,
/// its secrets in one recorder the box both writes and reads.
struct Serving {
    base: String,
    fake: FakeGoogle,
    secrets: Arc<SecretRecorder>,
}

fn serving(tag: &str, model: Option<common::FakeModel>) -> Serving {
    let fake = FakeGoogle::start(0, ADA);
    let secrets = Arc::new(SecretRecorder::new());
    let login = common::login("ada@example.com");
    let second = common::login("bea@example.com");
    let whois = common::FakeWhois::new()
        .naming(common::v4(), login.clone())
        .naming(SECOND, second.clone());
    let deployment = common::deployment(
        [login.clone(), second.clone()],
        [
            (login, common::actor("ada")),
            (second, common::actor("bea")),
        ],
    );
    let declared = suzatary_core::connect::Declared {
        accounts: vec![account()],
        ..suzatary_core::connect::Declared::default()
    };
    let settables =
        suzatary_core::settings::Settables::derive(suzatary_io::model::TOKEN, [], None, &[]).join(
            suzatary_core::settings::Settables::google(&declared.accounts),
        );
    let empty = SettingsFixture::empty();
    let settings = SettingsFixture {
        live: suzatary_cli::serve::Live {
            settables,
            ..empty.live.clone()
        },
        secrets: Arc::clone(&secrets) as _,
        edits: Arc::new(EditsRecorder::declaring(declared)),
        ..empty
    };
    let fakes = Fakes::new(tag)
        .whois(whois)
        .secrets(Arc::clone(&secrets) as _)
        .mailbox(common::FakeMailbox::new().with_accounts(vec![ADA.to_owned()]));
    let fakes = Arc::new(match model {
        Some(model) => fakes.model(model),
        None => fakes,
    });
    let running = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .settings(settings)
        .google(
            suzatary_core::google::Endpoints::at(&fake.base),
            Arc::clone(&secrets) as _,
        )
        .real_acts_over_instance(
            tag,
            common::again(&fakes.conversations),
            common::ScriptedOauth::new(Vec::new()),
        )
        .start();
    Serving {
        base: running.base,
        fake,
        secrets,
    }
}

/// The box's own conversation as it reads now.
fn setup(base: &str) -> serde_json::Value {
    common::poll_conversation(
        base,
        CONVERSATION.as_str(),
        std::time::Duration::from_secs(5),
        |_| true,
    )
}

/// The index of the first card of `kind` in `conversation` that is `state`.
fn card(conversation: &serde_json::Value, kind: &str, state: &str) -> Option<usize> {
    conversation["proposals"]
        .as_array()?
        .iter()
        .find(|row| row["act"]["kind"] == kind && row["state"] == state)
        .and_then(|row| row["index"].as_u64())
        .and_then(|index| usize::try_from(index).ok())
}

/// Tap card `index` of the box's conversation with `body`.
fn tap(base: &str, index: usize, body: &serde_json::Value) -> serde_json::Value {
    let mut tapped = common::act(
        &format!(
            "{base}/api/secretary/conversations/{}/proposals/{index}/tap",
            CONVERSATION.as_str()
        ),
        body,
    );
    assert_eq!(common::status(&tapped), 200);
    common::body_json(&mut tapped)
}

/// A client that follows no redirect, so a test reads where each one points.
fn unfollowing() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_idle_connections(0)
        .build()
        .new_agent()
}

/// Where `url` redirects to, and its status.
fn redirected(url: &str) -> (u16, String) {
    let answered = unfollowing()
        .get(url)
        .call()
        .expect("the request completes");
    let location = answered
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    (answered.status().as_u16(), location)
}

/// The client kept, then the sign-in tapped: its consent's link.
fn consent(serving: &Serving) -> String {
    let base = &serving.base;
    let client = card(&setup(base), "google_client", "standing")
        .expect("the box files the client's card while no client is kept");
    let kept = tap(
        base,
        client,
        &serde_json::json!({ "value": fake_google::client_file(REDIRECT) }),
    );
    assert_eq!(kept["proposals"][client]["state"], "acted", "{kept}");
    let sign_in = card(&setup(base), "google_sign_in", "standing")
        .expect("with a client kept, the box files the account's sign-in");
    let started = tap(base, sign_in, &serde_json::json!({}));
    let row = &started["proposals"][sign_in];
    assert_eq!(row["state"], "started", "{started}");
    let path = row["card"]["link"]["href"]
        .as_str()
        .expect("the card draws the consent as its button");
    assert!(path.starts_with(suzatary_core::google::CONSENT), "{path}");
    let (status, link) = redirected(&format!("{base}{path}"));
    assert_eq!(status, 303, "the box sends the browser on to Google");
    link
}

/// A GET of `path` from [`SECOND`]: its status and where it redirects.
fn second_writer_gets(base: &str, path: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let host = base.strip_prefix("http://").expect("an http base");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let answer = runtime.block_on(async {
        let socket = tokio::net::TcpSocket::new_v4().expect("a socket");
        socket
            .bind(std::net::SocketAddr::new(SECOND, 0))
            .expect("binds the second loopback address");
        let mut stream = socket
            .connect(host.parse().expect("a socket address"))
            .await
            .expect("connects");
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .expect("sent");
        let mut answer = String::new();
        stream.read_to_string(&mut answer).await.expect("an answer");
        answer
    });
    let status = answer
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("a status line");
    let location = answer
        .lines()
        .find_map(|line| line.strip_prefix("location: "))
        .unwrap_or_default()
        .to_owned();
    (status, location)
}

/// Google's page, consented to: the callback on the box it sends the browser to.
fn consented(serving: &Serving, link: &str) -> String {
    let (status, location) = redirected(link);
    assert_eq!(status, 302, "the fake's consent page redirects");
    let callback = location
        .strip_prefix("https://example.test")
        .expect("Google sends the browser to the box's exact callback");
    format!("{}{callback}", serving.base)
}

/// The whole sign-in keeps the grant once, under the account's name: the client only when it
/// lists the exact callback, the consent bound to an S256 challenge, and a callback read twice
/// refused the second time without asking Google.
#[test]
fn the_sign_in_keeps_the_grant_once_and_a_replayed_callback_is_refused() {
    let serving = serving("google-sign-in", None);
    let link = consent(&serving);
    assert!(link.starts_with(&serving.fake.base), "{link}");
    for part in [
        "code_challenge_method=S256",
        "redirect_uri=https%3A%2F%2Fexample.test%2Fapi%2Foauth%2Fgoogle%2Fcallback",
        "access_type=offline",
        "login_hint=ada%40example.test",
        "include_granted_scopes=true",
    ] {
        assert!(link.contains(part), "{part} in {link}");
    }
    assert!(!link.contains("code_verifier"), "{link}");
    assert_eq!(
        serving.secrets.calls()[..2],
        [
            (
                suzatary_core::google::CLIENT_ID.to_owned(),
                fake_google::CLIENT_ID.to_owned()
            ),
            (
                suzatary_core::google::CLIENT_SECRET.to_owned(),
                fake_google::CLIENT_SECRET.to_owned()
            ),
        ],
        "the client's id and secret, and nothing else of the file"
    );

    let callback = consented(&serving, &link);
    let (status, location) = redirected(&callback);
    assert_eq!(status, 303, "the callback sends the browser to the Inbox");
    assert_eq!(location, "/#/inbox");
    let kept: Vec<_> = serving
        .secrets
        .calls()
        .into_iter()
        .filter(|(name, _)| name == GRANT)
        .collect();
    assert_eq!(
        kept,
        vec![(
            GRANT.to_owned(),
            "mail,calendar 1//fake-refresh-1".to_owned()
        )],
        "the grant, with the parts Google granted"
    );
    let read = setup(&serving.base);
    let row = card(&read, "google_sign_in", "acted").expect("the card ended signed in");
    let outcome = read["proposals"][row]["outcome"]
        .as_str()
        .unwrap_or_default();
    assert!(
        outcome.contains("its mail and its calendars (view and edit)"),
        "{outcome}"
    );

    let (status, _) = redirected(&callback);
    assert_eq!(status, 400, "a callback is read once");
    assert_eq!(
        serving.fake.exchanges(),
        1,
        "the replay asked Google nothing"
    );
    assert_eq!(
        serving
            .secrets
            .calls()
            .iter()
            .filter(|(name, _)| name == GRANT)
            .count(),
        1
    );

    // The sync hands lieer a token minted from the grant, never the grant or the client.
    let maildir =
        std::env::temp_dir().join(format!("suzatary-google-lieer-{}", std::process::id()));
    let grants = suzatary_io::google::Grants::new(
        suzatary_io::google::Google::new(suzatary_core::google::Endpoints::at(&serving.fake.base)),
        Arc::clone(&serving.secrets) as _,
    );
    grants
        .ready(&ada(), &maildir)
        .expect("lieer is handed a token");
    let handed = std::fs::read_to_string(maildir.join(suzatary_io::google::LIEER_CREDENTIALS))
        .expect("lieer's file");
    assert!(handed.contains(fake_google::ACCESS), "{handed}");
    for kept in ["fake-refresh", fake_google::CLIENT_SECRET] {
        assert!(!handed.contains(kept), "{kept} in {handed}");
    }
    let _ = std::fs::remove_dir_all(&maildir);
}

/// A second writer, signed in to the tailnet as someone else, can neither follow the consent
/// another tapped nor finish it: the button is refused, and a callback carrying the first
/// writer's state closes that consent, exchanges nothing and keeps nothing, so not even its
/// owner can finish it afterwards.
#[test]
fn a_second_writer_can_neither_open_nor_finish_anothers_consent() {
    let serving = serving("google-second-writer", None);
    let base = &serving.base;
    let client = card(&setup(base), "google_client", "standing").expect("the client's card");
    tap(
        base,
        client,
        &serde_json::json!({ "value": fake_google::client_file(REDIRECT) }),
    );
    let sign_in = card(&setup(base), "google_sign_in", "standing").expect("the sign-in card");
    let started = tap(base, sign_in, &serde_json::json!({}));
    let path = started["proposals"][sign_in]["card"]["link"]["href"]
        .as_str()
        .expect("the consent's button")
        .to_owned();

    let (status, location) = second_writer_gets(base, &path);
    assert_eq!(status, 403, "the button is the tapper's alone");
    assert!(location.is_empty(), "{location}");

    let (_, link) = redirected(&format!("{base}{path}"));
    let callback = consented(&serving, &link);
    let callback_path = callback
        .strip_prefix(base.as_str())
        .expect("the box's callback");
    let (status, _) = second_writer_gets(base, callback_path);
    assert_eq!(status, 403, "the callback is the tapper's alone");
    assert_eq!(serving.fake.exchanges(), 0, "no code was exchanged");
    assert!(
        serving
            .secrets
            .calls()
            .iter()
            .all(|(name, _)| name != GRANT),
        "nothing was kept"
    );
    let read = setup(base);
    let ended = card(&read, "google_sign_in", "failed").expect("the card ended");
    let outcome = read["proposals"][ended]["outcome"]
        .as_str()
        .unwrap_or_default();
    assert!(
        outcome.contains("someone other than who tapped"),
        "{outcome}"
    );
    let (status, _) = redirected(&callback);
    assert_eq!(status, 400, "the consent is closed, for its owner too");
    assert_eq!(serving.fake.exchanges(), 0);
}

/// A part the owner leaves unticked on Google's page is asked for alone: the grant is kept with
/// the mail, the box files a card asking for the calendars only, and that consent asks Google
/// for nothing else and adds them to the grant the account holds.
#[test]
fn a_part_left_unticked_is_asked_for_alone() {
    let serving = serving("google-incremental", None);
    serving.fake.unticking(&[
        "https://www.googleapis.com/auth/calendar.readonly",
        "https://www.googleapis.com/auth/calendar.events.owned",
    ]);
    let link = consent(&serving);
    assert!(link.contains("gmail.readonly"), "{link}");
    let (status, _) = redirected(&consented(&serving, &link));
    assert_eq!(status, 303);
    let kept = |n: usize| {
        serving
            .secrets
            .calls()
            .into_iter()
            .filter(|(name, _)| name == GRANT)
            .map(|(_, value)| value)
            .nth(n)
    };
    assert_eq!(kept(0).as_deref(), Some("mail 1//fake-refresh-1"));

    let read = common::poll_conversation(
        &serving.base,
        CONVERSATION.as_str(),
        std::time::Duration::from_secs(5),
        |read| card(read, "google_sign_in", "standing").is_some(),
    );
    let first = card(&read, "google_sign_in", "acted").expect("the first sign-in ended");
    let outcome = read["proposals"][first]["outcome"]
        .as_str()
        .unwrap_or_default();
    assert!(outcome.contains("not its calendars"), "{outcome}");
    let again = card(&read, "google_sign_in", "standing").expect("a card for the calendars");
    assert_eq!(
        read["proposals"][again]["act"]["parts"],
        serde_json::json!(["its calendars (view and edit)"]),
        "{read}"
    );

    serving.fake.unticking(&[]);
    let started = tap(&serving.base, again, &serde_json::json!({}));
    let path = started["proposals"][again]["card"]["link"]["href"]
        .as_str()
        .expect("the consent's button")
        .to_owned();
    let (_, link) = redirected(&format!("{}{path}", serving.base));
    assert!(link.contains("calendar.readonly"), "{link}");
    assert!(
        !link.contains("gmail"),
        "only the calendars are asked for: {link}"
    );
    let (status, _) = redirected(&consented(&serving, &link));
    assert_eq!(status, 303);
    assert_eq!(
        kept(1).as_deref(),
        Some("mail,calendar 1//fake-refresh-2"),
        "Google added the calendars to the grant the account holds"
    );
    let read = common::poll_conversation(
        &serving.base,
        CONVERSATION.as_str(),
        std::time::Duration::from_secs(5),
        |read| card(read, "google_sign_in", "standing").is_none(),
    );
    assert_eq!(card(&read, "google_sign_in", "standing"), None, "{read}");
}

/// A callback whose state the box never drew is refused before any exchange, and leaves the
/// consent it imitates open for its own callback.
#[test]
fn a_callback_with_a_state_the_box_never_drew_exchanges_nothing() {
    let serving = serving("google-wrong-state", None);
    let link = consent(&serving);
    let callback = consented(&serving, &link);
    let forged = callback.replace(
        &callback[callback.find("state=").expect("a state") + 6..][..43],
        "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
    );
    assert_ne!(forged, callback);
    let (status, _) = redirected(&forged);
    assert_eq!(status, 400);
    let (status, _) = redirected(&format!(
        "{}/api/oauth/google/callback?code=4%2Ffake-code-1",
        serving.base
    ));
    assert_eq!(status, 400, "no state, no callback");
    assert_eq!(
        serving.fake.exchanges(),
        0,
        "nothing reached Google's token endpoint"
    );
    assert!(
        serving
            .secrets
            .calls()
            .iter()
            .all(|(name, _)| name != GRANT)
    );

    let (status, _) = redirected(&callback);
    assert_eq!(status, 303, "the consent's own callback still finishes it");
}

/// A client file that does not list the box's exact callback keeps nothing and says the address
/// to add.
#[test]
fn a_client_without_the_exact_callback_is_kept_nowhere() {
    let serving = serving("google-wrong-client", None);
    let client = card(&setup(&serving.base), "google_client", "standing").expect("the card");
    let refused = tap(
        &serving.base,
        client,
        &serde_json::json!({ "value": fake_google::client_file("https://example.test/") }),
    );
    let row = &refused["proposals"][client];
    assert_eq!(row["state"], "failed", "{refused}");
    let outcome = row["outcome"].as_str().unwrap_or_default();
    assert!(outcome.contains(REDIRECT), "{outcome}");
    assert!(!outcome.contains(fake_google::CLIENT_SECRET), "{outcome}");
    assert!(serving.secrets.calls().is_empty());
    assert_eq!(
        card(&setup(&serving.base), "google_sign_in", "standing"),
        None
    );
}

/// The owner declining on Google's page ends the card saying so, and keeps nothing.
#[test]
fn a_declined_consent_ends_the_card_and_keeps_nothing() {
    let serving = serving("google-declined", None);
    let link = consent(&serving);
    let state = link
        .split('&')
        .find_map(|pair| pair.strip_prefix("state="))
        .expect("a state");
    let (status, _) = redirected(&format!(
        "{}/api/oauth/google/callback?state={state}&error=access_denied",
        serving.base
    ));
    assert_eq!(status, 303);
    let read = setup(&serving.base);
    let row = card(&read, "google_sign_in", "failed").expect("the card ended unsigned");
    assert!(
        read["proposals"][row]["outcome"]
            .as_str()
            .unwrap_or_default()
            .contains("access_denied"),
        "{read}"
    );
    assert_eq!(serving.fake.exchanges(), 0);
}

/// A code Google already redeemed is refused at its token endpoint, and the box reads that as no
/// grant.
#[test]
fn a_code_google_already_redeemed_keeps_nothing() {
    let fake = FakeGoogle::start(0, ADA);
    let google = suzatary_io::google::Google::new(suzatary_core::google::Endpoints::at(&fake.base));
    let redirect =
        suzatary_core::google::RedirectUri::of_app("https://example.test").expect("a callback");
    let verifier = suzatary_core::google::Verifier::draw(&suzatary_io::texts::SystemEntropy)
        .expect("a verifier");
    let state =
        suzatary_core::google::State::draw(&suzatary_io::texts::SystemEntropy).expect("a state");
    let client = suzatary_core::google::Client::of_download(
        &suzatary_core::capability::Secret::new(fake_google::client_file(REDIRECT)),
        &redirect,
    )
    .expect("the fake's client");
    let link = suzatary_core::google::authorize(
        google.endpoints(),
        &client.id,
        &redirect,
        (&state, &verifier.challenge()),
        &suzatary_core::google::Scopes::all(),
        &ada(),
    );
    let (_, location) = redirected(&link);
    let query = location
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let field = |name: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .map(|value| value.replace("%2F", "/"))
    };
    let Ok(suzatary_core::google::Callback::Code { code, .. }) =
        suzatary_core::google::Callback::of_query(
            field("state").as_deref(),
            field("code").as_deref(),
            None,
        )
    else {
        panic!("a code in {location}");
    };
    assert!(
        google
            .exchange(&client, &code, &verifier, &redirect, &ada())
            .is_ok()
    );
    let again = google
        .exchange(&client, &code, &verifier, &redirect, &ada())
        .expect_err("a code is redeemed once");
    assert!(again.to_string().contains("invalid_grant"), "{again}");

    // A verifier other than the one the challenge was made from is refused the same way.
    let (_, location) = redirected(&link);
    let fresh = location
        .split('&')
        .find_map(|pair| pair.strip_prefix("code="))
        .map(|code| code.replace("%2F", "/"))
        .expect("a second code");
    let Ok(suzatary_core::google::Callback::Code { code, .. }) =
        suzatary_core::google::Callback::of_query(Some(state.as_str()), Some(&fresh), None)
    else {
        panic!("a code");
    };
    let other = suzatary_core::google::Verifier::draw(&suzatary_io::texts::SystemEntropy)
        .expect("a verifier");
    assert!(
        google
            .exchange(&client, &code, &other, &redirect, &ada())
            .is_err()
    );
}

/// A revoked grant, or one a client in Testing ended after 7 days, reads as the sign-in its fix
/// card asks for; a missing client as the client's.
#[test]
fn a_revoked_grant_needs_the_sign_in_and_a_missing_client_the_client() {
    let fake = FakeGoogle::start(0, ADA);
    let secrets = Arc::new(SecretRecorder::new());
    let grants = suzatary_io::google::Grants::new(
        suzatary_io::google::Google::new(suzatary_core::google::Endpoints::at(&fake.base)),
        Arc::clone(&secrets) as _,
    );
    let refused = grants.live(&ada()).expect_err("no client is kept");
    assert_eq!(refused.class(), suzatary_core::connect::Failure::Unresolved);

    let settables = suzatary_core::settings::Settables::google(&[account()]);
    let write = |name: &str, value: &str| {
        use suzatary_core::settings::Writes as _;
        secrets
            .write_secret(
                &suzatary_core::settings::Settable::mint(&settables, name).expect("declared"),
                &suzatary_core::capability::Secret::new(value),
            )
            .expect("written");
    };
    write(suzatary_core::google::CLIENT_ID, fake_google::CLIENT_ID);
    write(
        suzatary_core::google::CLIENT_SECRET,
        fake_google::CLIENT_SECRET,
    );
    let unsigned = grants.live(&ada()).expect_err("never signed in");
    assert_eq!(
        unsigned.class(),
        suzatary_core::connect::Failure::NeedsLogin
    );

    write(GRANT, "mail,calendar 1//not-one-google-issued");
    let lapsed = grants.live(&ada()).expect_err("Google refuses it");
    assert_eq!(lapsed.class(), suzatary_core::connect::Failure::NeedsLogin);
    assert!(lapsed.to_string().contains("7 days"), "{lapsed}");
}

/// The calendars are read bounded: however many events Google answers with, at most `EVENTS`
/// of each calendar are read, and what is learned reads back as the calendar's days.
#[test]
fn the_calendars_are_read_bounded_and_learned_once() {
    let fake = FakeGoogle::start(0, ADA);
    fake.answering_events(400);
    let secrets = Arc::new(SecretRecorder::new());
    let settables = suzatary_core::settings::Settables::google(&[account()]);
    for (name, value) in [
        (suzatary_core::google::CLIENT_ID, fake_google::CLIENT_ID),
        (
            suzatary_core::google::CLIENT_SECRET,
            fake_google::CLIENT_SECRET,
        ),
    ] {
        use suzatary_core::settings::Writes as _;
        secrets
            .write_secret(
                &suzatary_core::settings::Settable::mint(&settables, name).expect("declared"),
                &suzatary_core::capability::Secret::new(value),
            )
            .expect("written");
    }
    // The grant the fake issues, by its own exchange.
    let google = suzatary_io::google::Google::new(suzatary_core::google::Endpoints::at(&fake.base));
    let redirect =
        suzatary_core::google::RedirectUri::of_app("https://example.test").expect("a callback");
    let verifier = suzatary_core::google::Verifier::draw(&suzatary_io::texts::SystemEntropy)
        .expect("a verifier");
    let state =
        suzatary_core::google::State::draw(&suzatary_io::texts::SystemEntropy).expect("a state");
    let client = suzatary_io::google::Grant::client(secrets.as_ref())
        .expect("the store answers")
        .expect("a client");
    let (_, location) = redirected(&suzatary_core::google::authorize(
        google.endpoints(),
        &client.id,
        &redirect,
        (&state, &verifier.challenge()),
        &suzatary_core::google::Scopes::all(),
        &ada(),
    ));
    let code = location
        .split('&')
        .find_map(|pair| pair.strip_prefix("code="))
        .map(|code| code.replace("%2F", "/"))
        .expect("a code");
    let Ok(suzatary_core::google::Callback::Code { code, .. }) =
        suzatary_core::google::Callback::of_query(Some(state.as_str()), Some(&code), None)
    else {
        panic!("a code");
    };
    let granted = google
        .exchange(&client, &code, &verifier, &redirect, &ada())
        .expect("a grant");
    let grant = suzatary_io::google::Grant {
        account: ada(),
        client,
        refresh: granted.refresh,
        parts: suzatary_core::google::Scopes::all(),
    };

    let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 17)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .expect("an instant");
    let read = google.occurrences(&grant, now).expect("occurrences");
    assert_eq!(
        read.len(),
        2 * suzatary_core::google::EVENTS,
        "two calendars, each read to its bound"
    );
    let root =
        std::env::temp_dir().join(format!("suzatary-google-calendar-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let at = prodrome_core::literal::Datetime::new(2026, 10, 17, 9, 0, 0, 0).expect("an instant");
    let learnt = suzatary_io::google::learn(&root, &read, at).expect("learned");
    assert_eq!(learnt.written, read.len());
    let again = suzatary_io::google::learn(&root, &read, at).expect("learned");
    assert_eq!(
        (again.written, again.known),
        (0, read.len()),
        "learned once"
    );
    let days = suzatary_io::calendar::Clocked::new([root], suzatary_core::zone::Zone::UTC)
        .days(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 19).expect("a day"),
            1,
        )
        .expect("the calendar reads");
    assert_eq!(days[0].entries.len(), read.len());
}

/// An event is written only by its tapped card: proposing it writes nothing, and the tap writes
/// it once, as the card showed it.
#[test]
fn an_event_is_written_only_by_its_tapped_card() {
    let model = common::FakeModel::saying_owned([
        format!(
            r#"{{"say": "Here is the event.", "calls": [{{"verb": "propose_act", "args": {{
              "act": "calendar_event", "account": "{ADA}", "title": "Dentist",
              "start": "2026-10-20T14:00:00Z", "end": "2026-10-20T14:45:00Z",
              "place": "Main St 4",
              "why": "the owner asked for the appointment in his calendar",
              "sources": "briefing:2026-10-17"}}}}]}}"#
        ),
        r#"{"say": "Proposed. Tap it to add it."}"#.to_owned(),
        r#"{"say": "Understood."}"#.to_owned(),
    ]);
    let serving = serving("google-event", Some(model));
    let mut opened = common::act(
        &format!("{}/api/secretary/conversations", serving.base),
        &serde_json::json!({"title": "The dentist"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let said = common::act(
        &format!("{}/api/secretary/conversations/{id}/say", serving.base),
        &serde_json::json!({"say": "put the dentist in my calendar"}),
    );
    assert_eq!(
        common::status(&said),
        202,
        "the round goes on after the answer"
    );
    let proposed = common::poll_conversation(
        &serving.base,
        &id,
        std::time::Duration::from_secs(10),
        |body| {
            body["proposals"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty())
        },
    );
    assert_eq!(
        proposed["proposals"][0]["act"]["kind"], "calendar_event",
        "{proposed}"
    );
    // The client and the grant, kept as the cards keep them: a sign-in writes no event.
    let link = consent(&serving);
    let (status, _) = redirected(&consented(&serving, &link));
    assert_eq!(status, 303);
    assert!(
        serving.fake.inserted().is_empty(),
        "proposing writes nothing"
    );

    let mut tapped = common::act(
        &format!(
            "{}/api/secretary/conversations/{id}/proposals/0/tap",
            serving.base
        ),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = common::body_json(&mut tapped);
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");
    assert_eq!(
        serving.fake.inserted(),
        vec![serde_json::json!({
            "summary": "Dentist",
            "location": "Main St 4",
            "start": {"dateTime": "2026-10-20T14:00:00Z"},
            "end": {"dateTime": "2026-10-20T14:45:00Z"},
        })]
    );
}

/// The calendar's schema this source writes is the one the calendar plugin declares.
#[test]
fn the_google_calendar_writes_the_calendar_plugins_schema() {
    let declared = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugin-ics/ics.schema"),
    )
    .expect("the plugin's schema");
    assert_eq!(declared, suzatary_core::google::SCHEMA);
}
