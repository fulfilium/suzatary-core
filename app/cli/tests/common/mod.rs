//! The shared test harness: fake capabilities, a real server per test over a real
//! socket (the gate reads `ConnectInfo`), and a six-todo fixture chain.
//!
//! Every loopback dial arrives as `127.0.0.1` or `::1`, and the gate caches a whois
//! answer per address process-wide, so each test file commits `v4` and `v6` to one
//! fixed resolution for its lifetime.

#![allow(dead_code)]

pub mod fake_google;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Duration;
use prodrome_core::event::{Actor, mk_completed, mk_created};
use prodrome_core::fpl::{self, mk_decay, mk_flat};
use prodrome_core::policy::Untrusted;
use suzatary_cli::serve::{self, Chained, State};
use suzatary_core::capability::{
    AnswerError, Answers, Clock, Conversation, Corpus, CorpusError, Datetime, Delivered, Halt,
    Html, Login, MailError, Mailbox, Manual, Manuals, Model, ModelError, Notify, NotifyError, Push,
    Query, Render, RenderError, Reply, Repository, RepositoryError, Session, SessionId,
    SessionModel, Sessioned, Sessions, Turn, Whois, WhoisError,
};
pub use suzatary_core::cast::{self, CAST};
use suzatary_core::deployment::Deployment;
use suzatary_core::escape::Escaped;
use suzatary_core::file_read::FileRead;
use suzatary_core::mail::state::Decision;
use suzatary_core::mail::verbs::{Marked, Message, NoiseAdded, Show, Summary, SyncReport};
use suzatary_core::mail::{AttachmentMeta, MailMessage, MailRow};
use suzatary_core::record::{EventStore, SubTodo, mk_authored, mk_source, mk_subtodo};
use suzatary_io::briefings::BriefingFiles;
use suzatary_io::conversations::ConversationStores as Conversations;
use suzatary_io::devices::Devices;

/// The one IPv4 identity every real connection from this process presents as.
#[must_use]
pub fn v4() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

/// The one IPv6 identity every real connection from this process presents as.
#[must_use]
pub fn v6() -> IpAddr {
    IpAddr::V6(Ipv6Addr::LOCALHOST)
}

/// Both loopback addresses, for the fake daemon to name as this box's own node: a server built
/// with them reads as the box does, by whois like every peer, and never writes.
#[must_use]
pub fn loopback_is_this_box() -> Vec<IpAddr> {
    vec![v4(), v6()]
}

/// A `Login`, minted the way a `Whois` fake may.
#[must_use]
pub fn login(text: &str) -> Login {
    Login::from_whois(text)
}

/// A roster name.
#[must_use]
pub fn actor(name: &str) -> Actor {
    Actor::new(name).expect("a roster name")
}

/// The fixture's one chain, with its roster and trust policy.
///
/// One, because the fakes hold one store and a second chain's policy would
/// have none behind it.
#[must_use]
pub fn chain() -> suzatary_core::chains::ChainPolicy {
    suzatary_core::chains::ChainPolicy {
        name: suzatary_core::chains::ChainName::new("life").expect("a slug"),
        untrusted: Untrusted::of([actor("triage")]),
        roster: [actor(CAST.owner.actor), actor("triage")]
            .into_iter()
            .collect(),
        root: suzatary_core::cast::keys::root(),
    }
}

/// A deployment with the fixture's roster and category vocabulary, differing
/// from test to test only in who may view and who may act.
#[must_use]
pub fn deployment(
    viewers: impl IntoIterator<Item = Login>,
    actors: impl IntoIterator<Item = (Login, Actor)>,
) -> Deployment {
    Deployment {
        viewers: viewers.into_iter().collect(),
        actors: actors.into_iter().collect(),
        chains: suzatary_core::chains::Chains::of(vec![chain()]).expect("one chain"),
        categories: [
            ("work".to_owned(), 1),
            ("home".to_owned(), 2),
            ("other".to_owned(), 3),
            ("study".to_owned(), 4),
        ]
        .into_iter()
        .collect(),
        app_url: "https://example.test".to_owned(),
        // A scene needing a settable builds its own, so no other test gets one free.
        coding: None,
        zone: suzatary_core::zone::Zone::UTC,
    }
}

/// `store` begun with a nonce of zeros, so what a test writes is named the same every run.
pub fn begun(store: &EventStore) {
    let genesis = suzatary_core::record::Envelope::Genesis(
        prodrome_core::genesis::mk_genesis(suzatary_core::record::GENESIS, &"0".repeat(32))
            .expect("a genesis"),
    );
    let name = prodrome_core::event::seal_hash(&genesis);
    store
        .adopt_objects(
            &std::collections::BTreeMap::from([(
                name.clone(),
                prodrome_core::event::canonical_envelope(&genesis),
            )]),
            &name,
        )
        .expect("begins");
}

/// A tab's replica of `store`: the Prodrome's `MemoryStore` holding the box's objects, which
/// appends byte for byte what a store holding them appends.
#[must_use]
pub fn tab(
    store: &dyn suzatary_core::store::Prodrome<suzatary_core::record::TodoEvent>,
) -> prodrome_core::store::MemoryStore<suzatary_core::record::TodoEvent> {
    use prodrome_core::store::Replica as _;
    let prints: std::collections::BTreeMap<prodrome_core::event::Hash, String> = store
        .objects()
        .expect("the box's objects read")
        .iter()
        .map(|(name, object)| {
            (
                name.clone(),
                prodrome_core::event::canonical_envelope(object),
            )
        })
        .collect();
    let tab = prodrome_core::store::MemoryStore::default();
    tab.receive(prints.keys().cloned().collect(), &|name| {
        prints.get(name).map(|print| print.clone().into_bytes())
    })
    .expect("the box's objects verify");
    tab
}

/// What a tab does: `event` appended to `tab` as the `Change` its append answers.
pub fn acted(
    tab: &prodrome_core::store::MemoryStore<suzatary_core::record::TodoEvent>,
    event: suzatary_core::record::TodoEvent,
) -> suzatary_core::record::Envelope {
    use prodrome_core::store::Replica as _;
    let name = tab.append(event).expect("the tab appends");
    let print = String::from_utf8(tab.print(&name).expect("held")).expect("utf8");
    prodrome_core::event::parse_envelope(&suzatary_core::record::TodoVocabulary::default(), &print)
        .expect("the tab printed it")
}

/// One act a fresh tab of `store` writes.
pub fn written(
    store: &dyn suzatary_core::store::Prodrome<suzatary_core::record::TodoEvent>,
    event: suzatary_core::record::TodoEvent,
) -> suzatary_core::record::Envelope {
    acted(&tab(store), event)
}

/// What a scene did, held against `tests/pins/<name>.pin`; `SUZATARY_UPDATE_PINS=1` rewrites
/// the pin, and a change to one is a change to what a conversation stores or a model is sent.
pub fn pinned(name: &str, got: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/pins")
        .join(format!("{name}.pin"));
    if std::env::var_os("SUZATARY_UPDATE_PINS").is_some() {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the pins dir");
        std::fs::write(&path, got).expect("the pin writes");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|failed| {
        panic!(
            "no pin at {}: {failed}; SUZATARY_UPDATE_PINS=1 writes it",
            path.display()
        )
    });
    assert!(
        got == want,
        "{name} moved from its pin:\n--- pinned\n{want}\n--- now\n{got}"
    );
}

/// A scene's record: the conversation's lines, one JSON object each, then what the model was
/// shown and sent.
#[must_use]
pub fn scene(caps: &Fakes, id: &suzatary_core::secretary::store::ConversationId) -> String {
    let lines: String =
        suzatary_core::secretary::transcript::lines(caps.conversations.conversations(), id)
            .unwrap_or_default()
            .iter()
            .map(|entry| serde_json::to_string(entry).expect("an entry is plain data") + "\n")
            .collect();
    format!(
        "# lines\n{lines}# shown\n{}\n# asked\n{:#?}\n# opened\n{}\n# resumed with\n{:#?}\n",
        caps.model
            .shown()
            .iter()
            .map(|names| names.join(" "))
            .collect::<Vec<_>>()
            .join("\n"),
        caps.model.asked(),
        caps.model.opened().len(),
        caps.model.resumed_with(),
    )
}

/// The fixed instant the six-todo chain is authored around, so every price is
/// reproducible.
#[must_use]
pub fn now() -> Datetime {
    Datetime::new(2026, 9, 2, 12, 0, 0, 0).expect("a real instant")
}

/// `at` shifted by whole days, through the Prodrome's own conversion.
#[must_use]
pub fn shift(at: Datetime, days: i64) -> Datetime {
    fpl::datetime_of(fpl::instant_of(at) + Duration::days(days)).expect("in range")
}

/// A scratch directory unique to this process, thread and call.
///
/// The counter, not the thread id, guarantees uniqueness: std does not promise
/// never to reuse a `ThreadId`, and an earlier test's server may still be reading.
#[must_use]
pub fn scratch(name: &str) -> PathBuf {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "suzatary-cli-tests-{name}-{}-{:?}-{call}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// Six todos, each carrying one thing a caller has to get right, with prose from
/// `CAST`:
///
/// 1. `urgent` — a Decay two days from its deadline, body carrying typst
///    markup (`*permit*`).
/// 2. `flat` — priced in the middle.
/// 3. `checklist` — refers to `flat` via `#link("todo:book-flight")`, so
///    `view::references` has something to resolve.
/// 4. `resolved` — authored, then completed the next day.
/// 5. `high`.
/// 6. `agents` — authored by `triage` (untrusted), body carrying markup too,
///    sourced from a message so the mail-reverse-index has something to find.
///
/// `born` is strictly before every `Authored`, because `chronological` breaks
/// an `at` tie by canonical print and `"Authored("` sorts before `"Created("`.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "six todos, each priced in full rather than through a shared \
              builder, so a reader can see exactly what each one carries"
)]
pub fn six_todo_chain(root: &Path) -> EventStore {
    let untrusted: Untrusted = Untrusted::of([actor("triage")]);
    let events = EventStore::new(root.join("events"), untrusted);
    let now = now();
    let born = shift(now, -30);
    let made = shift(now, -20);
    let owner = CAST.owner.actor;
    let todos = &CAST.todos;
    begun(&events);

    let append = |event| {
        suzatary_core::store::append_to(&events, suzatary_core::record::GENESIS, event)
            .expect("appends");
    };
    let authored = |todo: &cast::Todo, at, actor, spec, source| {
        let subtodos: Vec<SubTodo> = todo
            .subtodos
            .iter()
            .map(|sub| mk_subtodo(sub.text, sub.done).expect("valid"))
            .collect();
        mk_authored(
            todo.id,
            at,
            actor,
            "todo",
            made,
            suzatary_core::record::MarkupSource::from_chain(todo.body),
            spec,
            todo.rationale.iter().map(|&line| line.to_owned()).collect(),
            todo.category,
            "",
            suzatary_core::record::MarkupSource::from_chain(todo.detail),
            source,
            subtodos,
            Vec::new(),
            "",
        )
        .expect("valid")
    };
    for todo in todos.all() {
        append(mk_created(todo.id, born, owner, todo.created, "").expect("valid"));
        match todo.id {
            id if id == todos.urgent.id => append(authored(
                todo,
                made,
                owner,
                mk_decay(
                    0.6,
                    0.05,
                    fpl::instant_of(shift(now, 2)),
                    Duration::days(14),
                    Some(fpl::instant_of(shift(now, -12))),
                )
                .expect("valid"),
                None,
            )),
            id if id == todos.flat.id => {
                append(authored(
                    todo,
                    made,
                    owner,
                    mk_flat(0.55).expect("valid"),
                    None,
                ));
            }
            id if id == todos.checklist.id => {
                append(authored(
                    todo,
                    made,
                    owner,
                    mk_flat(0.72).expect("valid"),
                    None,
                ));
            }
            id if id == todos.resolved.id => {
                append(authored(
                    todo,
                    made,
                    owner,
                    mk_flat(0.4).expect("valid"),
                    None,
                ));
                append(mk_completed(todo.id, shift(made, 1), owner, "").expect("valid"));
            }
            id if id == todos.high.id => {
                append(authored(
                    todo,
                    made,
                    owner,
                    mk_flat(0.88).expect("valid"),
                    None,
                ));
            }
            _ => append(authored(
                todo,
                shift(made, 2),
                "triage",
                mk_flat(0.66).expect("valid"),
                Some(mk_source(
                    CAST.mail[0].from.email,
                    CAST.mail[0].subject,
                    made,
                    AGENTS_TODO_MESSAGE_HASH,
                    "",
                    "",
                )),
            )),
        }
    }
    events
}

/// The source hash the agent's todo was authored from.
pub const AGENTS_TODO_MESSAGE_HASH: &str =
    "e2e0000message0000hash0000000000000000000000000000000000000000";

/// A settable address-to-login table, plus addresses that answer
/// `Err(Unreachable)`.
#[derive(Default)]
pub struct FakeWhois {
    named: BTreeMap<IpAddr, Login>,
    unreachable: BTreeSet<IpAddr>,
    this_box: BTreeSet<IpAddr>,
    asked: Mutex<Vec<IpAddr>>,
}

impl FakeWhois {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This address resolves to this login.
    #[must_use]
    pub fn naming(mut self, address: IpAddr, who: Login) -> Self {
        self.named.insert(address, who);
        self
    }

    /// This address is this box's own on the tailnet.
    #[must_use]
    pub fn this_box_at(mut self, address: IpAddr) -> Self {
        self.this_box.insert(address);
        self
    }

    /// This address answers `Err(Unreachable)`, as a dead daemon does.
    #[must_use]
    pub fn unreachable_at(mut self, address: IpAddr) -> Self {
        self.unreachable.insert(address);
        self
    }

    /// Every address asked, in order.
    #[must_use]
    pub fn asked(&self) -> Vec<IpAddr> {
        self.asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// A daemon that names `at` as this box's own node and asks `inner` of every other address.
struct ThisBoxAt {
    inner: &'static dyn Whois,
    at: Vec<IpAddr>,
}

impl Whois for ThisBoxAt {
    fn peer(&self, address: IpAddr) -> Result<suzatary_core::capability::Peer, WhoisError> {
        if self.at.contains(&address) {
            return Ok(suzatary_core::capability::Peer::ThisBox);
        }
        self.inner.peer(address)
    }
}

/// A listener on a port of `ip` nothing else holds, and where it is.
/// `router` served on `listener` until the test binary exits.
fn served(listener: std::net::TcpListener, router: axum::Router) {
    let service = router.into_make_service_with_connect_info::<SocketAddr>();
    runtime().spawn(async move {
        let server = axum_server::from_tcp(listener).expect("wraps the std listener");
        let _ = server.serve(service).await;
    });
}

fn bound(ip: IpAddr) -> (std::net::TcpListener, SocketAddr) {
    let listener =
        std::net::TcpListener::bind(SocketAddr::new(ip, 0)).expect("binds a loopback port");
    listener.set_nonblocking(true).expect("nonblocking");
    let address = listener.local_addr().expect("a local address");
    (listener, address)
}

/// The one chain these fakes serve (they hold one store), as the box dialled at `address` serves
/// it: the gate answers only at the deployment's own address, and the daemon names `this_box` as
/// the box's own node.
fn dialled(
    record: &Effects,
    deploying: Deployment,
    this_box: Vec<IpAddr>,
    address: SocketAddr,
) -> (Chained, Arc<Deployment>, suzatary_core::gate::Door) {
    let whois: &'static dyn Whois = if this_box.is_empty() {
        record.whois
    } else {
        Box::leak(Box::new(ThisBoxAt {
            inner: record.whois,
            at: this_box,
        }))
    };
    let chained = Chained {
        policy: Arc::new(deploying.chains.first().clone()),
        caps: Box::leak(Box::new(Effects { whois, ..*record })),
    };
    let app_url = format!("http://{address}");
    let door = serve::door(&app_url).expect("a test server's address is an app_url");
    (
        chained,
        Arc::new(Deployment {
            app_url,
            ..deploying
        }),
        door,
    )
}

impl Whois for FakeWhois {
    fn peer(&self, address: IpAddr) -> Result<suzatary_core::capability::Peer, WhoisError> {
        self.asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(address);
        if self.unreachable.contains(&address) {
            return Err(WhoisError::Unreachable {
                peer: address,
                reason: "the tailnet daemon did not answer (fake)".to_owned(),
            });
        }
        Ok(if self.this_box.contains(&address) {
            suzatary_core::capability::Peer::ThisBox
        } else {
            self.named.get(&address).cloned().map_or(
                suzatary_core::capability::Peer::Stranger,
                suzatary_core::capability::Peer::Known,
            )
        })
    }
}

/// A settable clock, defaulting to `now()`.
///
/// It ticks one microsecond per read, because under a frozen clock a write is
/// invisible to a fold stamped at the same instant.
pub struct FakeClock {
    base: Mutex<Datetime>,
    ticks: std::sync::atomic::AtomicI64,
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new(now())
    }
}

impl FakeClock {
    /// A clock starting at `at`.
    #[must_use]
    pub fn new(at: Datetime) -> Self {
        Self {
            base: Mutex::new(at),
            ticks: std::sync::atomic::AtomicI64::new(0),
        }
    }

    /// Re-anchor the clock so the next read is exactly `at`.
    pub fn set(&self, at: Datetime) {
        *self
            .base
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = at;
        self.ticks.store(0, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Datetime {
        let base = *self
            .base
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tick = self.ticks.fetch_add(1, Ordering::SeqCst);
        fpl::datetime_of(fpl::instant_of(base) + Duration::microseconds(tick)).unwrap_or(base)
    }
}

/// Echoes the markup wrapped as one paragraph, so a test sees which fragment
/// reached it.
#[derive(Default)]
pub struct FakeRender {
    compiled: Mutex<HashMap<String, String>>,
    cold: AtomicBool,
    warms: AtomicUsize,
}

impl FakeRender {
    /// A renderer with nothing compiled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// After this, `cached` answers `None` for everything.
    pub fn make_cold(&self) {
        self.cold.store(true, Ordering::SeqCst);
    }

    /// How many times `Render::warm` ran.
    #[must_use]
    pub fn warms(&self) -> usize {
        self.warms.load(Ordering::SeqCst)
    }
}

/// The real closed world: a door a test drives is the door the box has.
impl suzatary_core::capability::Typeset for FakeRender {
    fn closed(&self, source: &str) -> Result<String, suzatary_core::capability::Rejected> {
        suzatary_io::typeset::compile(source)
    }
}

impl Render for FakeRender {
    fn html(&self, markup: &Escaped) -> Result<Html, RenderError> {
        let html = format!("<p>{}</p>", markup.as_str());
        self.compiled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(markup.as_str().to_owned(), html.clone());
        Ok(Html::from_render(html))
    }

    fn cached(&self, markup: &Escaped) -> Option<Html> {
        if self.cold.load(Ordering::SeqCst) {
            return None;
        }
        self.compiled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(markup.as_str())
            .cloned()
            .map(Html::from_render)
    }

    fn warm(&self, markup: &[Escaped]) -> usize {
        self.warms.fetch_add(1, Ordering::SeqCst);
        markup.iter().filter(|m| self.html(m).is_ok()).count()
    }

    fn page(&self, page: &suzatary_core::reading::Page) -> Result<Html, RenderError> {
        Ok(Html::from_render(format!("<pre>{}</pre>", page.as_str())))
    }
}

/// A block containing this string is `FakeAnswers`' one refusal to compile.
pub const BREAKS_THE_COMPILE: &str = "BREAKS_THE_COMPILE";

/// Echoes each block in a `<p>`, so a test can see which block reached it.
#[derive(Default)]
pub struct FakeAnswers;

impl FakeAnswers {
    /// The echoing renderer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Answers for FakeAnswers {
    fn block(
        &self,
        block: &suzatary_core::secretary::answer::Renderable,
    ) -> Result<Html, AnswerError> {
        if block.as_str().contains(BREAKS_THE_COMPILE) {
            return Err(AnswerError("the fake typst refused this block".to_owned()));
        }
        let text = block
            .as_str()
            .trim()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        Ok(Html::from_render(format!("<p>{text}</p>")))
    }
}

/// Canned rows and messages, with message and thread reads recorded so a test can
/// prove one was never asked.
#[derive(Default)]
pub struct FakeMailbox {
    /// The accounts `accounts` lists.
    pub accounts: Vec<String>,
    /// The rows every `inbox` and `search` answers.
    pub rows: Vec<MailRow>,
    /// Messages by id.
    pub messages: HashMap<String, MailMessage>,
    /// Threads by id.
    pub threads: HashMap<String, Vec<MailMessage>>,
    /// Attachment listings by message id.
    pub attachment_lists: HashMap<String, Vec<AttachmentMeta>>,
    /// Attachment reads by message id and attachment name.
    pub attachment_reads: HashMap<(String, String), FileRead>,
    /// Attachment bytes by message id and attachment name.
    pub attachment_files: Mutex<HashMap<(String, String), suzatary_core::mail::AttachmentFile>>,
    /// Each message's file, by notmuch id.
    pub message_files: HashMap<String, Vec<u8>>,
    asked_messages: Mutex<Vec<String>>,
    asked_threads: Mutex<Vec<String>>,
}

impl FakeMailbox {
    /// An empty mailbox.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This mailbox, listing these accounts.
    #[must_use]
    pub fn with_accounts(mut self, accounts: Vec<String>) -> Self {
        self.accounts = accounts;
        self
    }

    /// This mailbox, plus one message.
    #[must_use]
    pub fn with_message(mut self, id: impl Into<String>, message: MailMessage) -> Self {
        self.messages.insert(id.into(), message);
        self
    }

    /// This mailbox, plus one message's attachment listing.
    #[must_use]
    pub fn with_attachments(mut self, id: impl Into<String>, listed: Vec<AttachmentMeta>) -> Self {
        self.attachment_lists.insert(id.into(), listed);
        self
    }

    /// This mailbox, plus one message's file.
    #[must_use]
    pub fn with_message_file(mut self, id: impl Into<String>, raw: Vec<u8>) -> Self {
        self.message_files.insert(id.into(), raw);
        self
    }

    /// This mailbox, plus what reading one attachment answers.
    #[must_use]
    pub fn with_attachment_read(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        read: FileRead,
    ) -> Self {
        self.attachment_reads.insert((id.into(), name.into()), read);
        self
    }

    /// This mailbox, plus one attachment's bytes.
    #[must_use]
    pub fn with_attachment_file(
        self,
        id: impl Into<String>,
        file: suzatary_core::mail::AttachmentFile,
    ) -> Self {
        self.replace_attachment_file(id, file);
        self
    }

    /// Change one attachment's bytes in place, as a message rewritten on disk would.
    pub fn replace_attachment_file(
        &self,
        id: impl Into<String>,
        file: suzatary_core::mail::AttachmentFile,
    ) {
        self.attachment_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert((id.into(), file.name.clone()), file);
    }

    /// Every message id read, in order.
    #[must_use]
    pub fn asked_messages(&self) -> Vec<String> {
        self.asked_messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Every thread id read, in order.
    #[must_use]
    pub fn asked_threads(&self) -> Vec<String> {
        self.asked_threads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Mailbox for FakeMailbox {
    fn accounts(&self) -> Vec<String> {
        self.accounts.clone()
    }

    fn inbox(
        &self,
        _account: Option<&str>,
        _limit: usize,
        _before: Option<Datetime>,
    ) -> Result<Vec<MailRow>, MailError> {
        Ok(self.rows.clone())
    }

    fn search(
        &self,
        _query: &Query,
        _account: Option<&str>,
        _limit: usize,
        _before: Option<Datetime>,
    ) -> Result<Vec<MailRow>, MailError> {
        Ok(self.rows.clone())
    }

    fn message(&self, id: &str) -> Result<MailMessage, MailError> {
        self.asked_messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(id.to_owned());
        self.messages
            .get(id)
            .cloned()
            .ok_or_else(|| MailError::Absent {
                kind: "message",
                name: id.to_owned(),
            })
    }

    fn thread(&self, thread: &str, _limit: usize) -> Result<Vec<MailMessage>, MailError> {
        self.asked_threads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(thread.to_owned());
        self.threads
            .get(thread)
            .cloned()
            .ok_or_else(|| MailError::Absent {
                kind: "conversation",
                name: thread.to_owned(),
            })
    }

    fn attachments(&self, id: &str) -> Result<Vec<AttachmentMeta>, MailError> {
        self.attachment_lists
            .get(id)
            .cloned()
            .ok_or_else(|| MailError::Absent {
                kind: "message",
                name: id.to_owned(),
            })
    }

    fn stored(&self, id: &str, cap: u64) -> Result<suzatary_core::mail::Stored, MailError> {
        let raw = self
            .message_files
            .get(id)
            .ok_or_else(|| MailError::Absent {
                kind: "message",
                name: id.to_owned(),
            })?;
        Ok(if raw.len() as u64 > cap {
            suzatary_core::mail::Stored::TooLarge {
                size: raw.len() as u64,
            }
        } else {
            suzatary_core::mail::Stored::Recorded(suzatary_io::mailbox::forwarded::stored(id, raw))
        })
    }

    fn attachment(
        &self,
        id: &str,
        name: &str,
    ) -> Result<suzatary_core::mail::AttachmentFile, MailError> {
        self.attachment_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(id.to_owned(), name.to_owned()))
            .cloned()
            .ok_or_else(|| MailError::Absent {
                kind: "attachment",
                name: name.to_owned(),
            })
    }

    fn read_attachment(&self, id: &str, name: &str) -> Result<FileRead, MailError> {
        self.attachment_reads
            .get(&(id.to_owned(), name.to_owned()))
            .cloned()
            .ok_or_else(|| MailError::Absent {
                kind: "attachment",
                name: name.to_owned(),
            })
    }
}

/// A recording sink: every push is accepted (to 0 devices) and kept, so a test
/// can assert on exactly what would have crossed the relay.
#[derive(Default)]
pub struct FakeNotify {
    pushed: Mutex<Vec<Push>>,
}

impl Notify for FakeNotify {
    fn push(&self, message: &Push) -> Result<Delivered, NotifyError> {
        self.pushed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(message.clone());
        Ok(Delivered::to_devices(0))
    }
}

impl FakeNotify {
    /// Every push sent so far, oldest first.
    #[must_use]
    pub fn pushed(&self) -> Vec<Push> {
        self.pushed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// A rendezvous a test holds across one model call, proving a round is in flight
/// rather than already finished.
#[derive(Clone, Default)]
pub struct Gate(Arc<GateState>);

#[derive(Default)]
struct GateState {
    entered: std::sync::Condvar,
    released: std::sync::Condvar,
    // (has the model call arrived, has the test released it)
    state: Mutex<(bool, bool)>,
}

impl Gate {
    /// A fresh, unentered gate.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Called from the model: block until `release` runs.
    fn enter(&self) {
        let mut guard = self.0.state.lock().expect("not poisoned");
        guard.0 = true;
        self.0.entered.notify_all();
        while !guard.1 {
            guard = self.0.released.wait(guard).expect("not poisoned");
        }
    }

    /// Called from the model: say it has arrived, without waiting for a release.
    fn arrive(&self) {
        let mut guard = self.0.state.lock().expect("not poisoned");
        guard.0 = true;
        self.0.entered.notify_all();
    }

    /// Called from the test: block until the model call has entered.
    pub fn wait_until_entered(&self) {
        let guard = self.0.state.lock().expect("not poisoned");
        let _guard = self
            .0
            .entered
            .wait_while(guard, |(entered, _)| !*entered)
            .expect("not poisoned");
    }

    /// Release whatever is waiting in `enter`.
    pub fn release(&self) {
        let mut guard = self.0.state.lock().expect("not poisoned");
        guard.1 = true;
        self.0.released.notify_all();
    }
}

/// A model that answers from a list, one reply per turn, in order.
///
/// Verbs are dispatched on this side of the trait, so a scripted reply runs the
/// real handlers against a real conversation file.
#[derive(Default)]
pub struct FakeModel {
    /// Each ask's reply in order, or the failure it meets instead.
    replies: Mutex<std::collections::VecDeque<Result<String, ModelError>>>,
    asked: Mutex<Vec<Conversation>>,
    opened: Mutex<Vec<SessionId>>,
    resumed_with: Mutex<Vec<Vec<Turn>>>,
    /// Reset the moment it fires, so a script loses exactly one resume.
    lose_next_resume: Mutex<bool>,
    /// Blocks the first call, whichever of `open`, `resume` or `turn` it is.
    gate: Mutex<Option<Gate>>,
    /// Fragments per `resume` call; when exhausted, the whole reply is one delta.
    deltas: Mutex<std::collections::VecDeque<Vec<String>>>,
    /// Blocks `resume` after its first fragment, so a late subscriber's partial is
    /// nonempty while the round is in flight.
    mid: Mutex<Option<Gate>>,
    /// Held by the next `open` or `resume` until its `Halt` is pulled.
    holding: Mutex<Option<Gate>>,
    /// The verb names each ask was shown, in order.
    shown: Mutex<Vec<Vec<String>>>,
}

impl FakeModel {
    /// A model that will answer these, in order, each parsed by `Reply::parse`.
    #[must_use]
    pub fn saying(replies: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            replies: Mutex::new(
                replies
                    .into_iter()
                    .map(|reply| Ok(reply.to_owned()))
                    .collect(),
            ),
            asked: Mutex::new(Vec::new()),
            opened: Mutex::new(Vec::new()),
            resumed_with: Mutex::new(Vec::new()),
            lose_next_resume: Mutex::new(false),
            gate: Mutex::new(None),
            deltas: Mutex::new(std::collections::VecDeque::new()),
            mid: Mutex::new(None),
            holding: Mutex::new(None),
            shown: Mutex::new(Vec::new()),
        }
    }

    /// The same, for a script built at run time (a review's calls carry hashes).
    #[must_use]
    pub fn saying_owned(replies: impl IntoIterator<Item = String>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().map(Ok).collect()),
            asked: Mutex::new(Vec::new()),
            opened: Mutex::new(Vec::new()),
            resumed_with: Mutex::new(Vec::new()),
            lose_next_resume: Mutex::new(false),
            gate: Mutex::new(None),
            deltas: Mutex::new(std::collections::VecDeque::new()),
            mid: Mutex::new(None),
            holding: Mutex::new(None),
            shown: Mutex::new(Vec::new()),
        }
    }

    /// After the scripted replies, the next ask answers `reply`.
    #[must_use]
    pub fn then_saying(self, reply: &str) -> Self {
        self.replies
            .lock()
            .expect("not poisoned")
            .push_back(Ok(reply.to_owned()));
        self
    }

    /// After the scripted replies, the next ask fails with `failed`.
    #[must_use]
    pub fn then_failing(self, failed: ModelError) -> Self {
        self.replies
            .lock()
            .expect("not poisoned")
            .push_back(Err(failed));
        self
    }

    /// Each next `resume` sends one entry's fragments to `on_delta` before answering.
    #[must_use]
    pub fn streaming(self, deltas: impl IntoIterator<Item = Vec<&'static str>>) -> Self {
        *self.deltas.lock().expect("not poisoned") = deltas
            .into_iter()
            .map(|fragments| fragments.into_iter().map(str::to_owned).collect())
            .collect();
        self
    }

    /// The first call blocks in `gate` until the test releases it.
    #[must_use]
    pub fn gated(self, gate: Gate) -> Self {
        *self.gate.lock().expect("not poisoned") = Some(gate);
        self
    }

    /// `resume` blocks in `gate` after sending its first streamed fragment.
    #[must_use]
    pub fn mid_stream(self, gate: Gate) -> Self {
        *self.mid.lock().expect("not poisoned") = Some(gate);
        self
    }

    /// The next `open` or `resume` arrives at `gate` and answers
    /// `ModelError::Halted` once its halt is pulled, as a killed process does.
    pub fn hold_next(&self, gate: Gate) {
        *self.holding.lock().expect("not poisoned") = Some(gate);
    }

    /// Hold until stopped, if `hold_next` set a gate.
    fn hold(&self, halt: &Halt) -> Result<(), ModelError> {
        let Some(gate) = self.holding.lock().expect("not poisoned").take() else {
            return Ok(());
        };
        gate.arrive();
        while !halt.halted() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Err(ModelError::Halted)
    }

    /// Block in the gate, if one is set, before a reply is popped.
    fn wait_for_gate(&self) {
        if let Some(gate) = self.gate.lock().expect("not poisoned").clone() {
            gate.enter();
        }
    }

    /// Every conversation it was handed, in order.
    #[must_use]
    pub fn asked(&self) -> Vec<Conversation> {
        self.asked.lock().expect("not poisoned").clone()
    }

    /// Every session id `open` was asked to mint under, in order.
    #[must_use]
    pub fn opened(&self) -> Vec<SessionId> {
        self.opened.lock().expect("not poisoned").clone()
    }

    /// Every delta `resume` was asked to send, in order.
    #[must_use]
    pub fn resumed_with(&self) -> Vec<Vec<Turn>> {
        self.resumed_with.lock().expect("not poisoned").clone()
    }

    /// The verb names each ask was shown, in order, whichever of `turn`, `open` or `resume`.
    #[must_use]
    pub fn shown(&self) -> Vec<Vec<String>> {
        self.shown.lock().expect("not poisoned").clone()
    }

    fn show(&self, tools: &serde_json::Value) {
        let names = tools
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
            .collect();
        self.shown.lock().expect("not poisoned").push(names);
    }

    /// The next `resume` fails once with `ModelError::Lost`.
    #[must_use]
    pub fn losing_next_resume(self) -> Self {
        *self.lose_next_resume.lock().expect("not poisoned") = true;
        self
    }
}

impl Model for FakeModel {
    fn name(&self) -> String {
        "a scripted model".to_owned()
    }

    fn turn(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        self.show(tools);
        self.asked
            .lock()
            .expect("not poisoned")
            .push(conversation.clone());
        self.wait_for_gate();
        let next = self.replies.lock().expect("not poisoned").pop_front();
        match next {
            Some(Ok(text)) => Ok(Reply::parse(&text)),
            Some(Err(failed)) => Err(failed),
            // A script that ran out is a test bug; a fake that improvised would
            // make a green run mean nothing.
            None => Err(ModelError::Failed("the script ran out".to_owned())),
        }
    }
}

/// `open` and `resume` pop the same scripted queue `turn` reads.
/// Always `Some`: the harness's model always holds a session.
impl Sessions for FakeModel {
    fn pick(&self) -> suzatary_core::capability::Pick<'_> {
        suzatary_core::capability::Pick::Held(self)
    }
}

impl SessionModel for FakeModel {
    fn open(
        &self,
        id: &SessionId,
        conversation: &Conversation,
        tools: &serde_json::Value,
        halt: &Halt,
    ) -> Result<Sessioned, ModelError> {
        self.show(tools);
        self.asked
            .lock()
            .expect("not poisoned")
            .push(conversation.clone());
        self.opened.lock().expect("not poisoned").push(id.clone());
        self.hold(halt)?;
        self.wait_for_gate();
        let next = self.replies.lock().expect("not poisoned").pop_front();
        match next {
            Some(Err(failed)) => Err(failed),
            Some(Ok(text)) => Ok(Sessioned {
                reply: Reply::parse(&text),
                session: Session {
                    id: id.clone(),
                    model: self.name(),
                    cwd: String::new(),
                    created: "now".to_owned(),
                    last: "now".to_owned(),
                    surface: None,
                },
            }),
            None => Err(ModelError::Failed("the script ran out".to_owned())),
        }
    }

    fn resume(
        &self,
        session: &Session,
        delta: &suzatary_core::capability::Delta,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Sessioned, ModelError> {
        self.show(tools);
        self.resumed_with
            .lock()
            .expect("not poisoned")
            .push(delta.turns().to_vec());
        let mut lose = self.lose_next_resume.lock().expect("not poisoned");
        if *lose {
            *lose = false;
            return Err(ModelError::Lost("the session is gone".to_owned()));
        }
        drop(lose);
        self.hold(halt)?;
        self.wait_for_gate();
        let next = self.replies.lock().expect("not poisoned").pop_front();
        match next {
            Some(Err(failed)) => Err(failed),
            Some(Ok(text)) => {
                let fragments = self.deltas.lock().expect("not poisoned").pop_front();
                match fragments {
                    Some(fragments) if !fragments.is_empty() => {
                        let mut remaining = fragments.iter();
                        if let Some(first) = remaining.next() {
                            on_delta(first);
                        }
                        if let Some(mid) = self.mid.lock().expect("not poisoned").clone() {
                            mid.enter();
                        }
                        for fragment in remaining {
                            on_delta(fragment);
                        }
                    }
                    _ => on_delta(&text),
                }
                Ok(Sessioned {
                    reply: Reply::parse(&text),
                    session: session.clone(),
                })
            }
            None => Err(ModelError::Failed("the script ran out".to_owned())),
        }
    }
}

/// A corpus with no maildir behind it, empty unless seeded.
///
/// `search` mimics notmuch only as far as the sweep's queries need: `thread:` and
/// `from:` match a field exactly, and a bare term matches subject or body.
#[derive(Default)]
pub struct FakeCorpus {
    messages: Vec<FakeMessage>,
}

/// One message `FakeCorpus` knows about.
#[derive(Debug, Clone)]
pub struct FakeMessage {
    /// The canonical hash `show` is asked by.
    pub hash: String,
    /// The `thread:` a `search` matches on.
    pub thread: String,
    /// The `from:` a `search` matches on.
    pub sender: String,
    /// The subject line.
    pub subject: String,
    /// The body.
    pub body: String,
    /// Shaped exactly as a real row's `timestamp`.
    pub timestamp: String,
}

impl FakeCorpus {
    /// An empty corpus.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This corpus, plus one more message.
    #[must_use]
    pub fn with(mut self, message: FakeMessage) -> Self {
        self.messages.push(message);
        self
    }
}

/// One `search` row, in the shape a real corpus answers with.
fn row(message: &FakeMessage) -> Summary {
    Summary {
        hash: message.hash.clone(),
        id: format!("id-{}", message.hash),
        sender: message.sender.clone(),
        title: message.subject.clone(),
        timestamp: Some(message.timestamp.clone()),
        container: "inbox".to_owned(),
        sources: 1,
    }
}

impl Corpus for FakeCorpus {
    fn sync(&self) -> Result<SyncReport, CorpusError> {
        Ok(SyncReport {
            synced: Vec::new(),
            notmuch: String::new(),
            unheld: None,
        })
    }
    fn queue(&self, _limit: Option<usize>) -> Result<Vec<Summary>, CorpusError> {
        Ok(Vec::new())
    }
    fn search(&self, query: &Query, _limit: Option<usize>) -> Result<Vec<Summary>, CorpusError> {
        let text = query.as_str();
        let hits = if let Some(thread) = text.strip_prefix("thread:") {
            self.messages
                .iter()
                .filter(|message| message.thread == thread)
                .map(row)
                .collect()
        } else if let Some(sender) = text.strip_prefix("from:") {
            self.messages
                .iter()
                .filter(|message| message.sender == sender)
                .map(row)
                .collect()
        } else {
            let term = text.trim_matches('"').to_lowercase();
            self.messages
                .iter()
                .filter(|message| {
                    message.body.to_lowercase().contains(&term)
                        || message.subject.to_lowercase().contains(&term)
                })
                .map(row)
                .collect()
        };
        Ok(hits)
    }
    fn show(&self, id: &str) -> Result<Show, CorpusError> {
        Ok(
            match self
                .messages
                .iter()
                .find(|message| message.hash == id || message.hash.starts_with(id))
            {
                Some(message) => Show::Shown(Message {
                    id: format!("id-{}", message.hash),
                    tags: vec!["inbox".to_owned()],
                    headers: [
                        ("From".to_owned(), message.sender.clone()),
                        ("Subject".to_owned(), message.subject.clone()),
                        ("Date".to_owned(), message.timestamp.clone()),
                    ]
                    .into_iter()
                    .collect(),
                    body: message.body.clone(),
                    links: Vec::new(),
                }),
                None => Show::NotFound { id: id.to_owned() },
            },
        )
    }
    fn mark(
        &self,
        hash: &str,
        decision: Decision,
        _note: Option<&str>,
        _native: Option<&str>,
    ) -> Result<Marked, CorpusError> {
        Ok(Marked {
            marked: hash.to_owned(),
            disposition: decision.disposition(),
        })
    }
    fn noise_add(&self, sender: &str) -> Result<NoiseAdded, CorpusError> {
        Ok(NoiseAdded {
            noise_added: sender.to_lowercase(),
        })
    }
    fn last_sync(&self) -> Option<SyncReport> {
        None
    }
    fn last_triage_pass(&self) -> Option<String> {
        None
    }
}

/// The data tree's diff, with no tree behind it.
#[derive(Default)]
pub struct FakeRepository;

impl Repository for FakeRepository {
    fn diff_summary(&self) -> Result<String, RepositoryError> {
        Ok(String::new())
    }
}

/// The manuals, as small strings.
#[derive(Default)]
pub struct FakeManuals;

impl Manuals for FakeManuals {
    fn document(&self, which: Manual) -> Option<String> {
        Some(match which {
            Manual::Constitution => "the constitution".to_owned(),
            Manual::Operator => "the operator's manual".to_owned(),
            Manual::Secretary => "the secretary's brief".to_owned(),
        })
    }
}

/// What a tap would have sent, kept in memory.
///
/// Every server this suite starts is wired with one, so no test can reach a
/// transport.
#[derive(Default)]
pub struct Recorder {
    sent: Mutex<Vec<suzatary_core::secretary::store::Draft>>,
    parts: Mutex<Vec<Vec<suzatary_core::mail::send::Attachment>>>,
}

impl Recorder {
    /// A recorder that has sent nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything a tap handed it, in order.
    #[must_use]
    pub fn sent(&self) -> Vec<suzatary_core::secretary::store::Draft> {
        self.sent.lock().expect("not poisoned").clone()
    }

    /// The attachments each send carried, checked, in the same order.
    #[must_use]
    pub fn parts(&self) -> Vec<Vec<suzatary_core::mail::send::Attachment>> {
        self.parts.lock().expect("not poisoned").clone()
    }
}

/// Records every act a tap began, performing nothing and answering `Started`.
#[derive(Default)]
pub struct ActRecorder {
    begun: Mutex<Vec<(String, usize, suzatary_core::secretary::acts::Act)>>,
}

impl ActRecorder {
    /// A recorder that has begun nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every `(conversation, index, act)` begun, in order.
    #[must_use]
    pub fn begun(&self) -> Vec<(String, usize, suzatary_core::secretary::acts::Act)> {
        self.begun.lock().expect("not poisoned").clone()
    }
}

impl suzatary_core::secretary::routes::Perform for ActRecorder {
    fn begin(
        &self,
        id: &suzatary_core::secretary::store::ConversationId,
        index: usize,
        _object: &suzatary_core::secretary::inbox::ProposalId,
        _proposer: &suzatary_core::secretary::proposer::Proposer,
        act: &suzatary_core::secretary::acts::Act,
        _consent: suzatary_core::mail::oauth::Consent,
    ) -> suzatary_core::secretary::responded::Performed {
        self.begun
            .lock()
            .expect("not poisoned")
            .push((id.as_str().to_owned(), index, act.clone()));
        suzatary_core::secretary::responded::Tried::Running {
            began: "2026-09-14T09:00:00".to_owned(),
            expires_at: "2026-09-14T09:15:00".to_owned(),
        }
        .into()
    }
}

/// A performer that begins a Connect card's sign-in through the server's table and records
/// every other act.
pub struct SignsIn {
    recorded: Arc<ActRecorder>,
    signing: Arc<serve::sign_ins::SignIns>,
}

impl suzatary_core::secretary::routes::Perform for SignsIn {
    fn begin(
        &self,
        id: &suzatary_core::secretary::store::ConversationId,
        index: usize,
        object: &suzatary_core::secretary::inbox::ProposalId,
        proposer: &suzatary_core::secretary::proposer::Proposer,
        act: &suzatary_core::secretary::acts::Act,
        consent: suzatary_core::mail::oauth::Consent,
    ) -> suzatary_core::secretary::responded::Performed {
        match act {
            suzatary_core::secretary::acts::Act::SignIn { .. } => self.signing.begin(object, now()),
            _ => self
                .recorded
                .begin(id, index, object, proposer, act, consent),
        }
    }
}

/// Every plugin a tap synced, in order, syncing none.
#[derive(Default)]
pub struct CheckRecorder(Mutex<Vec<String>>);

impl CheckRecorder {
    /// Every plugin it was asked to sync.
    #[must_use]
    pub fn checked(&self) -> Vec<String> {
        self.0.lock().expect("not poisoned").clone()
    }
}

impl suzatary_core::settings::Checks for CheckRecorder {
    fn check(&self, plugin: &suzatary_core::plugin::PluginName) {
        self.0
            .lock()
            .expect("not poisoned")
            .push(plugin.as_str().to_owned());
    }
}

/// What a settings write would have set, kept in memory.
///
/// It records the value on purpose: the no-read-back rule is about the route's
/// response, not this recorder.
#[derive(Default)]
pub struct SecretRecorder {
    calls: Mutex<Vec<(String, String)>>,
    cleared: Mutex<Vec<String>>,
}

impl SecretRecorder {
    /// A recorder that has set nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every name this recorder was asked to clear, in order.
    #[must_use]
    pub fn cleared(&self) -> Vec<String> {
        self.cleared.lock().expect("not poisoned").clone()
    }

    /// Every `(name, value)` this recorder was handed, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().expect("not poisoned").clone()
    }
}

/// What a recorder holds now: the last value written under each name not cleared since, so a
/// scene's later reads find what its taps stored.
impl suzatary_io::secrets::Resolves for SecretRecorder {
    fn get(
        &self,
        name: &str,
        _: &str,
    ) -> Result<Option<suzatary_core::capability::Secret>, suzatary_io::secrets::SecretError> {
        let cleared = self.cleared.lock().expect("not poisoned");
        let calls = self.calls.lock().expect("not poisoned");
        Ok(calls
            .iter()
            .rev()
            .find(|(held, _)| held == name)
            .filter(|_| !cleared.iter().any(|gone| gone == name))
            .map(|(_, value)| suzatary_core::capability::Secret::new(value.clone())))
    }
}

/// Where a name stands in a recorder: set while it holds a value; a Google account holds its
/// reading grant while its sign-in with Google is held.
impl suzatary_core::capability::Secrets for SecretRecorder {
    fn secret_state(
        &self,
        settable: &suzatary_core::settings::Settable,
    ) -> suzatary_core::capability::SecretState {
        use suzatary_io::secrets::Resolves as _;
        match self.get(settable.name(), "a scene") {
            Ok(Some(_)) => suzatary_core::capability::SecretState::Set,
            Ok(None) | Err(_) => suzatary_core::capability::SecretState::Missing,
        }
    }

    fn mail_grants(
        &self,
        account: &suzatary_core::mail::account::MailAccount,
    ) -> suzatary_core::capability::MailGrants {
        let held = suzatary_core::mail::address::EmailAddress::new(account.email.as_str())
            .ok()
            .filter(|_| matches!(account.sync, suzatary_core::mail::account::Sync::Google))
            .is_some_and(|email| {
                matches!(
                    self.google_grant(&email),
                    suzatary_core::google::Held::Signed(_)
                )
            });
        suzatary_core::capability::MailGrants {
            imap: held,
            smtp: false,
        }
    }

    fn google_grant(
        &self,
        account: &suzatary_core::mail::address::EmailAddress,
    ) -> suzatary_core::google::Held {
        use suzatary_io::secrets::Resolves as _;
        match self.get(&suzatary_core::google::grant_name(account), "a scene") {
            Ok(Some(kept)) => suzatary_core::google::KeptGrant::parse(kept.expose())
                .map_or(suzatary_core::google::Held::NotSignedIn, |kept| {
                    suzatary_core::google::Held::Signed(kept.parts)
                }),
            Ok(None) | Err(_) => suzatary_core::google::Held::NotSignedIn,
        }
    }
}

impl suzatary_core::settings::Writes for SecretRecorder {
    fn write_secret(
        &self,
        name: &suzatary_core::settings::Settable,
        value: &suzatary_core::capability::Secret,
    ) -> Result<(), String> {
        self.calls
            .lock()
            .expect("not poisoned")
            .push((name.name().to_owned(), value.expose().to_owned()));
        Ok(())
    }

    fn clear_secret(&self, name: &suzatary_core::settings::Settable) -> Result<(), String> {
        self.cleared
            .lock()
            .expect("not poisoned")
            .push(name.name().to_owned());
        Ok(())
    }
}

/// A store that keeps no declarations of its own, so every name it is told of is declared.
pub struct Undeclared;

impl suzatary_core::settings::Declares for Undeclared {
    fn declare(&self, _: &suzatary_core::settings::Settables) -> Result<(), String> {
        Ok(())
    }
}

/// A store over the real [`suzatary_io::secrets::Declarations`], beside a code root whose own
/// file declares no name, that refuses, as `secretspec` does, to set or clear a name its file
/// does not declare at that moment, and otherwise records as [`SecretRecorder`] does.
pub struct DeclaredStore {
    /// What declares its names, which the server's `declares` is.
    pub declarations: suzatary_io::secrets::Declarations,
    /// What it set and cleared.
    pub recorded: SecretRecorder,
    /// The code root and the state directory, removed with the store.
    _dir: tempfile::TempDir,
}

impl DeclaredStore {
    /// A store declaring the built-in keys alone, as a deployment's server starts.
    #[must_use]
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let base = dir.path().join("code").join("secretspec.toml");
        std::fs::create_dir_all(base.parent().expect("a parent")).expect("a code root");
        std::fs::write(
            &base,
            "[project]\nname = \"suzatary\"\nrevision = \"1.0\"\n\n[profiles.default]\n",
        )
        .expect("the code root's own file");
        let declarations = suzatary_io::secrets::Declarations::new(
            base,
            dir.path().join("state"),
            suzatary_core::settings::Settables::derive(suzatary_io::model::TOKEN, [], None, &[]),
        );
        Self {
            declarations,
            recorded: SecretRecorder::new(),
            _dir: dir,
        }
    }

    /// Whether the file the store reads now declares `name`.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        let declared = self.declarations.opened().expect("declared");
        let text = std::fs::read_to_string(declared.file()).expect("the declared file");
        let table: toml::Table = text.parse().expect("TOML");
        table["profiles"]["default"]
            .as_table()
            .is_some_and(|default| default.contains_key(name))
    }

    fn refused(&self, name: &suzatary_core::settings::Settable) -> Result<(), String> {
        if self.declares(name.name()) {
            Ok(())
        } else {
            Err(format!(
                "Secret '{}' is not defined in profile 'default'",
                name.name()
            ))
        }
    }
}

impl suzatary_core::settings::Writes for DeclaredStore {
    fn write_secret(
        &self,
        name: &suzatary_core::settings::Settable,
        value: &suzatary_core::capability::Secret,
    ) -> Result<(), String> {
        self.refused(name)?;
        self.recorded.write_secret(name, value)
    }

    fn clear_secret(&self, name: &suzatary_core::settings::Settable) -> Result<(), String> {
        self.refused(name)?;
        self.recorded.clear_secret(name)
    }
}

/// A store that will not take any name it is told of, as one whose state directory is full.
pub struct DeclaresAlwaysFail;

impl suzatary_core::settings::Declares for DeclaresAlwaysFail {
    fn declare(&self, _: &suzatary_core::settings::Settables) -> Result<(), String> {
        Err("the declared file was not written (fake)".to_owned())
    }
}

/// A secret writer that always refuses.
#[derive(Default)]
pub struct SecretsAlwaysFail;

impl suzatary_core::settings::Writes for SecretsAlwaysFail {
    fn write_secret(
        &self,
        _name: &suzatary_core::settings::Settable,
        _value: &suzatary_core::capability::Secret,
    ) -> Result<(), String> {
        Err("the store refused (fake)".to_owned())
    }

    fn clear_secret(&self, _name: &suzatary_core::settings::Settable) -> Result<(), String> {
        Err("the store refused (fake)".to_owned())
    }
}

/// Records every allow rule it is handed and always succeeds; a connection's edit is the
/// algebra's, over the tables it holds.
#[derive(Default)]
pub struct EditsRecorder {
    declared: std::sync::Mutex<suzatary_core::connect::Declared>,
    allowed: std::sync::Mutex<Vec<suzatary_core::web::AllowRule>>,
    folders: std::sync::Mutex<Vec<suzatary_core::drive::DrivePath>>,
}

impl EditsRecorder {
    /// A recorder that has recorded nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A recorder whose instance declares `declared`.
    #[must_use]
    pub fn declaring(declared: suzatary_core::connect::Declared) -> Self {
        Self {
            declared: std::sync::Mutex::new(declared),
            ..Self::default()
        }
    }

    /// Every web allow rule written, oldest first.
    #[must_use]
    pub fn allowed(&self) -> Vec<suzatary_core::web::AllowRule> {
        self.allowed.lock().expect("not poisoned").clone()
    }

    /// Every Drive folder "Always allow this folder" wrote, oldest first.
    #[must_use]
    pub fn folders(&self) -> Vec<suzatary_core::drive::DrivePath> {
        self.folders.lock().expect("not poisoned").clone()
    }
}

impl suzatary_core::settings::Edits for EditsRecorder {
    fn allow_web(
        &self,
        rule: &suzatary_core::web::AllowRule,
    ) -> Result<(), suzatary_core::settings::EditError> {
        self.allowed
            .lock()
            .expect("not poisoned")
            .push(rule.clone());
        Ok(())
    }

    fn allow_drive(
        &self,
        folder: &suzatary_core::drive::DrivePath,
    ) -> Result<(), suzatary_core::settings::EditError> {
        self.folders
            .lock()
            .expect("not poisoned")
            .push(folder.clone());
        Ok(())
    }

    fn add_connection(
        &self,
        connection: &suzatary_core::connect::Connection,
    ) -> Result<(), suzatary_core::settings::EditError> {
        let mut declared = self.declared.lock().expect("not poisoned");
        *declared = declared.with(connection)?;
        Ok(())
    }

    fn remove_connection(
        &self,
        connection: &suzatary_core::connect::Connection,
    ) -> Result<(), suzatary_core::settings::EditError> {
        let mut declared = self.declared.lock().expect("not poisoned");
        *declared = declared.without(connection)?;
        Ok(())
    }

    fn add_origin(
        &self,
        plugin: &suzatary_core::plugin::PluginName,
        origin: &suzatary_core::plugin::Origin,
    ) -> Result<(), suzatary_core::settings::EditError> {
        let mut declared = self.declared.lock().expect("not poisoned");
        *declared = declared.with_origin(plugin, origin)?;
        Ok(())
    }
}

impl suzatary_core::settings::Configuration for EditsRecorder {
    fn declared(
        &self,
    ) -> Result<suzatary_core::connect::Declared, suzatary_core::settings::EditError> {
        Ok(self.declared.lock().expect("not poisoned").clone())
    }
}

/// Everything `State` needs for `/api/overview` and the settings writes beside the effect record.
pub struct SettingsFixture {
    /// The settable names, the channels, the accounts and the plugins the server starts with.
    pub live: serve::Live,
    /// The Claude models the server runs.
    pub models: Arc<serve::models::Models>,
    /// Where a secret write goes.
    pub secrets: Arc<dyn suzatary_core::settings::Writes + Send + Sync>,
    /// What declares the settable names to the store a secret write goes to.
    pub declares: Arc<dyn suzatary_core::settings::Declares + Send + Sync>,
    /// Where an instance-file edit goes.
    pub edits: Arc<dyn suzatary_core::settings::Edits + Send + Sync>,
    /// Where a removed account's sign-ins are kept, `None` for none.
    pub grants: Option<suzatary_io::oauth::TokenStore>,
}

impl SettingsFixture {
    /// The settings a server holds, over the data repository at `data`.
    fn over(self, data: Arc<PathBuf>) -> serve::Settings {
        serve::Settings {
            live: self.live.shared(),
            models: self.models,
            secrets: self.secrets,
            declares: self.declares,
            edits: self.edits,
            data,
        }
    }

    /// Only the built-in keys settable, an empty roster, and fresh recorders.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            live: serve::Live {
                settables: suzatary_core::settings::Settables::derive(
                    suzatary_io::model::TOKEN,
                    std::iter::empty(),
                    None,
                    &[],
                ),
                channels: None,
                accounts: suzatary_core::mail::account::Roster::new(Vec::new())
                    .expect("an empty roster"),
                plugins: Vec::new(),
            },
            models: Arc::new(serve::models::Models {
                chat: suzatary_core::model_name::ModelName::parse("claude-opus-5-5")
                    .expect("a model name"),
                compacting: suzatary_core::secretary::compacting::Compacting::Off,
                reserve: suzatary_cli::instance::Reserve::default(),
            }),
            secrets: Arc::new(SecretRecorder::new()),
            declares: Arc::new(Undeclared),
            edits: Arc::new(EditsRecorder::new()),
            grants: None,
        }
    }

    /// Like `empty`, with `secrets` overridden.
    #[must_use]
    pub fn with_writer(secrets: Arc<dyn suzatary_core::settings::Writes + Send + Sync>) -> Self {
        Self {
            secrets,
            ..Self::empty()
        }
    }

    /// Like `empty`, with `edits` overridden.
    #[must_use]
    pub fn with_edits(edits: Arc<dyn suzatary_core::settings::Edits + Send + Sync>) -> Self {
        Self {
            edits,
            ..Self::empty()
        }
    }
}

impl suzatary_core::secretary::routes::Post for Recorder {
    fn send(
        &self,
        draft: &suzatary_core::secretary::store::Draft,
        body: suzatary_core::held_value::Filled,
        attachments: suzatary_core::secretary::attached::Checked,
    ) -> suzatary_core::secretary::responded::Performed {
        // What went on the wire: the body with its held values written in.
        self.sent
            .lock()
            .expect("not poisoned")
            .push(suzatary_core::secretary::store::Draft {
                body: body.into_string(),
                ..draft.clone()
            });
        self.parts
            .lock()
            .expect("not poisoned")
            .push(attachments.into_parts());
        suzatary_core::secretary::responded::Done::Sent {
            recipients: draft.to.iter().chain(&draft.cc).cloned().collect(),
            message_id: "<recorded@test>".to_owned(),
        }
        .into()
    }
}

/// A transport that fails with each of `failures` in turn, then records like
/// `Recorder`.
pub struct FailsFirst {
    failures: Mutex<std::collections::VecDeque<suzatary_core::secretary::responded::Performed>>,
    sent: Arc<Recorder>,
}

impl suzatary_core::secretary::routes::Post for FailsFirst {
    fn send(
        &self,
        draft: &suzatary_core::secretary::store::Draft,
        body: suzatary_core::held_value::Filled,
        attachments: suzatary_core::secretary::attached::Checked,
    ) -> suzatary_core::secretary::responded::Performed {
        match self.failures.lock().expect("not poisoned").pop_front() {
            Some(failed) => failed,
            None => self.sent.send(draft, body, attachments),
        }
    }
}

/// A scripted OAuth endpoint for driving `Acting::Real` with no network:
/// the answers in order, then a transport failure once they run out.
pub struct ScriptedOauth(suzatary_io::http::Scripted);

impl ScriptedOauth {
    /// An endpoint answering these, in order.
    #[must_use]
    pub fn new(
        answers: Vec<
            Result<suzatary_core::mail::oauth::Answered, suzatary_core::mail::oauth::OauthError>,
        >,
    ) -> Self {
        use suzatary_io::http::{HttpError, Resp, Scripted};
        let queue = Mutex::new(std::collections::VecDeque::from(answers));
        Self(Scripted::new(move |_| {
            match queue.lock().expect("not poisoned").pop_front() {
                Some(Ok(answered)) => Ok(Resp::new(answered.status, answered.body.to_string())),
                Some(Err(failed)) => Err(HttpError::Lost(failed.to_string())),
                None => Err(HttpError::Lost("the script ran out of answers".to_owned())),
            }
        }))
    }

    /// A device-code answer whose `message` carries the user code and URI.
    #[must_use]
    pub fn device_code(user_code: &str, interval: u64, expires_in: u64) -> serde_json::Value {
        serde_json::json!({
            "device_code": format!("the-secret-code-for-{user_code}"),
            "user_code": user_code,
            "verification_uri": "https://microsoft.com/devicelogin",
            "message": format!(
                "To sign in, use a web browser to open https://microsoft.com/devicelogin \
                 and enter the code {user_code} to authenticate."
            ),
            "interval": interval,
            "expires_in": expires_in,
        })
    }

    /// A granted poll answer, with a refresh token to store.
    #[must_use]
    pub fn granted(refresh_token: &str) -> serde_json::Value {
        serde_json::json!({
            "access_token": "access-token",
            "refresh_token": refresh_token,
            "expires_in": 3600,
        })
    }
}

impl suzatary_io::http::Http for ScriptedOauth {
    fn send(
        &self,
        req: suzatary_io::http::Req,
    ) -> Result<suzatary_io::http::Resp, suzatary_io::http::HttpError> {
        self.0.send(req)
    }
}

/// Poll conversation `id` until `until` holds of it.
///
/// An act's outcome and its turn are two writes, so `until` must cover
/// everything the assertions read.
///
/// # Panics
/// If `until` does not hold within `timeout`.
#[must_use]
pub fn poll_conversation(
    base: &str,
    id: &str,
    timeout: std::time::Duration,
    until: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let body = reading(base, id, &[]);
        if until(&body) {
            return body;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("conversation {id} never reached what the test waits for within {timeout:?}");
}

/// A 200 answer over JSON, for a `ScriptedOauth` script.
#[must_use]
pub fn answered(body: serde_json::Value) -> suzatary_core::mail::oauth::Answered {
    suzatary_core::mail::oauth::Answered { status: 200, body }
}

/// The whole capability set: real stores over temp directories, so the real file
/// formats are tested, and fakes for everything that talks to a daemon or
/// subprocess.
pub struct Fakes {
    /// The scratch root every store here lives under.
    pub root: PathBuf,
    /// The chain store.
    pub chain: suzatary_io::objects::Objects<suzatary_core::record::TodoEvent>,
    /// Who each peer is.
    pub whois: FakeWhois,
    /// The recorded pushes.
    pub notify: FakeNotify,
    /// The mail the routes read.
    pub mailbox: FakeMailbox,
    /// The markup renderer.
    pub render: FakeRender,
    /// The answer-block renderer.
    pub answers: FakeAnswers,
    /// The clock.
    pub clock: FakeClock,
    /// The briefings store.
    pub briefings: BriefingFiles,
    /// The push-device store.
    pub devices: Devices,
    /// The watches on expected mail.
    pub watches: suzatary_io::watches::Watches,
    /// The mail corpus the verbs read.
    pub corpus: FakeCorpus,
    /// The repository's gates.
    pub repository: FakeRepository,
    /// The manuals.
    pub manuals: FakeManuals,
    /// The scripted model.
    pub model: FakeModel,
    /// The web the `web` verb reads.
    pub web: FakeWeb,
    /// The Drive the `drive_*` verbs read.
    pub drive: FakeDriveAccess,
    /// The Microsoft accounts' Junk.
    pub junk: FakeJunk,
    /// The owner's uploads.
    pub files: FakeFiles,
    /// The upload index, over the upload store the server's upload route writes.
    pub upload_index: suzatary_io::upload_index::UploadIndexFiles,
    /// The secretary's conversation store.
    pub conversations: Conversations,
    /// Every answer block an erasure emptied from the render cache, by source.
    pub forgot: Arc<suzatary_core::secretary::conversations::Forgot>,
    /// The secretary's memory, its history beside the conversations.
    pub memory: suzatary_io::objects::Objects<suzatary_core::secretary::memory_event::MemoryEvent>,
    /// The recorded decisions, beside the memory.
    pub decisions: suzatary_io::objects::Objects<suzatary_core::decisions::DecisionEvent>,
    /// The memory's texts, beside its history.
    pub memory_texts: suzatary_io::texts::Texts<suzatary_core::secretary::memory::Fact>,
    /// The digest's history, beside the memory.
    pub digests: suzatary_io::objects::Objects<suzatary_core::secretary::digest_event::DigestEvent>,
    /// The digest's summaries' words, beside its history.
    pub digest_texts: suzatary_io::texts::Texts<suzatary_core::secretary::digest_line::Line>,
    /// Whose retraction or erasure the digest believes: none here.
    pub owners: std::collections::BTreeSet<prodrome_core::event::Actor>,
    /// The topic stores, begun, as a box's server begins them.
    pub topics: suzatary_io::topics::Topics,
    /// The inbox store, real so a submit holds the server's `flock`.
    pub inbox: suzatary_io::objects::Objects<suzatary_core::secretary::inbox_event::InboxEvent>,
    /// The owner's repositories, none by default.
    pub repos: Arc<suzatary_io::repos::Mirrors>,
    /// The calendar, a cache-tier store under the root; empty unless a test learns into it.
    pub calendar: suzatary_io::calendar::Clocked,
    /// The tables a Connect card is checked against; none declared.
    pub configuration: Arc<dyn suzatary_core::settings::Configuration>,
    /// Where each declared secret stands: nothing set, unless a scene reads its own store.
    pub secrets: Arc<dyn suzatary_core::capability::Secrets + Send + Sync>,
}

impl Fakes {
    /// Fakes over the six-todo chain, with everything else empty and a
    /// fresh temp root per capability that needs a directory.
    #[must_use]
    pub fn new(tag: &str) -> Self {
        let root = scratch(tag);
        let events = six_todo_chain(&root);
        Self::over(root, suzatary_io::objects::Objects::chain(events))
    }

    /// Fakes over a caller-built chain.
    ///
    /// Its auxiliary directories get a name distinct from `tag`, because `scratch`
    /// deletes what it names and would wipe the caller's chain.
    #[must_use]
    pub fn with_store(tag: &str, events: EventStore) -> Self {
        let root = scratch(&format!("{tag}-aux"));
        Self::over(root, suzatary_io::objects::Objects::chain(events))
    }

    /// Fakes over `chain`, every directory it needs under `root`, laid out
    /// as an instance whose data root is `root`.
    #[must_use]
    pub fn over(
        root: PathBuf,
        chain: suzatary_io::objects::Objects<suzatary_core::record::TodoEvent>,
    ) -> Self {
        // Every deployment signs: a scene's chain has adopted the cast's root, as a box's first
        // writer does (its keys registered, its history an epoch), and the owner's phone is
        // trusted, as `device trust` at the terminal does.
        suzatary_core::signatures::adopt(&chain, &cast_keys(), &self::chain())
            .expect("the scene adopts its root");
        suzatary_core::signatures::trust(
            &chain,
            cast_keys().root(),
            &actor(CAST.owner.actor),
            &phone().public(),
        )
        .expect("the phone is trusted at the terminal");
        let forgot = Arc::new(suzatary_core::secretary::conversations::Forgot::default());
        let topics = suzatary_io::topics::Topics::open(
            root.join("secretary-topics"),
            root.join("secretary-topic-aliases"),
        );
        topics.begin().expect("the scene begins its topic stores");
        Self {
            topics,
            chain,
            whois: FakeWhois::new(),
            notify: FakeNotify::default(),
            mailbox: FakeMailbox::new(),
            render: FakeRender::new(),
            answers: FakeAnswers::new(),
            clock: FakeClock::default(),
            briefings: BriefingFiles::new(root.join("briefings")),
            devices: Devices::open(root.join("devices")),
            watches: suzatary_io::watches::Watches::open(root.join("watches")),
            corpus: FakeCorpus::new(),
            repository: FakeRepository,
            manuals: FakeManuals,
            model: FakeModel::default(),
            web: FakeWeb::default(),
            inbox: suzatary_io::objects::Objects::open(root.join("inbox")),
            repos: Arc::new(suzatary_io::repos::Mirrors::new(
                root.join("mirrors"),
                suzatary_io::repos::Source::Local {
                    base: root.join("remotes"),
                },
                suzatary_core::repos::Declared::default(),
                Box::new(NoSecrets),
            )),
            drive: FakeDriveAccess::default(),
            junk: FakeJunk::default(),
            files: FakeFiles::default(),
            upload_index: suzatary_io::upload_index::UploadIndexFiles::new(
                root.join("secretary/upload-index"),
                upload_store(&root),
            ),
            conversations: Conversations::open(
                root.join("secretary-lines"),
                root.join("secretary-lines-keys"),
                Arc::clone(&forgot) as _,
            ),
            forgot,
            memory: suzatary_io::objects::Objects::open(root.join("memory")),
            decisions: suzatary_io::objects::Objects::open(root.join("decisions")),
            memory_texts: suzatary_io::texts::Texts::open(root.join("secretary-memory-texts")),
            digests: suzatary_io::objects::Objects::digests(
                root.join("digests"),
                std::collections::BTreeSet::new(),
            ),
            digest_texts: suzatary_io::texts::Texts::open(root.join("secretary-digest-texts")),
            owners: std::collections::BTreeSet::new(),
            secrets: Arc::new(suzatary_core::capability::Nothing),
            calendar: suzatary_io::calendar::Clocked::new(
                [root.join("cache/plugins/ics/store")],
                suzatary_core::zone::Zone::UTC,
            ),
            configuration: Arc::new(EditsRecorder::new()),
            root,
        }
    }

    /// The same, a Connect card checked against `configuration`.
    #[must_use]
    pub fn configuration(
        self,
        configuration: Arc<dyn suzatary_core::settings::Configuration>,
    ) -> Self {
        Self {
            configuration,
            ..self
        }
    }

    /// A bundle whose peers resolve through `whois`.
    #[must_use]
    pub fn whois(mut self, whois: FakeWhois) -> Self {
        self.whois = whois;
        self
    }

    /// A bundle whose secrets stand as `secrets` says.
    #[must_use]
    pub fn secrets(
        mut self,
        secrets: Arc<dyn suzatary_core::capability::Secrets + Send + Sync>,
    ) -> Self {
        self.secrets = secrets;
        self
    }

    /// A bundle whose mail is `mailbox`.
    #[must_use]
    pub fn mailbox(mut self, mailbox: FakeMailbox) -> Self {
        self.mailbox = mailbox;
        self
    }

    /// A bundle whose mail corpus answers over seeded messages.
    #[must_use]
    pub fn corpus(mut self, corpus: FakeCorpus) -> Self {
        self.corpus = corpus;
        self
    }

    /// A bundle whose model answers a script.
    #[must_use]
    pub fn model(mut self, model: FakeModel) -> Self {
        self.model = model;
        self
    }

    /// A bundle reading these repositories.
    #[must_use]
    pub fn repos(mut self, repos: Arc<suzatary_io::repos::Mirrors>) -> Self {
        self.repos = repos;
        self
    }

    /// A bundle whose Microsoft accounts' Junk is `junk`.
    #[must_use]
    pub fn junk(mut self, junk: FakeJunk) -> Self {
        self.junk = junk;
        self
    }

    /// A bundle whose Drive is `drive`.
    #[must_use]
    pub fn drive(mut self, drive: &FakeDrive) -> Self {
        self.drive.drive = Some(drive.proton());
        self
    }

    /// A bundle whose `drive_read` reads files under `folder` at once.
    #[must_use]
    pub fn allowing_folder(mut self, folder: &str) -> Self {
        self.drive
            .allow
            .push(folder.parse().expect("a Drive folder"));
        self
    }

    /// A bundle whose `web` verb may read hosts this rule covers.
    #[must_use]
    pub fn allowing(mut self, rule: &str) -> Self {
        self.web.allow.push(rule.parse().expect("an allow rule"));
        self
    }
}

impl Fakes {
    /// The server's record over these fakes.
    #[must_use]
    /// The memory, both halves and the system's salt.
    pub fn memories(&self) -> suzatary_core::secretary::memories::Memories<'_> {
        suzatary_core::secretary::memories::Memories {
            history: &self.memory,
            texts: &self.memory_texts,
            cards: &self.inbox,
            entropy: &suzatary_io::texts::SystemEntropy,
            digests: suzatary_core::secretary::digests::Digests {
                history: &self.digests,
                texts: &self.digest_texts,
                lines: self.conversations.conversations(),
                owners: &self.owners,
                compacting: &suzatary_core::secretary::compacting::Compacting::Off,
            },
        }
    }

    /// The secretary's record over these fakes.
    #[must_use]
    pub fn secretary(&self) -> suzatary_core::capability::Secretary<'_> {
        suzatary_core::capability::Secretary {
            chain: &self.chain,
            corpus: &self.corpus,
            mail: &self.mailbox,
            repository: &self.repository,
            manuals: &self.manuals,
            render: &self.render,
            clock: &self.clock,
            model: &self.model,
            conversations: self.conversations.conversations(),
            inbox: &self.inbox,
            memories: self.memories(),
            decisions: suzatary_core::decisions::Decisions {
                store: &self.decisions,
            },
            topics: self.topics.stores(),
            briefings: &self.briefings,
            weather: &FakeWeather,
            calendar: &self.calendar,
            secrets: self.secrets.as_ref(),
            web: &self.web,
            repositories: self.repos.as_ref(),
            drive: &self.drive,
            junk: &self.junk,
            sync: &suzatary_core::capability::Nothing,
            older: &suzatary_core::capability::Nothing,
            uploads: &self.files,
            upload_index: &self.upload_index,
            kept: &self.files,
            configuration: self.configuration.as_ref(),
            watches: &self.watches,
        }
    }

    pub fn server(&self) -> suzatary_core::capability::Server<'_> {
        suzatary_core::capability::Server {
            secretary: self.secretary(),
            whois: &self.whois,
            notify: &self.notify,
            answers: &self.answers,
            devices: &self.devices,
            usage: &suzatary_core::capability::Nothing,
            sessions: &self.model,
        }
    }
}

/// A fixed forecast, never a socket: no test here is about the weather.
pub struct FakeWeather;

impl suzatary_core::capability::Weather for FakeWeather {
    fn today(&self) -> Result<String, suzatary_core::capability::WeatherError> {
        Ok("62-71°F, clear, rain chance peaks at 5% — no umbrella needed.".to_owned())
    }
}

/// The web: the rules a scene allows, and every request recorded and answered with a line
/// naming it, never the network.
#[derive(Default)]
pub struct FakeWeb {
    /// Hosts the `web` verb reads without asking.
    pub allow: Vec<suzatary_core::web::AllowRule>,
    /// Every web request performed, oldest first.
    pub done: std::sync::Mutex<Vec<suzatary_core::web::WebRequest>>,
}

impl suzatary_core::capability::Web for FakeWeb {
    fn allow_rules(&self) -> Vec<suzatary_core::web::AllowRule> {
        self.allow.clone()
    }

    fn perform(
        &self,
        request: &suzatary_core::web::WebRequest,
    ) -> Result<suzatary_core::capability::WebAnswer, suzatary_core::capability::WebFailure> {
        self.done
            .lock()
            .expect("the web log lock")
            .push(request.clone());
        Ok(suzatary_core::capability::WebAnswer {
            url: request.sentence(),
            status: 200,
            text: format!("the page for: {}", request.sentence()),
        })
    }
}

/// The Drive through the real interpreter over a `FakeDrive` when a scene gave one,
/// refusing otherwise.
#[derive(Default)]
pub struct FakeDriveAccess {
    /// Drive folders read without asking.
    pub allow: Vec<suzatary_core::drive::DrivePath>,
    /// The Drive, when a scene gives one.
    pub drive: Option<suzatary_io::drive::ProtonDrive>,
}

impl suzatary_core::capability::Drive for FakeDriveAccess {
    fn allowed_folders(&self) -> Vec<suzatary_core::drive::DrivePath> {
        self.allow.clone()
    }

    fn port(&self) -> &dyn suzatary_core::drive::Port {
        match &self.drive {
            Some(drive) => drive,
            None => &NoDrive,
        }
    }

    fn read(
        &self,
        file: &suzatary_core::drive::DrivePath,
    ) -> Result<FileRead, suzatary_core::capability::DriveFailure> {
        self.drive
            .as_ref()
            .ok_or_else(no_drive)
            .and_then(|drive| drive.read(file))
    }
}

/// A scene with no Drive: every operation says so.
struct NoDrive;

impl suzatary_core::drive::Port for NoDrive {
    fn list(
        &self,
        _: &suzatary_core::drive::DrivePath,
    ) -> Result<Vec<suzatary_core::drive::DriveEntry>, suzatary_core::capability::DriveFailure>
    {
        Err(no_drive())
    }
    fn download(
        &self,
        _: &suzatary_core::drive::DrivePath,
    ) -> Result<Vec<u8>, suzatary_core::capability::DriveFailure> {
        Err(no_drive())
    }
}

/// The owner's uploads and the files kept for the model, held in memory.
#[derive(Default)]
pub struct FakeFiles {
    /// Uploads by id.
    pub uploads: HashMap<suzatary_core::attachment::AttachmentId, suzatary_core::upload::Upload>,
    /// Every file a verb kept for the model, in order.
    pub kept: Mutex<Vec<Vec<u8>>>,
}

impl
    suzatary_core::store::Doc<
        suzatary_core::attachment::AttachmentId,
        suzatary_core::upload::Upload,
    > for FakeFiles
{
    fn get(
        &self,
        id: &suzatary_core::attachment::AttachmentId,
    ) -> Result<Option<suzatary_core::upload::Upload>, suzatary_core::store::StoreError> {
        Ok(self.uploads.get(id).cloned())
    }

    fn set(
        &self,
        id: &suzatary_core::attachment::AttachmentId,
        _upload: &suzatary_core::upload::Upload,
    ) -> Result<(), suzatary_core::store::StoreError> {
        Err(suzatary_core::store::StoreError::unwritable(
            "upload store",
            format!("{id} is an upload"),
        ))
    }

    fn remove(
        &self,
        id: &suzatary_core::attachment::AttachmentId,
    ) -> Result<(), suzatary_core::store::StoreError> {
        Err(suzatary_core::store::StoreError::unwritable(
            "upload store",
            format!("{id} is an upload"),
        ))
    }
}

impl suzatary_core::store::Doc<suzatary_core::attachment::AttachmentId, Vec<u8>> for FakeFiles {
    fn get(
        &self,
        id: &suzatary_core::attachment::AttachmentId,
    ) -> Result<Option<Vec<u8>>, suzatary_core::store::StoreError> {
        Ok(self
            .kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|bytes| &suzatary_core::attachment::AttachmentId::of(bytes) == id)
            .cloned())
    }

    fn set(
        &self,
        _id: &suzatary_core::attachment::AttachmentId,
        bytes: &Vec<u8>,
    ) -> Result<(), suzatary_core::store::StoreError> {
        self.kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(bytes.clone());
        Ok(())
    }

    fn remove(
        &self,
        id: &suzatary_core::attachment::AttachmentId,
    ) -> Result<(), suzatary_core::store::StoreError> {
        self.kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|bytes| &suzatary_core::attachment::AttachmentId::of(bytes) != id);
        Ok(())
    }
}

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("a tokio runtime"))
}

/// A path no build ever writes, so the server serves its "not built" page.
#[must_use]
pub fn no_dist() -> PathBuf {
    std::env::temp_dir().join("suzatary-cli-tests-no-such-dist")
}

/// Start a default `Server` over `caps` and answer its base URL.
#[must_use]
pub fn start(caps: Arc<Fakes>, deploying: Deployment, this_box: Vec<IpAddr>) -> String {
    Server::new(caps, deploying, this_box).start().base
}

/// What performs a tapped act, built at `Server::start` so it shares the
/// server's worker table.
enum Performer {
    /// Every act recorded, none performed.
    Recorded,
    /// `Acting::Recording`: a tap completes in the call that begins it, so a
    /// scene is deterministic.
    #[cfg(feature = "fixture")]
    Recording(Box<suzatary_io::conversations::ConversationStores>),
    /// Every act recorded as [`Performer::Recorded`] does, but a Connect card's, which begins a
    /// sign-in through the server's own table, running the `claude` its [`Reaching`] names.
    SigningIn,
    /// `Acting::Real` over a scripted OAuth transport and a one-account
    /// roster, so a test drives the whole device flow through the real route.
    Real {
        tag: String,
        /// The one Microsoft address its roster declares, or `None` for the roster the server's
        /// instance file declares as it reads at the tap.
        account: Option<String>,
        conversations: Box<suzatary_io::conversations::ConversationStores>,
        oauth: ScriptedOauth,
    },
}

/// A test server builder: `new`, then the settings a scene needs, then `start`.
///
/// Every effect defaults to a recorder, so nothing in this suite is sent. A
/// performer's conversation store must be the one `caps` reads, or its appends
/// and the route's reads would be two different files.
#[must_use]
pub struct Server {
    caps: Arc<Fakes>,
    effects: Box<Swap>,
    deploying: Deployment,
    /// The addresses the daemon names as this box's own node, before it asks `caps`' whois.
    this_box: Vec<IpAddr>,
    bind: IpAddr,
    dist: PathBuf,
    settings: SettingsFixture,
    failures: Vec<suzatary_core::secretary::responded::Performed>,
    performer: Performer,
    idle: Option<std::time::Duration>,
    publishing: Option<Arc<suzatary_io::repos::Mirrors>>,
    drive: Option<suzatary_io::drive::ProtonDrive>,
    firing: Option<suzatary_cli::commands::coding::Firing>,
    keys: Option<suzatary_core::signatures::BoxKeys>,
    models: Reaching,
    checks: Option<Arc<CheckRecorder>>,
    watching: Option<PathBuf>,
    google: Googled,
    reveal: Option<Arc<dyn suzatary_core::held_value::Reveal + Send + Sync>>,
}

/// Where a test server asks Google and keeps what a sign-in with Google keeps: Google's own and
/// a store holding nothing, so nothing is ever asked, unless a scene gives it a fake.
pub struct Googled {
    /// The endpoints.
    pub endpoints: suzatary_core::google::Endpoints,
    /// The store a client and a grant are read from.
    pub store: Arc<dyn suzatary_io::secrets::Resolves + Send + Sync>,
}

impl Default for Googled {
    fn default() -> Self {
        Self {
            endpoints: suzatary_core::google::Endpoints::google(),
            store: Arc::new(serve::Unresolved),
        }
    }
}

impl Googled {
    /// What a tapped calendar event is written through.
    fn grants(&self) -> suzatary_io::google::Grants {
        suzatary_io::google::Grants::new(
            suzatary_io::google::Google::new(self.endpoints.clone()),
            Arc::clone(&self.store),
        )
    }
}

/// How a test server reaches its models: the `claude` a sign-in runs, what the register says of
/// the model (a server whose model answers, by default), and the test call a credential takes.
pub struct Reaching {
    /// The program a tapped Connect card runs.
    pub claude: String,
    /// The register's first reading; `None` for the server's own default.
    pub answered: Option<suzatary_core::mail::health::Healths>,
    /// What a credential's test call answers.
    pub verify: Arc<VerifyRecorder>,
}

impl Default for Reaching {
    fn default() -> Self {
        Self {
            claude: "/nonexistent/claude".to_owned(),
            answered: None,
            verify: Arc::new(VerifyRecorder::default()),
        }
    }
}

impl Reaching {
    /// `state`, reaching its models so, its sign-ins held in `signing`, having filed what the box
    /// owes as `listen` does before the first request.
    fn serving(self, state: State, signing: Arc<serve::sign_ins::SignIns>) -> State {
        let state = state.with_sign_ins(signing, self.verify);
        let state = match self.answered {
            Some(answered) => state.with_answered(Arc::new(serve::Remembered::holding(answered))),
            None => state,
        };
        // Filed from four threads at once, as a start, an hourly tick and two taps may: the
        // filing lock makes it one filing's cards, which every scene then reads.
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| state.notice());
            }
        });
        state
    }
}

/// Every credential a tap asked to be tested, by name; each answers unless `refuses` names it.
#[derive(Default)]
pub struct VerifyRecorder {
    /// The names asked about, in order.
    pub asked: Mutex<Vec<String>>,
    /// A name whose test call does not answer.
    pub refuses: Option<String>,
}

impl suzatary_core::settings::Verify for VerifyRecorder {
    fn verify(
        &self,
        name: &suzatary_core::settings::Settable,
        _: &suzatary_core::capability::Secret,
    ) -> Result<(), String> {
        self.asked
            .lock()
            .expect("unpoisoned")
            .push(name.name().to_owned());
        if self.refuses.as_deref() == Some(name.name()) {
            Err("401 from the fake model".to_owned())
        } else {
            Ok(())
        }
    }
}

/// The record a started server serves.
type Effects = suzatary_core::capability::Server<'static>;

/// A change a scene makes to the fakes' record.
type Swap = dyn FnOnce(Effects) -> Effects;

/// A started `Server`: its base URL and what its effects recorded.
pub struct Running {
    /// `http://<address>`.
    pub base: String,
    /// Every message a tap would have sent, except the ones made to fail.
    pub sent: Arc<Recorder>,
    /// Every act a tap would have performed, under the default performer.
    pub acts: Arc<ActRecorder>,
    /// The conversation worker table, so a test observes workers rather than
    /// guessing at timing.
    pub workers: Arc<serve::Workers>,
    /// Every file the owner said to keep private.
    pub vault: Arc<FakeVault>,
    /// Where the requests devices signed and the server admitted are recorded.
    pub requests: suzatary_io::requests::Requests,
    /// The last pairing code, as `suzatary device pair` would keep it.
    pub pairings: Arc<suzatary_core::signatures::InMemoryPairings>,
}

impl Running {
    /// The port the server listens on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.base
            .rsplit(':')
            .next()
            .and_then(|port| port.parse().ok())
            .expect("a base ends in its port")
    }
}

/// A Drive that keeps what it is handed, in memory.
#[derive(Default)]
pub struct FakeVault(pub Mutex<Vec<(String, Vec<u8>)>>);

impl suzatary_core::vault::Vault for FakeVault {
    fn keep(
        &self,
        name: &str,
        bytes: &[u8],
    ) -> Result<suzatary_core::drive::DrivePath, suzatary_core::capability::DriveFailure> {
        self.0
            .lock()
            .expect("unpoisoned")
            .push((name.to_owned(), bytes.to_vec()));
        suzatary_core::vault::folder()
            .join(name)
            .map_err(|failed| suzatary_core::capability::DriveFailure::Refused(failed.to_string()))
    }
}

impl Server {
    /// Bound to `v4`, no frontend, no settings, every effect recorded.
    pub fn new(caps: Arc<Fakes>, deploying: Deployment, this_box: Vec<IpAddr>) -> Self {
        Self {
            caps,
            effects: Box::new(|effects| effects),
            deploying,
            this_box,
            bind: v4(),
            dist: no_dist(),
            settings: SettingsFixture::empty(),
            failures: Vec::new(),
            performer: Performer::Recorded,
            idle: None,
            publishing: None,
            drive: None,
            firing: None,
            keys: Some(cast_keys()),
            models: Reaching::default(),
            checks: None,
            watching: None,
            google: Googled::default(),
            reveal: None,
        }
    }

    /// Reading held values back through `reveal`, as a box over its mail does.
    pub fn reveal(self, reveal: Arc<dyn suzatary_core::held_value::Reveal + Send + Sync>) -> Self {
        Self {
            reveal: Some(reveal),
            ..self
        }
    }

    /// Reading the instance file at `path` again whenever it moves, as `serve` does.
    pub fn watching(self, path: PathBuf) -> Self {
        Self {
            watching: Some(path),
            ..self
        }
    }

    /// Recording into `checks` each plugin a tap syncs, rather than syncing none.
    pub fn checks(self, checks: Arc<CheckRecorder>) -> Self {
        Self {
            checks: Some(checks),
            ..self
        }
    }

    /// Asking Google at `endpoints`, reading a client and a grant from `store`.
    pub fn google(
        self,
        endpoints: suzatary_core::google::Endpoints,
        store: Arc<dyn suzatary_io::secrets::Resolves + Send + Sync>,
    ) -> Self {
        Self {
            google: Googled { endpoints, store },
            ..self
        }
    }

    /// Reaching its models as `models` says.
    pub fn reaching(self, models: Reaching) -> Self {
        Self { models, ..self }
    }

    /// With the fakes' record changed by `swap`, as a scene substitutes one effect.
    pub fn effects(self, swap: impl FnOnce(Effects) -> Effects + 'static) -> Self {
        Self {
            effects: Box::new(swap),
            ..self
        }
    }

    /// Bound to `v6`, the one other peer identity this process can present as.
    pub fn v6(self) -> Self {
        Self { bind: v6(), ..self }
    }

    /// Serving `dist` as the frontend (or the "not built" page, when it is
    /// not a directory).
    pub fn dist(self, dist: &Path) -> Self {
        Self {
            dist: dist.to_path_buf(),
            ..self
        }
    }

    /// With `/api/overview` and the settings writes over `settings`.
    pub fn settings(self, settings: SettingsFixture) -> Self {
        Self { settings, ..self }
    }

    /// With the transport failing the next sends with `failures`, in order.
    pub fn failing(
        self,
        failures: impl IntoIterator<Item = suzatary_core::secretary::responded::Performed>,
    ) -> Self {
        Self {
            failures: failures.into_iter().collect(),
            ..self
        }
    }

    /// With a performer that begins a tapped Connect card's sign-in for real.
    pub fn signing_in(self) -> Self {
        Self {
            performer: Performer::SigningIn,
            ..self
        }
    }

    /// With the fixture's `Acting::Recording` as the performer.
    #[cfg(feature = "fixture")]
    pub fn recording_acts(
        self,
        conversations: suzatary_io::conversations::ConversationStores,
    ) -> Self {
        Self {
            performer: Performer::Recording(Box::new(conversations)),
            ..self
        }
    }

    /// With `Acting::Real` as the performer: `account` is the one Microsoft
    /// address the roster knows, `tag` roots the token store.
    pub fn real_acts(
        self,
        tag: &str,
        account: &str,
        conversations: suzatary_io::conversations::ConversationStores,
        oauth: ScriptedOauth,
    ) -> Self {
        Self {
            performer: Performer::Real {
                tag: tag.to_owned(),
                account: Some(account.to_owned()),
                conversations: Box::new(conversations),
                oauth,
            },
            ..self
        }
    }

    /// With `Acting::Real` as the performer, resolving an account against the instance file the
    /// settings' editor reads at the tap.
    pub fn real_acts_over_instance(
        self,
        tag: &str,
        conversations: suzatary_io::conversations::ConversationStores,
        oauth: ScriptedOauth,
    ) -> Self {
        Self {
            performer: Performer::Real {
                tag: tag.to_owned(),
                account: None,
                conversations: Box::new(conversations),
                oauth,
            },
            ..self
        }
    }

    /// With this routine behind a tapped coding session; a test points it at
    /// loopback, never the real one.
    pub fn firing(self, firing: suzatary_cli::commands::coding::Firing) -> Self {
        Self {
            firing: Some(firing),
            ..self
        }
    }

    /// Signing with the box's keys, whose root a tapped device card registers keys with.
    pub fn keys(self, keys: suzatary_core::signatures::BoxKeys) -> Self {
        Self {
            keys: Some(keys),
            ..self
        }
    }

    /// With these repositories behind a tapped patch, rather than none.
    pub fn publishing(self, mirrors: Arc<suzatary_io::repos::Mirrors>) -> Self {
        Self {
            publishing: Some(mirrors),
            ..self
        }
    }

    /// With this fake Drive behind a tapped document.
    pub fn reading(self, drive: &FakeDrive) -> Self {
        Self {
            drive: Some(drive.proton()),
            ..self
        }
    }

    /// With a chosen idle wait instead of the worker's real thirty seconds,
    /// for a test that watches a worker drop itself.
    pub fn idle(self, idle: std::time::Duration) -> Self {
        Self {
            idle: Some(idle),
            ..self
        }
    }

    /// Every deployment signs, so the owner's phone is trusted for each actor a login writes as,
    /// by the root, before the first request: as `device trust` at the terminal does.
    fn trust_the_phone(&self) {
        if let Some(keys) = &self.keys
            && keys.root().public() == self.deploying.chains.first().root
        {
            let actors: std::collections::BTreeSet<&Actor> =
                self.deploying.actors.values().collect();
            for actor in actors {
                suzatary_core::signatures::trust(
                    &self.caps.chain,
                    keys.root(),
                    actor,
                    &phone().public(),
                )
                .expect("the phone is trusted at the terminal");
            }
        }
    }

    /// The same server starting with `names` settable.
    pub fn settings_names(mut self, names: suzatary_core::settings::Settables) -> Self {
        self.settings.live.settables = names;
        self
    }

    /// The same server deleting a removed account's sign-ins from `tokens`.
    pub fn grants(mut self, tokens: suzatary_io::oauth::TokenStore) -> Self {
        self.settings.grants = Some(tokens);
        self
    }

    /// Bind and serve until the process exits; test binaries are short-lived,
    /// so nothing shuts it down.
    #[must_use]
    pub fn start(self) -> Running {
        self.trust_the_phone();
        let (listener, address) = bound(self.bind);
        let workers = Arc::new(
            self.idle
                .map_or_else(serve::Workers::new, serve::Workers::with_idle),
        );
        let (sent, acts) = (Arc::new(Recorder::new()), Arc::new(ActRecorder::new()));
        let postman: Arc<dyn suzatary_core::secretary::routes::Post + Send + Sync> =
            Arc::new(FailsFirst {
                failures: Mutex::new(self.failures.into()),
                sent: Arc::clone(&sent),
            });
        let publishing = publishing(self.publishing);
        let drive = Arc::new(self.drive.unwrap_or_else(drive_nowhere).read_only());
        // The server outlives every test, so its fakes are held for the process.
        let held: &'static Fakes = Box::leak(Box::new(Arc::clone(&self.caps)));
        let signing = Arc::new(serve::sign_ins::SignIns::new(self.models.claude.clone()));
        let acting = performer(
            self.performer,
            held,
            &acts,
            &workers,
            (
                publishing,
                drive,
                Arc::clone(&signing),
                Arc::clone(&self.settings.edits),
            ),
            self.firing
                .unwrap_or(suzatary_cli::commands::coding::Firing::Undeclared),
            Arc::new(self.google.grants()),
        );
        let consenting = serve::google::Consenting::new(
            suzatary_io::google::Google::new(self.google.endpoints.clone()),
            Arc::clone(&self.google.store),
            Arc::clone(&self.settings.edits),
            &self.deploying.app_url,
        );
        let vault = Arc::new(FakeVault::default());
        let requests = recorded_requests(address.port());
        let pairings = Arc::new(suzatary_core::signatures::InMemoryPairings::default());
        let grants = self.settings.grants.clone();
        let (chained, deploying, door) = dialled(
            &(self.effects)(held.server()),
            self.deploying,
            self.this_box,
            address,
        );
        let state = State::over(
            chained,
            Vec::new(),
            deploying,
            door,
            serve::Performers { postman, acting },
            Arc::clone(&workers),
            self.settings.over(Arc::new(held.root.clone())),
        )
        .with_keys(self.keys)
        .with_pairings(pairings.clone())
        .with_requests(requests.clone())
        .with_wall(stamped)
        // The store the fakes' upload index reads, under their scratch root.
        .with_uploads(upload_store(&held.root), vault.clone());
        let state = match grants {
            Some(tokens) => state.with_grants(tokens),
            None => state,
        };
        let state = match self.checks {
            Some(checks) => state.with_checks(checks),
            None => state,
        }
        .with_google(consenting);
        let state = match self.reveal {
            Some(reveal) => state.with_reveal(reveal),
            None => state,
        };
        let state = self.models.serving(state, signing);
        if let Some(path) = self.watching {
            runtime().spawn(serve::instance_watched(state.clone(), path));
        }
        served(listener, serve::app(state, &self.dist));
        Running {
            base: format!("http://{address}"),
            sent,
            acts,
            workers,
            vault,
            requests,
            pairings,
        }
    }
}

/// How a test server publishes: through `mirrors`, or by default through a mirror of no repos,
/// under a fixed author.
fn publishing(
    mirrors: Option<Arc<suzatary_io::repos::Mirrors>>,
) -> Arc<suzatary_cli::commands::secretary::Publishing> {
    Arc::new(suzatary_cli::commands::secretary::Publishing {
        mirrors: mirrors.unwrap_or_else(|| {
            Arc::new(suzatary_io::repos::Mirrors::new(
                std::env::temp_dir().join("suzatary-test-no-repos"),
                suzatary_io::repos::Source::Local {
                    base: std::env::temp_dir().join("suzatary-test-no-remotes"),
                },
                suzatary_core::repos::Declared::default(),
                Box::new(NoSecrets),
            ))
        }),
        // Fixed, so a scene never depends on who runs it having a git identity.
        author: Some(suzatary_io::repos::Author {
            name: "Ada Test".to_owned(),
            email: "ada@example.test".to_owned(),
        }),
    })
}

/// Where a server records the requests it admitted, and keeps their answers, emptied first: a
/// port is reused across runs, and a record is the run's own.
fn recorded_requests(port: u16) -> suzatary_io::requests::Requests {
    let dir = requests_dir(port);
    let answered = std::env::temp_dir().join(format!("suzatary-test-answers-{port}"));
    for scratch in [&dir, &answered] {
        let _ = std::fs::remove_dir_all(scratch);
    }
    suzatary_io::requests::Requests::open(dir, answered)
}

/// The directory a server on `port` pushes its request records from.
#[must_use]
pub fn requests_dir(port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("suzatary-test-requests-{port}"))
}

/// The box's clock in UTC as a request's window reads it: [`STAMPED`].
fn stamped() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(STAMPED)
        .expect("an instant")
        .with_timezone(&chrono::Utc)
}

/// The owner's uploads under a scratch root, as the instance lays them out.
fn upload_store(root: &std::path::Path) -> suzatary_io::attachments::AttachmentStore {
    suzatary_io::attachments::AttachmentStore::new(
        root.join("secretary/attachments"),
        suzatary_io::pdftotext::Pdftotext::new("."),
    )
}

/// Where a tapped document is kept for the model, under the scene's own root.
fn kept_store(root: &std::path::Path) -> suzatary_io::blobs::Blobs {
    suzatary_io::blobs::Blobs::open(root.join("secretary").join("files"))
}

/// A Drive CLI nothing is at: a tapped document in a scene that did not ask for a Drive fails,
/// never reads.
fn drive_nowhere() -> suzatary_io::drive::ProtonDrive {
    suzatary_io::drive::ProtonDrive::new(
        std::env::temp_dir().join("suzatary-test-no-proton-drive"),
        std::env::temp_dir().join("suzatary-test-no-drive-scratch"),
        PathBuf::from("."),
    )
}

/// The chosen performer, over the server's own worker table and, where it
/// The same conversations' store opened again, for a second holder of it.
pub fn again(conversations: &Conversations) -> Conversations {
    conversations.reopened()
}

/// writes decisions, the same chain `caps` serves.
fn performer(
    chosen: Performer,
    caps: &'static Fakes,
    acts: &Arc<ActRecorder>,
    workers: &Arc<serve::Workers>,
    (publishing, drive, signing, edits): (
        Arc<suzatary_cli::commands::secretary::Publishing>,
        Arc<suzatary_io::drive::ReadOnly>,
        Arc<serve::sign_ins::SignIns>,
        Arc<dyn suzatary_core::settings::Edits + Send + Sync>,
    ),
    firing: suzatary_cli::commands::coding::Firing,
    google: Arc<suzatary_io::google::Grants>,
) -> Arc<dyn suzatary_core::secretary::routes::Perform + Send + Sync> {
    use suzatary_cli::commands::coding::Coder;
    use suzatary_cli::commands::secretary::Acting;
    match chosen {
        Performer::Recorded => Arc::clone(acts) as _,
        Performer::SigningIn => Arc::new(SignsIn {
            recorded: Arc::clone(acts),
            signing,
        }),
        #[cfg(feature = "fixture")]
        Performer::Recording(conversations) => Arc::new(Acting::Recording {
            coder: Arc::new(Coder {
                firing,
                conversations: again(&conversations),
            }),
            conversations: Box::leak(conversations),
            workers: Arc::clone(workers),
            publishing,
            drive,
            kept: kept_store(&caps.root),
            signing,
            google,
        }),
        Performer::Real {
            tag,
            account,
            conversations,
            oauth,
        } => {
            use suzatary_core::mail::account::{Authority, MailAccount, MsImap, Sync as MailSync};
            let roster: Arc<dyn suzatary_core::settings::Edits + Send + Sync> = match account {
                Some(account) => {
                    Arc::new(EditsRecorder::declaring(suzatary_core::connect::Declared {
                        accounts: vec![MailAccount {
                            email: suzatary_core::mail::account_email::AccountEmail::new(&account)
                                .expect("a lower-case address"),
                            subdir: "study".to_owned(),
                            sync: MailSync::MsImap(MsImap {
                                authority: Authority::Organizations,
                                inbox_folders: suzatary_core::mail::folders::Folders::inbox(),
                                sent_folder: "Sent Items".to_owned(),
                            }),
                        }],
                        ..suzatary_core::connect::Declared::default()
                    }))
                }
                None => edits,
            };
            let root = scratch(&format!("{tag}-acting-real"));
            let tokens =
                suzatary_io::oauth::TokenStore::with_http(root.join("oauth"), Arc::new(oauth));
            Arc::new(Acting::Real {
                roster,
                coder: Arc::new(Coder {
                    firing,
                    conversations: again(&conversations),
                }),
                conversations: Box::leak(conversations),
                tokens,
                // The same store `caps` reads, so an act's end lands where
                // the test's own reads find it.
                inbox: &caps.inbox,
                workers: Arc::clone(workers),
                publishing,
                drive,
                kept: kept_store(&caps.root),
                signing,
                google,
            })
        }
    }
}

/// The shared client, keeping 4xx/5xx bodies that `ureq` discards by default.
#[must_use]
pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            // No keep-alive: a refusal closes the socket, and a pooled POST
            // onto it fails instead of answering the asserted 403.
            .max_idle_connections(0)
            .build()
            .new_agent()
    })
}

/// A plain read.
#[must_use]
pub fn get(url: &str) -> ureq::http::Response<ureq::Body> {
    agent().get(url).call().expect("the request completes")
}

/// A write as the app makes one: the CSRF triple over a JSON body, from a page that keeps the
/// box's write contract, signed by the owner's [`phone`] with a nonce of its own, so two acts a
/// test makes alike are two requests.
#[must_use]
pub fn act(url: &str, body: &serde_json::Value) -> ureq::http::Response<ureq::Body> {
    static DRAWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let nonce = format!(
        "{:032x}",
        DRAWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    sent_signed(url, body, &phone(), STAMPED, &nonce)
}

/// `object`'s print, then the print of the owner's [`phone`]'s signature of it: what a tab
/// posts for one write.
#[must_use]
pub fn signed_prints(object: &suzatary_core::record::Envelope) -> Vec<String> {
    let name = prodrome_core::event::seal_hash(object);
    vec![
        prodrome_core::event::canonical_envelope(object),
        prodrome_core::event::canonical_envelope(&suzatary_core::record::Envelope::Signed(
            phone().sign(&name),
        )),
    ]
}

/// [`act`] with no signature at all, as an older page or a stranger's script sends one.
#[must_use]
pub fn act_unsigned(url: &str, body: &serde_json::Value) -> ureq::http::Response<ureq::Body> {
    agent()
        .post(url)
        .header("x-suzatary-act", "1")
        .header(
            suzatary_core::gate::CLIENT_HEADER,
            suzatary_core::gate::CLIENT_MIN.to_string(),
        )
        .header("sec-fetch-site", "same-origin")
        .send_json(body)
        .expect("the request completes")
}

/// The cast's deployment keys: its root, and the agent's.
#[must_use]
pub fn cast_keys() -> suzatary_core::signatures::BoxKeys {
    use suzatary_core::cast::keys;
    suzatary_core::signatures::BoxKeys::of(
        &keys::root(),
        &suzatary_core::capability::Secret::new(keys::ROOT),
        &suzatary_core::capability::Secret::new(keys::AGENT),
    )
    .expect("the cast's keys")
}

/// The owner's phone: the device every scene's [`act`] is signed by, trusted when a server
/// starts.
#[must_use]
pub fn phone() -> prodrome_core::sign::Secret {
    suzatary_core::cast::keys::secret(suzatary_core::cast::keys::PHONE)
}

/// The instant a test's device stamps on its requests: the fake clock's own, and every test
/// server's UTC now as a request's window reads it.
pub const STAMPED: &str = "2026-09-02T12:00:00.000Z";

/// [`act`], as a device holding `key` sends it under a deployment that signs, stamped
/// [`STAMPED`]: the request named by its content and signed, as `web/src/wire/http.ts` does.
pub fn act_signed(
    url: &str,
    body: &serde_json::Value,
    key: &prodrome_core::sign::Secret,
) -> ureq::http::Response<ureq::Body> {
    act_signed_at(url, body, key, STAMPED)
}

/// The nonce a test's device draws for every request: a copy of a request is the same request,
/// and two a test means apart differ by what they ask.
pub const NONCE: &str = "0123456789abcdef0123456789abcdef";

/// The name of the request [`act_signed_at`] sends for `key`.
#[must_use]
pub fn request_named(
    url: &str,
    body: &serde_json::Value,
    at: &str,
    key: &prodrome_core::sign::PublicKey,
) -> prodrome_core::event::Hash {
    named_with(url, body, at, NONCE, key)
}

fn named_with(
    url: &str,
    body: &serde_json::Value,
    at: &str,
    nonce: &str,
    key: &prodrome_core::sign::PublicKey,
) -> prodrome_core::event::Hash {
    let bytes = serde_json::to_vec(body).expect("JSON prints");
    suzatary_core::signatures::request_name(
        "POST",
        &target_of(url),
        at,
        nonce,
        &suzatary_core::signatures::body_digest(&bytes),
        key.as_str(),
    )
}

fn target_of(url: &str) -> String {
    url.splitn(4, '/')
        .nth(3)
        .map_or_else(|| "/".to_owned(), |rest| format!("/{rest}"))
}

/// [`act_signed`] stamped `at`.
pub fn act_signed_at(
    url: &str,
    body: &serde_json::Value,
    key: &prodrome_core::sign::Secret,
    at: &str,
) -> ureq::http::Response<ureq::Body> {
    sent_signed(url, body, key, at, NONCE)
}

fn sent_signed(
    url: &str,
    body: &serde_json::Value,
    key: &prodrome_core::sign::Secret,
    at: &str,
    nonce: &str,
) -> ureq::http::Response<ureq::Body> {
    sent_bytes_signed(
        url,
        &serde_json::to_vec(body).expect("JSON prints"),
        key,
        at,
        nonce,
    )
}

/// `bytes` posted as the owner's [`phone`] signs a request, whatever the bytes are: what a
/// route's guard and door see of a write before its body is read.
#[must_use]
pub fn act_bytes(url: &str, bytes: &[u8]) -> ureq::http::Response<ureq::Body> {
    sent_bytes_signed(url, bytes, &phone(), STAMPED, NONCE)
}

fn sent_bytes_signed(
    url: &str,
    bytes: &[u8],
    key: &prodrome_core::sign::Secret,
    at: &str,
    nonce: &str,
) -> ureq::http::Response<ureq::Body> {
    let named = suzatary_core::signatures::request_name(
        "POST",
        &target_of(url),
        at,
        nonce,
        &suzatary_core::signatures::body_digest(bytes),
        key.public().as_str(),
    );
    let signed = key.sign(&named);
    agent()
        .post(url)
        .header("x-suzatary-act", "1")
        .header(
            suzatary_core::gate::CLIENT_HEADER,
            suzatary_core::gate::CLIENT_MIN.to_string(),
        )
        .header("sec-fetch-site", "same-origin")
        .header("content-type", "application/json")
        .header("x-suzatary-at", at)
        .header("x-suzatary-nonce", nonce)
        .header("x-suzatary-key", signed.key.as_str())
        .header("x-suzatary-signature", signed.signature.as_str())
        .send(bytes)
        .expect("the request completes")
}

/// A read as a tab under a deployment that signs sends one: named over its method, target,
/// [`STAMPED`], [`NONCE`] and an empty body, and signed by `key`.
pub fn get_signed(
    url: &str,
    key: &prodrome_core::sign::Secret,
) -> ureq::http::Response<ureq::Body> {
    let name = suzatary_core::signatures::request_name(
        "GET",
        &target_of(url),
        STAMPED,
        NONCE,
        &suzatary_core::signatures::body_digest(b""),
        key.public().as_str(),
    );
    let signed = key.sign(&name);
    agent()
        .get(url)
        .header("x-suzatary-at", STAMPED)
        .header("x-suzatary-nonce", NONCE)
        .header("x-suzatary-key", signed.key.as_str())
        .header("x-suzatary-signature", signed.signature.as_str())
        .call()
        .expect("the request completes")
}

/// The body of a response, as JSON.
pub fn body_json(response: &mut ureq::http::Response<ureq::Body>) -> serde_json::Value {
    response.body_mut().read_json().expect("a JSON body")
}

/// The status code, as a plain number.
#[must_use]
pub fn status(response: &ureq::http::Response<ureq::Body>) -> u16 {
    response.status().as_u16()
}

/// Each account's Junk folder as a value; no accounts by default.
#[derive(Default)]
pub struct FakeJunk {
    /// The churn list.
    pub churn: suzatary_core::mail::junk::ChurnList,
    /// Each account's folder.
    pub folders: Vec<(String, JunkFolder)>,
    /// Every header read, as `(account, since)`.
    pub asked: std::sync::Mutex<Vec<(String, chrono::NaiveDate)>>,
}

/// One account's Junk in a scene.
pub enum JunkFolder {
    /// Messages by UID: headers and a body.
    Holds(Vec<(suzatary_core::mail::junk::JunkHeader, &'static str)>),
    /// The account's grant lapsed.
    NeedsLogin,
}

impl FakeJunk {
    fn folder(
        &self,
        account: &str,
    ) -> Result<
        &[(suzatary_core::mail::junk::JunkHeader, &'static str)],
        suzatary_core::capability::JunkFailure,
    > {
        use suzatary_core::capability::JunkFailure;
        match self.folders.iter().find(|(name, _)| name == account) {
            Some((_, JunkFolder::Holds(held))) => Ok(held),
            Some((_, JunkFolder::NeedsLogin)) => Err(JunkFailure::NeedsLogin(
                suzatary_core::mail::oauth::NeedsLogin::Lapsed {
                    email: account.to_owned(),
                    slot: "imap",
                    code: "invalid_grant".to_owned(),
                    description: "AADSTS700082: The refresh token has expired.".to_owned(),
                },
            )),
            None => Err(JunkFailure::Unknown(account.to_owned())),
        }
    }
}

impl suzatary_core::capability::Junk for FakeJunk {
    fn junk_accounts(&self) -> Vec<String> {
        self.folders.iter().map(|(name, _)| name.clone()).collect()
    }

    fn churn(&self) -> suzatary_core::mail::junk::ChurnList {
        self.churn.clone()
    }

    fn junk_headers(
        &self,
        account: &str,
        since: chrono::NaiveDate,
    ) -> Result<Vec<suzatary_core::mail::junk::JunkHeader>, suzatary_core::capability::JunkFailure>
    {
        self.asked
            .lock()
            .expect("the junk log lock")
            .push((account.to_owned(), since));
        Ok(self
            .folder(account)?
            .iter()
            .map(|(header, _)| header.clone())
            .collect())
    }

    fn junk_message(
        &self,
        account: &str,
        uid: suzatary_core::mail::junk::Uid,
    ) -> Result<suzatary_core::mail::junk::JunkMessage, suzatary_core::capability::JunkFailure>
    {
        self.folder(account)?
            .iter()
            .find(|(header, _)| header.uid() == uid)
            .map(|(header, body)| suzatary_core::mail::junk::JunkMessage {
                account: account.to_owned(),
                header: header.clone(),
                body: (*body).to_owned(),
                truncated: false,
            })
            .ok_or(suzatary_core::capability::JunkFailure::Missing {
                account: account.to_owned(),
                uid: uid.0,
            })
    }
}

fn no_drive() -> suzatary_core::capability::DriveFailure {
    suzatary_core::capability::DriveFailure::Unreachable("no Drive in this scene".to_owned())
}

/// A fake Proton Drive CLI script that answers the two reads, logs every call
/// and exits 99 on anything else, so a write reaching it fails the scene.
///
/// `/my-files/Docs` holds `letter.txt` and a folder `Tax`;
/// `/shared-with-me/Ada Documents` holds `w2.txt`, carrying an SSN, an EIN and a
/// phone number.
pub struct FakeDrive {
    /// The scratch directory holding the script, fixtures and call log.
    pub dir: PathBuf,
}

impl FakeDrive {
    /// Write the script and its fixtures under a scratch directory.
    ///
    /// # Panics
    /// If the scratch directory cannot be written.
    #[must_use]
    pub fn new(tag: &str) -> Self {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = scratch(&format!("{tag}-drive"));
        std::fs::create_dir_all(dir.join("files")).expect("a fixture dir");
        std::fs::write(
            dir.join("docs.json"),
            r#"[
{"uid":"a","name":{"ok":true,"value":"letter.txt"},"type":"file","activeRevision":{"claimedSize":58,"claimedModificationTime":"2026-09-01T09:00:00.000Z"}},
{"uid":"b","name":{"ok":true,"value":"Tax"},"type":"folder"}
]"#,
        )
        .expect("a listing");
        std::fs::write(
            dir.join("ada.json"),
            r#"[
{"uid":"c","name":{"ok":true,"value":"w2.txt"},"type":"file","activeRevision":{"claimedSize":49356}}
]"#,
        )
        .expect("a listing");
        std::fs::write(
            dir.join("files/letter.txt"),
            "The lease renews on 1 October. Rent is unchanged.\n",
        )
        .expect("a letter");
        std::fs::write(
            dir.join("files/w2.txt"),
            "Form W-2. Employee SSN 123-45-6789. Employer EIN 12-3456789. \
             Wages 95000. Payroll 617-555-1234.\n",
        )
        .expect("a form");
        let d = dir.display();
        let script = format!(
            "#!/bin/sh\n\
             echo \"$*\" >> '{d}/calls.log'\n\
             if [ \"$1 $2 $3\" = 'filesystem list -j' ] && [ $# -eq 4 ]; then\n\
               case \"$4\" in\n\
                 '/my-files/Docs') cat '{d}/docs.json' ;;\n\
                 '/shared-with-me/Ada Documents') cat '{d}/ada.json' ;;\n\
                 '/my-files/Suzatary/Private') cat '{d}/private.json' ;;\n\
                 *) echo \"not found: $4\" >&2; exit 3 ;;\n\
               esac\n\
             elif [ \"$1 $2\" = 'filesystem download' ] && [ $# -eq 4 ]; then\n\
               case \"$3\" in\n\
                 '/my-files/Docs/letter.txt') cp '{d}/files/letter.txt' \"$4/letter.txt\" ;;\n\
                 '/shared-with-me/Ada Documents/w2.txt') cp '{d}/files/w2.txt' \"$4/w2.txt\" ;;\n\
                 '/my-files/Suzatary/Private/'*) cp \"{d}/files/private/$(basename \"$3\")\" \"$4/\" ;;\n\
                 *) echo \"cannot download $3\" >&2; exit 4 ;;\n\
               esac\n\
             else\n\
               echo \"refused: $*\" >&2; exit 99\n\
             fi\n"
        );
        let cli = dir.join("proton-drive");
        std::fs::write(&cli, script).expect("the script");
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).expect("executable");
        Self { dir }
    }

    /// The real interpreter over this script.
    #[must_use]
    pub fn proton(&self) -> suzatary_io::drive::ProtonDrive {
        suzatary_io::drive::ProtonDrive::new(
            self.dir.join("proton-drive"),
            self.dir.join("scratch"),
            PathBuf::from("."),
        )
    }

    /// The same Drive with `bytes` kept privately as `name`, as a private upload leaves it.
    ///
    /// # Panics
    /// If the scratch directory cannot be written.
    #[must_use]
    pub fn holding_private(self, name: &str, bytes: &[u8]) -> Self {
        std::fs::create_dir_all(self.dir.join("files/private")).expect("a fixture dir");
        std::fs::write(self.dir.join("files/private").join(name), bytes).expect("the file");
        std::fs::write(
            self.dir.join("private.json"),
            serde_json::json!([{
                "uid": "p",
                "name": {"ok": true, "value": name},
                "type": "file",
                "activeRevision": {"claimedSize": bytes.len()},
            }])
            .to_string(),
        )
        .expect("a listing");
        self
    }

    /// Every call the script saw, oldest first.
    #[must_use]
    pub fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Every call was one of the two reads, and no download is left behind.
    ///
    /// # Panics
    /// When a call was anything else, or a scratch directory survived.
    pub fn only_reads_ran(&self) {
        for call in self.calls() {
            assert!(
                call.starts_with("filesystem list -j /")
                    || call.starts_with("filesystem download /"),
                "a command other than a read reached the Drive CLI: {call}"
            );
        }
        let left = std::fs::read_dir(self.dir.join("scratch"))
            .into_iter()
            .flatten()
            .count();
        assert_eq!(left, 0, "a download survived its read");
    }
}

/// No secret resolves: a local repository source never asks for one.
pub struct NoSecrets;

impl suzatary_io::secrets::Resolves for NoSecrets {
    fn get(
        &self,
        _name: &str,
        _reason: &str,
    ) -> Result<Option<suzatary_core::capability::Secret>, suzatary_io::secrets::SecretError> {
        Ok(None)
    }
}

/// The compiled CV in [`local_repos`]: binary, so only an attachment carries it.
pub const CV_PDF: &[u8] = b"%PDF-1.7\n\0\x01\x02 Ada Quill\n%%EOF\n";

/// A local public repository `bmabsout/cv` holding `cv.typ` and `cv.pdf`, and mirrors over
/// it, under `root`: a scene's repositories, with no network and no token.
#[must_use]
pub fn local_repos(root: &Path) -> Arc<suzatary_io::repos::Mirrors> {
    let run = |dir: &Path, args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let work = root.join("cv-work");
    std::fs::create_dir_all(&work).expect("work dir");
    std::fs::write(work.join("cv.typ"), "= Ada Quill\nResearch engineer\n").expect("cv");
    std::fs::write(work.join("cv.pdf"), CV_PDF).expect("the compiled cv");
    run(&work, &["init", "-q", "-b", "main"]);
    run(&work, &["add", "."]);
    run(&work, &["commit", "-q", "-m", "first"]);
    std::fs::create_dir_all(root.join("remotes/bmabsout")).expect("remotes");
    run(
        root,
        &[
            "clone",
            "-q",
            "--bare",
            "cv-work",
            "remotes/bmabsout/cv.git",
        ],
    );
    Arc::new(local_mirrors(root))
}

/// Mirrors over the local repositories under `root/remotes`, declaring `bmabsout/cv` alone, with
/// `public = true`, since a local remote lists as public.
#[must_use]
pub fn local_mirrors(root: &Path) -> suzatary_io::repos::Mirrors {
    local_mirrors_declaring(
        root,
        suzatary_core::repos::RepoTarget {
            public: true,
            ..suzatary_core::repos::RepoTarget::default()
        },
    )
}

/// [`local_mirrors`], declaring `bmabsout/cv` as `target` says.
#[must_use]
pub fn local_mirrors_declaring(
    root: &Path,
    target: suzatary_core::repos::RepoTarget,
) -> suzatary_io::repos::Mirrors {
    suzatary_io::repos::Mirrors::new(
        root.join("mirrors"),
        suzatary_io::repos::Source::Local {
            base: root.join("remotes"),
        },
        suzatary_core::repos::Declared::new([("bmabsout/cv".parse().expect("a name"), target)])
            .expect("one repository"),
        Box::new(NoSecrets),
    )
}

/// One of the secretary's stores as a tab pulls it, `since` the heads it holds: its objects read
/// into their nodes, and every head.
pub fn pulled<E: prodrome_core::schema::Schema<Vocabulary: Default>>(
    base: &str,
    store: &str,
) -> Vec<prodrome_core::registers::Node<E>> {
    pulled_with_keys(base, store).0
}

/// [`pulled`], and the keys the pull carried, as a tab keeps them.
pub fn pulled_with_keys<E: prodrome_core::schema::Schema<Vocabulary: Default>>(
    base: &str,
    store: &str,
) -> (
    Vec<prodrome_core::registers::Node<E>>,
    suzatary_core::secretary::key_ring::KeyRing,
) {
    let mut answer = get(&format!("{base}/api/stores/{store}/objects"));
    assert_eq!(status(&answer), 200, "the {store} store answers");
    let payload = body_json(&mut answer);
    let keys = suzatary_core::secretary::key_ring::KeyRing::of(
        &serde_json::from_value(payload["keys"].clone()).unwrap_or_default(),
    );
    let prints = payload["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| {
            (
                prodrome_core::event::Hash::new(
                    object["hash"].as_str().expect("a name").to_owned(),
                )
                .expect("a name"),
                Ok(object["literal"]
                    .as_str()
                    .expect("a literal")
                    .as_bytes()
                    .to_vec()),
            )
        });
    let nodes = prodrome_core::dag::Dag::from_prints(&E::Vocabulary::default(), prints)
        .nodes()
        .expect("a replica of the store reads");
    (nodes, keys)
}

/// The inbox's fold and every conversation, as a tab's replicas of the two stores read them.
pub fn replicas(
    base: &str,
) -> (
    suzatary_core::secretary::inbox::Fold,
    Vec<suzatary_core::secretary::store::Stored>,
) {
    // The conversations first: the box files a proposal before the line that names it, so an inbox
    // pulled after them holds every proposal they name.
    let (lines, keys) = pulled_with_keys(base, "lines");
    let inbox = suzatary_core::secretary::inbox::Fold::of(&pulled(base, "inbox"));
    let conversations = suzatary_core::secretary::transcript::transcripts(
        &lines,
        &|name: &suzatary_core::secretary::locked_text::KeyName| keys.key(name),
    )
    .expect("the conversations read")
    .into_values()
    .rev()
    .map(suzatary_core::secretary::transcript::Transcript::into_stored)
    .collect();
    (inbox, conversations)
}

/// One conversation's window as a tab reads it from its replicas, `before`, `limit` and `about`
/// as the box's query spells them, dressed as the tab dresses it: the memory's words from its
/// route, each block's HTML from `…/blocks/<sha>`, and a proposal drawn against its chain from
/// `…/drawn?indices=<n>,…`, every proposal it draws in one read.
pub fn reading(base: &str, id: &str, query: &[(&str, &str)]) -> serde_json::Value {
    let asked = |key: &str| {
        query
            .iter()
            .find_map(|(name, value)| (*name == key).then_some(*value))
    };
    let window = suzatary_core::secretary::readings::Window {
        before: suzatary_core::query::turn_before(asked("before")).expect("a before"),
        limit: suzatary_core::query::turn_limit(
            asked("limit"),
            suzatary_core::secretary::readings::DEFAULT_TURN_WINDOW,
        )
        .expect("a limit"),
        about: suzatary_core::query::about(asked("about")).expect("an about"),
    };
    let (inbox, conversations) = replicas(base);
    let stored = conversations
        .iter()
        .find(|stored| stored.id.as_str() == id)
        .unwrap_or_else(|| panic!("no conversation named {id}"));
    let mut memory = get(&format!("{base}/api/secretary/memory"));
    let memory: Option<suzatary_core::wire::MemoryPayload> =
        (status(&memory) == 200).then(|| memory.body_mut().read_json().expect("a memory"));
    let read = suzatary_core::secretary::readings::conversation(
        stored,
        &inbox,
        &window,
        &suzatary_core::secretary::readings::Dress {
            memory: memory.as_ref(),
            ..suzatary_core::secretary::readings::Dress::BARE
        },
    );
    let mut read = serde_json::to_value(read).expect("a payload prints");
    // Only a model's turn holds blocks.
    for turn in read["turns"].as_array_mut().expect("turns") {
        let Some(blocks) = turn
            .get_mut("blocks")
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for block in blocks {
            let sha =
                suzatary_core::secretary::answer::sha(block["source"].as_str().expect("a source"));
            let mut rendered = get(&format!(
                "{base}/api/secretary/conversations/{id}/blocks/{sha}"
            ));
            if status(&rendered) == 200 {
                block["html"] = body_json(&mut rendered)["html"].clone();
            }
        }
    }
    let proposals = read["proposals"].as_array_mut().expect("proposals");
    let indices: Vec<String> = proposals
        .iter()
        .filter(|p| p["act"]["kind"] == "changes" || p["coding"]["state"] == "reported")
        .map(|p| p["index"].as_u64().expect("an index").to_string())
        .collect();
    if !indices.is_empty() {
        let mut drawn = get(&format!(
            "{base}/api/secretary/conversations/{id}/drawn?indices={}",
            indices.join(",")
        ));
        assert_eq!(status(&drawn), 200, "proposals {indices:?} draw");
        let drawn = body_json(&mut drawn)["proposals"].clone();
        for row in drawn.as_array().expect("rows") {
            let at = proposals
                .iter()
                .position(|p| p["index"] == row["index"])
                .expect("a row asked for");
            proposals[at] = row.clone();
        }
    }
    read
}

/// Every conversation, newest first, as a tab's replicas read them.
pub fn listing(base: &str) -> serde_json::Value {
    let (inbox, conversations) = replicas(base);
    serde_json::to_value(suzatary_core::secretary::readings::conversations(
        &conversations,
        &inbox,
    ))
    .expect("a payload prints")
}

/// The chains the deployment serves (`/api/me`'s list), pulled from `/api/<chain>/objects` as
/// a tab's replicas pull them: the set a proposal is priced through.
pub fn chains_read(base: &str) -> suzatary_core::chain_set::Read {
    let mut me = get(&format!("{base}/api/me"));
    assert_eq!(status(&me), 200, "the box says what it serves");
    let served: Vec<String> = body_json(&mut me)["chains"]
        .as_array()
        .expect("chains")
        .iter()
        .map(|name| name.as_str().expect("a name").to_owned())
        .collect();
    let held: Vec<_> = served
        .iter()
        .map(|name| {
            let mut answer = get(&format!("{base}/api/{name}/objects"));
            assert_eq!(status(&answer), 200, "the chain {name} answers");
            {
                let payload = body_json(&mut answer);
                let prints = payload["objects"]
                    .as_array()
                    .expect("objects")
                    .iter()
                    .map(|object| {
                        (
                            prodrome_core::event::Hash::new(
                                object["hash"].as_str().expect("a name").to_owned(),
                            )
                            .expect("a name"),
                            Ok(object["literal"]
                                .as_str()
                                .expect("a literal")
                                .as_bytes()
                                .to_vec()),
                        )
                    });
                let untrusted = prodrome_core::policy::Untrusted::of(
                    payload["untrusted"]
                        .as_array()
                        .expect("untrusted")
                        .iter()
                        .map(|actor| {
                            prodrome_core::event::Actor::new(actor.as_str().expect("a name"))
                                .expect("an actor")
                        }),
                );
                (
                    suzatary_core::chains::ChainName::new(name).expect("a slug"),
                    untrusted,
                    prodrome_core::dag::Dag::from_prints(
                        &suzatary_core::record::TodoVocabulary::default(),
                        prints,
                    )
                    .objects()
                    .clone()
                    .into_iter()
                    .collect(),
                )
            }
        })
        .collect();
    suzatary_core::chain_set::Read::of_objects(held)
}

/// Every proposal the Inbox lists at `at`, as a tab's replica reads them.
pub fn inbox_listing(base: &str, at: &str) -> Vec<serde_json::Value> {
    let (inbox, _) = replicas(base);
    let at =
        prodrome_core::fpl::datetime_of(prodrome_core::fpl::parse_iso(at).expect("an instant"))
            .expect("in range");
    let read = chains_read(base);
    let set = read.set(at);
    // As the tab asks it: the memory by its route, for a remember card's words.
    let mut memory = get(&format!("{base}/api/secretary/memory"));
    let memory: Option<suzatary_core::wire::MemoryPayload> =
        (status(&memory) == 200).then(|| memory.body_mut().read_json().expect("a memory"));
    serde_json::to_value(suzatary_core::secretary::readings::inbox(
        &inbox,
        &set,
        memory.as_ref(),
    ))
    .expect("a payload prints")["proposals"]
        .as_array()
        .expect("proposals")
        .clone()
}

/// No file under `root` holds `value`, but the device token a phone registers and the bytes of
/// a file the owner uploads, each that route's datum and not a secret's value.
pub fn never_stored(root: &std::path::Path, value: &str) {
    for entry in std::fs::read_dir(root).expect("the store's root") {
        let path = entry.expect("an entry").path();
        if path.ends_with("devices") || path.ends_with("secretary/attachments") {
            continue;
        }
        if path.is_dir() {
            never_stored(&path, value);
        } else {
            let bytes = std::fs::read(&path).expect("a file");
            assert!(
                !bytes
                    .windows(value.len())
                    .any(|window| window == value.as_bytes()),
                "{} holds the value",
                path.display()
            );
        }
    }
}
