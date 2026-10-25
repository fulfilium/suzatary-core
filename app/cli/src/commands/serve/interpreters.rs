//! The one place that chooses an interpreter for every capability.
//!
//! `suzatary-core` is generic over a record and `suzatary-io` implements the traits;
//! this names which implementation each gets, so a fixture is a substitution the
//! routes cannot tell apart.

use std::net::IpAddr;
use std::sync::Arc;

use suzatary_core::capability::{
    Compact, Conversation, Model, ModelError, Pick, Reply, SessionModel, Sessions, Usage, Weather,
    WeatherError, Whois, WhoisError,
};
use suzatary_core::drive::WritePort as _;
use suzatary_io::secrets::Secretspec;
use suzatary_io::whois::Tailscaled;

use crate::instance::{Instance, Reserve};

/// Who names a peer.
///
/// The fixture arm is test-only and compiled out of the shipped build; it exists because
/// a headless browser here always arrives from loopback, which no daemon names.
pub enum Peers {
    /// The local daemon, over its unix socket.
    Daemon(Tailscaled),
    /// Every address is this login; test only.
    #[cfg(feature = "fixture")]
    Fixture(suzatary_core::capability::Login),
}

impl Whois for Peers {
    fn peer(&self, address: IpAddr) -> Result<suzatary_core::capability::Peer, WhoisError> {
        match self {
            Peers::Daemon(daemon) => daemon.peer(address),
            // Every peer is the fixture's writer, so none is this box.
            #[cfg(feature = "fixture")]
            Peers::Fixture(login) => Ok(suzatary_core::capability::Peer::Known(login.clone())),
        }
    }
}

/// Which model answers a conversation.
///
/// The scripted arm is test-only and compiled out of the shipped build, because no
/// browser test can drive a paid, networked, nondeterministic `claude -p`. Verbs are
/// dispatched on this side of the trait, so any arm can only answer text.
pub enum Brain {
    /// The `claude` CLI as a subprocess, each call joined into the model health.
    Cli(Box<Recorded>),
    /// A file of scripted replies; test only.
    #[cfg(feature = "fixture")]
    Scripted(crate::commands::fixture::Scripted),
}

impl Model for Brain {
    fn name(&self) -> String {
        match self {
            Brain::Cli(cli) => cli.name(),
            #[cfg(feature = "fixture")]
            Brain::Scripted(scripted) => scripted.name(),
        }
    }

    fn turn(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        match self {
            Brain::Cli(cli) => cli.turn(conversation, tools),
            #[cfg(feature = "fixture")]
            Brain::Scripted(scripted) => scripted.turn(conversation, tools),
        }
    }
}

/// Only the `claude` CLI subprocess holds a session.
impl Sessions for Brain {
    fn pick(&self) -> Pick<'_> {
        match self {
            Brain::Cli(cli) => Pick::Held(&**cli),
            #[cfg(feature = "fixture")]
            Brain::Scripted(_) => Pick::Turn(Box::new(self)),
        }
    }
}

/// The day's forecast for `[persona]`'s city.
pub struct Forecast(pub suzatary_core::persona::Persona);

impl Weather for Forecast {
    fn today(&self) -> Result<String, WeatherError> {
        // Wall-clock UTC, because `Clock::now()` is naive and cannot feed a zone conversion.
        suzatary_io::weather::line(chrono::Utc::now(), &self.0)
    }
}

/// Whether each declared secret resolves and which mail grants are held, never a value.
pub struct Declared {
    /// The secret store.
    pub secrets: Secretspec,
    /// This box's OAuth slots.
    pub tokens: suzatary_io::oauth::TokenStore,
    /// The reason the store's audit records for each question.
    pub asking: &'static str,
}

impl suzatary_core::capability::Secrets for Declared {
    // The value is discarded in the expression that reads it: only presence crosses.
    fn secret_state(
        &self,
        settable: &suzatary_core::settings::Settable,
    ) -> suzatary_core::capability::SecretState {
        suzatary_io::secrets::state(&self.secrets.get(settable.name(), self.asking))
    }

    fn mail_grants(
        &self,
        account: &suzatary_core::mail::account::MailAccount,
    ) -> suzatary_core::capability::MailGrants {
        use suzatary_core::mail::account::Sync;
        match account.sync {
            // A Google account's reading grant is its sign-in with Google, in the secret store.
            Sync::Google => suzatary_core::capability::MailGrants {
                imap: suzatary_core::mail::address::EmailAddress::new(account.email.as_str())
                    .is_ok_and(|email| {
                        matches!(
                            self.google_grant(&email),
                            suzatary_core::google::Held::Signed(_)
                        )
                    }),
                smtp: false,
            },
            Sync::MsImap(_) | Sync::Gmail | Sync::Proton(_) => {
                let held = self.tokens.granted(account);
                suzatary_core::capability::MailGrants {
                    imap: held.contains(&"imap"),
                    smtp: held.contains(&"smtp"),
                }
            }
        }
    }

    // The grant is read and parsed here, and only its parts leave.
    fn google_grant(
        &self,
        account: &suzatary_core::mail::address::EmailAddress,
    ) -> suzatary_core::google::Held {
        match self
            .secrets
            .get(&suzatary_core::google::grant_name(account), self.asking)
        {
            Ok(Some(kept)) => suzatary_core::google::KeptGrant::parse(kept.expose())
                .map_or(suzatary_core::google::Held::NotSignedIn, |kept| {
                    suzatary_core::google::Held::Signed(kept.parts)
                }),
            Ok(None) | Err(_) => suzatary_core::google::Held::NotSignedIn,
        }
    }
}

/// One test call before the model's token is kept, with the value given and never the stored
/// one, its result joined into the model health, so a model that answered is known to answer.
pub enum Verifying {
    /// For real.
    Real {
        /// The deployment, for the model's interpreter.
        instance: Arc<Instance>,
        /// Where each call's result is joined.
        answered: suzatary_io::model_health::Answered,
    },
    /// No call leaves: the fixture's model is scripted and answers whatever it is given.
    #[cfg(feature = "fixture")]
    Fixture {
        /// The deployment, for the model's name.
        instance: Arc<Instance>,
        /// Where the answer is joined.
        answered: suzatary_io::model_health::Answered,
    },
}

/// One call of the model with `token`, asking for a word.
fn probe(cli: &suzatary_io::model::ClaudeCli) -> Result<(), ModelError> {
    let asked = Conversation {
        system: "This is a test that the model answers. Answer with the one word: ok.".to_owned(),
        turns: vec![suzatary_core::capability::Turn::Person {
            text: "ok?".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        }],
    };
    cli.turn(&asked, &serde_json::json!([])).map(|_| ())
}

/// Join one call of the model named `model` into `answered`, now, `failed` saying why it did
/// not answer. Only a refused credential is joined as a failure: a rate limit, a timeout or an
/// outage says nothing of whether the model answers this box, so it is joined as nothing. A
/// register that cannot be written costs the call nothing, since the next call writes it again.
pub fn answered_by(
    answered: &suzatary_io::model_health::Answered,
    model: &str,
    failed: Option<&ModelError>,
) {
    use suzatary_core::capability::Clock as _;
    if failed.is_some_and(|failed| !matches!(failed, ModelError::Refused(_))) {
        return;
    }
    let at = prodrome_core::fpl::instant_of(suzatary_io::clock::SystemClock.now());
    let words = failed.map(ToString::to_string);
    if let Err(failed) = answered.record(model, at, words.as_deref()) {
        tracing::warn!("model health: {failed}");
    }
}

impl suzatary_core::settings::Verify for Verifying {
    fn verify(
        &self,
        name: &suzatary_core::settings::Settable,
        value: &suzatary_core::capability::Secret,
    ) -> Result<(), String> {
        if name.name() != suzatary_io::model::TOKEN {
            return Ok(());
        }
        match self {
            Verifying::Real { instance, answered } => {
                let cli = cli_of(
                    instance,
                    instance.secretary.model.as_str(),
                    &instance.secrets(),
                )
                .with_token(suzatary_io::model::TokenSource::fixed(value.clone()));
                let called = probe(&cli);
                answered_by(answered, &cli.name(), called.as_ref().err());
                called.map_err(|failed| format!("{}: {failed}", cli.name()))
            }
            #[cfg(feature = "fixture")]
            Verifying::Fixture { instance, answered } => {
                answered_by(
                    answered,
                    &suzatary_io::model::name_of(instance.secretary.model.as_str()),
                    None,
                );
                Ok(())
            }
        }
    }
}

/// Where the usage page reads its transcripts from.
///
/// The fixture reads nowhere, because a cold walk of the box's own transcripts
/// exceeds a browser scene's time budget.
#[derive(Debug, Clone)]
pub enum Ledger {
    /// The box's own transcripts.
    Box,
    /// An empty root the fixture owns; the usage page answers "nothing yet".
    Nowhere(std::path::PathBuf),
}

impl Ledger {
    /// A fresh empty directory under the system temp dir, this process's own.
    #[must_use]
    pub fn nowhere() -> Self {
        let root = std::env::temp_dir().join(format!(
            "suzatary-fixture-transcripts-{}",
            std::process::id()
        ));
        // A directory that cannot be made reads as empty too, so failure is ignored.
        let _ = std::fs::create_dir_all(&root);
        Self::Nowhere(root)
    }
}

/// Whether the subscription has room above the reserve.
///
/// `RateLimited` and `Refused` are the allowance endpoint refusing to answer, not the
/// subscription being full, so the first is judged by its remembered reading and either,
/// with none, counts as room.
fn subscription_has_room(allowance: &suzatary_core::usage::Allowance, reserve: Reserve) -> bool {
    let reserve = reserve.get();
    match allowance {
        suzatary_core::usage::Allowance::Reported(reported) => {
            reported.utilization <= 1.0 - reserve
        }
        suzatary_core::usage::Allowance::RateLimited {
            last_reported: Some(reported),
            ..
        } => reported.utilization <= 1.0 - reserve,
        suzatary_core::usage::Allowance::RateLimited {
            last_reported: None,
            ..
        }
        | suzatary_core::usage::Allowance::Refused { .. }
        | suzatary_core::usage::Allowance::Unknown => true,
    }
}

/// The deny-all every call to the model's CLI is fenced by, as the instance names it.
#[must_use]
pub fn fence(instance: &Instance) -> suzatary_io::model::Fence {
    suzatary_io::model::Fence::at(instance.secretary.settings.clone())
}

/// The `claude` CLI subprocess at `model`, with the settings fence, working
/// directory and token; the one spawner every caller shares so none can drift.
#[must_use]
pub fn cli_of(
    instance: &Instance,
    model: &str,
    secrets: &suzatary_io::secrets::Secretspec,
) -> suzatary_io::model::ClaudeCli {
    suzatary_io::model::ClaudeCli::new(model.to_owned(), fence(instance))
        .in_directory(instance.code().to_path_buf())
        .with_token(suzatary_io::model::TokenSource::from_store(secrets.clone()))
        .with_uploads(upload_store(instance))
        .with_files(file_store(instance))
}

/// The web as the secretary may read it.
///
/// Allow rules are reread from the instance file on every ask so a new rule counts
/// without a restart; a file that no longer reads allows nothing.
#[derive(Debug, Clone)]
pub struct WebAccess {
    fetcher: suzatary_io::web::Fetcher,
    instance: std::path::PathBuf,
}

impl WebAccess {
    /// Over this deployment's instance file, ten seconds a request.
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self {
            fetcher: suzatary_io::web::Fetcher::new(std::time::Duration::from_secs(10)),
            instance: instance.path.clone(),
        }
    }
}

impl suzatary_core::capability::Web for WebAccess {
    fn allow_rules(&self) -> Vec<suzatary_core::web::AllowRule> {
        crate::instance::File::read(&self.instance)
            .map(|file| file.secretary.web_allow)
            .unwrap_or_default()
    }

    fn perform(
        &self,
        request: &suzatary_core::web::WebRequest,
    ) -> Result<suzatary_core::capability::WebAnswer, suzatary_core::capability::WebFailure> {
        self.fetcher.perform(request)
    }
}

/// The owner's Drive as the secretary may read it.
///
/// `[secretary] drive_allow` is reread on every ask so a new folder counts at once; a
/// file that no longer reads allows nothing.
#[derive(Debug, Clone)]
pub struct DriveAccess {
    drive: suzatary_io::drive::ReadOnly,
    instance: std::path::PathBuf,
}

impl DriveAccess {
    /// Over this deployment's instance file and Drive CLI.
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self {
            drive: proton_drive(instance).read_only(),
            instance: instance.path.clone(),
        }
    }
}

/// Claude Code's transcripts on this box.
pub(crate) fn transcripts(instance: &Instance) -> suzatary_io::usage::Transcripts {
    suzatary_io::usage::Transcripts::new(
        instance.paths.transcripts.clone(),
        &instance.paths.home,
        instance.code(),
    )
}

/// The Drive CLI this instance names, downloading into the user's runtime
/// directory (memory, private to the user) when there is one.
#[must_use]
pub fn proton_drive(instance: &Instance) -> suzatary_io::drive::ProtonDrive {
    suzatary_io::drive::ProtonDrive::new(
        instance.paths.drive_cli.clone(),
        instance.paths.drive_scratch.clone(),
        instance.code().to_path_buf(),
    )
}

impl suzatary_core::capability::Drive for DriveAccess {
    fn allowed_folders(&self) -> Vec<suzatary_core::drive::DrivePath> {
        crate::instance::File::read(&self.instance)
            .map(|file| file.secretary.drive_allow)
            .unwrap_or_default()
    }

    fn port(&self) -> &dyn suzatary_core::drive::Port {
        &self.drive
    }

    fn read(
        &self,
        file: &suzatary_core::drive::DrivePath,
    ) -> Result<suzatary_core::file_read::FileRead, suzatary_core::capability::DriveFailure> {
        self.drive.read(file)
    }
}

/// The owner's Drive as the vault a private upload goes to.
pub struct DriveVault(suzatary_io::drive::ProtonDrive);

impl suzatary_core::vault::Vault for DriveVault {
    fn keep(
        &self,
        name: &str,
        bytes: &[u8],
    ) -> Result<suzatary_core::drive::DrivePath, suzatary_core::capability::DriveFailure> {
        self.0.keep(&suzatary_core::vault::folder(), name, bytes)
    }
}

/// This instance's Drive, as the vault.
#[must_use]
pub fn drive_vault(instance: &Instance) -> DriveVault {
    DriveVault(proton_drive(instance))
}

/// The Junk reader the server and the CLI secretary hold.
pub type Junk = suzatary_io::junk::OutlookJunk<suzatary_io::junk::SignedIn>;

/// The roster's Microsoft accounts' Junk, with `[mail.junk]`'s churn.
///
/// `SUZATARY_JUNK_SEED` names a seed the reader rereads per digest, and the token is
/// minted silently from the box's grant store or not at all.
#[must_use]
pub fn junk(instance: &Instance) -> Junk {
    suzatary_io::junk::OutlookJunk::new(
        &instance.mail.accounts,
        instance.mail.junk.clone(),
        std::env::var_os("SUZATARY_JUNK_SEED").map(std::path::PathBuf::from),
        suzatary_io::junk::SignedIn::new(crate::commands::mail::tokens(instance)),
    )
}

/// Every account's mail server, searched for mail older than the routine sync's window.
pub type Older = suzatary_io::older::Servers<
    suzatary_io::junk::SignedIn,
    suzatary_io::http::Ureq,
    suzatary_io::older::Notmuched,
>;

/// The mail servers an older search reaches: each Microsoft account's IMAP session signed in
/// silently, Gmail's API on lieer's grant or a Google account's own, its leaves under the mail root and its states beside
/// the sync's, indexed as a sync indexes.
#[must_use]
pub fn older(instance: &Instance) -> Older {
    suzatary_io::older::Servers::new(
        instance.mail.accounts.clone(),
        (
            instance.mail.root.clone(),
            instance.paths.imap_state.clone(),
        ),
        suzatary_io::junk::SignedIn::new(crate::commands::mail::tokens(instance)),
        suzatary_io::http::Ureq::new(),
        suzatary_io::older::Notmuched::new(
            crate::commands::mail::syncer(instance),
            crate::commands::mail::notmuch(instance),
        ),
        std::sync::Arc::new(crate::commands::mail::google_grants(instance)),
    )
}

/// The timer's own mail sync, whose report is the state directory's `last-sync.json`.
#[must_use]
pub fn mail_sync(instance: &Instance) -> suzatary_io::mail_sync::Timer {
    suzatary_io::mail_sync::Timer::new(instance.paths.last_sync.clone(), &instance.mail.heartbeat)
}

/// The owner's uploads, skipped by the conversations' listing.
#[must_use]
pub fn upload_store(instance: &Instance) -> suzatary_io::attachments::AttachmentStore {
    suzatary_io::attachments::AttachmentStore::new(
        instance.secretary.uploads(),
        suzatary_io::pdftotext::Pdftotext::new(instance.code()),
    )
}

/// The upload index: `upload-index/` inside the conversations directory, beside the uploads it
/// is read from, so it is box-local and git-ignored with them and never pushed.
#[must_use]
pub fn upload_index(instance: &Instance) -> suzatary_io::upload_index::UploadIndexFiles {
    suzatary_io::upload_index::UploadIndexFiles::new(
        instance.secretary.conversations.join("upload-index"),
        upload_store(instance),
    )
}

/// The files verbs hand the model.
#[must_use]
pub fn file_store(instance: &Instance) -> suzatary_io::blobs::Blobs {
    suzatary_io::blobs::Blobs::open(instance.secretary.kept())
}

/// The `claude` CLI at `[secretary] model`, each call joined into `answered`: the model the
/// conversation, the reviews and the titles run.
#[must_use]
pub fn brain(instance: &Instance, answered: suzatary_io::model_health::Answered) -> Brain {
    if !fence(instance).stands() {
        tracing::warn!(
            "the model's tool fence {} is not there — every call to the model's CLI is refused \
             until it is",
            instance.secretary.settings.display()
        );
    }
    let cli = cli_of(
        instance,
        instance.secretary.model.as_str(),
        &instance.secrets(),
    );
    Brain::Cli(Box::new(Recorded { cli, answered }))
}

/// The `claude` subprocess, each of its calls joined into the model health under its name.
pub struct Recorded {
    cli: suzatary_io::model::ClaudeCli,
    answered: suzatary_io::model_health::Answered,
}

impl Recorded {
    /// Join `result` unless it says nothing of whether the model answers: a halt is a person's,
    /// and a lost session is the CLI's.
    fn noted<T>(&self, result: Result<T, ModelError>) -> Result<T, ModelError> {
        match &result {
            Err(ModelError::Halted | ModelError::Lost(_)) => {}
            Ok(_) => answered_by(&self.answered, &self.cli.name(), None),
            Err(failed) => answered_by(&self.answered, &self.cli.name(), Some(failed)),
        }
        result
    }
}

impl Model for Recorded {
    fn name(&self) -> String {
        self.cli.name()
    }

    fn turn(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        self.noted(self.cli.turn(conversation, tools))
    }
}

impl SessionModel for Recorded {
    fn open(
        &self,
        id: &suzatary_core::capability::SessionId,
        conversation: &Conversation,
        tools: &serde_json::Value,
        halt: &suzatary_core::capability::Halt,
    ) -> Result<suzatary_core::capability::Sessioned, ModelError> {
        self.noted(self.cli.open(id, conversation, tools, halt))
    }

    fn resume(
        &self,
        session: &suzatary_core::capability::Session,
        delta: &suzatary_core::capability::Delta,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &suzatary_core::capability::Halt,
    ) -> Result<suzatary_core::capability::Sessioned, ModelError> {
        self.noted(self.cli.resume(session, delta, tools, on_delta, halt))
    }
}

/// The five-hour window's allowance as `allowance` reads it now.
fn five_hour(allowance: &suzatary_io::usage::Allowance) -> suzatary_core::usage::Allowance {
    allowance
        .allowance()
        .into_iter()
        .find_map(|(window, sum)| (window == suzatary_core::usage::Window::FiveHour).then_some(sum))
        .unwrap_or(suzatary_core::usage::Allowance::Unknown)
}

/// The compactor's model: `[secretary] compactor` through the subscription's `claude` CLI, on
/// its plain path (the prompt as the whole system prompt, the ask's text as it stands, no
/// tools), from an empty directory under an empty configuration, and only while the subscription
/// has room above the reserve it never takes.
pub struct CompactorModel {
    asked: suzatary_io::model::ClaudeCli,
    name: suzatary_core::model_name::ModelName,
    allowance: suzatary_io::usage::Allowance,
    reserve: Reserve,
}

impl CompactorModel {
    /// `name` over `instance`'s subscription, under `[secretary] reserve`.
    #[must_use]
    pub fn new(instance: &Instance, name: suzatary_core::model_name::ModelName) -> Self {
        let (empty, config) = (&instance.paths.compactor, &instance.paths.compactor_config);
        for directory in [empty, config] {
            if let Err(failed) = std::fs::create_dir_all(directory) {
                tracing::warn!(
                    "compactor: {} could not be made: {failed}",
                    directory.display()
                );
            }
        }
        let secrets = instance.secrets();
        Self {
            asked: suzatary_io::model::ClaudeCli::new(name.as_str(), fence(instance))
                .in_directory(empty.clone())
                .with_config_dir(config.clone())
                .with_token(suzatary_io::model::TokenSource::from_store(secrets)),
            name,
            allowance: suzatary_io::usage::Allowance::new(
                transcripts(instance),
                instance.secrets(),
                "the compactor: whether the subscription has room above the reserve",
                &instance.paths.usage_cache,
            ),
            reserve: instance.secretary.reserve,
        }
    }
}

impl CompactorModel {
    /// `text` under `system` on the plain path, only while the subscription has room above the
    /// reserve; the compactor and the topics backfill ask through it.
    ///
    /// # Errors
    /// [`ModelError::Unreachable`] at the reserve, or the CLI's own failure.
    pub fn ask(
        &self,
        system: &suzatary_core::for_model::ForModel,
        text: suzatary_core::for_model::ForModel,
    ) -> Result<String, ModelError> {
        if !subscription_has_room(&five_hour(&self.allowance), self.reserve) {
            return Err(ModelError::Unreachable(
                "the subscription is down to the reserve automated jobs never take".to_owned(),
            ));
        }
        self.asked.plain(system, text)
    }
}

impl Compact for CompactorModel {
    fn model(&self) -> &suzatary_core::model_name::ModelName {
        &self.name
    }

    fn compact(
        &self,
        ask: &suzatary_core::secretary::compact_ask::Ask,
    ) -> Result<String, ModelError> {
        self.ask(
            &suzatary_core::for_model::ForModel::of(suzatary_core::secretary::compact_ask::PROMPT),
            suzatary_core::for_model::ForModel::of(&ask.text()),
        )
    }
}

/// The box's mirrors, built one way for the server and the CLI alike.
#[must_use]
pub fn mirrors(instance: &Instance) -> std::sync::Arc<suzatary_io::repos::Mirrors> {
    std::sync::Arc::new(suzatary_io::repos::Mirrors::new(
        instance.paths.repos.clone(),
        suzatary_io::repos::Source::GitHub,
        instance.repos.clone(),
        Box::new(instance.secrets()),
    ))
}

#[cfg(test)]
mod subscription_tests {
    use chrono::Utc;

    use super::*;

    fn reported(utilization: f64) -> suzatary_core::usage::Allowance {
        suzatary_core::usage::Allowance::Reported(suzatary_core::usage::Reported {
            utilization,
            resets_at: None,
            as_of: Utc::now(),
        })
    }

    fn rate_limited(last: Option<f64>) -> suzatary_core::usage::Allowance {
        suzatary_core::usage::Allowance::RateLimited {
            since: Utc::now(),
            retry_after_seconds: 30,
            last_reported: last.map(|utilization| suzatary_core::usage::Reported {
                utilization,
                resets_at: None,
                as_of: Utc::now(),
            }),
        }
    }

    /// The compactor runs while no reading puts the subscription over the reserve's line: a rate
    /// limit with no remembered reading, one remembered under the line and an unknown allowance
    /// all leave it room.
    #[test]
    fn the_subscription_has_room_until_a_reading_crosses_the_reserve() {
        let reserve = Reserve::parse(0.30).expect("a share");
        for (allowance, room) in [
            (reported(0.69), true),
            (reported(0.71), false),
            (rate_limited(None), true),
            (rate_limited(Some(0.2)), true),
            (rate_limited(Some(0.9)), false),
            (suzatary_core::usage::Allowance::Unknown, true),
        ] {
            assert_eq!(
                subscription_has_room(&allowance, reserve),
                room,
                "{allowance:?}"
            );
        }
    }
}
