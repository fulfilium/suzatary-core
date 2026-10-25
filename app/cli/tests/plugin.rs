//! `suzatary plugin sync` over a loopback feed: the reference calendar component, the host, the
//! store and the ledger, everything but the transport and the secret store the box's own.
//!
//! Run by name with `SUZATARY_PLUGINS` naming `nix build .#suzatary-plugins`'s output, as
//! `scripts/merge-gate.sh` does; the production transport refuses loopback by design, so the
//! test lends the verb's body a transport that reaches the server it starts, by struct update.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use suzatary_cli::commands::plugin;
use suzatary_cli::instance::Plugins;
use suzatary_core::calendar::{Calendar, Status};
use suzatary_core::capability::Secret;
use suzatary_core::plugin::{Came, Manifest};
use suzatary_core::store::Prodrome;
use suzatary_io::http::Ureq;
use suzatary_io::plugin_store::PluginStore;
use suzatary_io::secrets::Resolves;
use suzatary_plugin::{Budget, Effects, Host};

/// A feed whose body the test changes between syncs.
struct Feed {
    origin: String,
    body: Arc<Mutex<String>>,
}

impl Feed {
    fn serve(body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let origin = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let body = Arc::new(Mutex::new(body.to_owned()));
        let served = Arc::clone(&body);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|read| read > 0) && line.trim() != "" {
                    line.clear();
                }
                let body = served
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                let _ = write!(
                    &stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/calendar\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { origin, body }
    }

    fn set(&self, body: &str) {
        body.clone_into(&mut self.body.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

/// The calendar's link, set; nothing else is.
struct Links(BTreeMap<String, String>);

impl Resolves for Links {
    fn get(
        &self,
        name: &str,
        _reason: &str,
    ) -> Result<Option<Secret>, suzatary_io::secrets::SecretError> {
        Ok(self.0.get(name).map(Secret::new))
    }
}

/// Every day at nine since 2020, and a meeting today that the second feed cancels.
fn calendar(standup: &str) -> String {
    let today = chrono::Utc::now().date_naive().format("%Y%m%d");
    format!(
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:daily@example.test\r\nDTSTART:20200101T090000Z\r\n\
         DURATION:PT30M\r\nRRULE:FREQ=DAILY\r\nSUMMARY:Daily\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:review@example.test\r\nDTSTART:{today}T150000Z\r\nDTEND:{today}T160000Z\r\n\
         SUMMARY:Review\r\nSTATUS:{standup}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

fn components() -> PathBuf {
    std::env::var_os("SUZATARY_PLUGINS").map_or_else(
        || panic!("SUZATARY_PLUGINS is not set: it names `nix build .#suzatary-plugins`'s output"),
        PathBuf::from,
    )
}

fn manifest(origin: &str) -> Manifest {
    toml::from_str(&format!(
        r#"
        name = "ics"
        component = "ics.wasm"
        schema = "ics.schema"
        origins = ["{origin}"]
        credentials = ["PLUGIN_ICS_HOME", "PLUGIN_ICS_WORK"]
        "#
    ))
    .expect("the calendar's manifest")
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|entry| {
                    entry
                        .expect("an entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn a_sync_learns_once_keeps_its_ledger_and_reads_a_cancellation() {
    let feed = Feed::serve(&calendar("CONFIRMED"));
    let scratch = tempfile::tempdir().expect("a scratch dir");
    let manifest = manifest(&feed.origin);
    let plugins = Plugins {
        manifests: vec![manifest.clone()],
        cache: scratch.path().join("cache"),
    };
    let effects = Effects {
        http: Arc::new(Ureq::new()),
        secrets: Arc::new(Links(BTreeMap::from([(
            "PLUGIN_ICS_HOME".to_owned(),
            format!("{}/calendar/private-s3cret/basic.ics", feed.origin),
        )]))),
    };
    let host = Host::new(Budget::default()).expect("the host");
    let sync = || plugin::sync(&host, &manifest, &components(), &plugins, &effects);

    let learnt = |report: &plugin::Report| report.learnt.expect("stored");
    let first = sync();
    assert!(learnt(&first).written >= 60, "{}", first.line);
    assert!(first.line.starts_with("plugin ics: "), "{}", first.line);
    assert!(first.line.contains("PLUGIN_ICS_HOME 200"), "{}", first.line);
    assert!(
        first
            .line
            .contains("PLUGIN_ICS_WORK refused (PLUGIN_ICS_WORK is declared and not set)"),
        "{}",
        first.line
    );
    assert!(!first.line.contains("s3cret"), "the link is never said");
    assert_eq!(
        files(&plugins.cache.join("plugins/ics/store")),
        [".gitattributes", ".lock", "objects", "schema", "store.lock"]
    );
    assert_eq!(
        files(&plugins.cache.join("plugins/ics")),
        ["cursor", "ledger.json", "store"]
    );
    let ledger = plugin::ledger(&plugins, manifest.name()).expect("the ledger reads");
    let synced = |link: &str| {
        ledger
            .links
            .health(link)
            .and_then(suzatary_core::mail::health::Health::succeeded)
    };
    assert!(synced("PLUGIN_ICS_HOME").is_some());
    assert_eq!(synced("PLUGIN_ICS_WORK"), None);
    assert!(
        !std::fs::read_to_string(plugins.cache.join("plugins/ics/ledger.json"))
            .expect("a ledger")
            .contains("s3cret")
    );

    let again = sync();
    assert_eq!(
        (learnt(&again).written, learnt(&again).known),
        (0, 0),
        "an unchanged feed reports nothing: {}",
        again.line
    );

    feed.set(&calendar("CANCELLED"));
    let cancelled = sync();
    assert_eq!(learnt(&cancelled).written, 1, "{}", cancelled.line);
    let store = PluginStore::at(plugins.store(manifest.name()))
        .read()
        .expect("reads")
        .expect("a store");
    let read = Calendar::of(&store.nodes().expect("reads"));
    let today = chrono::Utc::now().date_naive();
    let days = read.days(today, 1, &|utc| utc);
    let review = days[0]
        .entries
        .iter()
        .find(|entry| entry.content.title() == "Review")
        .expect("today's review");
    assert_eq!(review.content.status(), Status::Cancelled);

    std::fs::write(plugins.cache.join("plugins/ics/cursor"), b"not a cursor").expect("writes");
    let restarted = sync();
    assert_eq!(
        learnt(&restarted).written,
        0,
        "a cursor the plugin cannot read starts it afresh, and the store learns nothing new: {}",
        restarted.line
    );

    // A cursor that is there and cannot be read fails the run, and the ledger says so, rather
    // than reading as none and reporting the whole window again.
    let cursor = plugins.cache.join("plugins/ics/cursor");
    std::fs::remove_file(&cursor).expect("removed");
    std::fs::create_dir(&cursor).expect("a directory where the cursor belongs");
    let unread = sync();
    assert!(unread.learnt.is_none(), "{}", unread.line);
    assert!(
        unread.line.contains("its cursor could not be read"),
        "{}",
        unread.line
    );
    let last = plugin::ledger(&plugins, manifest.name())
        .expect("the ledger reads")
        .last
        .expect("the run is recorded");
    assert!(
        matches!(&last.came, Came::Failed { why } if why.contains("its cursor could not be read")),
        "{last:?}"
    );
}
