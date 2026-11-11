//! The junk sweep's verbs, called as the secretary calls them, over a fake Junk folder: a digest
//! asks nothing of an account or a window it refuses, and both verbs taint the round.

mod common;

use common::{FakeJunk, Fakes, JunkFolder};
use serde_json::{Value, json};
use suzatary_core::cast::CAST;
use suzatary_core::mail::junk::{ChurnList, JunkHeader, KeepList, Uid};

const CHURN: &str = "offers.example.net";
const SMALL: &str = "ada@example.org";

fn header(uid: u32, name: &str, address: &str, subject: &str) -> JunkHeader {
    JunkHeader::new(
        Uid(uid),
        Some(name),
        Some(address),
        subject,
        "Tue, 01 Sep 2026 09:00:00 +0000",
    )
}

fn fakes(tag: &str) -> Fakes {
    Fakes::new(tag).junk(FakeJunk {
        churn: ChurnList::new([CHURN], KeepList::default()).expect("a churn list"),
        folders: vec![
            (
                CAST.owner.email.to_owned(),
                JunkFolder::Holds(vec![
                    (
                        header(4, "Deals", "deals@offers.example.net", "70% off"),
                        "buy",
                    ),
                    (
                        header(9, CAST.friend.name, CAST.friend.email, "RSVP for Saturday"),
                        "Will you come to the wedding?",
                    ),
                    (
                        header(12, "Deals", "news@mail.offers.example.net", "80% off"),
                        "buy",
                    ),
                ]),
            ),
            (SMALL.to_owned(), JunkFolder::NeedsLogin),
        ],
        asked: std::sync::Mutex::default(),
    })
}

fn call(fakes: &Fakes, verb: &str, args: Value) -> Result<Value, suzatary_base::refusal::Refusal> {
    let deployment = common::deployment([], []);
    let policy = common::chain();
    let chains = [suzatary_core::secretary::Bound {
        name: "life",
        caps: fakes.secretary(),
        policy: &policy,
    }];
    let Value::Object(args) = args else {
        panic!("arguments are an object: {args}");
    };
    suzatary_core::secretary::call(
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &suzatary_base::conversation_id::DEFAULT.clone(),
        verb,
        &args,
    )
    .map(|answer| answer.value)
}

#[test]
fn a_digest_refuses_an_account_it_does_not_read_and_a_window_out_of_range() {
    let fakes = fakes("junk-refused");
    let refused = call(
        &fakes,
        "junk_digest",
        json!({"account": CAST.counsel.email}),
    )
    .expect_err("not a Junk account");
    assert!(refused.to_string().contains(CAST.owner.email), "{refused}");
    call(&fakes, "junk_digest", json!({"days": 0})).expect_err("no days");
    call(&fakes, "junk_digest", json!({"days": 400})).expect_err("more than a year");
    assert!(fakes.junk.asked.lock().expect("the log").is_empty());
}

/// Reading a Junk message taints the round, so a later `remember` becomes a card.
#[test]
fn the_junk_verbs_read_and_are_tainted() {
    use suzatary_core::secretary::surface::SecretaryVerb;
    use suzatary_core::secretary::taint::Taint;
    use suzatary_core::secretary::verb_name::Effect;
    for verb in [SecretaryVerb::JunkDigest, SecretaryVerb::JunkShow] {
        assert_eq!(verb.class().powers().effect, Effect::Reads, "{verb}");
        assert_eq!(verb.class().powers().taint, Taint::Tainted, "{verb}");
    }
}
