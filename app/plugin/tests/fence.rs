//! The plugin host's fences, each tried by the fence plugin (`app/plugin-fence`) and refused by
//! its typed error, the host carrying on after every one.
//!
//! The components are built by the flake, not by this test:
//! `SUZATARY_FENCE_PLUGINS=$(nix build .#suzatary-plugin-fence --no-link --print-out-paths)`
//! names the directory holding `fence.wasm` and `escapee.wasm`, and `SUZATARY_PLUGINS`
//! (`.#suzatary-plugins`) the app's reference plugins, which are held to the same fences.
//! `scripts/merge-gate.sh` and the flake's `suzatary-plugin-fence` check set both and run this
//! target by name.
//!
//! Nothing here reaches outward: `fetch` goes to a server on 127.0.0.1 this test starts, the
//! secret store is a fake that records every name it is asked for, and no path is read but the
//! components.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use suzatary_core::capability::Secret;
use suzatary_core::plugin::{
    CredentialName, MAX_FETCHES, MAX_SAID, Manifest, Origin, Refusal, Said, admit,
};
use suzatary_io::http::Ureq;
use suzatary_io::secrets::Resolves;
use suzatary_plugin::{Budget, Effects, Host, LoadError, Run, RunError, Source};
use wasmtime::component::Component;

/// An HTTP server on 127.0.0.1 that keeps the path of every request it answers.
struct Server {
    origin: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let origin = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("its address").port()
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&seen);
        let me = origin.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                let mut header = String::from("-");
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                while reader.read_line(&mut header).is_ok_and(|read| read > 2) {
                    header.clear();
                }
                let path = line.split(' ').nth(1).unwrap_or_default().to_owned();
                let body = match path.as_str() {
                    "/items" => "alpha\nbeta\ngamma".to_owned(),
                    "/secret/s3cret" => "from the secret feed".to_owned(),
                    // The URL it was asked, as a server that echoes its request would answer.
                    "/echo/s3cret" => format!("you asked for {me}{path}"),
                    // Every day at nine since 2020, so today's window always holds some.
                    "/calendar/s3cret" => "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n\
                        UID:daily@example.test\r\nDTSTART:20200101T090000Z\r\nDURATION:PT30M\r\n\
                        RRULE:FREQ=DAILY\r\nSUMMARY:Daily\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
                        .to_owned(),
                    _ => String::new(),
                };
                kept.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(path);
                let _ = write!(
                    &stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { origin, seen }
    }

    fn seen(&self) -> Vec<String> {
        std::mem::take(&mut *self.seen.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// A secret store holding the plugin's links and one of the app's own keys, which records every
/// name it is asked for.
struct Secrets {
    values: BTreeMap<String, String>,
    asked: Mutex<Vec<String>>,
}

impl Secrets {
    fn asked(&self) -> Vec<String> {
        std::mem::take(&mut *self.asked.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Resolves for Secrets {
    fn get(
        &self,
        name: &str,
        _reason: &str,
    ) -> Result<Option<Secret>, suzatary_io::secrets::SecretError> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(name.to_owned());
        Ok(self.values.get(name).map(Secret::new))
    }
}

fn read(variable: &str, name: &str) -> Vec<u8> {
    let directory = std::env::var_os(variable).unwrap_or_else(|| {
        panic!("{variable} is not set: scripts/merge-gate.sh passes the flake's components in")
    });
    let path = PathBuf::from(directory).join(name);
    std::fs::read(&path).unwrap_or_else(|failed| panic!("{}: {failed}", path.display()))
}

/// The fence plugin's schema; its items are never stored, so only admission reads it.
const FENCE_SCHEMA: &str = "Schema(key='line', events=(Event(name='Said', fields=(Field(name='line', \
    type=Text()), Field(name='at', type=Instant()), Field(name='actor', type=Text()))),), \
    registers=(Register(name='line', order=Discrete(), inflationary=False, \
    writes=(Write(event='Said', field='line'),)),))";

fn manifest(name: &str, origin: &str, credentials: &[&str]) -> Manifest {
    let credentials: Vec<String> = credentials.iter().map(|one| format!("{one:?}")).collect();
    toml::from_str(&format!(
        "name = \"{name}\"\ncomponent = \"{name}.wasm\"\nschema = \"{name}.schema\"\n\
         origins = [\"{origin}\"]\ncredentials = [{}]\n",
        credentials.join(", ")
    ))
    .expect("a manifest")
}

fn host() -> Host {
    Host::new(Budget {
        time: Duration::from_secs(2),
        memory: 32 * 1024 * 1024,
    })
    .expect("the host starts")
}

fn effects(secrets: &Arc<Secrets>) -> Effects {
    Effects {
        http: Arc::new(Ureq::new()),
        secrets: Arc::clone(secrets) as Arc<dyn Resolves + Send + Sync>,
    }
}

/// One host, one server and the fence plugin loaded against a manifest allowing that server.
struct Fixture {
    server: Server,
    secrets: Arc<Secrets>,
    source: Source,
}

const FENCE: [&str; 3] = [
    "PLUGIN_FENCE_FEED",
    "PLUGIN_FENCE_ECHO",
    "PLUGIN_FENCE_UNSET",
];

impl Fixture {
    fn new() -> Self {
        let server = Server::start();
        let secrets = Arc::new(Secrets {
            values: BTreeMap::from([
                (
                    "PLUGIN_FENCE_FEED".to_owned(),
                    format!("{}/secret/s3cret", server.origin),
                ),
                (
                    "PLUGIN_FENCE_ECHO".to_owned(),
                    format!("{}/echo/s3cret", server.origin),
                ),
                (
                    "CLAUDE_CODE_OAUTH_TOKEN".to_owned(),
                    "the-app-s-own-key".to_owned(),
                ),
            ]),
            asked: Mutex::new(Vec::new()),
        });
        let source = host()
            .load(
                manifest("fence", &server.origin, &FENCE),
                &read("SUZATARY_FENCE_PLUGINS", "fence.wasm"),
                FENCE_SCHEMA,
            )
            .expect("the fence plugin is a source");
        Self {
            server,
            secrets,
            source,
        }
    }

    fn sync(&self, cursor: &str) -> Run {
        self.source
            .sync(&effects(&self.secrets), Some(cursor.as_bytes()))
    }

    /// The honest path still works: what a stopped or refused call left behind did not.
    fn still_answers(&self) {
        let run = self.sync(&format!("get {}/items", self.server.origin));
        assert_eq!(run.result.map(|synced| synced.items.len()), Ok(3));
        self.server.seen();
    }
}

fn named(name: &str) -> CredentialName {
    manifest("fence", "https://a.example", &FENCE)
        .declared(name)
        .expect("declared")
        .clone()
}

/// The grant's law: a request reaches only an origin the manifest allows, a credential is read
/// only when the manifest declares it and goes nowhere but its own URL, and an answer carrying
/// its value is withheld.
#[test]
fn each_request_reaches_only_what_the_grant_allows() {
    let fixture = Fixture::new();
    let origin = &fixture.server.origin;
    let port = origin.rsplit(':').next().expect("a port");
    let outside = |text: &str| {
        Err(Refusal::OriginNotAllowed(
            Origin::parse(text).expect("an origin"),
        ))
    };
    // Each case: the cursor, what the fetch came to, the paths the server saw, and the names the
    // secret store was asked for.
    let refused = |refusal: Refusal| Err::<u16, _>(refusal);
    let cases = [
        (
            format!("get {origin}/items"),
            Ok(200),
            &["/items"][..],
            &[][..],
        ),
        (
            "get https://example.com/".to_owned(),
            outside("https://example.com"),
            &[],
            &[],
        ),
        (
            format!("get http://localhost:{port}/items"),
            outside(&format!("http://localhost:{port}")),
            &[],
            &[],
        ),
        (
            format!("get {origin}@example.com/"),
            refused(Refusal::BadUrl(Said::clip(format!(
                "{origin}@example.com/"
            )))),
            &[],
            &[],
        ),
        (
            "credential CLAUDE_CODE_OAUTH_TOKEN".to_owned(),
            refused(Refusal::CredentialNotDeclared(Said::clip(
                "CLAUDE_CODE_OAUTH_TOKEN",
            ))),
            &[],
            &[],
        ),
        (
            "credential PLUGIN_FENCE_FEED".to_owned(),
            Ok(200),
            &["/secret/s3cret"],
            &["PLUGIN_FENCE_FEED"],
        ),
        (
            "credential PLUGIN_FENCE_UNSET".to_owned(),
            refused(Refusal::CredentialUnset(named("PLUGIN_FENCE_UNSET"))),
            &[],
            &["PLUGIN_FENCE_UNSET"],
        ),
        (
            "credential PLUGIN_FENCE_ECHO".to_owned(),
            refused(Refusal::Echoed(named("PLUGIN_FENCE_ECHO"))),
            &["/echo/s3cret"],
            &["PLUGIN_FENCE_ECHO"],
        ),
    ];
    for (cursor, outcome, seen, asked) in cases {
        let run = fixture.sync(&cursor);
        assert_eq!(
            run.fetches
                .iter()
                .map(|fetch| fetch.outcome.clone())
                .collect::<Vec<_>>(),
            std::slice::from_ref(&outcome),
            "{cursor}"
        );
        match outcome {
            Ok(_) => assert!(run.result.is_ok(), "{cursor}: {:?}", run.result),
            Err(refusal) => assert_eq!(
                run.result,
                Err(RunError::Plugin(Said::clip(refusal.to_string()))),
                "{cursor}: the plugin is told why, in the host's words"
            ),
        }
        assert_eq!(fixture.server.seen(), seen, "{cursor}");
        assert_eq!(fixture.secrets.asked(), asked, "{cursor}");
        assert!(
            !format!("{run:?}").contains("s3cret"),
            "{cursor}: no value in the run"
        );
    }
}

/// The plugin is lent the names its manifest declares, and lending a name reads no value.
#[test]
fn a_plugin_is_lent_the_names_it_declares_and_no_value() {
    let fixture = Fixture::new();
    let lent = fixture.sync("lent").result.expect("it answers").items;
    let mut lent: Vec<String> = lent
        .into_iter()
        .map(|name| String::from_utf8(name).expect("text"))
        .collect();
    lent.sort();
    assert_eq!(
        lent,
        [
            "PLUGIN_FENCE_ECHO",
            "PLUGIN_FENCE_FEED",
            "PLUGIN_FENCE_UNSET"
        ]
    );
    assert!(
        fixture.secrets.asked().is_empty(),
        "lending a name asks the store nothing"
    );
}

/// A call past its time or its memory, or one that traps, is stopped by its typed error, and the
/// host answers the next call.
#[test]
fn a_call_past_its_budget_is_stopped_and_the_host_carries_on() {
    let fixture = Fixture::new();
    for mode in ["loop", "sleep"] {
        let started = Instant::now();
        assert_eq!(fixture.sync(mode).result, Err(RunError::Deadline), "{mode}");
        let took = started.elapsed();
        assert!(
            took >= Duration::from_secs(2) && took < Duration::from_secs(10),
            "{mode}: {took:?}"
        );
        fixture.still_answers();
    }
    assert_eq!(fixture.sync("grow").result, Err(RunError::Memory));
    fixture.still_answers();
    let trapped = fixture.sync("panic").result;
    assert!(matches!(trapped, Err(RunError::Trapped(_))), "{trapped:?}");
    fixture.still_answers();
}

/// A call makes at most `MAX_FETCHES` fetches; every one past it is refused unsent.
#[test]
fn a_flood_of_fetches_is_cut_at_the_cap() {
    let fixture = Fixture::new();
    let run = fixture.sync(&format!("flood {}/items", fixture.server.origin));
    let sent = run.result.expect("it answers").items;
    assert_eq!(sent, [MAX_FETCHES.to_string().into_bytes()]);
    assert_eq!(fixture.server.seen().len(), MAX_FETCHES);
    assert!(
        run.fetches[MAX_FETCHES..]
            .iter()
            .all(|fetch| fetch.outcome == Err(Refusal::TooManyFetches))
    );
}

/// Whatever a plugin says, at whatever door, the call the host keeps holds a bound's worth of it.
#[test]
fn a_megabyte_said_at_every_door_is_kept_bounded() {
    let fixture = Fixture::new();
    let run = fixture.sync("megabyte");
    let Err(RunError::Plugin(said)) = &run.result else {
        panic!("{:?}", run.result);
    };
    assert!(said.as_str().len() <= MAX_SAID);
    assert_eq!(run.fetches.len(), 2);
    assert!(run.fetches.iter().all(|fetch| fetch.credential.is_none()));
    assert!(
        format!("{run:?}").len() < 8 * MAX_SAID,
        "the whole call as the host keeps it"
    );
    fixture.still_answers();
}

#[test]
fn a_component_importing_a_filesystem_or_a_network_is_never_loaded() {
    let refused = host()
        .load(
            manifest("fence", "http://127.0.0.1:9", &[]),
            &read("SUZATARY_FENCE_PLUGINS", "escapee.wasm"),
            FENCE_SCHEMA,
        )
        .map(|_| ())
        .expect_err("the escapee imports what the host does not link");
    assert!(
        matches!(&refused, LoadError::NotASource(why) if why.contains("wasi:filesystem/preopens")),
        "{refused:?}"
    );
}

/// The calendar is a source like any other: it imports no WASI filesystem or socket, its links
/// are read through the host by name, an unset one is refused and recorded, every item it emits
/// is one the store admits, and a second sync of the same feed reports nothing.
#[test]
fn the_calendar_plugin_holds_to_the_same_fences() {
    let server = Server::start();
    let manifest = manifest(
        "ics",
        &server.origin,
        &["PLUGIN_ICS_HOME", "PLUGIN_ICS_UNSET"],
    );
    let secrets = Arc::new(Secrets {
        values: BTreeMap::from([
            (
                "PLUGIN_ICS_HOME".to_owned(),
                format!("{}/calendar/s3cret", server.origin),
            ),
            (
                "CLAUDE_CODE_OAUTH_TOKEN".to_owned(),
                "the-app-s-own-key".to_owned(),
            ),
        ]),
        asked: Mutex::new(Vec::new()),
    });
    let bytes = read("SUZATARY_PLUGINS", "ics.wasm");
    let engine = wasmtime::Engine::default();
    let imports: Vec<String> = Component::from_binary(&engine, &bytes)
        .expect("a component")
        .component_type()
        .imports(&engine)
        .map(|(name, _)| name.to_owned())
        .collect();
    assert!(
        imports
            .iter()
            .all(|name| name.starts_with("suzatary:plugin/fetch@")
                || ["wasi:io/", "wasi:clocks/", "wasi:random/", "wasi:cli/"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix))),
        "{imports:?}"
    );
    let schema = String::from_utf8(read("SUZATARY_PLUGINS", "ics.schema")).expect("text");
    let source = host()
        .load(manifest.clone(), &bytes, &schema)
        .expect("the calendar is a source");
    let run = source.sync(&effects(&secrets), None);
    assert!(
        !format!("{run:?}").contains("s3cret"),
        "the link is in no run"
    );
    let unset = CredentialName::parse(manifest.name(), "PLUGIN_ICS_UNSET").expect("a name");
    assert_eq!(
        run.fetches
            .iter()
            .map(|fetch| (fetch.credential.clone(), fetch.outcome.clone()))
            .collect::<Vec<_>>(),
        [
            (manifest.declared("PLUGIN_ICS_HOME").cloned(), Ok(200)),
            (Some(unset.clone()), Err(Refusal::CredentialUnset(unset))),
        ],
        "the host's own record of each link"
    );
    let synced = run.result.expect("the calendar answers");
    assert!(synced.items.len() >= 60, "a daily event over the window");
    for item in &synced.items {
        let event = admit(source.schema(), manifest.name(), item)
            .unwrap_or_else(|refused| panic!("{refused}"));
        suzatary_core::calendar::Occurrence::of(&event)
            .unwrap_or_else(|refused| panic!("{refused}"));
    }
    let mut asked = secrets.asked();
    asked.sort();
    assert_eq!(
        asked,
        ["PLUGIN_ICS_HOME", "PLUGIN_ICS_UNSET"],
        "never the app's own key"
    );
    assert_eq!(server.seen(), ["/calendar/s3cret"]);
    let again = source.sync(&effects(&secrets), Some(&synced.cursor));
    assert_eq!(
        again.result.map(|synced| synced.items.len()),
        Ok(0),
        "an unchanged feed"
    );
}
