//! The mail verbs, each calling one operation and printing its result as JSON.
//!
//! `send` alone bypasses the corpus operator, which holds no send path, so sending stays
//! human-only on both sides of the seam; `--dry-run` prints exactly what would go on the wire.

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use clap::Subcommand;
use suzatary_core::capability::{Corpus as _, Query};
use suzatary_core::excerpt::Excerpt;
use suzatary_core::mail::account::{AccountName, MailAccount, Sender};
use suzatary_core::mail::idle;
use suzatary_core::mail::imap::Session as _;
use suzatary_core::mail::oauth::{Consent, DeviceCode, Imap, Smtp};
use suzatary_core::mail::send::{
    Attachment, Composed, Outgoing, SendError, Step, build_mime, guess_mime_type,
};
use suzatary_core::mail::state::Decision;
use suzatary_core::mail::transport::Transport as _;
use suzatary_io::corpus::Operator;
use suzatary_io::imap::{IMAP_HOST, IMAP_PORT, Outlook};
use suzatary_io::mail_sync::{TRIAGE_UNIT, Timer};
use suzatary_io::mailbox::Notmuch;
use suzatary_io::oauth::TokenStore;
use suzatary_io::sync::Syncer;

use crate::instance::Instance;

/// Print a value as indented JSON, the shape the agent reads.
fn emit<T: serde::Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// The grants this deployment holds.
#[must_use]
pub fn tokens(instance: &Instance) -> TokenStore {
    TokenStore::new(&instance.paths.oauth)
}

/// The triage's bookkeeping: `state.json`, pushed, and the box-local rest, joined by the key
/// secretspec holds.
#[must_use]
pub fn ledger(instance: &Instance) -> suzatary_io::state::Ledger {
    suzatary_io::state::Ledger::resolving(
        instance.mail.state.clone(),
        instance.local.mail(),
        instance.secrets(),
    )
}

/// The corpus, read-only.
#[must_use]
pub fn notmuch(instance: &Instance) -> Notmuch {
    crate::wiring::Ground::of(instance).notmuch(instance.code())
}

/// The pull.
#[must_use]
pub fn syncer(instance: &Instance) -> Syncer {
    crate::wiring::Ground::of(instance).syncer()
}

/// The grants a sign-in with Google keeps, against Google's own endpoints.
#[must_use]
pub fn google_grants(instance: &Instance) -> suzatary_io::google::Grants {
    suzatary_io::google::Grants::new(
        suzatary_io::google::Google::new(suzatary_core::google::Endpoints::google()),
        std::sync::Arc::new(instance.secrets()),
    )
}

/// The mechanical operations bound to this deployment, shared with `suzatary mcp` so the
/// agent and the operator run against one corpus, dedup authority and syncer.
#[must_use]
pub fn operator(instance: &Instance) -> Operator {
    crate::wiring::Ground::of(instance).operator(instance.code(), instance.persona.zone)
}

/// `suzatary sync`: pull every account, reindex, re-tag (which rings each open watch the corpus
/// now holds a match for), then push each ring not yet delivered.
///
/// The sync keeps its report in the state directory (`Paths::last_sync`) and prints it to stdout,
/// progress to stderr.
pub fn sync(path: &Path) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let synced = operator(&instance).sync();
    deliver_rings(&instance);
    emit(&synced?)
}

/// Push each ring still owed, and record each one the push service accepted, so a ring whose
/// push failed is pushed again by the next sync.
fn deliver_rings(instance: &Instance) {
    use suzatary_core::capability::Clock as _;
    use suzatary_core::mail::watch;
    let watches = suzatary_io::watches::Watches::open(instance.local.watches());
    let now = || prodrome_core::fpl::instant_of(suzatary_io::clock::SystemClock.now());
    let owed = match watch::owed(&watches, now()) {
        Ok(owed) => owed,
        Err(failed) => return eprintln!("sync: the watches did not read: {}", failed.message()),
    };
    for (id, message) in owed {
        let push = suzatary_core::capability::Push::Expected { message };
        let recorded = super::notify::deliver(instance, &push)
            .map_err(|failed| failed.to_string())
            .and_then(|_| {
                watch::delivered(&watches, &id, now()).map_err(|r| r.message().to_owned())
            });
        if let Err(failed) = recorded {
            eprintln!(
                "sync: watch {} rang and its push is still owed: {failed}",
                id.as_str()
            );
        }
    }
}

/// What `suzatary new` was told.
#[derive(Debug, clap::Args)]
pub struct NewArgs {
    /// At most this many, newest first.
    #[arg(long)]
    limit: Option<usize>,
}

/// `suzatary new`: the triage queue.
pub fn queue(path: &Path, args: &NewArgs) -> anyhow::Result<()> {
    emit(&operator(&Instance::open(path)?).queue(args.limit)?)
}

/// `suzatary gate`: fire the triage rule on the queue, printing what it read and the firing,
/// and exit 3 when it does not fire.
///
/// A queue the corpus cannot answer is an error, which fails the pass loudly
/// rather than reading as an empty inbox.
pub fn gate(path: &Path) -> anyhow::Result<()> {
    use suzatary_core::mail::triage::{Mailbox, Queue, Triage};
    let instance = Instance::open(path)?;
    let rows = operator(&instance).queue(None)?;
    let report = suzatary_io::mail_sync::read_report(&instance.paths.last_sync)
        .with_context(|| format!("reading {}", instance.paths.last_sync.display()))?
        .map(|(report, _)| report);
    let queue = Queue::of(&rows, report);
    // Each pass is a fresh process: the memo that lasts is the dedup authority, whose
    // dispositions take what the last pass triaged out of the queue.
    let Some((_, firing)) = suzatary_core::rule::Engine::new(vec![Triage]).due(&Mailbox, &queue)
    else {
        println!("{queue}");
        std::process::exit(3);
    };
    println!("{queue} ({firing})");
    Ok(())
}

/// What `suzatary search` was told.
#[derive(Debug, clap::Args)]
pub struct SearchArgs {
    /// The query, in notmuch's own language.
    query: String,
    /// At most this many.
    #[arg(long)]
    limit: Option<usize>,
}

/// `suzatary search`: any notmuch query, newest first.
pub fn search(path: &Path, args: &SearchArgs) -> anyhow::Result<()> {
    let query = Query::new(&args.query)?;
    emit(&operator(&Instance::open(path)?).search(&query, args.limit)?)
}

/// What `suzatary show` was told.
#[derive(Debug, clap::Args)]
pub struct ShowArgs {
    /// The locator or the hash.
    id: String,
}

/// `suzatary show`: one message by notmuch id or canonical hash.
pub fn show(path: &Path, args: &ShowArgs) -> anyhow::Result<()> {
    emit(&operator(&Instance::open(path)?).show(&args.id)?)
}

/// What `suzatary mark` was told.
#[derive(Debug, clap::Args)]
pub struct MarkArgs {
    /// The canonical hash from the `new`/`search` row.
    hash: String,
    /// What was decided.
    #[arg(value_parser = ["todo", "noise", "skip"])]
    disposition: String,
    /// A one-line why.
    #[arg(long)]
    note: Option<String>,
    /// The todo a `todo` disposition named.
    #[arg(long)]
    todo_id: Option<String>,
    /// The notmuch id from the same row. Passing it is free and keeps the
    /// hash resolvable by `show` without a rescan.
    #[arg(long)]
    native_id: Option<String>,
}

/// `suzatary mark`: record a decision about a canonical message.
pub fn mark(path: &Path, args: &MarkArgs) -> anyhow::Result<()> {
    let decision = Decision::parse(&args.disposition, args.todo_id.as_deref())?;
    emit(&operator(&Instance::open(path)?).mark(
        &args.hash,
        decision,
        args.note.as_deref(),
        args.native_id.as_deref(),
    )?)
}

/// What `noise` can do; a subcommand because the operator's manual spells it `noise add`.
#[derive(Debug, Subcommand)]
pub enum NoiseAction {
    /// Ignore this sender wholesale.
    Add {
        /// The address.
        sender: String,
    },
}

/// Which act on the wholesale-ignore list.
#[derive(Debug, clap::Args)]
pub struct NoiseArgs {
    /// Which act.
    #[command(subcommand)]
    action: NoiseAction,
}

/// `suzatary noise add`: a sender to ignore wholesale.
pub fn noise(path: &Path, args: &NoiseArgs) -> anyhow::Result<()> {
    let NoiseAction::Add { sender } = &args.action;
    emit(&operator(&Instance::open(path)?).noise_add(sender)?)
}

/// What `suzatary backfill` was told.
#[derive(Debug, clap::Args)]
pub struct BackfillArgs {
    /// A Microsoft account in the roster.
    #[arg(long)]
    account: String,
    /// The IMAP folder, e.g. "Sent Items".
    #[arg(long)]
    folder: String,
    /// The maildir leaf (default: the folder's slug).
    #[arg(long)]
    subdir: Option<String>,
    /// Only the newest N, for a trial run.
    #[arg(long)]
    limit: Option<usize>,
}

/// `suzatary backfill`: a read-only pull of a whole server folder.
pub fn backfill(path: &Path, args: &BackfillArgs) -> anyhow::Result<()> {
    emit(&operator(&Instance::open(path)?).backfill(
        &args.account,
        &args.folder,
        args.subdir.as_deref(),
        args.limit,
    )?)
}

/// What `suzatary auth` was told; with no address it runs the box's own setup.
#[derive(Debug, clap::Args)]
pub struct AuthArgs {
    /// A Microsoft account in the roster.
    email: Option<String>,
    /// Which protocol's grant: `imap` for syncing, `smtp` for sending, kept as separate
    /// consents so a leaked read token cannot send.
    #[arg(long, default_value = "imap", value_parser = ["imap", "smtp"])]
    scope: String,
}

/// `suzatary auth [ADDRESS]`: an account's grant, or the box's own setup script.
pub fn auth(path: &Path, args: &AuthArgs) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    match &args.email {
        Some(email) => authorise(&instance, email, &args.scope),
        None => super::script(&instance, "auth-setup.sh"),
    }
}

/// The device-code flow; the only caller of `Consent::given_at_a_terminal`, and it
/// refuses without a terminal because an unwatched device flow hangs until the code expires.
fn authorise(instance: &Instance, email: &str, scope: &str) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!(
            "`suzatary auth` is interactive: it prints a code somebody has to type into a \
             browser. Run it from a terminal."
        );
    }
    let account = instance.mail.accounts.microsoft(email).ok_or_else(|| {
        let known: Vec<&str> = instance
            .mail
            .accounts
            .accounts()
            .iter()
            .filter(|a| a.microsoft().is_some())
            .map(|a| a.email.as_str())
            .collect();
        anyhow::anyhow!(
            "{email} is not a Microsoft account in the roster (have: {})",
            known.join(", ")
        )
    })?;

    let consent = Consent::given_at_a_terminal();
    let store = tokens(instance);
    // The code goes to stderr because stdout is this verb's JSON answer.
    let mut announce = |code: &DeviceCode| {
        eprintln!("\n{}\n", code.message);
    };
    let wait = |how_long: std::time::Duration| std::thread::sleep(how_long);

    match scope {
        "imap" => store.authorise::<Imap>(&consent, account, &mut announce, &wait)?,
        "smtp" => store.authorise::<Smtp>(&consent, account, &mut announce, &wait)?,
        other => bail!("--scope is imap or smtp, got {other:?}"),
    }
    emit(&serde_json::json!({
        "authorised": account.email,
        "scope": scope,
        "granted": store.granted(account),
    }))
}

/// Which accounts can sync and which can send, read off the token store for `doctor`.
#[must_use]
pub fn grants(instance: &Instance) -> Vec<(String, Vec<&'static str>)> {
    let store = tokens(instance);
    instance
        .mail
        .accounts
        .accounts()
        .iter()
        .filter(|account| account.microsoft().is_some())
        .map(|account| (account.email.to_string(), store.granted(account)))
        .collect()
}

/// What `suzatary send` was told — read it back with `--dry-run` before any of it goes on a wire.
#[derive(Debug, clap::Args)]
pub struct SendArgs {
    /// The address to send as.
    #[arg(long = "from", value_name = "ADDR")]
    sender: String,
    /// A visible recipient. Repeatable.
    #[arg(long, required = true, value_name = "ADDR")]
    to: Vec<String>,
    /// A visible copy. Repeatable. There is no Bcc, by design.
    #[arg(long, value_name = "ADDR")]
    cc: Vec<String>,
    /// The subject.
    #[arg(long)]
    subject: String,
    /// The body, inline.
    #[arg(long, group = "the-body")]
    body: Option<String>,
    /// The body, from a file.
    #[arg(long, group = "the-body")]
    body_file: Option<PathBuf>,
    /// A file to attach. Repeatable.
    #[arg(long, value_name = "PATH")]
    attach: Vec<PathBuf>,
    /// The bare Message-Id this replies to.
    #[arg(long, value_name = "MESSAGE_ID")]
    in_reply_to: Option<String>,
    /// The reply chain, bare. Repeatable.
    #[arg(long, value_name = "MESSAGE_ID")]
    references: Vec<String>,
    /// Print the MIME and send nothing.
    #[arg(long)]
    dry_run: bool,
}

/// Read a file to attach, guessing its type from its name.
fn attachment(path: &Path) -> anyhow::Result<Attachment> {
    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Attachment {
        mime_type: guess_mime_type(&filename),
        content: std::fs::read(path)
            .with_context(|| format!("reading the attachment {}", path.display()))?,
        filename,
    })
}

/// One message onto its account's transport, reached only by the human's `send` and the
/// owner's tap, and deliberately in no effect record so no verb can send.
///
/// # Errors
/// The transport's own refusal, whole.
pub fn deliver(
    instance: &Instance,
    sender: &Sender,
    out: &Outgoing,
    message_id: &str,
    at: i64,
) -> Result<suzatary_core::mail::send::Sent, SendError> {
    match sender {
        Sender::Microsoft { email, .. } => {
            let account = instance
                .mail
                .accounts
                .microsoft(email)
                .ok_or(SendError::Transport {
                    step: Step::Auth,
                    why: format!("{email} is not on this deployment's roster"),
                })?;
            let auth = |why: String| SendError::Transport {
                step: Step::Auth,
                why,
            };
            let token = tokens(instance)
                .access::<Smtp>(account)
                .map_err(|failed| auth(failed.to_string()))?
                .required()
                .map_err(|needs| auth(needs.to_string()))?;
            suzatary_io::smtp::Microsoft {
                email,
                token: &token,
            }
            .send(out, message_id, at)
        }
        Sender::Proton { smtp_user, .. } => suzatary_io::smtp::Bridge {
            address: suzatary_io::smtp::BRIDGE,
            secrets: &instance.secrets(),
            cert: &instance.paths.bridge_cert,
            smtp_user,
        }
        .send(out, message_id, at),
        Sender::Gmail { subdir, .. } => suzatary_io::smtp::Gmail {
            maildir: &instance.mail.root.join(subdir),
        }
        .send(out, message_id, at),
        Sender::Google { from } => {
            let auth = |why: String| SendError::Transport {
                step: Step::Auth,
                why,
            };
            let account = suzatary_core::mail::address::EmailAddress::new(from)
                .map_err(|refused| auth(refused.message().to_owned()))?;
            let token = google_grants(instance)
                .token(&account)
                .map_err(|refused| auth(refused.to_string()))?;
            suzatary_io::smtp::GoogleApi {
                token: &token.token,
            }
            .send(out, message_id, at)
        }
    }
}

/// The `Message-Id` this box mints, for a caller outside this module.
#[must_use]
pub fn mint_message_id(from: &str) -> String {
    message_id(from)
}

/// `suzatary send`: the human's send verb.
pub fn send(path: &Path, args: &SendArgs) -> anyhow::Result<()> {
    let instance = &Instance::open(path)?;
    let sender = instance
        .mail
        .accounts
        .sending(&args.sender)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{} has no send path (sendable: {})",
                args.sender,
                instance.mail.accounts.sendable().join(", ")
            )
        })?;

    let body = match (&args.body_file, &args.body) {
        (Some(path), _) => std::fs::read_to_string(path)
            .with_context(|| format!("reading the body from {}", path.display()))?,
        (None, Some(text)) => text.clone(),
        (None, None) => String::new(),
    };
    let composed = Composed {
        // The resolved sender, since the flag matches case-insensitively and the header
        // must carry the account's canonical spelling.
        sender: sender.from().to_owned(),
        to: args.to.clone(),
        cc: args.cc.clone(),
        subject: args.subject.clone(),
        body,
        attachments: args
            .attach
            .iter()
            .map(|path| attachment(path))
            .collect::<anyhow::Result<Vec<_>>>()?,
        in_reply_to: args.in_reply_to.clone(),
        references: args.references.clone(),
    };

    let message_id = message_id(sender.from());
    let now = chrono::Utc::now();
    let out = Outgoing::new(composed);

    if args.dry_run {
        return match out {
            Ok(out) => {
                let mime = build_mime(&out, &message_id, now.timestamp())?;
                print!("{}", String::from_utf8_lossy(&mime));
                emit(&serde_json::json!({
                    "dry_run": true,
                    "would_send_to": out.recipients(),
                    "problem": null,
                }))
            }
            Err(refused) => emit(&serde_json::json!({
                "dry_run": true,
                "would_send_to": [],
                "problem": refused.to_string(),
            })),
        };
    }

    let sent = out.and_then(|out| deliver(instance, &sender, &out, &message_id, now.timestamp()));

    match sent {
        Ok(sent) => emit(&sent),
        Err(failed) => {
            // Exit non-zero too, so a caller reading only the status never takes this as sent.
            let refused: Vec<String> = match &failed {
                SendError::RecipientsRefused(who) => who.clone(),
                _ => Vec::new(),
            };
            emit(&serde_json::json!({
                "error": failed.to_string(),
                "refused": refused,
            }))?;
            std::process::exit(1);
        }
    }
}

/// A bare `Message-Id` carrying the sender's domain, so a bounce is traceable to it.
fn message_id(from: &str) -> String {
    let domain = from.rsplit_once('@').map_or("localhost", |(_, d)| d);
    let now = chrono::Utc::now();
    // Pid and nanoseconds suffice: one message per invocation, sent by hand.
    format!(
        "{}.{}.{}@{domain}",
        now.format("%Y%m%d%H%M%S"),
        now.timestamp_subsec_nanos(),
        std::process::id()
    )
}

// `mail idle` never syncs: a debounced wake starts the same unit the timer does, through the one
// door `suzatary_io::mail_sync` is, and the timing decisions are `mail::idle`'s.

/// How long before `IDLE` is re-issued, since RFC 2177 lets a server drop one after 30 minutes.
const RENEW_AFTER: Duration = Duration::from_mins(29);

/// How long a burst of wakes, across accounts or duplicated by a server, is absorbed into
/// one trigger.
const DEBOUNCE_WINDOW: Duration = Duration::from_secs(30);

/// `suzatary mail <verb>`: the long-running mail verbs.
#[derive(Debug, clap::Args)]
pub struct MailArgs {
    /// Which long-running verb.
    #[command(subcommand)]
    action: MailAction,
}

/// A long-running mail verb.
#[derive(Debug, Subcommand)]
pub enum MailAction {
    /// Watch every IMAP account's inbox and trigger the triage pipeline on
    /// new mail, instead of waiting out the 5-minute timer. Never returns.
    Idle,
}

/// `suzatary mail <verb>`.
pub fn mail_command(path: &Path, args: &MailArgs) -> anyhow::Result<()> {
    match args.action {
        MailAction::Idle => idle_run(&Instance::open(path)?),
    }
}

/// The accounts `suzatary mail idle` would watch: the Microsoft ones, since Gmail and
/// Proton do not speak IMAP here.
#[must_use]
pub fn idle_watches(instance: &Instance) -> Vec<&str> {
    instance
        .mail
        .accounts
        .accounts()
        .iter()
        .filter(|account| account.microsoft().is_some())
        .map(|account| account.email.as_str())
        .collect()
}

/// One scope-prefixed line per state change, so the journal reads as a story.
fn log(scope: &str, message: &str) {
    eprintln!("mail idle: {scope}: {message}");
}

/// `suzatary mail idle`: one read-only IMAP session per Microsoft account, blocking on
/// `IDLE`, and the debounce they wake.
///
/// Every thread runs forever, so the first to end, a panic included, ends the process with an
/// error and systemd restarts the whole watch: a watch missing one account is a failure, never
/// a quieter success.
fn idle_run(instance: &Instance) -> anyhow::Result<()> {
    let roster = &instance.mail.accounts;
    let accounts: Vec<(AccountName, MailAccount)> = roster
        .names()
        .filter(|(_, account)| account.microsoft().is_some())
        .map(|(name, account)| (name, account.clone()))
        .collect();

    if accounts.is_empty() {
        log("idle", "no IMAP accounts in the roster — nothing to watch");
        return Ok(());
    }
    log(
        "idle",
        &format!(
            "watching {} account(s): {} (gmail and proton stay on the 5-minute timer)",
            accounts.len(),
            accounts
                .iter()
                .map(|(name, _)| name.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );

    // One shared debounce, so accounts waking together collapse into one trigger.
    let (wakes, incoming) = mpsc::channel::<()>();
    let unit = Timer::new(instance.paths.last_sync.clone());
    let tokens = tokens(instance);
    let mut jobs: Vec<(String, Job)> = accounts
        .into_iter()
        .map(|(name, account)| {
            let wakes = wakes.clone();
            let tokens = tokens.clone();
            let job: Job = Box::new(move || account_loop(name, &account, &tokens, &wakes));
            (name.to_string(), job)
        })
        .collect();
    drop(wakes);
    jobs.push((
        "debounce".to_owned(),
        Box::new(move || debounce_loop(DEBOUNCE_WINDOW, &incoming, &unit)),
    ));

    let ended = first_to_end(jobs).unwrap_or_default();
    bail!("the {ended} thread stopped; ending so the watch restarts whole")
}

/// A thread's body.
type Job = Box<dyn FnOnce() + Send>;

/// Run each named job on its own thread and answer the name of the first to end, by returning
/// or by a panic's unwind; `None` when there are none. The rest keep running until the process
/// ends.
fn first_to_end(jobs: Vec<(String, Job)>) -> Option<String> {
    /// Sends its job's name when dropped, which the unwind of a panic does too.
    struct Ends(mpsc::Sender<String>, String);
    impl Drop for Ends {
        fn drop(&mut self) {
            let _ = self.0.send(std::mem::take(&mut self.1));
        }
    }
    let (ended, ends) = mpsc::channel();
    for (name, job) in jobs {
        let ends = Ends(ended.clone(), name);
        thread::spawn(move || {
            let _ends = ends;
            job();
        });
    }
    drop(ended);
    ends.recv().ok()
}

/// One account's connection, held open and reconnected forever; every transition is
/// decided by the core's tested `idle::after`, never here. Its lines name it by `name`, never by
/// its address, and quote a failure as an [`Excerpt`].
fn account_loop(
    name: AccountName,
    account: &MailAccount,
    tokens: &TokenStore,
    wakes: &mpsc::Sender<()>,
) {
    let name = name.to_string();
    let mut attempt: u32 = 0;
    'reconnect: loop {
        log(&name, "connecting");
        let mut session = match connect(account, tokens) {
            Ok(session) => session,
            Err(error) => {
                let (delay, next_attempt) = idle::after_failure(attempt);
                attempt = next_attempt;
                log(
                    &name,
                    &format!(
                        "could not connect: {} — retrying in {delay:?}",
                        Excerpt::of(&format!("{error:#}"))
                    ),
                );
                thread::sleep(delay);
                continue 'reconnect;
            }
        };
        log(&name, "idle");

        loop {
            let watched = session.watch(RENEW_AFTER);
            let failure = watched.as_ref().err().map(ToString::to_string);
            let (step, next_attempt) = idle::after(watched.map_err(|_| ()), attempt);
            attempt = next_attempt;
            match step {
                idle::Step::Reidle => {
                    log(&name, "renewing IDLE (29 minutes, no change)");
                }
                idle::Step::Trigger => {
                    log(&name, "changed — waking the debouncer");
                    let _ = wakes.send(());
                }
                idle::Step::Reconnect(delay) => {
                    let why = failure.unwrap_or_else(|| "the connection was lost".to_owned());
                    log(
                        &name,
                        &format!("{} — reconnecting in {delay:?}", Excerpt::of(&why)),
                    );
                    thread::sleep(delay);
                    continue 'reconnect;
                }
            }
        }
    }
}

/// One fresh, `EXAMINE`d connection for one account.
fn connect(account: &MailAccount, tokens: &TokenStore) -> anyhow::Result<Outlook> {
    let token = tokens.access::<Imap>(account)?.required()?;
    let mut session = Outlook::connect(IMAP_HOST, IMAP_PORT, account.email.as_str(), &token)?;
    // Read-only: this verb triggers the sync and never mutates a mailbox itself.
    session.select("INBOX", true)?;
    Ok(session)
}

/// The shared debounce on its own thread, so no account thread decides alone when to
/// trigger.
fn debounce_loop(window: Duration, wakes: &mpsc::Receiver<()>, unit: &Timer) {
    let mut debounce = idle::Debounce::new(window);
    loop {
        // With no window open, an hour stands in for forever.
        let wait = debounce
            .deadline()
            .map_or(Duration::from_hours(1), |deadline| {
                deadline.saturating_duration_since(Instant::now())
            });
        match wakes.recv_timeout(wait) {
            Ok(()) => {
                if debounce.wake(Instant::now()) {
                    log("debounce", "window opened (30s)");
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if debounce.poll(Instant::now()) {
                    log("debounce", &format!("triggering {TRIAGE_UNIT}"));
                    trigger(unit);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                log("debounce", "every watcher stopped — exiting");
                return;
            }
        }
    }
}

/// Start the triage unit, relying on systemd to dedup against a run already in progress.
fn trigger(unit: &Timer) {
    match unit.run_now() {
        Ok(ran) if ran.status.success() => log("debounce", &format!("started {TRIAGE_UNIT}")),
        Ok(ran) => log("debounce", &format!("systemctl exited {}", ran.status)),
        Err(error) => log("debounce", &format!("could not run systemctl: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A panicking watcher is heard while its siblings still run, so `mail idle` ends rather
    /// than watching fewer accounts in silence.
    #[test]
    fn the_first_thread_to_end_is_named_even_when_it_panics() {
        let forever: Job = Box::new(|| {
            loop {
                thread::park();
            }
        });
        let panics: Job = Box::new(|| panic!("a watcher's bug"));
        assert_eq!(
            first_to_end(vec![
                ("one@example.test".to_owned(), forever),
                ("two@example.test".to_owned(), panics),
            ]),
            Some("two@example.test".to_owned())
        );
        assert_eq!(first_to_end(Vec::new()), None);
    }
}
