//! The shared test harness: fakes, a real server per test over a real socket and a six-todo
//! chain; the gate caches a whois answer per loopback address process-wide.

pub mod fake_google;
pub mod fake_graph;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Duration;
use prodrome_data::event::Actor;
use prodrome_data::policy::Untrusted;
use prodrome_term::fpl::{self, mk_decay, mk_flat};
use prodrome_todo::event::{mk_completed, mk_created};
use suzatary_base::capability::RenderError;
use suzatary_base::capability::{Clock, Datetime, Login, Manual, Manuals, Whois, WhoisError};
use suzatary_base::escape::Escaped;
use suzatary_cards::notify::{Delivered, Notify, NotifyError, Push};
pub use suzatary_connect::cast::{self, CAST};
use suzatary_connect::deployment::Deployment;
use suzatary_conversations::secretary::model::{
    Conversation, Halt, Model, ModelError, Picker, Reply,
};
use suzatary_held::file_read::FileRead;
use suzatary_held::mail_error::MailError;
use suzatary_inbox::secretary::acts;
use suzatary_inbox::secretary::proposer;
use suzatary_inbox::secretary::responded;
use suzatary_io_chains::conversations::ConversationStores as Conversations;
use suzatary_io_chains::devices::Devices;
use suzatary_io_sources::briefings::BriefingFiles;
use suzatary_lines::secretary::transcript;
use suzatary_mail::mail::mailbox::{Mailbox, Query};
use suzatary_mail::mail::sync_report::SyncReport;
use suzatary_mail::mail::{AttachmentMeta, MailMessage, MailRow};
use suzatary_record::record::{EventStore, SubTodo, mk_authored, mk_source, mk_subtodo};
use suzatary_server::serve::{self, Chained, State};
use suzatary_services::repository::{Repository, RepositoryError};
use suzatary_todo::render::{Html, Render};
use suzatary_topics::secretary::answers::{AnswerError, Answers};
use suzatary_triage::mail::corpus::{Corpus, CorpusError};
use suzatary_triage::mail::junk;
use suzatary_triage::mail::junk_folder::{Junk, JunkFailure};
use suzatary_triage::mail::state::Decision;
use suzatary_triage::mail::verbs::{Marked, Message, Show, Summary};

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

#[must_use]
pub fn actor(name: &str) -> Actor {
    Actor::new(name).expect("a roster name")
}

/// The fixture's one chain, with its roster and trust policy: the fakes hold one store.
#[must_use]
pub fn chain() -> suzatary_base::chains::ChainPolicy {
    suzatary_base::chains::ChainPolicy {
        name: suzatary_base::chains::ChainName::new("life").expect("a slug"),
        untrusted: Untrusted::of([actor("triage")]),
        roster: [actor(CAST.owner.actor), actor("triage")]
            .into_iter()
            .collect(),
        root: suzatary_connect::cast::keys::root(),
        naming: suzatary_base::chains::Naming::AsActs,
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
        chains: suzatary_base::chains::Chains::of(vec![chain()]).expect("one chain"),
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
        upkeep: None,
        zone: suzatary_base::zone::Zone::UTC,
    }
}

/// `store` begun with a nonce of zeros, so what a test writes is named the same every run.
pub fn begun(store: &EventStore) {
    let genesis = suzatary_record::record::Envelope::Genesis(
        prodrome_data::genesis::mk_genesis(suzatary_record::record::GENESIS, &"0".repeat(32))
            .expect("a genesis"),
    );
    let name = prodrome_data::event::seal_hash(&genesis);
    store
        .adopt_objects(
            &std::collections::BTreeMap::from([(
                name.clone(),
                prodrome_data::event::canonical_envelope(&genesis),
            )]),
            &name,
            prodrome_data::husk::Sender::Stranger,
        )
        .expect("begins");
}

/// A tab's replica of `store`: the Prodrome's `MemoryStore` holding the box's objects, which
/// appends byte for byte what a store holding them appends.
#[must_use]
pub fn tab(
    store: &dyn suzatary_store::store::Prodrome<suzatary_record::record::TodoEvent>,
) -> prodrome_store::store::MemoryStore<suzatary_record::record::TodoEvent> {
    use prodrome_store::store::Replica as _;
    let prints: std::collections::BTreeMap<prodrome_data::event::Hash, String> = store
        .objects()
        .expect("the box's objects read")
        .iter()
        .map(|(name, object)| {
            (
                name.clone(),
                prodrome_data::event::canonical_envelope(object),
            )
        })
        .collect();
    let tab = prodrome_store::store::MemoryStore::default();
    tab.receive(
        prints.keys().cloned().collect(),
        &|name| prints.get(name).map(|print| print.clone().into_bytes()),
        prodrome_data::husk::Sender::Stranger,
    )
    .expect("the box's objects verify");
    tab
}

/// What a tab does: `event` appended to `tab` as the `Change` its append answers.
pub fn acted(
    tab: &prodrome_store::store::MemoryStore<suzatary_record::record::TodoEvent>,
    event: suzatary_record::record::TodoEvent,
) -> suzatary_record::record::Envelope {
    use prodrome_store::store::Replica as _;
    let name = tab.append(event).expect("the tab appends");
    let print = String::from_utf8(tab.print(&name).expect("held")).expect("utf8");
    prodrome_data::event::parse_envelope(
        &suzatary_record::record::TodoVocabulary::default(),
        &print,
    )
    .expect("the tab printed it")
}

/// One act a fresh tab of `store` writes.
pub fn written(
    store: &dyn suzatary_store::store::Prodrome<suzatary_record::record::TodoEvent>,
    event: suzatary_record::record::TodoEvent,
) -> suzatary_record::record::Envelope {
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
pub fn scene(caps: &Fakes, id: &suzatary_base::conversation_id::ConversationId) -> String {
    let lines: String = transcript::read(caps.conversations.conversations(), id)
        .map(|read| claims_named(&read))
        .unwrap_or_default()
        .iter()
        .map(|line| line.clone() + "\n")
        .collect();
    format!(
        "# lines\n{lines}# shown\n{}\n# asked\n{:#?}\n",
        caps.model
            .shown()
            .iter()
            .map(|names| names.join(" "))
            .collect::<Vec<_>>()
            .join("\n"),
        caps.model.asked(),
    )
}

/// Every line of `read` as JSON, a claim's covered lines named by their turn's index and its run
/// by the order runs first claim, since names and runs are drawn afresh on every run of a test.
fn claims_named(read: &transcript::Transcript) -> Vec<String> {
    let turn = |name: &prodrome_data::event::Hash| {
        (0..read.stored().turns.len()).find(|&turn| {
            read.line_of(turn)
                .is_some_and(|line| read.lines()[line].name == *name)
        })
    };
    let mut runs: Vec<String> = Vec::new();
    read.lines()
        .iter()
        .map(|written| match &written.entry {
            suzatary_conversations::secretary::store::Entry::Began { at, covers, run } => {
                let run = run.as_str().to_owned();
                if !runs.contains(&run) {
                    runs.push(run.clone());
                }
                serde_json::to_string(&serde_json::json!({
                    "line": "began",
                    "at": suzatary_inbox::secretary::stamp::stamp(*at),
                    "covers": covers
                        .iter()
                        .map(|covered| serde_json::json!({
                            "turn": turn(&covered.line),
                            "taint": covered.taint.as_str(),
                        }))
                        .collect::<Vec<_>>(),
                    "run": runs.iter().position(|seen| *seen == run),
                }))
                .expect("a claim is plain data")
            }
            entry => serde_json::to_string(entry).expect("an entry is plain data"),
        })
        .collect()
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

/// A path nothing is at yet, unique by a counter, in the running test's directory
/// ([`suzatary_testkit::this_test`]), which goes when the test ends though a server outlives it.
#[must_use]
pub fn scratch(name: &str) -> PathBuf {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, Ordering::Relaxed);
    suzatary_testkit::this_test().join(format!("{name}-{call}"))
}

/// Six todos from `CAST`: `urgent` (a Decay near its deadline, markup in its body), `flat`,
/// `checklist` (links `flat`), `resolved` (completed), `high`, and `agents` (by `triage`, sourced
/// from a message), with `born` before every `Authored` since an `at` tie breaks by print.
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
        suzatary_store::store::append_to(&events, suzatary_record::record::GENESIS, event)
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
            suzatary_record::record::MarkupSource::from_chain(todo.body),
            spec,
            todo.rationale.iter().map(|&line| line.to_owned()).collect(),
            todo.category,
            "",
            suzatary_record::record::MarkupSource::from_chain(todo.detail),
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
}

impl FakeWhois {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

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
}

/// A daemon that names `at` as this box's own node and asks `inner` of every other address.
struct ThisBoxAt {
    inner: &'static dyn Whois,
    at: Vec<IpAddr>,
}

impl Whois for ThisBoxAt {
    fn peer(&self, address: IpAddr) -> Result<suzatary_base::capability::Peer, WhoisError> {
        if self.at.contains(&address) {
            return Ok(suzatary_base::capability::Peer::ThisBox);
        }
        self.inner.peer(address)
    }
}

/// `router` served on `listener` until the test binary exits.
fn served(listener: std::net::TcpListener, gated: serve::Gated) {
    let service = gated.service();
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

/// The one chain these fakes serve, as the box dialled at `address` serves it.
fn dialled(
    record: &Effects,
    deploying: Deployment,
    this_box: Vec<IpAddr>,
    address: SocketAddr,
) -> (Chained, Arc<Deployment>, suzatary_server::gate::Door) {
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
    let door =
        suzatary_server::gate::door(&app_url).expect("a test server's address is an app_url");
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
    fn peer(&self, address: IpAddr) -> Result<suzatary_base::capability::Peer, WhoisError> {
        if self.unreachable.contains(&address) {
            return Err(WhoisError::Unreachable {
                peer: address,
                reason: "the tailnet daemon did not answer (fake)".to_owned(),
            });
        }
        Ok(if self.this_box.contains(&address) {
            suzatary_base::capability::Peer::ThisBox
        } else {
            self.named.get(&address).cloned().map_or(
                suzatary_base::capability::Peer::Stranger,
                suzatary_base::capability::Peer::Known,
            )
        })
    }
}

/// A settable clock, defaulting to `now()`, ticking one microsecond per read so a write is never
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
}

impl FakeRender {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// The real closed world: a door a test drives is the door the box has.
impl suzatary_base::capability::Typeset for FakeRender {
    fn closed(&self, source: &str) -> Result<String, suzatary_base::capability::Rejected> {
        suzatary_io_typeset::typeset::compile(&suzatary_todo::template::Job::text(source))
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
        self.compiled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(markup.as_str())
            .cloned()
            .map(Html::from_render)
    }

    fn warm(&self, markup: &[Escaped]) -> usize {
        markup.iter().filter(|m| self.html(m).is_ok()).count()
    }

    fn page(&self, page: &suzatary_todo::template::Called) -> Result<Html, RenderError> {
        Ok(Html::from_render(format!(
            "<pre>{}</pre>",
            page.job().state
        )))
    }
}

/// A block containing this string is `FakeAnswers`' one refusal to compile.
pub const BREAKS_THE_COMPILE: &str = "BREAKS_THE_COMPILE";

/// Echoes each block in a `<p>`, so a test can see which block reached it.
#[derive(Default)]
pub struct FakeAnswers;

impl FakeAnswers {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Answers for FakeAnswers {
    fn block(&self, block: &suzatary_base::answer::Renderable) -> Result<Html, AnswerError> {
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
    pub accounts: Vec<String>,
    /// The rows every `inbox` and `search` answers.
    pub rows: Vec<MailRow>,
    pub messages: HashMap<String, MailMessage>,
    pub threads: HashMap<String, Vec<MailMessage>>,
    pub attachment_lists: HashMap<String, Vec<AttachmentMeta>>,
    /// Attachment reads by message id and attachment name.
    pub attachment_reads: HashMap<(String, String), FileRead>,
    /// Attachment bytes by message id and attachment name.
    pub attachment_files: Mutex<HashMap<(String, String), suzatary_mail::mail::AttachmentFile>>,
    /// Each message's file, by notmuch id.
    pub message_files: HashMap<String, Vec<u8>>,
    asked_messages: Mutex<Vec<String>>,
}

impl FakeMailbox {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_accounts(mut self, accounts: Vec<String>) -> Self {
        self.accounts = accounts;
        self
    }

    #[must_use]
    pub fn with_message(mut self, id: impl Into<String>, message: MailMessage) -> Self {
        self.messages.insert(id.into(), message);
        self
    }

    #[must_use]
    pub fn with_attachments(mut self, id: impl Into<String>, listed: Vec<AttachmentMeta>) -> Self {
        self.attachment_lists.insert(id.into(), listed);
        self
    }

    #[must_use]
    pub fn with_message_file(mut self, id: impl Into<String>, raw: Vec<u8>) -> Self {
        self.message_files.insert(id.into(), raw);
        self
    }

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

    #[must_use]
    pub fn with_attachment_file(
        self,
        id: impl Into<String>,
        file: suzatary_mail::mail::AttachmentFile,
    ) -> Self {
        self.replace_attachment_file(id, file);
        self
    }

    /// Change one attachment's bytes in place, as a message rewritten on disk would.
    pub fn replace_attachment_file(
        &self,
        id: impl Into<String>,
        file: suzatary_mail::mail::AttachmentFile,
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

    fn stored(&self, id: &str, cap: u64) -> Result<suzatary_mail::mail::Stored, MailError> {
        let raw = self
            .message_files
            .get(id)
            .ok_or_else(|| MailError::Absent {
                kind: "message",
                name: id.to_owned(),
            })?;
        Ok(if raw.len() as u64 > cap {
            suzatary_mail::mail::Stored::TooLarge {
                size: raw.len() as u64,
            }
        } else {
            suzatary_mail::mail::Stored::Recorded(suzatary_io_mail::mailbox::forwarded::stored(
                id, raw,
            ))
        })
    }

    fn attachment(
        &self,
        id: &str,
        name: &str,
    ) -> Result<suzatary_mail::mail::AttachmentFile, MailError> {
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

/// A model that answers from a list, one reply per turn; its verbs run the real handlers.
#[derive(Default)]
pub struct FakeModel {
    /// Each ask's reply in order, or the failure it meets instead.
    replies: Mutex<std::collections::VecDeque<Result<String, ModelError>>>,
    asked: Mutex<Vec<Conversation>>,
    /// Blocks the first call.
    gate: Mutex<Option<Gate>>,
    /// Fragments per `streamed` call; when exhausted, the whole reply is one delta.
    deltas: Mutex<std::collections::VecDeque<Vec<String>>>,
    /// Blocks `streamed` after its first fragment, so a late subscriber's partial is
    /// nonempty while the round is in flight.
    mid: Mutex<Option<Gate>>,
    /// Held by the next `streamed` call until its `Halt` is pulled.
    holding: Mutex<Option<Gate>>,
    /// The verb names each ask was shown, in order.
    shown: Mutex<Vec<Vec<String>>>,
    /// The limit its account is under, as a door reads it.
    limited: Option<suzatary_health::health::Limited>,
    /// How many next calls panic, as a round's task that ends with no close.
    panics: Mutex<usize>,
    /// What every reply reports it cost.
    usage: Option<suzatary_base::call_log::TokenUsage>,
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
            gate: Mutex::new(None),
            deltas: Mutex::new(std::collections::VecDeque::new()),
            mid: Mutex::new(None),
            holding: Mutex::new(None),
            shown: Mutex::new(Vec::new()),
            limited: None,
            panics: Mutex::new(0),
            usage: None,
        }
    }

    /// The same, for a script built at run time (a review's calls carry hashes).
    #[must_use]
    pub fn saying_owned(replies: impl IntoIterator<Item = String>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().map(Ok).collect()),
            asked: Mutex::new(Vec::new()),
            gate: Mutex::new(None),
            deltas: Mutex::new(std::collections::VecDeque::new()),
            mid: Mutex::new(None),
            holding: Mutex::new(None),
            shown: Mutex::new(Vec::new()),
            limited: None,
            panics: Mutex::new(0),
            usage: None,
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

    /// The next `n` calls panic, whichever of `open`, `resume` or `turn` they are.
    #[must_use]
    pub fn panicking(self, n: usize) -> Self {
        *self.panics.lock().expect("not poisoned") = n;
        self
    }

    /// Block in the gate, if one is set, before a reply is popped; panic first while the
    /// panics last.
    fn wait_for_gate(&self) {
        {
            let mut panics = self.panics.lock().expect("not poisoned");
            if *panics > 0 {
                *panics -= 1;
                drop(panics);
                panic!("the fake ends the round's task here, with no close");
            }
        }
        if let Some(gate) = self.gate.lock().expect("not poisoned").clone() {
            gate.enter();
        }
    }

    /// Every conversation it was handed, in order.
    #[must_use]
    pub fn asked(&self) -> Vec<Conversation> {
        self.asked.lock().expect("not poisoned").clone()
    }

    /// The verb names each ask was shown, in order.
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

    /// The same model, every reply reporting that it cost `usage`.
    #[must_use]
    pub fn costing(self, usage: suzatary_base::call_log::TokenUsage) -> Self {
        Self {
            usage: Some(usage),
            ..self
        }
    }

    /// The same model, its account under `limited`.
    #[must_use]
    pub fn limited(self, limited: suzatary_health::health::Limited) -> Self {
        Self {
            limited: Some(limited),
            ..self
        }
    }
}

impl Model for FakeModel {
    fn name(&self) -> String {
        "a scripted model".to_owned()
    }

    /// Pops the scripted queue, holding until its halt when asked to, and streams the scripted
    /// fragments.
    fn streamed(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Reply, ModelError> {
        self.show(tools);
        self.asked
            .lock()
            .expect("not poisoned")
            .push(conversation.clone());
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
                Ok(Reply::parse(&text).with_usage(self.usage))
            }
            None => Err(ModelError::Failed("the script ran out".to_owned())),
        }
    }

    fn limited(&self) -> Option<suzatary_health::health::Limited> {
        self.limited
    }
}

impl Picker for FakeModel {
    fn pick(&self) -> suzatary_conversations::secretary::model::Pick<'_> {
        Box::new(self)
    }
}

/// A corpus with no maildir, empty unless seeded; `search` mimics notmuch only as the sweep needs.
#[derive(Default)]
pub struct FakeCorpus {
    messages: Vec<FakeMessage>,
}

#[derive(Debug, Clone)]
pub struct FakeMessage {
    /// The canonical hash `show` is asked by.
    pub hash: String,
    /// The `thread:` a `search` matches on.
    pub thread: String,
    /// The `from:` a `search` matches on.
    pub sender: String,
    pub subject: String,
    pub body: String,
    /// Shaped exactly as a real row's `timestamp`.
    pub timestamp: String,
}

impl FakeCorpus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

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
        held: suzatary_held::held_value::Kinds::empty(),
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

/// What a tap would have sent, kept in memory: every server here holds one, so no test reaches a transport.
#[derive(Default)]
pub struct Recorder {
    sent: Mutex<Vec<suzatary_inbox::secretary::proposal::Draft>>,
    parts: Mutex<Vec<Vec<suzatary_held::message_part::Attachment>>>,
}

impl Recorder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything a tap handed it, in order.
    #[must_use]
    pub fn sent(&self) -> Vec<suzatary_inbox::secretary::proposal::Draft> {
        self.sent.lock().expect("not poisoned").clone()
    }

    /// The attachments each send carried, checked, in the same order.
    #[must_use]
    pub fn parts(&self) -> Vec<Vec<suzatary_held::message_part::Attachment>> {
        self.parts.lock().expect("not poisoned").clone()
    }
}

/// Records every act a tap began, performing nothing and answering `Started`.
#[derive(Default)]
pub struct ActRecorder {
    begun: Mutex<Vec<(String, usize, acts::Act)>>,
}

impl ActRecorder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every `(conversation, index, act)` begun, in order.
    #[must_use]
    pub fn begun(&self) -> Vec<(String, usize, acts::Act)> {
        self.begun.lock().expect("not poisoned").clone()
    }
}

impl suzatary_server::secretary::routes::Perform for ActRecorder {
    fn begin(
        &self,
        id: &suzatary_base::conversation_id::ConversationId,
        index: usize,
        _object: &suzatary_inbox::secretary::inbox::ProposalId,
        _proposer: &proposer::Proposer,
        act: &acts::Act,
        _consent: suzatary_mail::mail::oauth::Consent,
    ) -> responded::Performed {
        self.begun
            .lock()
            .expect("not poisoned")
            .push((id.as_str().to_owned(), index, act.clone()));
        responded::Tried::Running {
            began: "2026-09-14T09:00:00".to_owned(),
            expires_at: "2026-09-14T09:15:00".to_owned(),
        }
        .into()
    }
}

/// A performer that begins a Connect card's sign-in through the server's table and records
/// every other act.
#[cfg(feature = "fixture")]
pub struct SignsIn {
    recorded: Arc<ActRecorder>,
    signing: Arc<serve::sign_ins::SignIns>,
}

#[cfg(feature = "fixture")]
impl suzatary_server::secretary::routes::Perform for SignsIn {
    fn begin(
        &self,
        id: &suzatary_base::conversation_id::ConversationId,
        index: usize,
        object: &suzatary_inbox::secretary::inbox::ProposalId,
        proposer: &proposer::Proposer,
        act: &acts::Act,
        consent: suzatary_mail::mail::oauth::Consent,
    ) -> responded::Performed {
        match act {
            acts::Act::SignIn { .. } => self.signing.begin(object, now()),
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

impl suzatary_connect::settings::Checks for CheckRecorder {
    fn check(&self, source: &suzatary_health::health::SourceKey) {
        self.0.lock().expect("not poisoned").push(match source {
            suzatary_health::health::SourceKey::Plugin(plugin) => plugin.as_str().to_owned(),
            suzatary_health::health::SourceKey::Mail(_)
            | suzatary_health::health::SourceKey::Link { .. }
            | suzatary_health::health::SourceKey::GoogleCalendar(_)
            | suzatary_health::health::SourceKey::Drive => source.to_string(),
        });
    }
}

/// What a settings write would have set, kept in memory, value included on purpose.
#[derive(Default)]
pub struct SecretRecorder {
    calls: Mutex<Vec<(String, String)>>,
    cleared: Mutex<Vec<String>>,
    /// Where a Microsoft account's grants are kept, as the box reads them.
    tokens: Option<suzatary_io_mail::oauth::TokenStore>,
}

impl SecretRecorder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reading a Microsoft account's grants from `tokens`, as the box's own secrets do.
    #[must_use]
    pub fn with_tokens(self, tokens: suzatary_io_mail::oauth::TokenStore) -> Self {
        Self {
            tokens: Some(tokens),
            ..self
        }
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
impl suzatary_io_access::secrets::Resolves for SecretRecorder {
    fn get(
        &self,
        name: &str,
        _: &str,
    ) -> Result<Option<suzatary_base::capability::Secret>, suzatary_io_access::secrets::SecretError>
    {
        let cleared = self.cleared.lock().expect("not poisoned");
        let calls = self.calls.lock().expect("not poisoned");
        Ok(calls
            .iter()
            .rev()
            .find(|(held, _)| held == name)
            .filter(|_| !cleared.iter().any(|gone| gone == name))
            .map(|(_, value)| suzatary_base::capability::Secret::new(value.clone())))
    }
}

/// Where a name stands in a recorder: set while it holds a value; a Google account holds its
/// reading grant while its sign-in with Google is held.
impl suzatary_connect::secrets::Secrets for SecretRecorder {
    fn secret_state(
        &self,
        settable: &suzatary_connect::settings::Settable,
    ) -> suzatary_connect::secrets::SecretState {
        use suzatary_io_access::secrets::Resolves as _;
        match self.get(settable.name(), "a scene") {
            Ok(Some(_)) => suzatary_connect::secrets::SecretState::Set,
            Ok(None) | Err(_) => suzatary_connect::secrets::SecretState::Missing,
        }
    }

    fn mail_grants(
        &self,
        account: &suzatary_mail::mail::account::MailAccount,
    ) -> suzatary_connect::secrets::MailGrants {
        let held = suzatary_mail::mail::address::EmailAddress::new(account.email.as_str())
            .ok()
            .filter(|_| matches!(account.sync, suzatary_mail::mail::account::Sync::Google))
            .is_some_and(|email| {
                matches!(
                    self.google_grant(&email),
                    suzatary_google::google::Held::Signed(_)
                )
            });
        let slots = self
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.granted(account).ok())
            .unwrap_or_default();
        suzatary_connect::secrets::MailGrants {
            imap: held || slots.contains(&"imap"),
            smtp: slots.contains(&"smtp"),
            calendar: slots.contains(&"calendar"),
        }
    }

    fn google_grant(
        &self,
        account: &suzatary_mail::mail::address::EmailAddress,
    ) -> suzatary_google::google::Held {
        use suzatary_io_access::secrets::Resolves as _;
        match self.get(&suzatary_google::google::grant_name(account), "a scene") {
            Ok(Some(kept)) => suzatary_google::google::KeptGrant::parse(kept.expose())
                .map_or(suzatary_google::google::Held::NotSignedIn, |kept| {
                    suzatary_google::google::Held::Signed(kept.parts)
                }),
            Ok(None) | Err(_) => suzatary_google::google::Held::NotSignedIn,
        }
    }
}

impl suzatary_connect::settings::Writes for SecretRecorder {
    fn write_secret(
        &self,
        name: &suzatary_connect::settings::Settable,
        value: &suzatary_base::capability::Secret,
    ) -> Result<(), String> {
        self.calls
            .lock()
            .expect("not poisoned")
            .push((name.name().to_owned(), value.expose().to_owned()));
        Ok(())
    }

    fn clear_secret(&self, name: &suzatary_connect::settings::Settable) -> Result<(), String> {
        self.cleared
            .lock()
            .expect("not poisoned")
            .push(name.name().to_owned());
        Ok(())
    }
}

/// A store that keeps no declarations of its own, so every name it is told of is declared.
pub struct Undeclared;

impl suzatary_connect::settings::Declares for Undeclared {
    fn declare(&self, _: &suzatary_connect::settings::Settables) -> Result<(), String> {
        Ok(())
    }
}

/// A store over the real secret declarations that refuses a name its file does not declare, and
/// otherwise records as [`SecretRecorder`] does.
pub struct DeclaredStore {
    /// What declares its names, which the server's `declares` is.
    pub declarations: suzatary_io_access::secrets::Declarations,
    pub recorded: SecretRecorder,
}

impl DeclaredStore {
    /// A store declaring the built-in keys alone, as a deployment's server starts.
    #[must_use]
    pub fn new() -> Self {
        let dir = scratch("declared-store");
        let base = dir.join("code").join("secretspec.toml");
        std::fs::create_dir_all(base.parent().expect("a parent")).expect("a code root");
        std::fs::write(
            &base,
            "[project]\nname = \"suzatary\"\nrevision = \"1.0\"\n\n[profiles.default]\n",
        )
        .expect("the code root's own file");
        let declarations = suzatary_io_access::secrets::Declarations::new(
            base,
            dir.join("state"),
            suzatary_connect::settings::Settables::derive(
                suzatary_io_model::model::TOKEN,
                [],
                None,
                &[],
            ),
        );
        Self {
            declarations,
            recorded: SecretRecorder::new(),
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

    fn refused(&self, name: &suzatary_connect::settings::Settable) -> Result<(), String> {
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

impl suzatary_connect::settings::Writes for DeclaredStore {
    fn write_secret(
        &self,
        name: &suzatary_connect::settings::Settable,
        value: &suzatary_base::capability::Secret,
    ) -> Result<(), String> {
        self.refused(name)?;
        self.recorded.write_secret(name, value)
    }

    fn clear_secret(&self, name: &suzatary_connect::settings::Settable) -> Result<(), String> {
        self.refused(name)?;
        self.recorded.clear_secret(name)
    }
}

/// Records every allow rule it is handed and always succeeds; a connection's edit is the
/// algebra's, over the tables it holds.
#[derive(Default)]
pub struct EditsRecorder {
    declared: std::sync::Mutex<suzatary_connect::connect::Declared>,
    allowed: std::sync::Mutex<Vec<suzatary_services::web::AllowRule>>,
    folders: std::sync::Mutex<Vec<suzatary_base::drive_path::DrivePath>>,
}

impl EditsRecorder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A recorder whose instance declares `declared`.
    #[must_use]
    pub fn declaring(declared: suzatary_connect::connect::Declared) -> Self {
        Self {
            declared: std::sync::Mutex::new(declared),
            ..Self::default()
        }
    }

    /// Every web allow rule written, oldest first.
    #[must_use]
    #[cfg_attr(not(feature = "fixture"), expect(dead_code))]
    pub fn allowed(&self) -> Vec<suzatary_services::web::AllowRule> {
        self.allowed.lock().expect("not poisoned").clone()
    }

    /// Every Drive folder "Always allow this folder" wrote, oldest first.
    #[must_use]
    #[cfg_attr(not(feature = "fixture"), expect(dead_code))]
    pub fn folders(&self) -> Vec<suzatary_base::drive_path::DrivePath> {
        self.folders.lock().expect("not poisoned").clone()
    }
}

impl suzatary_connect::settings::Edits for EditsRecorder {
    fn allow_web(
        &self,
        rule: &suzatary_services::web::AllowRule,
    ) -> Result<(), suzatary_connect::settings::EditError> {
        self.allowed
            .lock()
            .expect("not poisoned")
            .push(rule.clone());
        Ok(())
    }

    fn allow_drive(
        &self,
        folder: &suzatary_base::drive_path::DrivePath,
    ) -> Result<(), suzatary_connect::settings::EditError> {
        self.folders
            .lock()
            .expect("not poisoned")
            .push(folder.clone());
        Ok(())
    }

    fn add_connection(
        &self,
        connection: &suzatary_connect::connect::Connection,
    ) -> Result<(), suzatary_connect::settings::EditError> {
        let mut declared = self.declared.lock().expect("not poisoned");
        *declared = declared.with(connection)?;
        Ok(())
    }

    fn remove_connection(
        &self,
        connection: &suzatary_connect::connect::Connection,
    ) -> Result<(), suzatary_connect::settings::EditError> {
        let mut declared = self.declared.lock().expect("not poisoned");
        *declared = declared.without(connection)?;
        Ok(())
    }

    fn add_origin(
        &self,
        plugin: &suzatary_health::health::plugin::PluginName,
        origin: &suzatary_health::health::plugin::Origin,
    ) -> Result<(), suzatary_connect::settings::EditError> {
        let mut declared = self.declared.lock().expect("not poisoned");
        *declared = declared.with_origin(plugin, origin)?;
        Ok(())
    }
}

impl suzatary_connect::settings::Configuration for EditsRecorder {
    fn declared(
        &self,
    ) -> Result<suzatary_connect::connect::Declared, suzatary_connect::settings::EditError> {
        Ok(self.declared.lock().expect("not poisoned").clone())
    }
}

/// Everything `State` needs for `/api/overview` and the settings writes beside the effect record.
pub struct SettingsFixture {
    /// The settable names, the channels, the accounts and the plugins the server starts with.
    pub live: serve::Live,
    pub models: Arc<serve::models::Models>,
    pub secrets: Arc<dyn suzatary_connect::settings::Writes + Send + Sync>,
    /// What declares the settable names to the store a secret write goes to.
    pub declares: Arc<dyn suzatary_connect::settings::Declares + Send + Sync>,
    pub edits: Arc<dyn suzatary_connect::settings::Edits + Send + Sync>,
    /// Where a removed account's sign-ins are kept, `None` for none.
    pub grants: Option<suzatary_io_mail::oauth::TokenStore>,
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
            forget: Arc::new(suzatary_cli::mail_cache::Unkept(self.grants)),
            stamp: suzatary_cli::stamp::stamped(serve::Build::Compiled(
                suzatary_cli::stamp::build(),
            )),
            data,
        }
    }

    /// Only the built-in keys settable, an empty roster, and fresh recorders.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            live: serve::Live {
                settables: suzatary_connect::settings::Settables::derive(
                    suzatary_io_model::model::TOKEN,
                    std::iter::empty(),
                    None,
                    &[],
                ),
                channels: None,
                accounts: suzatary_mail::mail::account::Roster::new(Vec::new())
                    .expect("an empty roster"),
                plugins: Vec::new(),
            },
            models: Arc::new(serve::models::Models {
                chat: suzatary_digest::model_name::ModelName::parse("claude-opus-5-5")
                    .expect("a model name"),
                compacting: suzatary_digest::secretary::compacting::Compacting::Off,
                reserve: suzatary_digest::secretary::reserve::Reserve::default(),
            }),
            secrets: Arc::new(SecretRecorder::new()),
            declares: Arc::new(Undeclared),
            edits: Arc::new(EditsRecorder::new()),
            grants: None,
        }
    }

    /// Like `empty`, with `secrets` overridden.
    #[must_use]
    pub fn with_writer(secrets: Arc<dyn suzatary_connect::settings::Writes + Send + Sync>) -> Self {
        Self {
            secrets,
            ..Self::empty()
        }
    }

    /// Like `empty`, with `edits` overridden.
    #[must_use]
    #[cfg_attr(not(feature = "fixture"), expect(dead_code))]
    pub fn with_edits(edits: Arc<dyn suzatary_connect::settings::Edits + Send + Sync>) -> Self {
        Self {
            edits,
            ..Self::empty()
        }
    }
}

impl suzatary_server::secretary::routes::Post for Recorder {
    fn send(
        &self,
        draft: &suzatary_inbox::secretary::proposal::Draft,
        body: suzatary_held::held_value::Filled,
        attachments: suzatary_held::secretary::attached::Checked,
    ) -> responded::Performed {
        // What went on the wire: the body with its held values written in.
        self.sent
            .lock()
            .expect("not poisoned")
            .push(suzatary_inbox::secretary::proposal::Draft {
                body: body.into_string(),
                ..draft.clone()
            });
        self.parts
            .lock()
            .expect("not poisoned")
            .push(attachments.into_parts());
        responded::Done::Sent {
            recipients: draft.to.iter().chain(&draft.cc).cloned().collect(),
            message_id: "<recorded@test>".to_owned(),
        }
        .into()
    }
}

/// A transport that fails with each of `failures` in turn, then records like
/// `Recorder`.
pub struct FailsFirst {
    failures: Mutex<std::collections::VecDeque<responded::Performed>>,
    sent: Arc<Recorder>,
}

impl suzatary_server::secretary::routes::Post for FailsFirst {
    fn send(
        &self,
        draft: &suzatary_inbox::secretary::proposal::Draft,
        body: suzatary_held::held_value::Filled,
        attachments: suzatary_held::secretary::attached::Checked,
    ) -> responded::Performed {
        match self.failures.lock().expect("not poisoned").pop_front() {
            Some(failed) => failed,
            None => self.sent.send(draft, body, attachments),
        }
    }
}

/// A scripted OAuth endpoint for driving `Acting::Real` with no network:
/// the answers in order, then a transport failure once they run out.
pub struct ScriptedOauth(suzatary_io_access::http::Scripted);

impl ScriptedOauth {
    #[must_use]
    pub fn new(
        answers: Vec<
            Result<suzatary_mail::mail::oauth::Answered, suzatary_mail::mail::oauth::OauthError>,
        >,
    ) -> Self {
        use suzatary_io_access::http::{HttpError, Resp, Scripted};
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

impl suzatary_io_access::http::Http for ScriptedOauth {
    fn send(
        &self,
        req: suzatary_io_access::http::Req,
    ) -> Result<suzatary_io_access::http::Resp, suzatary_io_access::http::HttpError> {
        self.0.send(req)
    }
}

/// How long a test waits for the box to answer in a conversation: the longest the suite gave one
/// exchange, for every wait on a round.
pub const ROUND_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Poll conversation `id` until `until` holds of it, for at most [`ROUND_WAIT`].
pub fn poll_conversation(
    base: &str,
    id: &str,
    until: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let started = std::time::Instant::now();
    let mut last = (serde_json::Value::Null, started);
    while started.elapsed() < ROUND_WAIT {
        let body = reading(base, id, &[]);
        if until(&body) {
            return body;
        }
        let moved = |read: &serde_json::Value| (read["state"].clone(), read["turns"].clone());
        if moved(&body) != moved(&last.0) {
            last = (body, std::time::Instant::now());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let (body, since) = last;
    panic!(
        "conversation {id} did not reach what the test waits for in {ROUND_WAIT:?}: it read \
         state {} with {} turns, unchanged for the last {:?}, the last turn {}",
        body["state"],
        body["turns"].as_array().map_or(0, Vec::len),
        since.elapsed(),
        body["turns"]
            .as_array()
            .and_then(|turns| turns.last())
            .unwrap_or(&serde_json::Value::Null),
    );
}

/// Conversation `id` once its round has ended and it reads `idle`.
pub fn settled(base: &str, id: &str) -> serde_json::Value {
    poll_conversation(base, id, |read| read["state"] == "idle")
}

/// A conversation reading as a failure message can show it: its state, then one line per turn
/// with its index, who said it, a verb's name and outcome, whether a person's turn is queued, and
/// its text.
#[must_use]
pub fn told(read: &serde_json::Value) -> String {
    let turns = read["turns"].as_array().map_or(&[][..], Vec::as_slice);
    std::iter::once(format!("state {}", read["state"]))
        .chain(turns.iter().map(|turn| {
            let detail = match turn["who"].as_str() {
                Some("verb") => format!(" {} {}", turn["verb"], turn["outcome"]["is"]),
                Some("person") => format!(" queued {}", turn["queued"]),
                _ => String::new(),
            };
            format!(
                "#{} {}{detail}: {}",
                turn["index"], turn["who"], turn["text"]
            )
        }))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A new conversation opened as a tab opens one: its opening line sealed as the poster's,
/// signed by the owner's [`phone`] and handed to the lines with its key beside it; its id.
#[must_use]
pub fn opened(base: &str, title: &str) -> String {
    opening(
        base,
        &suzatary_conversations::secretary::store::Entry::Opened {
            at: next_instant(),
            title: suzatary_memory::secretary::words::Words::kept(title.to_owned()),
            model: String::new(),
            kind: suzatary_conversations::secretary::store::Kind::Topic,
        },
    )
}

/// A topic begun on the box's own store, as `secretary ask new` begins one; its id.
#[must_use]
pub fn topic(
    conversations: suzatary_lines::secretary::conversations::Conversations<'_>,
) -> suzatary_base::conversation_id::ConversationId {
    transcript::begin(
        conversations,
        &suzatary_conversations::secretary::store::Entry::Opened {
            at: next_instant(),
            title: suzatary_memory::secretary::words::Words::kept(String::new()),
            model: "a-cli/small".to_owned(),
            kind: suzatary_conversations::secretary::store::Kind::Topic,
        },
    )
    .expect("begun")
}

/// A conversation begun by `first`, an opening or a setup, as a tab begins one; its id.
#[must_use]
pub fn opening(base: &str, first: &suzatary_lines::secretary::spoken::Plain) -> String {
    use suzatary_lines::secretary::line_event::LineEvent;
    let (line, keys) = LineEvent::begin(poster(base), first, &Drawing).expect("a line");
    let id = line.conversation().as_str().to_owned();
    let _ = handed(base, line, &keys, Vec::new());
    id
}

/// Conversation `id` renamed `title` as a tab renames it: a `Titled` line by the poster over the
/// conversation's heads, handed to the lines with its key beside it.
pub fn renamed(base: &str, id: &str, title: &str) {
    use suzatary_conversations::secretary::store::{Entry, Namer, Title};
    let by = poster(base);
    line_written(
        base,
        id,
        &Entry::Titled {
            at: next_instant(),
            title: suzatary_memory::secretary::words::Words::kept(
                Title::new(title).expect("a title"),
            ),
            by: Namer::Person {
                actor: by.as_str().to_owned(),
            },
        },
    );
}

/// `text` said in conversation `id` as a tab says it: a person's turn stamped as the poster on a
/// device, over the conversation's heads, handed to the lines: its line's name.
pub fn say(base: &str, id: &str, text: &str) -> prodrome_data::event::Hash {
    say_with(base, id, text, Vec::new(), Vec::new())
}

/// [`say`] with what the turn points at and the files it names, each as the upload answered it.
pub fn say_with(
    base: &str,
    id: &str,
    text: &str,
    references: Vec<suzatary_base::reference::Reference>,
    attachments: Vec<suzatary_held::attachment::Attachment>,
) -> prodrome_data::event::Hash {
    let posted = sealed(base, id, &question(base, text, references, attachments));
    let name = posted.name.clone();
    let receipt = post_lines(base, &posted.body);
    assert!(
        receipt["adopted"]
            .as_array()
            .is_some_and(|adopted| adopted.iter().any(|n| n == name.as_str())),
        "the box took the line: {receipt}"
    );
    name
}

/// A person's turn as the poster on a device says it.
#[must_use]
pub fn question(
    base: &str,
    text: &str,
    references: Vec<suzatary_base::reference::Reference>,
    attachments: Vec<suzatary_held::attachment::Attachment>,
) -> suzatary_lines::secretary::spoken::Plain {
    let by = poster(base);
    suzatary_conversations::secretary::store::Entry::Said {
        at: next_instant(),
        turn: suzatary_lines::secretary::spoken::Spoken::person(
            text.to_owned(),
            references,
            attachments,
        ),
        by: Some(suzatary_inbox::secretary::stamp::Stamp::Actor(
            suzatary_inbox::secretary::stamp::Speaker::device(by.as_str()),
        )),
    }
}

/// A line sealed by the poster: its name, and the body of its post to the lines.
pub struct Sealed {
    pub name: prodrome_data::event::Hash,
    pub body: serde_json::Value,
}

/// `entry` sealed into conversation `id` by the poster over the heads the box serves, signed by
/// the owner's phone, as a post's body; nothing is posted.
#[must_use]
pub fn sealed(base: &str, id: &str, entry: &suzatary_lines::secretary::spoken::Plain) -> Sealed {
    use suzatary_lines::secretary::line_event::LineEvent;
    let mut answer = get(&format!("{base}/api/stores/lines/skeleton"));
    let skeleton = body_json(&mut answer);
    let nodes: Vec<&serde_json::Value> = skeleton["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .filter(|n| n["header"]["conversation"] == id)
        .collect();
    let under: std::collections::BTreeSet<&str> = nodes
        .iter()
        .flat_map(|n| n["parents"].as_array().expect("parents").iter())
        .filter_map(|p| p.as_str())
        .collect();
    let heads = nodes
        .iter()
        .filter_map(|n| n["hash"].as_str())
        .filter(|n| !under.contains(n))
        .map(|n| prodrome_data::event::Hash::new(n).expect("a name"))
        .collect();
    let conversation = suzatary_base::conversation_id::ConversationId::new(id).expect("an id");
    let count = u32::try_from(nodes.len()).expect("a count");
    let (line, keys) =
        LineEvent::write(conversation, poster(base), count, entry, &Drawing).expect("a line");
    let genesis = lines_genesis(base);
    let change = prodrome_data::event::Envelope::Change(
        prodrome_data::change::mk_change(genesis, heads, line).expect("a change"),
    );
    let name = prodrome_data::event::seal_hash(&change);
    let signature: prodrome_data::event::Envelope<LineEvent> =
        prodrome_data::event::Envelope::Signed(phone().sign(&name));
    let body = serde_json::json!({
        "objects": [
            prodrome_data::event::canonical_envelope(&change),
            prodrome_data::event::canonical_envelope(&signature),
        ],
        "keys": { name.as_str(): keys.iter().map(|k| hex::encode(k.bytes())).collect::<Vec<_>>() },
    });
    Sealed { name, body }
}

/// `body` posted to the lines: the box's receipt.
pub fn post_lines(base: &str, body: &serde_json::Value) -> serde_json::Value {
    let mut taken = act(&format!("{base}/api/stores/lines/objects"), body);
    body_json(&mut taken)
}

/// Why the box refused the line `sealed` names, once posted.
pub fn refused_line(base: &str, sealed: &Sealed) -> String {
    let receipt = post_lines(base, &sealed.body);
    receipt["refused"]
        .as_array()
        .and_then(|refused| refused.iter().find(|r| r["hash"] == sealed.name.as_str()))
        .and_then(|r| r["reason"].as_str())
        .unwrap_or_else(|| panic!("the box refused {}: {receipt}", sealed.name))
        .to_owned()
}

/// Conversation `id`'s answer stopped as a tab stops it: a `Stopped` line by the poster over the
/// conversation's heads, handed to the lines.
pub fn stop(base: &str, id: &str) -> prodrome_data::event::Hash {
    use suzatary_conversations::secretary::store::Entry;
    let by = poster(base);
    line_written(
        base,
        id,
        &Entry::Stopped {
            at: next_instant(),
            by: by.as_str().to_owned(),
        },
    )
}

/// The poster's `Again` of the question whose line is `question`, asked again `how`.
#[must_use]
pub fn again_line(
    base: &str,
    question: &prodrome_data::event::Hash,
    how: suzatary_conversations::secretary::store::Rewind,
) -> suzatary_lines::secretary::spoken::Plain {
    suzatary_conversations::secretary::store::Entry::Again {
        at: next_instant(),
        question: question.clone(),
        how,
        by: poster(base).as_str().to_owned(),
    }
}

/// The question whose line is `question` asked again `how` as a tab asks it: an [`again_line`]
/// over the conversation's heads the box serves, handed to the lines.
pub fn asked_again(
    base: &str,
    id: &str,
    question: &prodrome_data::event::Hash,
    how: suzatary_conversations::secretary::store::Rewind,
) -> prodrome_data::event::Hash {
    line_written(base, id, &again_line(base, question, how))
}

/// `entry` written to conversation `id` by the poster, over the heads of the conversation the
/// box serves, and handed to the lines: its line's name.
fn line_written(
    base: &str,
    id: &str,
    entry: &suzatary_lines::secretary::spoken::Plain,
) -> prodrome_data::event::Hash {
    use suzatary_lines::secretary::line_event::LineEvent;
    let (heads, count) = served_heads(base, id);
    let conversation = suzatary_base::conversation_id::ConversationId::new(id).expect("an id");
    let (line, keys) =
        LineEvent::write(conversation, poster(base), count, entry, &Drawing).expect("a line");
    handed(base, line, &keys, heads)
}

/// The heads of conversation `id` the box serves, and how many lines it serves of it.
fn served_heads(base: &str, id: &str) -> (Vec<prodrome_data::event::Hash>, u32) {
    let mut answer = get(&format!("{base}/api/stores/lines/skeleton"));
    let skeleton = body_json(&mut answer);
    let nodes: Vec<&serde_json::Value> = skeleton["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .filter(|n| n["header"]["conversation"] == id)
        .collect();
    assert!(!nodes.is_empty(), "the box serves {id}");
    let under: std::collections::BTreeSet<&str> = nodes
        .iter()
        .flat_map(|n| n["parents"].as_array().expect("parents").iter())
        .filter_map(|p| p.as_str())
        .collect();
    let heads = nodes
        .iter()
        .filter_map(|n| n["hash"].as_str())
        .filter(|n| !under.contains(n))
        .map(|n| prodrome_data::event::Hash::new(n).expect("a name"))
        .collect();
    (heads, u32::try_from(nodes.len()).expect("a count"))
}

/// The question whose line is `question` edited to `text` as a tab edits it: an edit's `Again`
/// over the heads the box serves, and the new question, an `Edited` line resting on it alone,
/// both handed to the lines: the new question's name.
pub fn edited(
    base: &str,
    id: &str,
    question: &prodrome_data::event::Hash,
    text: &str,
) -> prodrome_data::event::Hash {
    use suzatary_lines::secretary::line_event::LineEvent;
    let edit = again_line(
        base,
        question,
        suzatary_conversations::secretary::store::Rewind::Edit,
    );
    let again = line_written(base, id, &edit);
    let (_, count) = served_heads(base, id);
    let conversation = suzatary_base::conversation_id::ConversationId::new(id).expect("an id");
    let (line, keys) = LineEvent::edited(
        conversation,
        poster(base),
        count,
        &self::question(base, text, Vec::new(), Vec::new()),
        &Drawing,
    )
    .expect("a line");
    handed(base, line, &keys, vec![again])
}

/// The actor `base` maps this test's peer to.
fn poster(base: &str) -> prodrome_data::event::Actor {
    let mut me = get(&format!("{base}/api/me"));
    actor(
        body_json(&mut me)["actor"]
            .as_str()
            .expect("a poster who writes"),
    )
}

/// `line` resting on `deps`, signed by the owner's [`phone`] and handed to the lines with `keys`
/// beside it.
fn handed(
    base: &str,
    line: suzatary_lines::secretary::line_event::LineEvent,
    keys: &[suzatary_memory::secretary::locked_text::TextKey],
    deps: Vec<prodrome_data::event::Hash>,
) -> prodrome_data::event::Hash {
    use suzatary_lines::secretary::line_event::LineEvent;
    let genesis = lines_genesis(base);
    let change = prodrome_data::event::Envelope::Change(
        prodrome_data::change::mk_change(genesis, deps, line).expect("a change"),
    );
    let name = prodrome_data::event::seal_hash(&change);
    let signature: prodrome_data::event::Envelope<LineEvent> =
        prodrome_data::event::Envelope::Signed(phone().sign(&name));
    let posted = serde_json::json!({
        "objects": [
            prodrome_data::event::canonical_envelope(&change),
            prodrome_data::event::canonical_envelope(&signature),
        ],
        "keys": { name.as_str(): keys.iter().map(|k| hex::encode(k.bytes())).collect::<Vec<_>>() },
    });
    let mut taken = act(&format!("{base}/api/stores/lines/objects"), &posted);
    let receipt = body_json(&mut taken);
    assert!(
        receipt["adopted"]
            .as_array()
            .is_some_and(|adopted| adopted.iter().any(|n| n == name.as_str())),
        "the box took the line: {receipt}"
    );
    name
}

/// The lines' genesis, once the box has begun the lines.
fn lines_genesis(base: &str) -> prodrome_data::event::Hash {
    let began = std::time::Instant::now();
    loop {
        let mut answer = get(&format!("{base}/api/stores/lines/skeleton"));
        let skeleton = body_json(&mut answer);
        if let Some(genesis) = skeleton["geneses"][0].as_str() {
            return prodrome_data::event::Hash::new(genesis).expect("a name");
        }
        assert!(
            began.elapsed() < std::time::Duration::from_secs(5),
            "the box began no lines"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A later instant at every call, so two openings in one test are two conversations.
#[must_use]
pub fn next_instant() -> prodrome_data::literal::Datetime {
    static DRAWN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = DRAWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // In the minute before the box's clock (`now`), a millisecond apart: a tab's line is dated
    // by its own clock, which the box takes up to a little ahead of its own.
    prodrome_data::literal::Datetime::new(2026, 9, 2, 11, 59, n / 1000 % 60, n % 1000 * 1000)
        .expect("an instant")
}

/// Fresh bytes at every call, for a tab's keys.
struct Drawing;

impl suzatary_base::capability::Entropy for Drawing {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        static DRAWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let seed = DRAWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let spread = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).to_le_bytes();
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = spread[i % 8] ^ u8::try_from(i % 256).unwrap_or(0);
        }
        Ok(())
    }
}

/// `text` said in conversation `id` as a tab says it, read once its round has ended.
pub fn said(base: &str, id: &str, text: &str) -> serde_json::Value {
    let _ = say(base, id, text);
    settled(base, id)
}

/// A conversation opened as `title` with `text` said in it, by its id and its reading once the
/// round has ended.
#[must_use]
pub fn exchange(base: &str, title: &str, text: &str) -> (String, serde_json::Value) {
    let id = opened(base, title);
    let payload = said(base, &id, text);
    (id, payload)
}

/// A 200 answer over JSON, for a `ScriptedOauth` script.
#[must_use]
pub fn answered(body: serde_json::Value) -> suzatary_mail::mail::oauth::Answered {
    suzatary_mail::mail::oauth::Answered { status: 200, body }
}

/// The whole capability set: real stores over temp directories, and fakes for every daemon.
pub struct Fakes {
    /// The scratch root every store here lives under.
    pub root: PathBuf,
    pub chain: suzatary_io_chains::objects::Objects<suzatary_record::record::TodoEvent>,
    pub whois: FakeWhois,
    pub notify: FakeNotify,
    /// The mail the routes read.
    pub mailbox: FakeMailbox,
    pub render: FakeRender,
    pub answers: FakeAnswers,
    pub clock: FakeClock,
    pub briefings: BriefingFiles,
    pub devices: Devices,
    pub watches: suzatary_io_chains::watches::Watches,
    /// The mail corpus the verbs read.
    pub corpus: FakeCorpus,
    pub repository: FakeRepository,
    pub manuals: FakeManuals,
    pub model: FakeModel,
    /// Every call to the model, as the journal was told of it.
    pub journal: suzatary_base::capability::Recording,
    pub web: FakeWeb,
    pub drive: FakeDriveAccess,
    pub junk: FakeJunk,
    pub files: FakeFiles,
    /// The upload index, over the upload store the server's upload route writes.
    pub upload_index: suzatary_io_local::upload_index::UploadIndexFiles,
    /// What a PDF's text layer reads as.
    pub pdf: FakePdf,
    /// What the taps read of withheld documents, where `Acting` keeps it.
    pub tapped: suzatary_io_local::docs::Docs<
        suzatary_held::held_value::HeldFile,
        suzatary_held::secretary::attached::Attached,
    >,
    pub conversations: Conversations,
    /// Every answer block an erasure emptied from the render cache, by source.
    pub forgot: Arc<suzatary_stores::stores::conversations::Forgot>,
    /// The secretary's memory, its history beside the conversations.
    pub memory:
        suzatary_io_chains::objects::Objects<suzatary_memory::secretary::memory_event::MemoryEvent>,
    /// The recorded decisions, beside the memory.
    pub decisions: suzatary_io_chains::objects::Objects<suzatary_todo::decisions::DecisionEvent>,
    /// The memory's texts, beside its history.
    pub memory_texts: suzatary_io_local::texts::Texts<suzatary_memory::secretary::memory::Fact>,
    /// The digest's history, beside the memory.
    pub digests:
        suzatary_io_chains::objects::Objects<suzatary_digest::secretary::digest_event::DigestEvent>,
    /// The digest's summaries' words, beside its history.
    pub digest_texts:
        suzatary_io_local::texts::Texts<suzatary_digest::secretary::digest_line::Line>,
    /// Whose retraction or erasure the digest believes: none here.
    pub owners: std::collections::BTreeSet<prodrome_data::event::Actor>,
    /// The topic stores, begun, as a box's server begins them.
    pub topics: suzatary_io_chains::topics::Topics,
    /// The inbox store, real so a submit holds the server's `flock`.
    pub inbox:
        suzatary_io_chains::objects::Objects<suzatary_inbox::secretary::inbox_event::InboxEvent>,
    /// The owner's repositories, none by default.
    pub repos: Arc<suzatary_io_sources::repos::Mirrors>,
    /// The calendar, a cache-tier store under the root; empty unless a test learns into it.
    pub calendar: suzatary_io_chains::calendar::Clocked,
    /// The tables a Connect card is checked against; none declared.
    pub configuration: Arc<dyn suzatary_connect::settings::Configuration>,
    /// Where each declared secret stands: nothing set, unless a scene reads its own store.
    pub secrets: Arc<dyn suzatary_connect::secrets::Secrets + Send + Sync>,
}

impl Fakes {
    /// Fakes over the six-todo chain, with everything else empty and a
    /// fresh temp root per capability that needs a directory.
    #[must_use]
    pub fn new(tag: &str) -> Self {
        let root = scratch(tag);
        let events = six_todo_chain(&root);
        Self::over(root, suzatary_io_chains::objects::Objects::chain(events))
    }

    /// Fakes over a caller-built chain, their other directories named apart from `tag`.
    #[must_use]
    pub fn with_store(tag: &str, events: EventStore) -> Self {
        let root = scratch(&format!("{tag}-aux"));
        Self::over(root, suzatary_io_chains::objects::Objects::chain(events))
    }

    /// Fakes over `chain`, every directory it needs under `root`, laid out
    /// as an instance whose data root is `root`.
    #[must_use]
    pub fn over(
        root: PathBuf,
        chain: suzatary_io_chains::objects::Objects<suzatary_record::record::TodoEvent>,
    ) -> Self {
        // Every deployment signs: the chain adopted the cast's root and trusts the owner's phone.
        suzatary_signatures::signatures::adopt(&chain, &cast_keys(), &self::chain())
            .expect("the scene adopts its root");
        suzatary_signatures::signatures::trust(
            &chain,
            cast_keys().root(),
            &actor(CAST.owner.actor),
            &phone().public(),
        )
        .expect("the phone is trusted at the terminal");
        let forgot = Arc::new(suzatary_stores::stores::conversations::Forgot::default());
        let topics = suzatary_io_chains::topics::Topics::open(
            root.join("secretary-topics"),
            root.join("secretary-topic-aliases"),
        );
        topics.begin().expect("the scene begins its topic stores");
        let conversations = Conversations::open(
            root.join("secretary-lines"),
            root.join("secretary-lines-keys"),
            root.join("secretary-lines-runs"),
            Arc::clone(&forgot) as _,
            Arc::clone(&forgot) as _,
        );
        conversations
            .lines
            .begin()
            .expect("the scene begins its lines, as the box does at start");
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
            watches: suzatary_io_chains::watches::Watches::open(root.join("watches")),
            corpus: FakeCorpus::new(),
            repository: FakeRepository,
            manuals: FakeManuals,
            model: FakeModel::default(),
            journal: suzatary_base::capability::Recording::default(),
            web: FakeWeb::default(),
            inbox: suzatary_io_chains::objects::Objects::open(root.join("inbox")),
            repos: Arc::new(suzatary_io_sources::repos::Mirrors::new(
                root.join("mirrors"),
                suzatary_io_sources::repos::Source::Local {
                    base: root.join("remotes"),
                },
                suzatary_services::repos::Declared::default(),
                Box::new(NoSecrets),
            )),
            drive: FakeDriveAccess::default(),
            junk: FakeJunk::default(),
            files: FakeFiles::default(),
            pdf: FakePdf::default(),
            tapped: suzatary_io_local::tapped::beside(&kept_store(&root)),
            upload_index: suzatary_io_local::upload_index::UploadIndexFiles::new(
                root.join("secretary/upload-index"),
                upload_store(&root),
            ),
            conversations,
            forgot,
            memory: suzatary_io_chains::objects::Objects::open(root.join("memory")),
            decisions: suzatary_io_chains::objects::Objects::open(root.join("decisions")),
            memory_texts: suzatary_io_local::texts::Texts::open(
                root.join("secretary-memory-texts"),
            ),
            digests: suzatary_io_chains::objects::Objects::digests(
                root.join("digests"),
                std::collections::BTreeSet::new(),
            ),
            digest_texts: suzatary_io_local::texts::Texts::open(
                root.join("secretary-digest-texts"),
            ),
            owners: std::collections::BTreeSet::new(),
            secrets: Arc::new(suzatary_base::capability::Nothing),
            calendar: suzatary_io_chains::calendar::Clocked::new(
                [root.join("cache/plugins/ics/store")],
                suzatary_base::zone::Zone::UTC,
            ),
            configuration: Arc::new(EditsRecorder::new()),
            root,
        }
    }

    /// The same, a Connect card checked against `configuration`.
    #[must_use]
    pub fn configuration(
        self,
        configuration: Arc<dyn suzatary_connect::settings::Configuration>,
    ) -> Self {
        Self {
            configuration,
            ..self
        }
    }

    #[must_use]
    pub fn whois(mut self, whois: FakeWhois) -> Self {
        self.whois = whois;
        self
    }

    /// A bundle whose secrets stand as `secrets` says.
    #[must_use]
    pub fn secrets(
        mut self,
        secrets: Arc<dyn suzatary_connect::secrets::Secrets + Send + Sync>,
    ) -> Self {
        self.secrets = secrets;
        self
    }

    #[must_use]
    pub fn mailbox(mut self, mailbox: FakeMailbox) -> Self {
        self.mailbox = mailbox;
        self
    }

    /// A bundle that reads every PDF's text layer as `layer`.
    #[must_use]
    pub fn pdf_layer(mut self, layer: &str) -> Self {
        self.pdf = FakePdf(Some(layer.to_owned()));
        self
    }

    /// A bundle whose mail corpus answers over seeded messages.
    #[must_use]
    pub fn corpus(mut self, corpus: FakeCorpus) -> Self {
        self.corpus = corpus;
        self
    }

    #[must_use]
    pub fn model(mut self, model: FakeModel) -> Self {
        self.model = model;
        self
    }

    #[must_use]
    pub fn repos(mut self, repos: Arc<suzatary_io_sources::repos::Mirrors>) -> Self {
        self.repos = repos;
        self
    }

    #[must_use]
    pub fn junk(mut self, junk: FakeJunk) -> Self {
        self.junk = junk;
        self
    }

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
    #[must_use]
    /// The memory, both halves and the system's salt.
    pub fn memories(&self) -> suzatary_digest::secretary::memories::Memories<'_> {
        suzatary_digest::secretary::memories::Memories {
            history: &self.memory,
            texts: &self.memory_texts,
            cards: &self.inbox,
            entropy: &suzatary_io_local::texts::SystemEntropy,
            digests: suzatary_digest::secretary::digests::Digests {
                history: &self.digests,
                texts: &self.digest_texts,
                lines: self.conversations.conversations(),
                owners: &self.owners,
                compacting: &suzatary_digest::secretary::compacting::Compacting::Off,
            },
        }
    }

    /// The secretary's record over these fakes.
    #[must_use]
    pub fn secretary(&self) -> suzatary_secretary::secretary::Secretary<'_> {
        suzatary_secretary::secretary::Secretary {
            chain: &self.chain,
            corpus: &self.corpus,
            mail: &self.mailbox,
            repository: &self.repository,
            manuals: &self.manuals,
            render: &self.render,
            clock: &self.clock,
            model: &self.model,
            journal: &self.journal,
            conversations: self.conversations.conversations(),
            inbox: &self.inbox,
            memories: self.memories(),
            decisions: suzatary_todo::decisions::Decisions {
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
            sync: &suzatary_base::capability::Nothing,
            health: &suzatary_base::capability::Nothing,
            older: &suzatary_base::capability::Nothing,
            uploads: &self.files,
            upload_index: &self.upload_index,
            kept: &self.files,
            pdf: &self.pdf,
            tapped: &self.tapped,
            configuration: self.configuration.as_ref(),
            watches: &self.watches,
        }
    }

    pub fn server(&self) -> suzatary_server::server::Server<'_> {
        suzatary_server::server::Server {
            secretary: self.secretary(),
            whois: &self.whois,
            notify: &self.notify,
            answers: &self.answers,
            devices: &self.devices,
            models: &self.model,
        }
    }
}

/// A fixed forecast, never a socket: no test here is about the weather.
pub struct FakeWeather;

impl suzatary_services::weather::Weather for FakeWeather {
    fn today(&self) -> Result<String, suzatary_services::weather::WeatherError> {
        Ok("62-71°F, clear, rain chance peaks at 5% — no umbrella needed.".to_owned())
    }
}

/// The web: the rules a scene allows, and every request recorded and answered with a line
/// naming it, never the network.
#[derive(Default)]
pub struct FakeWeb {
    /// Hosts the `web` verb reads without asking.
    pub allow: Vec<suzatary_services::web::AllowRule>,
    /// Every web request performed, oldest first.
    pub done: std::sync::Mutex<Vec<suzatary_services::web::WebRequest>>,
}

impl suzatary_services::web::Web for FakeWeb {
    fn allow_rules(&self) -> Vec<suzatary_services::web::AllowRule> {
        self.allow.clone()
    }

    fn perform(
        &self,
        request: &suzatary_services::web::WebRequest,
    ) -> Result<suzatary_services::web::WebAnswer, suzatary_services::web::WebFailure> {
        self.done
            .lock()
            .expect("the web log lock")
            .push(request.clone());
        Ok(suzatary_services::web::WebAnswer {
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
    pub allow: Vec<suzatary_base::drive_path::DrivePath>,
    pub drive: Option<suzatary_io_sources::drive::ProtonDrive>,
}

impl suzatary_services::drive::Drive for FakeDriveAccess {
    fn allowed_folders(&self) -> Vec<suzatary_base::drive_path::DrivePath> {
        self.allow.clone()
    }

    fn port(&self) -> &dyn suzatary_services::drive::Port {
        match &self.drive {
            Some(drive) => drive,
            None => &NoDrive,
        }
    }

    fn read(
        &self,
        file: &suzatary_base::drive_path::DrivePath,
    ) -> Result<FileRead, suzatary_base::drive_path::DriveFailure> {
        self.drive
            .as_ref()
            .ok_or_else(no_drive)
            .and_then(|drive| drive.read(file))
    }
}

/// A scene with no Drive: every operation says so.
struct NoDrive;

impl suzatary_services::drive::Port for NoDrive {
    fn list(
        &self,
        _: &suzatary_base::drive_path::DrivePath,
    ) -> Result<Vec<suzatary_services::drive::DriveEntry>, suzatary_base::drive_path::DriveFailure>
    {
        Err(no_drive())
    }
    fn download(
        &self,
        _: &suzatary_base::drive_path::DrivePath,
    ) -> Result<Vec<u8>, suzatary_base::drive_path::DriveFailure> {
        Err(no_drive())
    }
}

/// The owner's uploads and the files kept for the model, held in memory.
#[derive(Default)]
pub struct FakeFiles {
    pub uploads: HashMap<suzatary_held::attachment::AttachmentId, suzatary_held::upload::Upload>,
    /// Every file a verb kept for the model, in order.
    pub kept: Mutex<Vec<Vec<u8>>>,
}

impl
    suzatary_store::store::Gets<
        suzatary_held::attachment::AttachmentId,
        suzatary_held::upload::Upload,
    > for FakeFiles
{
    fn get(
        &self,
        id: &suzatary_held::attachment::AttachmentId,
    ) -> Result<Option<suzatary_held::upload::Upload>, suzatary_store::store::StoreError> {
        Ok(self.uploads.get(id).cloned())
    }
}

impl suzatary_store::store::Gets<suzatary_held::attachment::AttachmentId, Vec<u8>> for FakeFiles {
    fn get(
        &self,
        id: &suzatary_held::attachment::AttachmentId,
    ) -> Result<Option<Vec<u8>>, suzatary_store::store::StoreError> {
        Ok(self
            .kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|bytes| &suzatary_held::attachment::AttachmentId::of(bytes) == id)
            .cloned())
    }
}

impl suzatary_store::store::Doc<suzatary_held::attachment::AttachmentId, Vec<u8>> for FakeFiles {
    fn set(
        &self,
        _id: &suzatary_held::attachment::AttachmentId,
        bytes: &Vec<u8>,
    ) -> Result<(), suzatary_store::store::StoreError> {
        self.kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(bytes.clone());
        Ok(())
    }

    fn remove(
        &self,
        id: &suzatary_held::attachment::AttachmentId,
    ) -> Result<(), suzatary_store::store::StoreError> {
        self.kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|bytes| &suzatary_held::attachment::AttachmentId::of(bytes) != id);
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
    Recording(Box<suzatary_io_chains::conversations::ConversationStores>),
    /// Every act recorded as [`Performer::Recorded`] does, but a Connect card's, which begins a
    /// sign-in through the server's own table, running the `claude` its [`Reaching`] names.
    #[cfg(feature = "fixture")]
    SigningIn,
    /// `Acting::Real` over a scripted OAuth transport and a one-account
    /// roster, so a test drives the whole device flow through the real route.
    Real {
        tag: String,
        /// The one Microsoft address its roster declares, or `None` for the roster the server's
        /// instance file declares as it reads at the tap.
        account: Option<String>,
        conversations: Box<suzatary_io_chains::conversations::ConversationStores>,
        oauth: Oauth,
    },
}

/// What a real performer signs in through.
enum Oauth {
    Scripted(ScriptedOauth),
    /// A token store the test built, such as one asking a fake Microsoft.
    Store(suzatary_io_mail::oauth::TokenStore),
}

/// A test server builder: `new`, then a scene's settings, then `start`; every effect defaults to a recorder.
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
    failures: Vec<responded::Performed>,
    performer: Performer,
    idle: Option<std::time::Duration>,
    publishing: Option<Arc<suzatary_io_sources::repos::Mirrors>>,
    drive: Option<suzatary_io_sources::drive::ProtonDrive>,
    firing: Option<suzatary_cli::commands::coding::Firing>,
    keys: Option<suzatary_signatures::signatures::BoxKeys>,
    models: Reaching,
    checks: Option<Arc<CheckRecorder>>,
    watching: Option<PathBuf>,
    health_watching: Option<(PathBuf, Arc<std::sync::atomic::AtomicUsize>)>,
    google: Googled,
    reveal: Option<Arc<dyn suzatary_held::held_value::Reveal + Send + Sync>>,
    mounts: suzatary_cli::instance::Mounts,
}

/// Where a test server asks Google and keeps what a sign-in with Google keeps: Google's own and
/// a store holding nothing, so nothing is ever asked, unless a scene gives it a fake.
pub struct Googled {
    pub endpoints: suzatary_google::google::Endpoints,
    /// The store a client and a grant are read from.
    pub store: Arc<dyn suzatary_io_access::secrets::Resolves + Send + Sync>,
}

impl Default for Googled {
    fn default() -> Self {
        Self {
            endpoints: suzatary_google::google::Endpoints::google(),
            store: Arc::new(serve::Unresolved),
        }
    }
}

impl Googled {
    /// What a tapped calendar event is written through.
    fn grants(&self) -> suzatary_io_mail::google::Grants {
        suzatary_io_mail::google::Grants::new(
            suzatary_io_mail::google::Google::new(self.endpoints.clone()),
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
    pub answered: Option<suzatary_health::health::Healths<String>>,
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
            Some(answered) => state.with_answered(Arc::new(std::sync::Mutex::new(answered))),
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

impl suzatary_connect::settings::Verify for VerifyRecorder {
    fn verify(
        &self,
        name: &suzatary_connect::settings::Settable,
        _: &suzatary_base::capability::Secret,
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
type Effects = suzatary_server::server::Server<'static>;

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
    pub requests: suzatary_io_local::requests::Requests,
    /// The last pairing code, as `suzatary device pair` would keep it.
    pub pairings: Arc<suzatary_signatures::signatures::InMemoryPairings>,
}

impl Running {
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

impl suzatary_held::vault::Vault for FakeVault {
    fn keep(
        &self,
        name: &str,
        bytes: &[u8],
    ) -> Result<suzatary_base::drive_path::DrivePath, suzatary_base::drive_path::DriveFailure> {
        self.0
            .lock()
            .expect("unpoisoned")
            .push((name.to_owned(), bytes.to_vec()));
        suzatary_held::vault::folder()
            .join(name)
            .map_err(|failed| suzatary_base::drive_path::DriveFailure::Refused(failed.to_string()))
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
            mounts: suzatary_cli::instance::Mounts::default(),
            keys: Some(cast_keys()),
            models: Reaching::default(),
            checks: None,
            watching: None,
            health_watching: None,
            google: Googled::default(),
            reveal: None,
        }
    }

    /// Reading held values back through `reveal`, as a box over its mail does.
    pub fn reveal(self, reveal: Arc<dyn suzatary_held::held_value::Reveal + Send + Sync>) -> Self {
        Self {
            reveal: Some(reveal),
            ..self
        }
    }

    /// Serving the sources' health the file beside `heartbeat` holds, and filing the cards owed
    /// whenever it moves what is owed, as `serve` does; each ring of the bell adds one to `rings`.
    pub fn health_watching(
        self,
        heartbeat: PathBuf,
        rings: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self {
            health_watching: Some((heartbeat, rings)),
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
        endpoints: suzatary_google::google::Endpoints,
        store: Arc<dyn suzatary_io_access::secrets::Resolves + Send + Sync>,
    ) -> Self {
        Self {
            google: Googled { endpoints, store },
            ..self
        }
    }

    /// Reaching its models as `models` says.
    #[cfg(feature = "fixture")]
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
    pub fn failing(self, failures: impl IntoIterator<Item = responded::Performed>) -> Self {
        Self {
            failures: failures.into_iter().collect(),
            ..self
        }
    }

    /// With a performer that begins a tapped Connect card's sign-in for real.
    #[cfg(feature = "fixture")]
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
        conversations: suzatary_io_chains::conversations::ConversationStores,
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
        conversations: suzatary_io_chains::conversations::ConversationStores,
        oauth: ScriptedOauth,
    ) -> Self {
        Self {
            performer: Performer::Real {
                tag: tag.to_owned(),
                account: Some(account.to_owned()),
                conversations: Box::new(conversations),
                oauth: Oauth::Scripted(oauth),
            },
            ..self
        }
    }

    /// With `Acting::Real` as the performer, signing in through `tokens`, and resolving an
    /// account against the instance file the settings' editor reads at the tap.
    pub fn real_acts_with_tokens(
        self,
        tag: &str,
        conversations: suzatary_io_chains::conversations::ConversationStores,
        tokens: suzatary_io_mail::oauth::TokenStore,
    ) -> Self {
        Self {
            performer: Performer::Real {
                tag: tag.to_owned(),
                account: None,
                conversations: Box::new(conversations),
                oauth: Oauth::Store(tokens),
            },
            ..self
        }
    }

    /// With `Acting::Real` as the performer, resolving an account against the instance file the
    /// settings' editor reads at the tap.
    pub fn real_acts_over_instance(
        self,
        tag: &str,
        conversations: suzatary_io_chains::conversations::ConversationStores,
        oauth: ScriptedOauth,
    ) -> Self {
        Self {
            performer: Performer::Real {
                tag: tag.to_owned(),
                account: None,
                conversations: Box::new(conversations),
                oauth: Oauth::Scripted(oauth),
            },
            ..self
        }
    }

    /// Where the server finds each chain's store, so a plan beside one is found.
    pub fn mounting(self, mounts: suzatary_cli::instance::Mounts) -> Self {
        Self { mounts, ..self }
    }

    /// A test points the coding routine at loopback, never the real one.
    pub fn firing(self, firing: suzatary_cli::commands::coding::Firing) -> Self {
        Self {
            firing: Some(firing),
            ..self
        }
    }

    /// Signing with the box's keys, whose root a tapped device card registers keys with.
    pub fn keys(self, keys: suzatary_signatures::signatures::BoxKeys) -> Self {
        Self {
            keys: Some(keys),
            ..self
        }
    }

    /// With these repositories behind a tapped patch, rather than none.
    pub fn publishing(self, mirrors: Arc<suzatary_io_sources::repos::Mirrors>) -> Self {
        Self {
            publishing: Some(mirrors),
            ..self
        }
    }

    /// With this fake Drive behind a tapped document.
    #[cfg_attr(not(feature = "fixture"), expect(dead_code))]
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
                suzatary_signatures::signatures::trust(
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
    pub fn settings_names(mut self, names: suzatary_connect::settings::Settables) -> Self {
        self.settings.live.settables = names;
        self
    }

    /// The same server deleting a removed account's sign-ins from `tokens`.
    pub fn grants(mut self, tokens: suzatary_io_mail::oauth::TokenStore) -> Self {
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
        let postman: Arc<dyn suzatary_server::secretary::routes::Post + Send + Sync> =
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
            suzatary_io_mail::google::Google::new(self.google.endpoints.clone()),
            Arc::clone(&self.google.store),
            Arc::clone(&self.settings.edits) as _,
            &self.deploying.app_url,
        );
        let vault = Arc::new(FakeVault::default());
        let requests = recorded_requests(address.port());
        let pairings = Arc::new(suzatary_signatures::signatures::InMemoryPairings::default());
        let mut effects = (self.effects)(held.server());
        health_served(&mut effects, self.health_watching.as_ref());
        let (chained, deploying, door) = dialled(&effects, self.deploying, self.this_box, address);
        let state = State::over(
            chained,
            Vec::new(),
            deploying,
            door,
            serve::Signed {
                requests: Arc::new(requests.clone()),
                asks: Arc::new(suzatary_io_chains::objects::Objects::open(
                    held.root.join("secretary-asks"),
                )),
                pairings: pairings.clone(),
            },
            serve::Performers { postman, acting },
            Arc::clone(&workers),
            self.settings.over(Arc::new(held.root.clone())),
        )
        .with_keys(self.keys)
        .with_projects(suzatary_cli::commands::serve::projects(self.mounts))
        .with_wall(stamped)
        // The store the fakes' upload index reads, under their scratch root.
        .with_uploads(upload_store(&held.root), vault.clone());
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
        if let Some((heartbeat, rings)) = self.health_watching {
            health_watched(&state, &heartbeat, rings);
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

/// `effects` reading the sources' health from the file beside the watched heartbeat, if any.
fn health_served(
    effects: &mut suzatary_server::server::Server<'_>,
    watching: Option<&(PathBuf, Arc<std::sync::atomic::AtomicUsize>)>,
) {
    if let Some((heartbeat, _)) = watching {
        effects.secretary.health = Box::leak(Box::new(suzatary_io_local::source_health::beside(
            heartbeat,
        )));
    }
}

/// The watcher `serve` runs over the health file beside `heartbeat`, filing on each ring and
/// counting it in `rings`.
fn health_watched(state: &State, heartbeat: &Path, rings: Arc<std::sync::atomic::AtomicUsize>) {
    let moved = Arc::new(tokio::sync::Notify::new());
    runtime().spawn(serve::notice_on_moved(state.clone(), Arc::clone(&moved)));
    runtime().spawn(serve::health_watched(
        state.clone(),
        suzatary_io_local::source_health::file_beside(heartbeat),
        move || {
            rings.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            moved.notify_one();
        },
    ));
}

/// How a test server publishes: through `mirrors`, or by default through a mirror of no repos,
/// under a fixed author.
fn publishing(
    mirrors: Option<Arc<suzatary_io_sources::repos::Mirrors>>,
) -> Arc<suzatary_cli::commands::secretary::Publishing> {
    Arc::new(suzatary_cli::commands::secretary::Publishing {
        mirrors: mirrors.unwrap_or_else(|| {
            Arc::new(suzatary_io_sources::repos::Mirrors::new(
                std::env::temp_dir().join("suzatary-test-no-repos"),
                suzatary_io_sources::repos::Source::Local {
                    base: std::env::temp_dir().join("suzatary-test-no-remotes"),
                },
                suzatary_services::repos::Declared::default(),
                Box::new(NoSecrets),
            ))
        }),
        // Fixed, so a scene never depends on who runs it having a git identity.
        author: Some(suzatary_io_sources::repos::Author {
            name: "Ada Test".to_owned(),
            email: "ada@example.test".to_owned(),
        }),
    })
}

/// Where a server records the requests it admitted, and keeps their answers.
fn recorded_requests(port: u16) -> suzatary_io_local::requests::Requests {
    suzatary_io_local::requests::Requests::open(requests_dir(port), answers_dir(port))
}

/// The directory a server on `port`, started by the running test, pushes its request records
/// from.
#[must_use]
pub fn requests_dir(port: u16) -> PathBuf {
    suzatary_testkit::this_test().join(format!("requests-{port}"))
}

/// The box-local directory a server on `port`, started by the running test, keeps its replay
/// answers in.
#[must_use]
pub fn answers_dir(port: u16) -> PathBuf {
    suzatary_testkit::this_test().join(format!("answers-{port}"))
}

/// The box's clock in UTC as a request's window reads it: [`STAMPED`].
fn stamped() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(STAMPED)
        .expect("an instant")
        .with_timezone(&chrono::Utc)
}

/// The owner's uploads under a scratch root, as the instance lays them out.
fn upload_store(root: &std::path::Path) -> suzatary_io_local::attachments::AttachmentStore {
    suzatary_io_local::attachments::AttachmentStore::new(
        root.join("secretary/attachments"),
        suzatary_io_local::pdftotext::Pdftotext::new("."),
    )
}

/// Where a tapped document is kept for the model, under the scene's own root.
fn kept_store(root: &std::path::Path) -> suzatary_io_local::blobs::Blobs {
    suzatary_io_local::blobs::Blobs::open(root.join("secretary").join("files"))
}

/// A Drive CLI nothing is at: a tapped document in a scene that did not ask for a Drive fails,
/// never reads.
fn drive_nowhere() -> suzatary_io_sources::drive::ProtonDrive {
    suzatary_io_sources::drive::ProtonDrive::new(
        std::env::temp_dir().join("suzatary-test-no-proton-drive"),
        std::env::temp_dir().join("suzatary-test-no-drive-scratch"),
        PathBuf::from("."),
    )
}

/// The same conversations' store opened again, for a second holder of it.
pub fn again(conversations: &Conversations) -> Conversations {
    conversations.reopened()
}

/// The chosen performer, over the server's own worker table and the chain `caps` serves.
fn performer(
    chosen: Performer,
    caps: &'static Fakes,
    acts: &Arc<ActRecorder>,
    workers: &Arc<serve::Workers>,
    (publishing, drive, signing, edits): (
        Arc<suzatary_cli::commands::secretary::Publishing>,
        Arc<suzatary_io_sources::drive::ReadOnly>,
        Arc<serve::sign_ins::SignIns>,
        Arc<dyn suzatary_connect::settings::Edits + Send + Sync>,
    ),
    firing: suzatary_cli::commands::coding::Firing,
    google: Arc<suzatary_io_mail::google::Grants>,
) -> Arc<dyn suzatary_server::secretary::routes::Perform + Send + Sync> {
    use suzatary_cli::commands::coding::Coder;
    use suzatary_cli::commands::secretary::Acting;
    match chosen {
        Performer::Recorded => Arc::clone(acts) as _,
        #[cfg(feature = "fixture")]
        Performer::SigningIn => Arc::new(SignsIn {
            recorded: Arc::clone(acts),
            signing,
        }),
        #[cfg(feature = "fixture")]
        Performer::Recording(conversations) => {
            let conversations: &'static Conversations = Box::leak(conversations);
            Arc::new(Acting::Recording {
                coder: Arc::new(Coder {
                    firing,
                    conversations,
                }),
                conversations,
                workers: Arc::clone(workers),
                publishing,
                drive,
                kept: kept_store(&caps.root),
                signing,
                google,
                ledger: Arc::new(test_ledger(&caps.root)),
            })
        }
        Performer::Real {
            tag,
            account,
            conversations,
            oauth,
        } => {
            use suzatary_mail::mail::account::{Authority, MailAccount, MsImap, Sync as MailSync};
            let roster: Arc<dyn suzatary_connect::settings::Edits + Send + Sync> = match account {
                Some(account) => Arc::new(EditsRecorder::declaring(
                    suzatary_connect::connect::Declared {
                        accounts: vec![MailAccount {
                            email: suzatary_mail::mail::account_email::AccountEmail::new(&account)
                                .expect("a lower-case address"),
                            subdir: "study".to_owned(),
                            sync: MailSync::MsImap(MsImap {
                                authority: Authority::Organizations,
                                inbox_folders: suzatary_mail::mail::folders::Folders::inbox(),
                                sent_folder: "Sent Items".to_owned(),
                            }),
                        }],
                        ..suzatary_connect::connect::Declared::default()
                    },
                )),
                None => edits,
            };
            let root = scratch(&format!("{tag}-acting-real"));
            let tokens = match oauth {
                Oauth::Scripted(oauth) => suzatary_io_mail::oauth::TokenStore::with_http(
                    root.join("oauth"),
                    Arc::new(oauth),
                ),
                Oauth::Store(tokens) => tokens,
            };
            let conversations: &'static Conversations = Box::leak(conversations);
            Arc::new(Acting::Real {
                roster,
                coder: Arc::new(Coder {
                    firing,
                    conversations,
                }),
                conversations,
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
                // A test server runs no filing task to ring.
                ended: Arc::new(|| {}),
                ledger: Arc::new(test_ledger(&caps.root)),
            })
        }
    }
}

/// A mail ledger under `root`, keyed by a fixed key: the one a tapped noise card writes.
fn test_ledger(root: &Path) -> suzatary_io_local::state::Ledger {
    suzatary_io_local::state::Ledger::new(
        root.join("state.json"),
        root.join("local/mail.json"),
        || {
            suzatary_mail::mail::dedup_key::DedupKey::of(&suzatary_base::capability::Secret::new(
                "12".repeat(32),
            ))
            .map_err(|failed| failed.to_string())
        },
    )
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

#[must_use]
pub fn get(url: &str) -> ureq::http::Response<ureq::Body> {
    agent().get(url).call().expect("the request completes")
}

/// A write as the app makes one: the CSRF triple over JSON, signed by the owner's [`phone`].
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
pub fn signed_prints(object: &suzatary_record::record::Envelope) -> Vec<String> {
    let name = prodrome_data::event::seal_hash(object);
    vec![
        prodrome_data::event::canonical_envelope(object),
        prodrome_data::event::canonical_envelope(&suzatary_record::record::Envelope::Signed(
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
            suzatary_signatures::signatures::CLIENT_HEADER,
            suzatary_signatures::signatures::CLIENT_MIN.to_string(),
        )
        .header("sec-fetch-site", "same-origin")
        .send_json(body)
        .expect("the request completes")
}

/// The cast's deployment keys: its root, and the agent's.
#[must_use]
pub fn cast_keys() -> suzatary_signatures::signatures::BoxKeys {
    use suzatary_connect::cast::keys;
    suzatary_signatures::signatures::BoxKeys::of(
        &keys::root(),
        &suzatary_base::capability::Secret::new(keys::ROOT),
        &suzatary_base::capability::Secret::new(keys::AGENT),
    )
    .expect("the cast's keys")
}

/// The owner's phone: the device every scene's [`act`] is signed by, trusted when a server
/// starts.
#[must_use]
pub fn phone() -> prodrome_dag::sign::Secret {
    suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE)
}

/// The box's operator at the test server at `base`: the cast's root key, the owner's actor, and
/// the instant every test server reads as now.
#[must_use]
pub fn operator(base: &str) -> suzatary_cli::commands::call::Operator {
    suzatary_cli::commands::call::Operator::new(
        base,
        Arc::clone(cast_keys().root()),
        actor(CAST.owner.actor),
    )
    .stamping(stamped)
}

/// The instant a test's device stamps on its requests: the fake clock's own, and every test
/// server's UTC now as a request's window reads it.
pub const STAMPED: &str = "2026-09-02T12:00:00.000Z";

/// [`act`], as a device holding `key` sends it under a deployment that signs, stamped
/// [`STAMPED`]: the request named by its content and signed, as `web/src/wire/http.ts` does.
pub fn act_signed(
    url: &str,
    body: &serde_json::Value,
    key: &prodrome_dag::sign::Secret,
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
    key: &prodrome_data::sign::PublicKey,
) -> prodrome_data::event::Hash {
    named_with(url, body, at, NONCE, key)
}

fn named_with(
    url: &str,
    body: &serde_json::Value,
    at: &str,
    nonce: &str,
    key: &prodrome_data::sign::PublicKey,
) -> prodrome_data::event::Hash {
    let bytes = serde_json::to_vec(body).expect("JSON prints");
    suzatary_signatures::signatures::request_name(
        "POST",
        &target_of(url),
        at,
        nonce,
        &suzatary_signatures::signatures::body_digest(&bytes),
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
    key: &prodrome_dag::sign::Secret,
    at: &str,
) -> ureq::http::Response<ureq::Body> {
    sent_signed(url, body, key, at, NONCE)
}

fn sent_signed(
    url: &str,
    body: &serde_json::Value,
    key: &prodrome_dag::sign::Secret,
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
    key: &prodrome_dag::sign::Secret,
    at: &str,
    nonce: &str,
) -> ureq::http::Response<ureq::Body> {
    suzatary_cli::commands::call::Signed::of("POST", &target_of(url), bytes, key, at, nonce)
        .headers
        .into_iter()
        .fold(
            agent()
                .post(url)
                .header("x-suzatary-act", "1")
                .header(
                    suzatary_signatures::signatures::CLIENT_HEADER,
                    suzatary_signatures::signatures::CLIENT_MIN.to_string(),
                )
                .header("sec-fetch-site", "same-origin")
                .header("content-type", "application/json"),
            |request, (name, value)| request.header(name, value),
        )
        .send(bytes)
        .expect("the request completes")
}

/// A read as a tab under a deployment that signs sends one: named over its method, target,
/// [`STAMPED`], [`NONCE`] and an empty body, and signed by `key`.
pub fn get_signed(url: &str, key: &prodrome_dag::sign::Secret) -> ureq::http::Response<ureq::Body> {
    suzatary_cli::commands::call::Signed::of("GET", &target_of(url), b"", key, STAMPED, NONCE)
        .headers
        .into_iter()
        .fold(agent().get(url), |request, (name, value)| {
            request.header(name, value)
        })
        .call()
        .expect("the request completes")
}

pub fn body_json(response: &mut ureq::http::Response<ureq::Body>) -> serde_json::Value {
    response.body_mut().read_json().expect("a JSON body")
}

#[must_use]
pub fn status(response: &ureq::http::Response<ureq::Body>) -> u16 {
    response.status().as_u16()
}

/// Each account's Junk folder as a value; no accounts by default.
#[derive(Default)]
pub struct FakeJunk {
    pub churn: junk::ChurnList,
    pub folders: Vec<(String, JunkFolder)>,
    /// Every header read, as `(account, since)`.
    pub asked: std::sync::Mutex<Vec<(String, chrono::NaiveDate)>>,
}

/// One account's Junk in a scene.
pub enum JunkFolder {
    /// Messages by UID: headers and a body.
    Holds(Vec<(junk::JunkHeader, &'static str)>),
    /// The account's grant lapsed.
    NeedsLogin,
}

impl FakeJunk {
    fn folder(&self, account: &str) -> Result<&[(junk::JunkHeader, &'static str)], JunkFailure> {
        match self.folders.iter().find(|(name, _)| name == account) {
            Some((_, JunkFolder::Holds(held))) => Ok(held),
            Some((_, JunkFolder::NeedsLogin)) => Err(JunkFailure::NeedsLogin(
                suzatary_mail::mail::oauth::NeedsLogin::Lapsed {
                    email: account.to_owned(),
                    slot: "imap",
                    code: suzatary_mail::mail::oauth::Lapse::InvalidGrant,
                    description: "AADSTS700082: The refresh token has expired.".to_owned(),
                },
            )),
            None => Err(JunkFailure::Unknown(account.to_owned())),
        }
    }
}

impl Junk for FakeJunk {
    fn junk_accounts(&self) -> Vec<String> {
        self.folders.iter().map(|(name, _)| name.clone()).collect()
    }

    fn churn(&self) -> junk::ChurnList {
        self.churn.clone()
    }

    fn junk_headers(
        &self,
        account: &str,
        since: chrono::NaiveDate,
    ) -> Result<Vec<junk::JunkHeader>, JunkFailure> {
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
        uid: junk::Uid,
    ) -> Result<junk::JunkMessage, JunkFailure> {
        self.folder(account)?
            .iter()
            .find(|(header, _)| header.uid() == uid)
            .map(|(header, body)| junk::JunkMessage {
                account: account.to_owned(),
                header: header.clone(),
                body: (*body).to_owned(),
                truncated: false,
            })
            .ok_or(JunkFailure::Missing {
                account: account.to_owned(),
                uid: uid.0,
            })
    }
}

fn no_drive() -> suzatary_base::drive_path::DriveFailure {
    suzatary_base::drive_path::DriveFailure::Unreachable("no Drive in this scene".to_owned())
}

/// A fake Proton Drive CLI answering the two reads, logging every call and exiting 99 on anything else.
pub struct FakeDrive {
    /// The scratch directory holding the script, fixtures and call log.
    pub dir: PathBuf,
}

impl FakeDrive {
    /// Write the script and its fixtures under a scratch directory.
    #[must_use]
    pub fn new(tag: &str) -> Self {
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
            "Form W-2. Employee SSN 123-45-6789. Employer EIN 00-0000001. \
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
        let _ = suzatary_testkit::fake_program(&dir, "proton-drive", &script);
        Self { dir }
    }

    /// The real interpreter over this script.
    #[must_use]
    pub fn proton(&self) -> suzatary_io_sources::drive::ProtonDrive {
        suzatary_io_sources::drive::ProtonDrive::new(
            self.dir.join("proton-drive"),
            self.dir.join("scratch"),
            PathBuf::from("."),
        )
    }

    /// The same Drive with `bytes` kept privately as `name`, as a private upload leaves it.
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

impl suzatary_io_access::secrets::Resolves for NoSecrets {
    fn get(
        &self,
        _name: &str,
        _reason: &str,
    ) -> Result<Option<suzatary_base::capability::Secret>, suzatary_io_access::secrets::SecretError>
    {
        Ok(None)
    }
}

/// The compiled CV in [`local_repos`]: binary, so only an attachment carries it.
pub const CV_PDF: &[u8] = b"%PDF-1.7\n\0\x01\x02 Ada Quill\n%%EOF\n";

/// A local public repository `bmabsout/cv` holding `cv.typ` and `cv.pdf`, and mirrors over
/// it, under `root`: a scene's repositories, with no network and no token.
#[must_use]
pub fn local_repos(root: &Path) -> Arc<suzatary_io_sources::repos::Mirrors> {
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
pub fn local_mirrors(root: &Path) -> suzatary_io_sources::repos::Mirrors {
    local_mirrors_declaring(
        root,
        suzatary_services::repos::RepoTarget {
            public: true,
            ..suzatary_services::repos::RepoTarget::default()
        },
    )
}

/// [`local_mirrors`], declaring `bmabsout/cv` as `target` says.
#[must_use]
pub fn local_mirrors_declaring(
    root: &Path,
    target: suzatary_services::repos::RepoTarget,
) -> suzatary_io_sources::repos::Mirrors {
    suzatary_io_sources::repos::Mirrors::new(
        root.join("mirrors"),
        suzatary_io_sources::repos::Source::Local {
            base: root.join("remotes"),
        },
        suzatary_services::repos::Declared::new([("bmabsout/cv".parse().expect("a name"), target)])
            .expect("one repository"),
        Box::new(NoSecrets),
    )
}

/// One of the secretary's stores as a tab pulls it, `since` the heads it holds: its objects read
/// into their nodes, and every head.
pub fn pulled<E: prodrome_data::schema::Schema<Vocabulary: Default>>(
    base: &str,
    store: &str,
) -> Vec<prodrome_dag::registers::Node<E>> {
    pulled_with_keys(base, store).0
}

/// [`pulled`], and the keys the pull carried, as a tab keeps them.
pub fn pulled_with_keys<E: prodrome_data::schema::Schema<Vocabulary: Default>>(
    base: &str,
    store: &str,
) -> (
    Vec<prodrome_dag::registers::Node<E>>,
    suzatary_memory::secretary::key_ring::KeyRing,
) {
    let mut answer = get(&format!("{base}/api/stores/{store}/objects"));
    assert_eq!(status(&answer), 200, "the {store} store answers");
    let payload = body_json(&mut answer);
    let keys = suzatary_memory::secretary::key_ring::KeyRing::of(
        &serde_json::from_value(payload["keys"].clone()).unwrap_or_default(),
    );
    let prints = payload["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| {
            (
                prodrome_data::event::Hash::new(
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
    let nodes = prodrome_dag::dag::Dag::from_prints(
        &E::Vocabulary::default(),
        prints,
        prodrome_data::husk::Sender::Stranger,
    )
    .nodes()
    .expect("a replica of the store reads");
    (nodes, keys)
}

/// The lines as a tab holds them: the skeleton pulled piece after piece and landed, then every
/// node's content asked by hash, each literal held as its print and each husk taken in its place.
pub fn held_lines(base: &str) -> suzatary_lines::secretary::held_replica::HeldReplica {
    use suzatary_lines::secretary::held_replica::{HeldReplica, NotAnAnswer};
    let mut replica = HeldReplica::pulled(|since| -> Result<_, NotAnAnswer> {
        let mut answer = get(&format!("{base}/api/stores/lines/skeleton?since={since}"));
        assert_eq!(status(&answer), 200, "the lines' skeleton answers");
        Ok(serde_json::from_value(body_json(&mut answer)).expect("a skeleton"))
    })
    .expect("the skeleton reads");
    let lacking = replica.lacking(&suzatary_lines::secretary::outlines::Scope::Nodes(
        suzatary_lines::secretary::outlines::Nodes::Whole,
    ));
    for ask in
        suzatary_api::routes::asks(lacking.iter().map(|node| (node.hash.as_str(), node.size)))
    {
        let mut answer = get(&format!(
            "{base}/api/stores/lines/content?hashes={}",
            ask.join(",")
        ));
        assert_eq!(status(&answer), 200, "the lines' content answers");
        let content: suzatary_api::wire::ContentPayload =
            serde_json::from_value(body_json(&mut answer)).expect("a content answer");
        replica
            .take(content.objects)
            .expect("the box's husks are its lines'");
    }
    replica
}

/// The inbox's fold and every conversation, as a tab's replicas of the two stores read them.
pub fn replicas(
    base: &str,
) -> (
    suzatary_inbox::secretary::inbox::Fold,
    Vec<suzatary_conversations::secretary::store::Stored>,
) {
    // The conversations first: the box files a proposal before the line that names it, so an inbox
    // pulled after them holds every proposal they name.
    let mut lines = held_lines(base);
    let inbox = suzatary_inbox::secretary::inbox::Fold::of(&pulled(base, "inbox"));
    let conversations = lines
        .lines
        .read()
        .0
        .expect("the conversations read")
        .into_iter()
        .rev()
        .map(|stored| (*stored).clone())
        .collect();
    (inbox, conversations)
}

/// One conversation's window as a tab reads and dresses it from its replicas and the box's routes.
pub fn reading(base: &str, id: &str, query: &[(&str, &str)]) -> serde_json::Value {
    let asked = |key: &str| {
        query
            .iter()
            .find_map(|(name, value)| (*name == key).then_some(*value))
    };
    let window = suzatary_topics::secretary::readings::Window {
        before: suzatary_api::query::turn_before(asked("before")).expect("a before"),
        limit: suzatary_api::query::turn_limit(
            asked("limit"),
            suzatary_topics::secretary::readings::DEFAULT_TURN_WINDOW,
        )
        .expect("a limit"),
        about: suzatary_api::query::about(asked("about")).expect("an about"),
    };
    let mut lines = held_lines(base);
    let inbox = suzatary_inbox::secretary::inbox::Fold::of(&pulled(base, "inbox"));
    let stored = lines
        .lines
        .transcript(&suzatary_base::conversation_id::ConversationId::new(id).expect("an id"))
        .unwrap_or_else(|| panic!("no conversation named {id}"))
        .expect("the conversation reads");
    let mut memory = get(&format!("{base}/api/secretary/memory"));
    let memory: Option<suzatary_topics::wire::topics::MemoryPayload> =
        (status(&memory) == 200).then(|| memory.body_mut().read_json().expect("a memory"));
    let read = suzatary_topics::secretary::readings::conversation(
        &stored,
        &inbox,
        &window,
        &suzatary_topics::secretary::readings::Dress {
            memory: memory.as_ref(),
            ..suzatary_topics::secretary::readings::Dress::BARE
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
            let sha = suzatary_base::answer::sha(block["source"].as_str().expect("a source"));
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
    serde_json::to_value(suzatary_topics::secretary::readings::conversations(
        &conversations,
        &inbox,
    ))
    .expect("a payload prints")
}

/// The chains the deployment serves (`/api/me`'s list), pulled from `/api/<chain>/objects` as
/// a tab's replicas pull them: the set a proposal is priced through.
pub fn chains_read(base: &str) -> suzatary_signatures::chain_set::Read {
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
                            prodrome_data::event::Hash::new(
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
                let untrusted = prodrome_data::policy::Untrusted::of(
                    payload["untrusted"]
                        .as_array()
                        .expect("untrusted")
                        .iter()
                        .map(|actor| {
                            prodrome_data::event::Actor::new(actor.as_str().expect("a name"))
                                .expect("an actor")
                        }),
                );
                (
                    suzatary_base::chains::ChainName::new(name).expect("a slug"),
                    untrusted,
                    prodrome_dag::dag::Dag::from_prints(
                        &suzatary_record::record::TodoVocabulary::default(),
                        prints,
                        prodrome_data::husk::Sender::Stranger,
                    )
                    .objects()
                    .clone()
                    .into_iter()
                    .collect(),
                )
            }
        })
        .collect();
    suzatary_signatures::chain_set::Read::of_objects(held)
}

/// Every proposal the Inbox lists at `at`, as a tab's replica reads them.
pub fn inbox_listing(base: &str, at: &str) -> Vec<serde_json::Value> {
    let (inbox, _) = replicas(base);
    let at = prodrome_data::instant::datetime_of(
        prodrome_data::instant::parse_iso(at).expect("an instant"),
    )
    .expect("in range");
    let read = chains_read(base);
    let set = read.set(at);
    // As the tab asks it: the memory by its route, for a remember card's words.
    let mut memory = get(&format!("{base}/api/secretary/memory"));
    let memory: Option<suzatary_topics::wire::topics::MemoryPayload> =
        (status(&memory) == 200).then(|| memory.body_mut().read_json().expect("a memory"));
    serde_json::to_value(suzatary_topics::secretary::readings::inbox(
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

/// A PDF reader that answers one text layer for every PDF, or fails when it holds none.
#[derive(Default)]
pub struct FakePdf(Option<String>);

impl suzatary_held::pdf_text::PdfText for FakePdf {
    fn layer(&self, _: &[u8]) -> Result<suzatary_held::file_read::Layer, String> {
        self.0
            .clone()
            .map(|text| {
                suzatary_held::file_read::Layer::read(text, suzatary_held::file_read::Form::Absent)
            })
            .ok_or_else(|| "this fake reads no PDF".to_owned())
    }
}

/// A one-page PDF that says `text`, which must hold no `(`, `)` or `\`.
pub fn pdf_saying(text: &str) -> Vec<u8> {
    let content = format!("BT /F1 24 Tf 20 100 Td ({text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> \
         /MediaBox [0 0 200 200] /Contents 5 0 R >>"
            .to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// `act` filed as the secretary's in `conversation`, as the setup agent files it: the box files only what a
/// reading owes, and withdraws any other card of its own.
pub fn asked(fakes: &Fakes, conversation: &str, act: acts::Act) {
    use suzatary_base::conversation_id::ConversationId;
    use suzatary_conversations::secretary::store::{Entry, Kind};
    use suzatary_inbox::secretary::proposal::Reasons;
    let id = ConversationId::new(conversation).expect("a slug");
    let opened = Entry::Opened {
        at: suzatary_base::capability::Datetime::new(2026, 9, 2, 11, 0, 0, 0).expect("an instant"),
        title: suzatary_memory::secretary::words::Words::kept("Connect".to_owned()),
        model: "a fake".to_owned(),
        kind: Kind::Topic,
    };
    // Opened by the first card, as a conversation is.
    let _ = transcript::start(fakes.conversations.conversations(), &id, &opened);
    suzatary_topics::secretary::filed::file(
        fakes.conversations.conversations(),
        &fakes.inbox,
        &[],
        fakes.clock.now(),
        suzatary_topics::secretary::filed::Filing {
            conversation: &id,
            proposer: proposer::Proposer::Secretary,
            proposal: suzatary_inbox::secretary::proposal::Proposal::Act(act),
            reasons: Reasons::new(
                "the test asks",
                vec!["briefing:2026-09-02".parse().expect("a reference")],
            )
            .expect("reasons"),
            serves: None,
            revises: None,
        },
    )
    .expect("filed");
}

/// A file read for the model, a PDF with `layer` as its text.
pub fn file_read(
    file: &suzatary_held::held_value::HeldFile,
    name: &str,
    mime: &str,
    bytes: Vec<u8>,
    layer: &str,
) -> suzatary_held::file_read::FileRead {
    let Ok(read) = suzatary_held::file_read::FileRead::of(
        file,
        name,
        mime,
        bytes,
        suzatary_held::file_read::read(layer),
    );
    read
}

/// `releases` written as the `releases.toml` beside the store at `dir`, naming the store's
/// genesis: what a deployment finds its plan store by.
pub fn plan_beside(dir: &std::path::Path, releases: &str) {
    let genesis = std::fs::read_dir(dir.join("objects"))
        .expect("the objects")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| std::fs::read_to_string(path).is_ok_and(|text| text.starts_with("Genesis(")))
        .and_then(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
        .expect("a genesis");
    std::fs::write(
        dir.join("releases.toml"),
        format!("genesis = \"{genesis}\"\n{releases}"),
    )
    .expect("the plan");
}

/// A store under `root` holding only a `Genesis` and, beside it, a plan declaring `suzatarytwo` as
/// 2.0: what a deployment mounts as its plan store.
#[must_use]
pub fn a_plan_store(root: &std::path::Path) -> PathBuf {
    let dir = root.join("roadmap");
    let store = suzatary_record::record::EventStore::new(
        dir.clone(),
        prodrome_data::policy::Untrusted::none(),
    );
    suzatary_store::store::begun(&store, suzatary_record::record::GENESIS).expect("begun");
    plan_beside(
        &dir,
        "base = 1\n\n[[release]]\ntodo = \"suzatarytwo\"\nmajor = 2\nminor = 0\n",
    );
    dir
}

/// The body of an erasure of turns `turns` of conversation `id`, each named by its line as the
/// tab's reading names it.
pub fn erasing(base: &str, id: &str, turns: &[usize]) -> serde_json::Value {
    let read = reading(base, id, &[]);
    let rows = read["turns"].as_array().expect("turns");
    let lines: Vec<serde_json::Value> = turns
        .iter()
        .map(|turn| {
            let at = rows
                .iter()
                .position(|row| row["index"] == *turn)
                .unwrap_or_else(|| panic!("no turn {turn} drawn"));
            read["lines"][at].clone()
        })
        .collect();
    serde_json::json!({ "lines": lines })
}
