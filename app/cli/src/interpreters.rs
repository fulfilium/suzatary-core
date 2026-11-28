//! The one place that chooses an interpreter for every capability, so a fixture is a
//! substitution the routes cannot tell apart.

use crate::instance::File;
use std::net::IpAddr;
use std::sync::Arc;
use suzatary_base::capability::Peer;
use suzatary_base::drive_path::DriveFailure;
use suzatary_base::drive_path::DrivePath;
use suzatary_conversations::secretary::model::Halt;
use suzatary_digest::model_name::ModelName;
use suzatary_health::health::Failure;
use suzatary_health::health::Limited;
use suzatary_held::for_model::ForModel;
use suzatary_io_access::http::Ureq;
use suzatary_io_local::attachments::AttachmentStore;
use suzatary_io_local::blobs::Blobs;
use suzatary_io_local::clock::SystemClock;
use suzatary_io_local::model_health::Answered;
use suzatary_io_local::upload_index::UploadIndexFiles;
use suzatary_io_mail::junk::OutlookJunk;
use suzatary_io_mail::junk::SignedIn;
use suzatary_io_mail::mail_sync::Timer;
use suzatary_io_mail::older::Notmuched;
use suzatary_io_mail::older::Servers;
use suzatary_io_model::model::Fence;
use suzatary_io_model::model::TokenSource;
use suzatary_io_model::model::{Cache, ClaudeCli};
use suzatary_io_sources::drive::ProtonDrive;
use suzatary_io_sources::repos::Mirrors;
use suzatary_io_sources::web::Fetcher;

use suzatary_base::capability::{Whois, WhoisError};
use suzatary_conversations::secretary::model::{
    Conversation, Model, ModelError, Pick, Picker, Plain, Reply,
};
use suzatary_digest::secretary::compact::Compact;
use suzatary_digest::secretary::compact_ask::Sharing;
use suzatary_io_access::secrets::Secretspec;
use suzatary_io_local::whois::Tailscaled;
use suzatary_services::drive::WritePort as _;
use suzatary_services::weather::{Weather, WeatherError};

use crate::instance::Instance;
use suzatary_connect::secrets::MailGrants;
use suzatary_connect::settings::Settable;
use suzatary_digest::secretary::reserve::Reserve;
use suzatary_google::google::Held;
use suzatary_services::usage::Allowance;

/// Who names a peer; the fixture arm, compiled out of the shipped build, exists because a
/// headless browser always arrives from loopback, which no daemon names.
pub enum Peers {
    Daemon(Tailscaled),
    /// Every address is this login; test only.
    #[cfg(feature = "fixture")]
    Fixture(suzatary_base::capability::Login),
}

impl Whois for Peers {
    fn peer(&self, address: IpAddr) -> Result<Peer, WhoisError> {
        match self {
            Peers::Daemon(daemon) => daemon.peer(address),
            // Every peer is the fixture's writer, so none is this box.
            #[cfg(feature = "fixture")]
            Peers::Fixture(login) => Ok(Peer::Known(login.clone())),
        }
    }
}

/// Which model answers a conversation; verbs are dispatched on this side of the trait, so any
/// arm can only answer text.
pub enum Brain {
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

    fn streamed(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Reply, ModelError> {
        match self {
            Brain::Cli(cli) => cli.streamed(conversation, tools, on_delta, halt),
            #[cfg(feature = "fixture")]
            Brain::Scripted(scripted) => scripted.streamed(conversation, tools, on_delta, halt),
        }
    }

    fn limited(&self) -> Option<Limited> {
        match self {
            Brain::Cli(cli) => cli.limited(),
            #[cfg(feature = "fixture")]
            Brain::Scripted(scripted) => scripted.limited(),
        }
    }
}

impl Picker for Brain {
    fn pick(&self) -> Pick<'_> {
        Box::new(self)
    }
}

/// The day's forecast for `[persona]`'s city.
pub struct Forecast(pub suzatary_base::persona::Persona);

impl Weather for Forecast {
    fn today(&self) -> Result<String, WeatherError> {
        // Wall-clock UTC, because `Clock::now()` is naive and cannot feed a zone conversion.
        suzatary_io_sources::weather::line(chrono::Utc::now(), &self.0)
    }
}

/// Whether each declared secret resolves and which mail grants are held, never a value.
pub struct Declared {
    pub secrets: Secretspec,
    pub tokens: suzatary_io_mail::oauth::TokenStore,
    /// The reason the store's audit records for each question.
    pub asking: &'static str,
}

impl suzatary_connect::secrets::Secrets for Declared {
    // The value is discarded in the expression that reads it: only presence crosses.
    fn secret_state(&self, settable: &Settable) -> suzatary_connect::secrets::SecretState {
        suzatary_io_access::secrets::state(&self.secrets.get(settable.name(), self.asking))
    }

    fn snapshot(&self, names: &[Settable]) -> suzatary_connect::secrets::Snapshot {
        self.secrets.snapshot(names, self.asking)
    }

    fn mail_grants(&self, account: &suzatary_mail::mail::account::MailAccount) -> MailGrants {
        use suzatary_mail::mail::account::Sync;
        match account.sync {
            // A Google account's reading grant is its sign-in with Google, in the secret store.
            Sync::Google => MailGrants {
                imap: matches!(self.google_grant(account.email.address()), Held::Signed(_)),
                smtp: false,
                calendar: false,
            },
            Sync::MsImap(_) | Sync::Gmail | Sync::Proton(_) => {
                let held = self
                    .tokens
                    .granted(account)
                    .inspect_err(|failed| tracing::warn!("grants of {}: {failed}", account.email))
                    .unwrap_or_default();
                MailGrants {
                    imap: held.contains(&"imap"),
                    smtp: held.contains(&"smtp"),
                    calendar: held.contains(&"calendar"),
                }
            }
        }
    }

    // The grant is read and parsed here, and only its parts leave.
    fn google_grant(&self, account: &suzatary_mail::mail::address::EmailAddress) -> Held {
        match self
            .secrets
            .get(&suzatary_google::google::grant_name(account), self.asking)
        {
            Ok(Some(kept)) => suzatary_google::google::KeptGrant::parse(kept.expose())
                .map_or(Held::NotSignedIn, |kept| Held::Signed(kept.parts)),
            Ok(None) | Err(_) => Held::NotSignedIn,
        }
    }
}

/// One test call before the model's token is kept, with the value given, its result joined into
/// the model health.
pub enum Verifying {
    Real {
        instance: Arc<Instance>,
        answered: Answered,
    },
    /// No call leaves: the fixture's model is scripted.
    #[cfg(feature = "fixture")]
    Fixture {
        instance: Arc<Instance>,
        answered: Answered,
    },
}

fn probe(cli: &ClaudeCli) -> Result<(), ModelError> {
    let asked = Conversation {
        system: "This is a test that the model answers. Answer with the one word: ok.".to_owned(),
        context: suzatary_conversations::secretary::model::Context::default(),
        earlier: Vec::new(),
        turns: vec![suzatary_conversations::secretary::model::Turn::Person {
            text: "ok?".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        }],
    };
    cli.turn(&asked, &serde_json::json!([])).map(|_| ())
}

/// Join one call of `model` into `answered`. Only a refused credential and a limit (under the
/// shared [`suzatary_health::health::SUBSCRIPTION`]) are joined as failures; a register that
/// cannot be written is only logged.
pub fn answered_by(answered: &Answered, model: &str, failed: Option<&ModelError>) {
    use suzatary_base::capability::Clock as _;
    use suzatary_health::health::Outcome;
    let (entry, outcome) = match failed {
        None => (model, Outcome::Succeeded),
        Some(ModelError::Refused(_)) => (model, Outcome::Failed(Failure::KeyRefused)),
        Some(ModelError::Limited(limited)) => (
            suzatary_health::health::SUBSCRIPTION,
            Outcome::Failed(Failure::Limited(*limited)),
        ),
        Some(
            ModelError::Unreachable(_)
            | ModelError::Failed(_)
            | ModelError::Overloaded(_)
            | ModelError::TimedOut(_)
            | ModelError::TooLong(_)
            | ModelError::Halted,
        ) => return,
    };
    let at = prodrome_data::instant::instant_of(SystemClock.now());
    let words = failed.map(ToString::to_string).unwrap_or_default();
    if let Err(failed) = answered.record(entry, at, outcome, &words) {
        tracing::warn!("model health: {failed}");
    }
}

/// The limit standing now in `instance`'s model health, for callers outside a [`Door`].
#[must_use]
pub fn limited_now(instance: &Instance) -> Option<Limited> {
    limited_in(&suzatary_io_local::model_health::beside(
        &instance.mail.heartbeat,
    ))
}

/// `None` also when the register does not read.
fn limited_in(answered: &Answered) -> Option<Limited> {
    use suzatary_base::capability::Clock as _;
    use suzatary_health::health::Register as _;
    let now = prodrome_data::instant::instant_of(SystemClock.now());
    answered
        .healths()
        .inspect_err(|failed| tracing::warn!("model health: {failed}"))
        .ok()
        .and_then(|calls| Limited::standing(&calls, now))
}

/// The one way the box asks a model it records: nothing while a limit stands in the model health
/// ([`Limited::standing`]), and each call joined into it under `entry`.
#[derive(Clone)]
pub struct Door {
    answered: Answered,
    entry: String,
}

impl Door {
    #[must_use]
    pub fn new(answered: Answered, entry: String) -> Self {
        Self { answered, entry }
    }

    #[must_use]
    pub fn limited(&self) -> Option<Limited> {
        limited_in(&self.answered)
    }

    /// `call`, unless a limit stands, its result joined into the register ([`answered_by`]).
    pub fn ask<T>(&self, call: impl FnOnce() -> Result<T, ModelError>) -> Result<T, ModelError> {
        if let Some(limited) = self.limited() {
            return Err(ModelError::Limited(limited));
        }
        let result = call();
        answered_by(&self.answered, &self.entry, result.as_ref().err());
        result
    }
}

impl suzatary_connect::settings::Verify for Verifying {
    fn verify(
        &self,
        name: &Settable,
        value: &suzatary_base::capability::Secret,
    ) -> Result<(), String> {
        if name.name() != suzatary_io_model::model::TOKEN {
            return Ok(());
        }
        match self {
            Verifying::Real { instance, answered } => {
                let cli = cli_of(
                    instance,
                    instance.secretary.model.as_str(),
                    &instance.secrets(),
                )
                .with_token(TokenSource::fixed(value.clone()));
                let called = probe(&cli);
                answered_by(answered, &cli.name(), called.as_ref().err());
                called.map_err(|failed| format!("{}: {failed}", cli.name()))
            }
            #[cfg(feature = "fixture")]
            Verifying::Fixture { instance, answered } => {
                answered_by(
                    answered,
                    &suzatary_io_model::model::name_of(instance.secretary.model.as_str()),
                    None,
                );
                Ok(())
            }
        }
    }
}

/// Whether the subscription has room above the reserve. `RateLimited` and `Refused` are the
/// endpoint refusing, not the subscription full: either with no remembered reading is room.
fn subscription_has_room(allowance: &Allowance, reserve: Reserve) -> bool {
    let reserve = reserve.get();
    match allowance {
        Allowance::Reported(reported)
        | Allowance::RateLimited {
            last_reported: Some(reported),
            ..
        } => reported.utilization <= 1.0 - reserve,
        Allowance::RateLimited {
            last_reported: None,
            ..
        }
        | Allowance::Refused { .. }
        | Allowance::Unknown => true,
    }
}

/// The deny-all every call to the model's CLI is fenced by.
#[must_use]
pub fn fence(instance: &Instance) -> Fence {
    Fence::at(instance.secretary.settings.clone())
}

/// The `claude` CLI at `model`, fenced: the one spawner every caller shares.
#[must_use]
pub fn cli_of(
    instance: &Instance,
    model: &str,
    secrets: &suzatary_io_access::secrets::Secretspec,
) -> ClaudeCli {
    spawner(instance, model)
        .with_token(TokenSource::from_store(secrets.clone()))
        .with_uploads(upload_store(instance))
        .with_files(file_store(instance))
}

/// The CLI as the secretary runs it, before its token: where its sessions are.
#[must_use]
pub(crate) fn spawner(instance: &Instance, model: &str) -> ClaudeCli {
    ClaudeCli::new(model.to_owned(), fence(instance)).in_directory(instance.code().to_path_buf())
}

/// The web as the secretary may read it, by allow rules reread on every ask (none while the
/// instance file does not read).
#[derive(Debug, Clone)]
pub struct WebAccess {
    fetcher: Fetcher,
    instance: std::path::PathBuf,
}

impl WebAccess {
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self {
            fetcher: Fetcher::new(std::time::Duration::from_secs(10)),
            instance: instance.path.clone(),
        }
    }
}

impl suzatary_services::web::Web for WebAccess {
    fn allow_rules(&self) -> Vec<suzatary_services::web::AllowRule> {
        File::read(&self.instance)
            .map(|file| file.secretary.web_allow)
            .unwrap_or_default()
    }

    fn perform(
        &self,
        request: &suzatary_services::web::WebRequest,
    ) -> Result<suzatary_services::web::WebAnswer, suzatary_services::web::WebFailure> {
        self.fetcher.perform(request)
    }
}

/// The owner's Drive as the secretary may read it, by `[secretary] drive_allow` reread on every
/// ask (none while the instance file does not read).
#[derive(Debug, Clone)]
pub struct DriveAccess {
    drive: suzatary_io_sources::drive::ReadOnly,
    instance: std::path::PathBuf,
}

impl DriveAccess {
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self {
            drive: proton_drive(instance).read_only(),
            instance: instance.path.clone(),
        }
    }
}

/// The Drive CLI, downloading into the user's runtime directory (memory, private to the
/// user) when there is one.
#[must_use]
pub fn proton_drive(instance: &Instance) -> ProtonDrive {
    ProtonDrive::new(
        instance.paths.drive_cli.clone(),
        instance.paths.drive_scratch.clone(),
        instance.code().to_path_buf(),
    )
}

impl suzatary_services::drive::Drive for DriveAccess {
    fn allowed_folders(&self) -> Vec<DrivePath> {
        File::read(&self.instance)
            .map(|file| file.secretary.drive_allow)
            .unwrap_or_default()
    }

    fn port(&self) -> &dyn suzatary_services::drive::Port {
        &self.drive
    }

    fn read(&self, file: &DrivePath) -> Result<suzatary_held::file_read::FileRead, DriveFailure> {
        self.drive.read(file)
    }
}

/// The owner's Drive as the vault a private upload goes to.
pub struct DriveVault(ProtonDrive);

impl suzatary_held::vault::Vault for DriveVault {
    fn keep(&self, name: &str, bytes: &[u8]) -> Result<DrivePath, DriveFailure> {
        self.0.keep(&suzatary_held::vault::folder(), name, bytes)
    }
}

#[must_use]
pub fn drive_vault(instance: &Instance) -> DriveVault {
    DriveVault(proton_drive(instance))
}

pub type Junk = OutlookJunk<SignedIn>;

/// The roster's Microsoft accounts' Junk, with `[mail.junk]`'s churn; `SUZATARY_JUNK_SEED` names a
/// seed reread per digest, and the token is minted silently from the grant store or not at all.
#[must_use]
pub fn junk(instance: &Instance) -> Junk {
    OutlookJunk::new(
        &instance.mail.accounts,
        instance.mail.junk.clone(),
        std::env::var_os("SUZATARY_JUNK_SEED").map(std::path::PathBuf::from),
        SignedIn::new(crate::commands::mail::tokens(instance)),
    )
}

/// Every account's mail server, searched for mail older than the routine sync's window.
pub type Older = Servers<SignedIn, Ureq, Notmuched>;

/// The mail servers an older search reaches: Microsoft IMAP signed in silently, Gmail's API on
/// lieer's grant or a Google account's own; leaves under the mail root, indexed as a sync does.
#[must_use]
pub fn older(instance: &Instance) -> Older {
    Servers::new(
        instance.mail.accounts.clone(),
        (
            instance.mail.root.clone(),
            instance.paths.imap_state.clone(),
        ),
        SignedIn::new(crate::commands::mail::tokens(instance)),
        Ureq::new(),
        Notmuched::new(
            crate::commands::mail::syncer(instance),
            crate::commands::mail::notmuch(instance),
        ),
        std::sync::Arc::new(crate::commands::mail::google_grants(instance)),
    )
}

/// The timer's own mail sync, whose report is the state directory's `last-sync.json`.
#[must_use]
pub fn mail_sync(instance: &Instance) -> Timer {
    Timer::new(instance.paths.last_sync.clone())
}

/// The owner's uploads, skipped by the conversations' listing.
#[must_use]
pub fn upload_store(instance: &Instance) -> AttachmentStore {
    AttachmentStore::new(
        instance.secretary.uploads(),
        suzatary_io_local::pdftotext::Pdftotext::new(instance.code()),
    )
}

/// The upload index, `upload-index/` inside the conversations directory: box-local,
/// git-ignored with the uploads, never pushed.
#[must_use]
pub fn upload_index(instance: &Instance) -> UploadIndexFiles {
    UploadIndexFiles::new(
        instance.secretary.conversations.join("upload-index"),
        upload_store(instance),
    )
}

/// The files verbs hand the model.
#[must_use]
pub fn file_store(instance: &Instance) -> Blobs {
    Blobs::open(instance.secretary.kept())
}

/// The `claude` CLI at `[secretary] model`, each call joined into `answered`.
#[must_use]
pub fn brain(instance: &Instance, answered: Answered) -> Brain {
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
    let door = Door::new(answered, cli.name());
    Brain::Cli(Box::new(Recorded { cli, door }))
}

/// The `claude` subprocess behind its [`Door`].
pub struct Recorded {
    cli: ClaudeCli,
    door: Door,
}

impl Model for Recorded {
    fn name(&self) -> String {
        self.cli.name()
    }

    fn streamed(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Reply, ModelError> {
        self.door
            .ask(|| self.cli.streamed(conversation, tools, on_delta, halt))
    }

    fn limited(&self) -> Option<Limited> {
        self.door.limited()
    }
}

fn five_hour(allowance: &suzatary_io_model::usage::Allowance) -> Allowance {
    allowance
        .allowance()
        .into_iter()
        .find_map(|(window, sum)| {
            (window == suzatary_services::usage::Window::FiveHour).then_some(sum)
        })
        .unwrap_or(Allowance::Unknown)
}

/// One model of `[secretary] compactors` on the `claude` CLI's plain path (no tools),
/// from an empty directory and configuration, only while the subscription has room above the
/// reserve.
pub struct CompactorModel {
    asked: ClaudeCli,
    door: Door,
    name: ModelName,
    allowance: suzatary_io_model::usage::Allowance,
    reserve: Reserve,
}

impl CompactorModel {
    #[must_use]
    pub fn new(instance: &Instance, name: ModelName) -> Self {
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
        let asked = ClaudeCli::new(name.as_str(), fence(instance))
            .in_directory(empty.clone())
            .with_config_dir(config.clone())
            .with_token(TokenSource::from_store(secrets));
        Self {
            door: Door::new(
                suzatary_io_local::model_health::beside(&instance.mail.heartbeat),
                asked.name(),
            ),
            asked,
            name,
            allowance: suzatary_io_model::usage::Allowance::new(
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
    /// reserve.
    pub fn ask(&self, system: &ForModel, text: ForModel) -> Result<String, ModelError> {
        self.plain(system, text).map(|plain| plain.text)
    }

    /// [`CompactorModel::ask`], with what the call cost.
    pub fn plain(&self, system: &ForModel, text: ForModel) -> Result<Plain, ModelError> {
        self.within_room(|| self.asked.plain(system, text))
    }

    fn within_room<T>(
        &self,
        call: impl FnOnce() -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        if !subscription_has_room(&five_hour(&self.allowance), self.reserve) {
            return Err(ModelError::Unreachable(
                "the subscription is down to the reserve automated jobs never take".to_owned(),
            ));
        }
        self.door.ask(call)
    }
}

impl Compact for CompactorModel {
    fn model(&self) -> &ModelName {
        &self.name
    }

    fn compact(
        &self,
        ask: &suzatary_digest::secretary::compact_ask::Ask,
    ) -> Result<Plain, ModelError> {
        let cache = match ask.sharing() {
            Sharing::Shared => Cache::Keep,
            Sharing::Alone => Cache::Off,
        };
        let (system, opening, own) = (
            ForModel::of(ask.prompt()),
            ForModel::of(&ask.opening()),
            ForModel::of(&ask.own()),
        );
        self.within_room(|| self.asked.plain_after(&system, &opening, &own, cache))
    }
}

#[must_use]
pub fn mirrors(instance: &Instance) -> std::sync::Arc<Mirrors> {
    std::sync::Arc::new(Mirrors::new(
        instance.paths.repos.clone(),
        suzatary_io_sources::repos::Source::GitHub,
        instance.repos.clone(),
        Box::new(instance.secrets()),
    ))
}

#[cfg(test)]
mod subscription_tests {
    use chrono::Utc;

    use super::*;

    fn reported(utilization: f64) -> suzatary_services::usage::Allowance {
        suzatary_services::usage::Allowance::Reported(suzatary_services::usage::Reported {
            utilization,
            resets_at: None,
            as_of: Utc::now(),
        })
    }

    fn rate_limited(last: Option<f64>) -> suzatary_services::usage::Allowance {
        suzatary_services::usage::Allowance::RateLimited {
            since: Utc::now(),
            retry_after_seconds: 30,
            last_reported: last.map(|utilization| suzatary_services::usage::Reported {
                utilization,
                resets_at: None,
                as_of: Utc::now(),
            }),
        }
    }

    #[test]
    fn the_subscription_has_room_until_a_reading_crosses_the_reserve() {
        let reserve = Reserve::parse(0.30).expect("a share");
        for (allowance, room) in [
            (reported(0.69), true),
            (reported(0.71), false),
            (rate_limited(None), true),
            (rate_limited(Some(0.2)), true),
            (rate_limited(Some(0.9)), false),
            (suzatary_services::usage::Allowance::Unknown, true),
        ] {
            assert_eq!(
                subscription_has_room(&allowance, reserve),
                room,
                "{allowance:?}"
            );
        }
    }
}
