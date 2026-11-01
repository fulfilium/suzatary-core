//! What one `GET /api/overview` costs over a box where nothing changed.
//!
//! The law: a request asks the secret store nothing the last filing answered, however many
//! requests come, and a filing asks each question once more.
//!
//! The measurement, ignored: the overview over the disk stores at the live Inbox's size, about
//! 760 proposals, its secret store answering each question through a real `gpg --decrypt`, as
//! `pass` does under `secretspec`. It prints how long each part took
//! (`suzatary_core::timings`), for a cold first request and for requests over a box where nothing
//! changed, and how many questions the secret store was asked.
//!
//! `cargo test --release -p suzatary --features fixture --test all overview_cost -- --ignored
//! --nocapture`

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use suzatary_core::secretary::acts::Act;
use suzatary_core::settings::Settables;

/// How many proposals the Inbox holds, as the live one did when the overview was measured.
const PROPOSALS: usize = 760;

/// A secret store that decrypts one file with `gpg` for every question, as `pass` decrypts the
/// name's file; the floor of what `secretspec get` costs, which also starts itself and `pass`.
struct Decrypting {
    home: PathBuf,
    file: PathBuf,
    asked: AtomicUsize,
}

impl Decrypting {
    /// A key with no passphrase under `dir`, and one value encrypted to it.
    fn under(dir: &Path) -> Self {
        let home = dir.join("gnupg");
        std::fs::create_dir_all(&home).expect("a GNUPGHOME");
        let gpg = |args: &[&str]| {
            let status = std::process::Command::new("gpg")
                .env("GNUPGHOME", &home)
                .args(["--batch", "--quiet", "--yes"])
                .args(args)
                .status()
                .expect("gpg runs");
            assert!(status.success(), "gpg {args:?}");
        };
        gpg(&[
            "--passphrase",
            "",
            "--quick-gen-key",
            "Box <box@example.test>",
            "default",
            "default",
            "never",
        ]);
        let plain = dir.join("value");
        std::fs::write(&plain, "a value that is never printed\n").expect("a value");
        let file = dir.join("value.gpg");
        gpg(&[
            "--trust-model",
            "always",
            "--recipient",
            "box@example.test",
            "--output",
            file.to_str().expect("a UTF-8 path"),
            "--encrypt",
            plain.to_str().expect("a UTF-8 path"),
        ]);
        Self {
            home,
            file,
            asked: AtomicUsize::new(0),
        }
    }

    fn decrypt(&self) -> bool {
        self.asked.fetch_add(1, Ordering::SeqCst);
        std::process::Command::new("gpg")
            .env("GNUPGHOME", &self.home)
            .args(["--batch", "--quiet", "--decrypt"])
            .arg(&self.file)
            .output()
            .is_ok_and(|out| out.status.success())
    }
}

impl suzatary_core::capability::Secrets for Decrypting {
    fn secret_state(
        &self,
        _: &suzatary_core::settings::Settable,
    ) -> suzatary_core::capability::SecretState {
        if self.decrypt() {
            suzatary_core::capability::SecretState::Set
        } else {
            suzatary_core::capability::SecretState::Failed
        }
    }

    fn mail_grants(
        &self,
        _: &suzatary_core::mail::account::MailAccount,
    ) -> suzatary_core::capability::MailGrants {
        suzatary_core::capability::MailGrants {
            imap: self.decrypt(),
            smtp: false,
        }
    }

    fn google_grant(
        &self,
        _: &suzatary_core::mail::address::EmailAddress,
    ) -> suzatary_core::google::Held {
        self.decrypt();
        suzatary_core::google::Held::NotSignedIn
    }
}

/// Every part's time, as the route's phases log them, in milliseconds.
#[derive(Default, Clone)]
struct Phases(Arc<std::sync::Mutex<Vec<(String, f64)>>>);

impl tracing::Subscriber for Phases {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target() == suzatary_core::timings::TARGET
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        #[derive(Default)]
        struct Fields(Option<String>, Option<f64>);
        impl tracing::field::Visit for Fields {
            fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
                if field.name() == "ms" {
                    self.1 = Some(value);
                }
            }
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "phase" {
                    self.0 = Some(value.to_owned());
                }
            }
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let Fields(Some(phase), Some(ms)) = fields {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((phase, ms));
        }
    }

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

impl Phases {
    fn said(&self) -> String {
        std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
        .iter()
        .map(|(phase, ms)| format!("{phase}={ms:.1}ms"))
        .collect::<Vec<_>>()
        .join(" ")
    }
}

#[test]
#[ignore = "a measurement, printed, not a law"]
fn measure_the_overview_over_the_disk_stores_at_the_live_size() {
    let phases = Phases::default();
    tracing::subscriber::set_global_default(phases.clone()).expect("the only subscriber");
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let store: &'static Decrypting = Box::leak(Box::new(Decrypting::under(scratch.path())));
    let ada = common::login("ada@example.com");
    let whois = common::FakeWhois::new().naming(common::v4(), ada.clone());
    let fakes = Arc::new(common::Fakes::new("overview-disk").whois(whois));
    let started = std::time::Instant::now();
    for n in 0..PROPOSALS {
        common::asked(
            &fakes,
            "connect",
            Act::Secret {
                name: format!("A_SECRET_{n}"),
                why: "the measurement asks".to_owned(),
            },
        );
    }
    eprintln!("{PROPOSALS} proposals filed in {:?}", started.elapsed());
    let deployment = common::deployment([ada.clone()], [(ada, common::actor("ada"))]);
    let served = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .effects(move |effects| suzatary_core::capability::Server {
            secretary: suzatary_core::capability::Secretary {
                secrets: store,
                ..effects.secretary
            },
            ..effects
        })
        .settings_names(declared())
        .start();
    let filed = store.asked.swap(0, Ordering::SeqCst);
    eprintln!("the server's start asked the secret store {filed} questions");
    let _ = phases.said();
    for run in ["first", "second", "third", "fourth"] {
        let started = std::time::Instant::now();
        let mut page = common::get(&format!("{}/api/overview", served.base));
        assert_eq!(common::status(&page), 200);
        let _ = common::body_json(&mut page);
        eprintln!(
            "-- {run} request: {:?}, {} questions to the secret store; {}",
            started.elapsed(),
            store.asked.swap(0, Ordering::SeqCst),
            phases.said()
        );
    }
}

/// Every name a deployment with each channel and a calendar of two links declares.
fn declared() -> Settables {
    Settables::derive(
        suzatary_io::model::TOKEN,
        suzatary_core::connect::Channel::ALL,
        None,
        &[calendar()],
    )
}

/// A calendar plugin reading two links.
fn calendar() -> suzatary_core::plugin::Manifest {
    toml::from_str(
        "name = \"ics\"\ncomponent = \"ics.wasm\"\nschema = \"ics.schema\"\n\
         credentials = [\"PLUGIN_ICS_HOME\", \"PLUGIN_ICS_WORK\"]\n",
    )
    .expect("a manifest")
}

/// A secret store holding every name, counting the questions it is asked.
#[derive(Default)]
struct Counting(AtomicUsize);

impl suzatary_core::capability::Secrets for Counting {
    fn secret_state(
        &self,
        _: &suzatary_core::settings::Settable,
    ) -> suzatary_core::capability::SecretState {
        self.0.fetch_add(1, Ordering::SeqCst);
        suzatary_core::capability::SecretState::Set
    }

    fn mail_grants(
        &self,
        _: &suzatary_core::mail::account::MailAccount,
    ) -> suzatary_core::capability::MailGrants {
        self.0.fetch_add(1, Ordering::SeqCst);
        suzatary_core::capability::MailGrants::default()
    }

    fn google_grant(
        &self,
        _: &suzatary_core::mail::address::EmailAddress,
    ) -> suzatary_core::google::Held {
        self.0.fetch_add(1, Ordering::SeqCst);
        suzatary_core::google::Held::NotSignedIn
    }
}

/// THE OVERVIEW ASKS THE SECRET STORE ONLY AFTER A FILING: the server's start files, so the
/// requests after it ask nothing, however many come; a tapped secret files again, which asks each
/// question once more, and the requests after it again ask nothing.
#[test]
fn an_overview_asks_the_secret_store_nothing_the_last_filing_answered() {
    let store: &'static Counting = Box::leak(Box::default());
    let ada = common::login("ada@example.com");
    let whois = common::FakeWhois::new().naming(common::v4(), ada.clone());
    let fakes = Arc::new(common::Fakes::new("overview-cost").whois(whois));
    let names = declared();
    let name = names
        .names()
        .into_iter()
        .find(|name| *name != suzatary_io::model::TOKEN)
        .expect("a channel's name")
        .to_owned();
    common::asked(
        &fakes,
        "connect",
        Act::Secret {
            name,
            why: "the test asks".to_owned(),
        },
    );
    let deployment = common::deployment([ada.clone()], [(ada, common::actor("ada"))]);
    let served = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .effects(move |effects| suzatary_core::capability::Server {
            secretary: suzatary_core::capability::Secretary {
                secrets: store,
                ..effects.secretary
            },
            ..effects
        })
        .settings_names(names.clone())
        .start();
    let overview = || {
        let mut page = common::get(&format!("{}/api/overview", served.base));
        assert_eq!(common::status(&page), 200);
        common::body_json(&mut page)
    };
    let filed = store.0.swap(0, Ordering::SeqCst);
    assert!(filed >= names.names().len(), "the start filed: {filed}");
    let first = overview();
    for _ in 0..3 {
        assert_eq!(overview(), first);
    }
    assert_eq!(store.0.load(Ordering::SeqCst), 0, "nothing moved");
    let mut tapped = common::act(
        &format!(
            "{}/api/secretary/conversations/connect/proposals/0/tap",
            served.base
        ),
        &serde_json::json!({ "value": "a value never shown" }),
    );
    assert_eq!(
        common::status(&tapped),
        200,
        "{}",
        common::body_json(&mut tapped)
    );
    let refiled = store.0.swap(0, Ordering::SeqCst);
    assert!(refiled >= names.names().len(), "the tap filed: {refiled}");
    for _ in 0..3 {
        let _ = overview();
    }
    assert_eq!(
        store.0.load(Ordering::SeqCst),
        0,
        "nothing moved since the tap"
    );
}
