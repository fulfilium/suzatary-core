//! Connecting a model with the subscription, through the real tap route: with no model
//! answering, the box files the Connect card (and, beside it, the API key's); its tap runs a fake `claude setup-token` in a
//! pseudo-terminal, whose link becomes the code card's button; the code card's tap types the code
//! in and keeps the token the CLI printed as the subscription's, after its test call. No answer
//! carries the code or the token. Never a real login.

mod common;

use std::sync::{Arc, Mutex};

use common::{Fakes, Reaching, SecretRecorder, SettingsFixture, VerifyRecorder};

const CODE: &str = "fake-code#fake-state";
const TOKEN: &str = "sk-ant-oat01-FAKEtoken_for-the-route-test";
const URL: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=fake&state=s1";

/// A `claude` that draws the link `setup-token` draws and prints a token for one code, as the
/// io crate's spike's fake does.
fn fake_claude(tag: &str) -> String {
    let dir =
        std::env::temp_dir().join(format!("suzatary-test-claude-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a directory");
    suzatary_testkit::fake_program(
        &dir,
        "claude",
        &format!(
            "#!/bin/sh\n\
             [ \"$1\" = \"setup-token\" ] || exit 2\n\
             [ -t 0 ] || exit 1\n\
             printf 'Browser did not open? Use the url below to sign in\\r\\n\\r\\n{URL}\\r\\n\\r\\nPaste code here if prompted > '\n\
             read -r code\n\
             [ \"$code\" = \"{CODE}\" ] || {{ printf '\\r\\nOAuth error: Invalid code\\r\\n'; exit 1; }}\n\
             printf '\\r\\nYour OAuth token (valid for 1 year):\\r\\n\\r\\n{TOKEN}\\r\\n\\r\\nStore this token securely.\\r\\n'\n"
        ),
    )
    .display()
    .to_string()
}

/// A server whose model's last call failed, so no model answers, over `secrets` and `verify`.
fn unanswered(tag: &str, secrets: &Arc<SecretRecorder>, verify: VerifyRecorder) -> String {
    unanswered_over(tag, secrets, verify).0.base
}

/// The same server, and the fakes whose stores it writes.
fn unanswered_over(
    tag: &str,
    secrets: &Arc<SecretRecorder>,
    verify: VerifyRecorder,
) -> (common::Running, Arc<Fakes>) {
    let ada = common::login("ada@example.com");
    let whois = common::FakeWhois::new().naming(common::v4(), ada.clone());
    let deployment = common::deployment([ada.clone()], [(ada, common::actor("ada"))]);
    let model = SettingsFixture::empty().models.recorded();
    let fakes = Arc::new(Fakes::new(tag).whois(whois));
    let running = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .settings(SettingsFixture::with_writer(Arc::clone(secrets) as _))
        .reaching(Reaching {
            claude: fake_claude(tag),
            answered: Some(suzatary_core::health::Healths::one(
                &model,
                chrono::NaiveDate::from_ymd_opt(2026, 10, 7)
                    .and_then(|day| day.and_hms_opt(9, 0, 0))
                    .expect("an instant"),
                suzatary_core::health::Outcome::Failed(suzatary_core::connect::Failure::KeyRefused),
                "401: the token expired",
            )),
            verify: Arc::new(verify),
        })
        .signing_in()
        .start();
    (running, fakes)
}

/// The code card's index in the `setup` conversation: after the Connect card, the only other
/// card the box files while no model answers.
const CODE_CARD: usize = 1;

/// Tap the box's card `index` in its `setup` conversation, with `body`.
fn tap(base: &str, index: usize, body: &serde_json::Value) -> serde_json::Value {
    let setup = suzatary_core::secretary::noticed::CONVERSATION.as_str();
    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{setup}/proposals/{index}/tap"),
        body,
    );
    let status = common::status(&tapped);
    let body = common::body_json(&mut tapped);
    assert_eq!(status, 200, "{body}");
    body
}

/// The whole sign-in, and then the code and the token are in no answer, no store the box keeps
/// and no request record it pushes.
#[test]
fn with_no_model_answering_the_sign_in_keeps_the_subscription_token() {
    let secrets = Arc::new(SecretRecorder::new());
    let (running, fakes) = unanswered_over("signin-happy", &secrets, VerifyRecorder::default());
    let base = running.base.clone();

    let started = tap(&base, 0, &serde_json::json!({}));
    let connect = &started["proposals"][0];
    assert_eq!(connect["act"]["kind"], "sign_in", "{started}");
    assert_eq!(connect["state"], "started", "{connect}");

    let finished = tap(&base, CODE_CARD, &serde_json::json!({ "value": CODE }));
    let code = &finished["proposals"][CODE_CARD];
    assert_eq!(code["act"]["kind"], "sign_in_code", "{finished}");
    assert_eq!(
        code["card"]["link"]["href"], URL,
        "the button is the link the CLI drew"
    );
    assert_eq!(code["state"], "acted", "{code}");
    assert_eq!(
        finished["proposals"][0]["state"], "acted",
        "the sign-in ended with it"
    );
    assert_eq!(
        secrets.calls(),
        vec![(suzatary_io::model::TOKEN.to_owned(), TOKEN.to_owned())]
    );
    for answer in [&started, &finished] {
        let raw = answer.to_string();
        assert!(!raw.contains(TOKEN) && !raw.contains(CODE), "{raw}");
    }
    for value in [CODE, TOKEN] {
        common::never_stored(&fakes.root, value);
        let records = common::requests_dir(running.port());
        if records.is_dir() {
            common::never_stored(&records, value);
        }
    }
}

#[test]
fn a_wrong_code_keeps_nothing_and_the_connect_card_may_be_tapped_again() {
    let secrets = Arc::new(SecretRecorder::new());
    let base = unanswered("signin-wrong", &secrets, VerifyRecorder::default());
    tap(&base, 0, &serde_json::json!({}));
    let refused = tap(
        &base,
        CODE_CARD,
        &serde_json::json!({ "value": "not-the-code" }),
    );
    assert_eq!(
        refused["proposals"][CODE_CARD]["state"], "failed",
        "{refused}"
    );
    assert_eq!(
        refused["proposals"][0]["state"], "failed",
        "Connect stands again"
    );
    assert!(secrets.calls().is_empty());
    assert!(!refused.to_string().contains("not-the-code"));
}

#[test]
fn a_token_whose_test_call_fails_is_not_kept() {
    let secrets = Arc::new(SecretRecorder::new());
    let verify = VerifyRecorder {
        asked: Mutex::new(Vec::new()),
        refuses: Some(suzatary_io::model::TOKEN.to_owned()),
    };
    let base = unanswered("signin-unverified", &secrets, verify);
    tap(&base, 0, &serde_json::json!({}));
    let refused = tap(&base, CODE_CARD, &serde_json::json!({ "value": CODE }));
    let raw = refused.to_string();
    assert_eq!(refused["proposals"][CODE_CARD]["state"], "failed", "{raw}");
    assert!(raw.contains("kept nothing"), "{raw}");
    assert!(
        secrets.calls().is_empty(),
        "an untested token is never kept"
    );
    assert!(!raw.contains(TOKEN));
}

/// Four filings at once file each card once: one Connect card and one line asking for it,
/// however the filings interleave.
#[test]
fn filings_at_once_file_each_card_once() {
    let secrets = Arc::new(SecretRecorder::new());
    let base = unanswered("signin-concurrent", &secrets, VerifyRecorder::default());
    let setup = suzatary_core::secretary::noticed::CONVERSATION.as_str();
    let reading = common::reading(&base, setup, &[]);
    let kinds: Vec<&str> = reading["proposals"]
        .as_array()
        .expect("proposals")
        .iter()
        .filter_map(|card| card["act"]["kind"].as_str())
        .collect();
    assert_eq!(kinds, ["sign_in"], "{reading}");
    let asked = reading["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["who"] == "box")
        .count();
    assert_eq!(asked, 1, "one line per card: {reading}");
}
