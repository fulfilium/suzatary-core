//! "Sign in with Google" through the real routes, against a fake Google
//! (`common::fake_google`).

use crate::common;

use std::sync::Arc;

use common::fake_google::{self, FakeGoogle};
use common::{EditsRecorder, Fakes, SecretRecorder, SettingsFixture};
use suzatary_base::conversation_id::SETUP;
use suzatary_google::calendar::effect::Calendar as _;
use suzatary_mail::mail::account::{MailAccount, Sync};
use suzatary_mail::mail::address::EmailAddress;

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

fn account(sync: Sync) -> MailAccount {
    MailAccount {
        email: suzatary_mail::mail::account_email::AccountEmail::new(ADA)
            .expect("a lower-case address"),
        subdir: "ada-at-example-test".to_owned(),
        sync,
    }
}

/// A server where `v4` writes as `ada`, declaring one Google account, asking `fake` for Google,
/// its secrets in one recorder the box both writes and reads.
struct Serving {
    base: String,
    fake: FakeGoogle,
    secrets: Arc<SecretRecorder>,
    edits: Arc<EditsRecorder>,
    checks: Arc<common::CheckRecorder>,
    fakes: Arc<Fakes>,
}

fn serving(tag: &str, model: Option<common::FakeModel>) -> Serving {
    serving_as(tag, model, Sync::Google)
}

fn serving_as(tag: &str, model: Option<common::FakeModel>, sync: Sync) -> Serving {
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
    let declared = suzatary_connect::connect::Declared {
        accounts: vec![account(sync)],
        ..suzatary_connect::connect::Declared::default()
    };
    let settables =
        suzatary_connect::settings::Settables::derive(suzatary_io::model::TOKEN, [], None, &[])
            .join(suzatary_connect::settings::Settables::google(
                &declared.accounts,
            ));
    let empty = SettingsFixture::empty();
    let accounts = suzatary_mail::mail::account::Roster::new(declared.accounts.clone())
        .expect("one account is a roster");
    let edits = Arc::new(EditsRecorder::declaring(declared));
    let checks = Arc::new(common::CheckRecorder::default());
    let settings = SettingsFixture {
        live: suzatary_server::serve::Live {
            settables,
            accounts,
            ..empty.live.clone()
        },
        secrets: Arc::clone(&secrets) as _,
        edits: Arc::clone(&edits) as _,
        ..empty
    };
    let fakes = Fakes::new(tag)
        .whois(whois)
        .secrets(Arc::clone(&secrets) as _)
        .mailbox(
            common::FakeMailbox::new()
                .with_accounts(vec![ADA.to_owned()])
                .with_message(MARKED_MAIL, marked_mail()),
        );
    let fakes = Arc::new(match model {
        Some(model) => fakes.model(model),
        None => fakes,
    });
    let running = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .settings(settings)
        .google(
            suzatary_google::google::Endpoints::at(&fake.base),
            Arc::clone(&secrets) as _,
        )
        .checks(Arc::clone(&checks))
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
        edits,
        checks,
        fakes,
    }
}

/// Words that stand only in the store's mail and in a card's recorded response, so an answer
/// that holds them read outside the round.
const MARKER: &str = "zebra-quartz-marker";

const MARKED_MAIL: &str = "a1b2c3d4e5f6a1b2";

fn marked_mail() -> suzatary_mail::mail::MailMessage {
    suzatary_mail::mail::MailMessage {
        row: suzatary_mail::mail::MailRow {
            hash: MARKED_MAIL.to_owned(),
            id: "marked@example.test".to_owned(),
            sender: "someone@example.test".to_owned(),
            subject: format!("About {MARKER}"),
            date: None,
            container: "inbox".to_owned(),
            account: ADA.to_owned(),
            disposition: suzatary_mail::mail::Disposed::Undecided,
            held: suzatary_held::held_value::Kinds::empty(),
        },
        thread: "000000000000c3fb".to_owned(),
        headers: Vec::new(),
        body: format!("Connect {MARKER}@example.test, please."),
        truncated: false,
        links: Vec::new(),
    }
}

fn setup(base: &str) -> serde_json::Value {
    common::poll_conversation(base, SETUP.as_str(), |_| true)
}

fn card(conversation: &serde_json::Value, kind: &str, state: &str) -> Option<usize> {
    conversation["proposals"]
        .as_array()?
        .iter()
        .find(|row| row["act"]["kind"] == kind && row["state"] == state)
        .and_then(|row| row["index"].as_u64())
        .and_then(|index| usize::try_from(index).ok())
}

fn tap(base: &str, index: usize, body: &serde_json::Value) -> serde_json::Value {
    let mut tapped = common::act(
        &format!(
            "{base}/api/secretary/conversations/{}/proposals/{index}/tap",
            SETUP.as_str()
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
    assert!(path.starts_with(suzatary_google::google::CONSENT), "{path}");
    let (status, link) = redirected(&format!("{base}{path}"));
    assert_eq!(status, 303, "the box sends the browser on to Google");
    link
}

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

/// The client is kept only when it lists the exact callback, the consent is bound to an S256
/// challenge, and a callback read twice is refused without asking Google.
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
                suzatary_google::google::CLIENT_ID.to_owned(),
                fake_google::CLIENT_ID.to_owned()
            ),
            (
                suzatary_google::google::CLIENT_SECRET.to_owned(),
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
        outcome.contains("its mail (read and send) and its calendars (view and edit)"),
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
    let maildir = common::scratch("google-lieer");
    let grants = suzatary_io::google::Grants::new(
        suzatary_io::google::Google::new(suzatary_google::google::Endpoints::at(
            &serving.fake.base,
        )),
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
}

/// A callback carrying another writer's state closes that consent and keeps nothing, so not even
/// its owner can finish it afterwards.
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

/// The grant is kept with the mail, and the calendars' own consent asks Google for nothing else
/// and adds them to the grant the account holds.
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

    let read = common::poll_conversation(&serving.base, SETUP.as_str(), |read| {
        card(read, "google_sign_in", "standing").is_some()
    });
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
    let read = common::poll_conversation(&serving.base, SETUP.as_str(), |read| {
        card(read, "google_sign_in", "standing").is_none()
    });
    assert_eq!(card(&read, "google_sign_in", "standing"), None, "{read}");
}

/// The forged callback leaves the consent it imitates open for its own callback.
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
    let google =
        suzatary_io::google::Google::new(suzatary_google::google::Endpoints::at(&fake.base));
    let redirect =
        suzatary_google::google::RedirectUri::of_app("https://example.test").expect("a callback");
    let verifier =
        suzatary_google::google::Verifier::draw(&suzatary_io_local::texts::SystemEntropy)
            .expect("a verifier");
    let state = suzatary_google::google::State::draw(&suzatary_io_local::texts::SystemEntropy)
        .expect("a state");
    let client = suzatary_google::google::Client::of_download(
        &suzatary_base::capability::Secret::new(fake_google::client_file(REDIRECT)),
        &redirect,
    )
    .expect("the fake's client");
    let link = suzatary_google::google::authorize(
        google.endpoints(),
        &client.id,
        &redirect,
        (&state, &verifier.challenge()),
        &suzatary_google::google::Scopes::all(),
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
    let Ok(suzatary_google::google::Callback::Code { code, .. }) =
        suzatary_google::google::Callback::of_query(
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
    let Ok(suzatary_google::google::Callback::Code { code, .. }) =
        suzatary_google::google::Callback::of_query(Some(state.as_str()), Some(&fresh), None)
    else {
        panic!("a code");
    };
    let other = suzatary_google::google::Verifier::draw(&suzatary_io_local::texts::SystemEntropy)
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
        suzatary_io::google::Google::new(suzatary_google::google::Endpoints::at(&fake.base)),
        Arc::clone(&secrets) as _,
    );
    let refused = grants.live(&ada()).expect_err("no client is kept");
    assert_eq!(
        refused.class(),
        suzatary_mail::mail::fault::Fault::Unresolved
    );

    let settables = suzatary_connect::settings::Settables::google(&[account(Sync::Google)]);
    let write = |name: &str, value: &str| {
        use suzatary_connect::settings::Writes as _;
        secrets
            .write_secret(
                &suzatary_connect::settings::Settable::mint(&settables, name).expect("declared"),
                &suzatary_base::capability::Secret::new(value),
            )
            .expect("written");
    };
    write(suzatary_google::google::CLIENT_ID, fake_google::CLIENT_ID);
    write(
        suzatary_google::google::CLIENT_SECRET,
        fake_google::CLIENT_SECRET,
    );
    let unsigned = grants.live(&ada()).expect_err("never signed in");
    assert_eq!(
        unsigned.class(),
        suzatary_mail::mail::fault::Fault::NeedsLogin
    );

    write(GRANT, "mail,calendar 1//not-one-google-issued");
    let lapsed = grants.live(&ada()).expect_err("Google refuses it");
    assert_eq!(
        lapsed.class(),
        suzatary_mail::mail::fault::Fault::NeedsLogin
    );
    assert!(lapsed.to_string().contains("7 days"), "{lapsed}");
}

/// However many events Google answers with, at most `EVENTS` of each calendar are read.
#[test]
fn the_calendars_are_read_bounded_and_learned_once() {
    let fake = FakeGoogle::start(0, ADA);
    fake.answering_events(400);
    let secrets = Arc::new(SecretRecorder::new());
    let settables = suzatary_connect::settings::Settables::google(&[account(Sync::Google)]);
    for (name, value) in [
        (suzatary_google::google::CLIENT_ID, fake_google::CLIENT_ID),
        (
            suzatary_google::google::CLIENT_SECRET,
            fake_google::CLIENT_SECRET,
        ),
    ] {
        use suzatary_connect::settings::Writes as _;
        secrets
            .write_secret(
                &suzatary_connect::settings::Settable::mint(&settables, name).expect("declared"),
                &suzatary_base::capability::Secret::new(value),
            )
            .expect("written");
    }
    // The grant the fake issues, by its own exchange.
    let google =
        suzatary_io::google::Google::new(suzatary_google::google::Endpoints::at(&fake.base));
    let redirect =
        suzatary_google::google::RedirectUri::of_app("https://example.test").expect("a callback");
    let verifier =
        suzatary_google::google::Verifier::draw(&suzatary_io_local::texts::SystemEntropy)
            .expect("a verifier");
    let state = suzatary_google::google::State::draw(&suzatary_io_local::texts::SystemEntropy)
        .expect("a state");
    let client = suzatary_io::google::Grant::client(secrets.as_ref())
        .expect("the store answers")
        .expect("a client");
    let (_, location) = redirected(&suzatary_google::google::authorize(
        google.endpoints(),
        &client.id,
        &redirect,
        (&state, &verifier.challenge()),
        &suzatary_google::google::Scopes::all(),
        &ada(),
    ));
    let code = location
        .split('&')
        .find_map(|pair| pair.strip_prefix("code="))
        .map(|code| code.replace("%2F", "/"))
        .expect("a code");
    let Ok(suzatary_google::google::Callback::Code { code, .. }) =
        suzatary_google::google::Callback::of_query(Some(state.as_str()), Some(&code), None)
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
        parts: suzatary_google::google::Scopes::all(),
    };

    let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 17)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .expect("an instant");
    let read = google.occurrences(&grant, now).expect("occurrences");
    assert_eq!(
        read.len(),
        2 * suzatary_google::google::EVENTS,
        "two calendars, each read to its bound"
    );
    let root = common::scratch("google-calendar");
    let at = prodrome_core::literal::Datetime::new(2026, 10, 17, 9, 0, 0, 0).expect("an instant");
    let learnt = suzatary_io::google::learn(&root, &read, at).expect("learned");
    assert_eq!(learnt.written, read.len());
    let again = suzatary_io::google::learn(&root, &read, at).expect("learned");
    assert_eq!(
        (again.written, again.known),
        (0, read.len()),
        "learned once"
    );
    let days = suzatary_io::calendar::Clocked::new([root], suzatary_base::zone::Zone::UTC)
        .days(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 19).expect("a day"),
            1,
        )
        .expect("the calendar reads");
    assert_eq!(days[0].entries.len(), read.len());
}

/// A line said before the round's closing reply joins that round (`Round::run` joins before every
/// ask), so the wait is for `idle`, not for the card.
fn event_proposed(base: &str) -> String {
    let id = common::opened(base, "The dentist");
    let proposed = common::said(base, &id, "put the dentist in my calendar");
    assert_eq!(
        proposed["proposals"][0]["act"]["kind"], "calendar_event",
        "{proposed}"
    );
    id
}

/// Proposing writes nothing, and the tap writes the event once, as the card showed it.
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
    let id = event_proposed(&serving.base);
    // Tapped before the box keeps a Google client, the card writes nothing and offers the
    // Google setup, and tapping it again is still allowed.
    let mut early = common::act(
        &format!(
            "{}/api/secretary/conversations/{id}/proposals/0/tap",
            serving.base
        ),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&early), 200);
    let early = common::body_json(&mut early);
    let card = &early["proposals"][0];
    assert_eq!(card["state"], "failed", "{early}");
    assert_eq!(
        card["outcome"],
        "Nothing was done: the box keeps no Google client, through which every Google account \
         signs in. Tap Set up Google: its card asks for the client once. Then tap Try again."
    );
    assert_eq!(
        card["card"]["link"],
        serde_json::json!({"kind": "fix", "label": "Set up Google",
            "then": {"kind": "place", "href": "#/settings"}})
    );
    assert_eq!(card["card"]["answers"][0]["label"], "Try again");
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

/// The fix is performed by the same tap, and the account's mail stays where it was read, the
/// grant holding the calendars alone. A forged callback writes nothing.
#[test]
fn a_lieer_card_that_lacks_a_grant_is_fixed_from_its_button() {
    fixed_from_its_button("google-fix", &Sync::Gmail);
}

/// [`a_lieer_card_that_lacks_a_grant_is_fixed_from_its_button`], for an account whose mail is
/// read by IMAP from Microsoft: its calendar is Google's all the same.
#[test]
fn a_microsoft_card_that_lacks_a_grant_is_fixed_from_its_button() {
    fixed_from_its_button(
        "google-fix-ms",
        &Sync::MsImap(suzatary_mail::mail::account::MsImap {
            authority: suzatary_mail::mail::account::Authority::Organizations,
            inbox_folders: suzatary_mail::mail::folders::Folders::inbox(),
            sent_folder: "Sent Items".to_owned(),
        }),
    );
}

/// The same, for an account whose mail Proton's Bridge reads.
#[test]
fn a_proton_card_that_lacks_a_grant_is_fixed_from_its_button() {
    fixed_from_its_button(
        "google-fix-proton",
        &Sync::Proton(suzatary_mail::mail::account::Proton {
            channel: "proton-all".to_owned(),
            smtp_user: ADA.to_owned(),
            send_addresses: vec![ADA.to_owned()],
            inbox_folders: suzatary_mail::mail::folders::Folders::inbox(),
        }),
    );
}

/// The one-tap scene for an account of `sync`, served as `tag`.
fn fixed_from_its_button(tag: &str, sync: &Sync) {
    let model = common::FakeModel::saying_owned([
        format!(
            r#"{{"say": "Here is the event.", "calls": [{{"verb": "propose_act", "args": {{
              "act": "calendar_event", "account": "{ADA}", "title": "Dentist",
              "start": "2026-10-20T14:00:00Z", "end": "2026-10-20T14:45:00Z",
              "why": "the owner asked for the appointment in his calendar",
              "sources": "briefing:2026-10-17"}}}}]}}"#
        ),
        r#"{"say": "Proposed. Tap it to add it."}"#.to_owned(),
    ]);
    let serving = serving_as(tag, Some(model), sync.clone());
    let base = &serving.base;
    let id = event_proposed(base);
    let tap_event = |body: &serde_json::Value| {
        let mut tapped = common::act(
            &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
            body,
        );
        assert_eq!(common::status(&tapped), 200);
        common::body_json(&mut tapped)["proposals"][0].clone()
    };
    let fix = serde_json::json!({"fix": true});

    // No client kept: the lack is the client, and its fix stands on Settings.
    let lacked = tap_event(&serde_json::json!({}));
    assert_eq!(lacked["state"], "failed", "{lacked}");
    assert_eq!(
        lacked["card"]["link"],
        serde_json::json!({"kind": "fix", "label": "Set up Google",
            "then": {"kind": "place", "href": "#/settings"}})
    );
    tap_event(&fix);
    let client =
        card(&setup(base), "google_client", "standing").expect("the fix files the client's card");
    tap(
        base,
        client,
        &serde_json::json!({ "value": fake_google::client_file(REDIRECT) }),
    );

    // A client kept: the lack is the account's grant, and its fix the consent.
    let lacked = tap_event(&serde_json::json!({}));
    assert_eq!(lacked["card"]["link"]["then"]["kind"], "leave", "{lacked}");
    let href = lacked["card"]["link"]["then"]["href"]
        .as_str()
        .expect("the fix's page")
        .to_owned();
    assert_eq!(lacked["card"]["link"]["label"], format!("Connect {ADA}"));
    assert!(href.starts_with(suzatary_google::google::CONSENT), "{href}");
    tap_event(&fix);
    assert!(
        card(&setup(base), "google_sign_in", "started").is_some(),
        "the fix tapped the account's sign-in card"
    );
    let (status, link) = redirected(&format!("{base}{href}"));
    assert_eq!(status, 303, "the card's own path opens the consent");
    for part in [
        "login_hint=ada%40example.test",
        "include_granted_scopes=true",
        "calendar.events.owned",
    ] {
        assert!(link.contains(part), "{part} in {link}");
    }
    assert!(
        !link.contains("gmail"),
        "a calendar asks for no mail: {link}"
    );

    let path = suzatary_google::google::CALLBACK;
    let (status, _) = redirected(&format!(
        "{base}{path}?state={}&code=4%2Fforged",
        "A".repeat(43)
    ));
    assert_eq!(status, 400, "a state the box never drew");
    assert!(serving.fake.inserted().is_empty(), "and nothing written");

    let (status, back) = redirected(&consented(&serving, &link));
    assert_eq!(status, 303);
    assert_eq!(back, format!("/#/secretary/{id}"), "back on the card");
    assert_eq!(serving.fake.inserted().len(), 1, "the event, once");
    let after = common::poll_conversation(base, &id, |_| true);
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");
    let kept: Vec<_> = serving
        .secrets
        .calls()
        .into_iter()
        .filter(|(name, _)| name == GRANT)
        .collect();
    assert_eq!(
        kept,
        vec![(GRANT.to_owned(), "calendar 1//fake-refresh-1".to_owned())],
        "the calendars' grant, without the mail"
    );
    assert_eq!(
        suzatary_connect::settings::Configuration::declared(serving.edits.as_ref())
            .expect("the recorder reads")
            .accounts,
        vec![account(sync.clone())],
        "a calendar fix leaves the account's sync as it was"
    );
    calendars_are_read(&serving, sync);
}

/// An account of `sync` holding a calendar grant has its calendars read through it, its store
/// written and its health recorded, while its mail stays where it was read; an account holding
/// no grant is not read at all.
fn calendars_are_read(serving: &Serving, sync: &Sync) {
    let grants = suzatary_io::google::Grants::new(
        suzatary_io::google::Google::new(suzatary_google::google::Endpoints::at(
            &serving.fake.base,
        )),
        Arc::clone(&serving.secrets) as _,
    );
    let unsigned = MailAccount {
        email: suzatary_mail::mail::account_email::AccountEmail::new("bo@example.test")
            .expect("a lower-case address"),
        subdir: "bo-at-example-test".to_owned(),
        sync: Sync::Gmail,
    };
    // The three fix-from-its-button scenes run as threads of one process, so a root named by the
    // process alone is one store that each removes while another appends to it.
    let scratch = suzatary_testkit::scratch("google");
    let root = scratch.path();
    let at = prodrome_core::literal::Datetime::new(2026, 10, 17, 9, 0, 0, 0).expect("an instant");
    // The unsigned account's store, written at an older schema, holds an address; it is not
    // fetched, and is emptied all the same.
    let address = format!("{}@{}", "mira", "example.invalid");
    let older = root.join(&unsigned.subdir);
    std::fs::create_dir_all(older.join("objects")).expect("a store");
    std::fs::write(older.join("schema"), "an older schema").expect("writes");
    std::fs::write(older.join("objects/kept.py"), &address).expect("writes");
    let (lines, healths, stored) = suzatary_cli::commands::google::read_calendars(
        &[account(sync.clone()), unsigned],
        &grants,
        |account| root.join(&account.subdir),
        at,
    );
    assert!(stored, "{lines:?}");
    assert_eq!(
        lines.len(),
        1,
        "only the account holding a calendar grant: {lines:?}"
    );
    assert!(
        lines[0].starts_with(&format!("google {ADA}: ")),
        "{lines:?}"
    );
    assert!(lines[0].contains("occurrences read"), "{lines:?}");
    let key = suzatary_health::health::SourceKey::GoogleCalendar(
        suzatary_mail::mail::account_email::AccountEmail::folded(&ada()),
    );
    assert!(healths.health(&key).is_some(), "its calendars' health");
    assert!(
        root.join("ada-at-example-test").exists(),
        "its store is written"
    );
    assert_eq!(
        suzatary_testkit::files_holding(root, &address),
        Vec::<std::path::PathBuf>::new(),
        "a store at another schema is emptied though its account is not fetched"
    );
    assert_eq!(
        suzatary_connect::settings::Configuration::declared(serving.edits.as_ref())
            .expect("the recorder reads")
            .accounts,
        vec![account(sync.clone())],
        "its mail stays where it was read"
    );
}

/// `propose_act` is clean (`taint`): no answer it gives, filed or refused, holds words from
/// outside the round.
#[test]
fn no_answer_of_propose_act_holds_words_from_outside_the_round() {
    let event = |extra: &str| {
        format!(
            r#"{{"verb": "propose_act", "args": {{"act": "calendar_event", "account": "{ADA}",
              "title": "Dentist", "start": "2026-10-20T14:00:00Z", "end": "2026-10-20T14:45:00Z",
              "why": "the owner asked for it", "sources": "briefing:2026-10-17"{extra}}}}}"#
        )
    };
    let refusals = [
        event(r#", "revises": 0"#),
        event(r#", "serves": "todo:life/nosuchtodo""#),
        event(""),
        r#"{"verb": "propose_act", "args": {"act": "connect", "template": "no-such-template",
          "params": [], "why": "a mistake", "sources": ""}}"#
            .to_owned(),
        r#"{"verb": "propose_act", "args": {"act": "retract", "handle": "9+9",
          "why": "a mistake", "sources": "briefing:2026-10-17"}}"#
            .to_owned(),
    ];
    let model = common::FakeModel::saying_owned([
        format!(
            r#"{{"say": "Here is the event.", "calls": [{}]}}"#,
            event("")
        ),
        r#"{"say": "Proposed."}"#.to_owned(),
        format!(
            r#"{{"say": "Trying.", "calls": [{}]}}"#,
            refusals.join(", ")
        ),
        r#"{"say": "Done."}"#.to_owned(),
    ]);
    let serving = serving("google-clean-answers", Some(model));
    let base = &serving.base;
    let id = event_proposed(base);
    let mut dismissed = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/dismiss"),
        &serde_json::json!({ "reason": format!("not {MARKER}") }),
    );
    assert_eq!(common::status(&dismissed), 200);
    let dismissed = common::body_json(&mut dismissed);
    assert_eq!(
        dismissed["proposals"][0]["state"], "dismissed",
        "{dismissed}"
    );

    // A reading whose last line is an act says `idle` before the second round begins, so the wait
    // is for a reply after the act.
    let read = common::poll_conversation(base, &id, |read| {
        let turns = read["turns"].as_array().map_or(&[][..], Vec::as_slice);
        let act = turns.iter().rposition(|turn| turn["who"] == "act");
        read["state"] == "idle"
            && act.is_some_and(|act| act + 1 < turns.len())
            && turns.last().is_some_and(|turn| turn["who"] == "model")
    });
    let answers: Vec<&serde_json::Value> = read["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["verb"] == "propose_act")
        .collect();
    assert_eq!(
        answers.len(),
        1 + refusals.len(),
        "one propose_act answer of the first round and one per refusal of the dismissal's:\n{}",
        common::told(&read)
    );
    for answer in &answers {
        assert!(!answer["text"].to_string().contains(MARKER), "{answer}");
    }
    let revised = &answers[1];
    assert_eq!(revised["outcome"]["is"], "refused", "{revised}");
}

#[test]
fn the_setup_opening_asks_for_the_client_only_while_none_is_kept() {
    let serving = serving("google-opening", None);
    // As Settings sets one up: a `SetUp` line a tab seals, whose note the box writes once it is
    // taken.
    let opening = || {
        let template = suzatary_connect::connect::TemplateId::Google;
        let id = common::opening(
            &serving.base,
            &suzatary_conversations::secretary::store::Entry::SetUp {
                at: common::next_instant(),
                title: suzatary_memory::secretary::words::Words::kept(template.template().called()),
                template,
            },
        );
        common::poll_conversation(&serving.base, &id, |read| {
            read["turns"][0]["text"].is_string()
        })["turns"][0]["text"]
            .as_str()
            .expect("the box's first line")
            .to_owned()
    };
    let client = "make the box's Google Cloud client";
    let before = opening();
    assert!(before.contains(client), "{before}");
    let _ = consent(&serving);
    let after = opening();
    assert!(!after.contains(client), "{after}");
    assert!(
        after.contains("First, tap the account's Sign in with Google card"),
        "{after}"
    );
}

#[test]
fn the_google_calendar_writes_the_calendar_plugins_schema() {
    let declared = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugin-ics/ics.schema"),
    )
    .expect("the plugin's schema");
    assert_eq!(declared, suzatary_google::google::SCHEMA);
}

/// Every sign-in that fails, abandoned, never called back, refused or holding a grant that cannot
/// read the mail, leaves the instance as it was, and the Connect card's tap writes nothing.
#[test]
fn a_lieer_account_moves_onto_the_sign_in_only_once_its_grant_reads_its_mail() {
    use suzatary_connect::settings::Configuration as _;
    let serving = serving_as("google-lieer", None, Sync::Gmail);
    let base = &serving.base;
    let declared = || {
        serving
            .edits
            .declared()
            .expect("the recorder reads")
            .accounts
    };
    assert_eq!(said_of_ada(base), "Gmail, through lieer's own grant.");
    let unchanged = |case: &str| {
        assert_eq!(
            declared(),
            vec![account(Sync::Gmail)],
            "{case}: the instance is as it was"
        );
        assert!(
            !serving
                .secrets
                .calls()
                .iter()
                .any(|(name, _)| name == GRANT),
            "{case}: no grant is kept"
        );
        assert!(
            serving.checks.checked().is_empty(),
            "{case}: nothing was read"
        );
    };
    common::asked(
        &serving.fakes,
        "connect",
        suzatary_inbox::secretary::acts::Act::Connect {
            connection: suzatary_connect::connect::Connection::parse("google", &[ADA.to_owned()])
                .expect("a Google account"),
        },
    );
    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/connect/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let tapped = common::body_json(&mut tapped);
    assert_eq!(tapped["proposals"][0]["state"], "acted", "{tapped}");
    unchanged("the Connect card's tap");

    // Abandoned: the sign-in tapped, Google's page never opened.
    let link = consent(&serving);
    unchanged("a sign-in abandoned before Google's page");
    // Google's page consented to, and its callback never reached the box.
    let callback = consented(&serving, &link);
    unchanged("a callback that never comes");
    // Google sends the browser back refused.
    let state = callback
        .split(['?', '&'])
        .find_map(|part| part.strip_prefix("state="))
        .expect("the callback carries its state");
    let path = suzatary_google::google::CALLBACK;
    let (status, _) = redirected(&format!("{base}{path}?state={state}&error=access_denied"));
    assert_eq!(status, 303);
    unchanged("a sign-in Google refused");

    // A grant that cannot read the mail is kept nowhere.
    serving.fake.refusing_mail(true);
    let (status, _) = redirected(&consented(&serving, &again(&serving)));
    assert_eq!(status, 303);
    unchanged("a grant that cannot read the mail");
    let read = setup(base);
    let ended = card(&read, "google_sign_in", "failed").expect("the card says it failed");
    assert!(
        read["proposals"][ended]["outcome"]
            .as_str()
            .is_some_and(|said| said.contains("lieer reads its mail as before")),
        "{read}"
    );

    serving.fake.refusing_mail(false);
    let (status, _) = redirected(&consented(&serving, &again(&serving)));
    assert_eq!(status, 303);
    assert_eq!(
        declared(),
        vec![account(Sync::Google)],
        "the account moved, its folder kept"
    );
    assert!(
        serving
            .secrets
            .calls()
            .iter()
            .any(|(name, _)| name == GRANT)
    );
    assert_eq!(
        serving.checks.checked().len(),
        1,
        "its calendars are read at once"
    );
    // Read with no wait after the callback answered: the callback rereads the instance file
    // before it answers, as a tap does, so the page that lands next reads the account moved.
    let said = said_of_ada(base);
    assert!(said.starts_with("Google: "), "{said}");
}

/// The sign-in card tapped again after it ended: its consent's link.
fn again(serving: &Serving) -> String {
    let base = &serving.base;
    let sign_in = card(&setup(base), "google_sign_in", "failed")
        .expect("the sign-in card ended without a grant");
    let started = tap(base, sign_in, &serde_json::json!({}));
    let path = started["proposals"][sign_in]["card"]["link"]["href"]
        .as_str()
        .expect("the card draws the consent as its button")
        .to_owned();
    let (status, link) = redirected(&format!("{base}{path}"));
    assert_eq!(status, 303, "the box sends the browser on to Google");
    link
}

/// What `GET /api/overview` says of [`ADA`]'s mail line now.
fn said_of_ada(base: &str) -> String {
    let mut page = common::get(&format!("{base}/api/overview"));
    assert_eq!(common::status(&page), 200);
    let page = common::body_json(&mut page);
    page["groups"]
        .as_array()
        .expect("a list")
        .iter()
        .filter(|group| group["group"] == "mail")
        .flat_map(|group| group["lines"].as_array().cloned().unwrap_or_default())
        .find(|line| line["key"] == format!("account:{ADA}"))
        .and_then(|line| line["detail"].as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("no line for {ADA}: {page}"))
}
