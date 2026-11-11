//! The one place that chooses an interpreter for every capability, so a fixture is a
//! substitution the routes cannot tell apart.

use std::net::IpAddr;
use std::sync::Arc;

use suzatary_core::capability::{Whois, WhoisError};
use suzatary_core::drive::WritePort as _;
use suzatary_core::secretary::compact::Compact;
use suzatary_core::secretary::model::{
    Conversation, Model, ModelError, Pick, Reply, SessionModel, Sessions,
};
use suzatary_core::weather::{Weather, WeatherError};
use suzatary_io::secrets::Secretspec;
use suzatary_io::whois::Tailscaled;

use crate::instance::{Instance, Reserve};

/// Who names a peer.
///
/// The fixture arm is compiled out of the shipped build; it exists because a headless browser
/// always arrives from loopback, which no daemon names.
pub enum Peers {
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
/// The scripted arm is compiled out of the shipped build. Verbs are dispatched on this side of
/// the trait, so any arm can only answer text.
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

    fn limited(&self) -> Option<suzatary_core::health::Limited> {
        match self {
            Brain::Cli(cli) => cli.limited(),
            #[cfg(feature = "fixture")]
            Brain::Scripted(scripted) => scripted.limited(),
        }
    }
}

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
    pub secrets: Secretspec,
    pub tokens: suzatary_io::oauth::TokenStore,
    /// The reason the store's audit records for each question.
    pub asking: &'static str,
}

impl suzatary_core::secrets::Secrets for Declared {
    // The value is discarded in the expression that reads it: only presence crosses.
    fn secret_state(
        &self,
        settable: &suzatary_core::settings::Settable,
    ) -> suzatary_core::secrets::SecretState {
        suzatary_io::secrets::state(&self.secrets.get(settable.name(), self.asking))
    }

    fn mail_grants(
        &self,
        account: &suzatary_core::mail::account::MailAccount,
    ) -> suzatary_core::secrets::MailGrants {
        use suzatary_core::mail::account::Sync;
        match account.sync {
            // A Google account's reading grant is its sign-in with Google, in the secret store.
            Sync::Google => suzatary_core::secrets::MailGrants {
                imap: matches!(
                    self.google_grant(account.email.address()),
                    suzatary_core::google::Held::Signed(_)
                ),
                smtp: false,
            },
            Sync::MsImap(_) | Sync::Gmail | Sync::Proton(_) => {
                let held = self
                    .tokens
                    .granted(account)
                    .inspect_err(|failed| tracing::warn!("grants of {}: {failed}", account.email))
                    .unwrap_or_default();
                suzatary_core::secrets::MailGrants {
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

/// One test call before the model's token is kept, with the value given, its result joined into
/// the model health.
pub enum Verifying {
    Real {
        instance: Arc<Instance>,
        answered: suzatary_io::model_health::Answered,
    },
    /// No call leaves: the fixture's model is scripted.
    #[cfg(feature = "fixture")]
    Fixture {
        instance: Arc<Instance>,
        answered: suzatary_io::model_health::Answered,
    },
}

fn probe(cli: &suzatary_io::model::ClaudeCli) -> Result<(), ModelError> {
    let asked = Conversation {
        system: "This is a test that the model answers. Answer with the one word: ok.".to_owned(),
        turns: vec![suzatary_core::secretary::model::Turn::Person {
            text: "ok?".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        }],
    };
    cli.turn(&asked, &serde_json::json!([])).map(|_| ())
}

/// Join one call of `model` into `answered`. Only a refused credential and a limit (under the
/// shared [`suzatary_core::health::SUBSCRIPTION`]) are joined as failures; a register that
/// cannot be written is only logged.
pub fn answered_by(
    answered: &suzatary_io::model_health::Answered,
    model: &str,
    failed: Option<&ModelError>,
) {
    use suzatary_core::capability::Clock as _;
    use suzatary_core::health::Outcome;
    let (entry, outcome) = match failed {
        None => (model, Outcome::Succeeded),
        Some(ModelError::Refused(_)) => (
            model,
            Outcome::Failed(suzatary_core::health::Failure::KeyRefused),
        ),
        Some(ModelError::Limited(limited)) => (
            suzatary_core::health::SUBSCRIPTION,
            Outcome::Failed(suzatary_core::health::Failure::Limited(*limited)),
        ),
        Some(
            ModelError::Unreachable(_)
            | ModelError::Failed(_)
            | ModelError::Overloaded(_)
            | ModelError::TimedOut(_)
            | ModelError::TooLong(_)
            | ModelError::Lost(_)
            | ModelError::Halted,
        ) => return,
    };
    let at = prodrome_core::fpl::instant_of(suzatary_io::clock::SystemClock.now());
    let words = failed.map(ToString::to_string).unwrap_or_default();
    if let Err(failed) = answered.record(entry, at, outcome, &words) {
        tracing::warn!("model health: {failed}");
    }
}

/// The limit standing now in `instance`'s model health, for callers outside a [`Door`].
#[must_use]
pub fn limited_now(instance: &Instance) -> Option<suzatary_core::health::Limited> {
    limited_in(&suzatary_io::model_health::beside(&instance.mail.heartbeat))
}

/// `None` also when the register does not read.
fn limited_in(
    answered: &suzatary_io::model_health::Answered,
) -> Option<suzatary_core::health::Limited> {
    use suzatary_core::capability::Clock as _;
    use suzatary_core::health::Register as _;
    let now = prodrome_core::fpl::instant_of(suzatary_io::clock::SystemClock.now());
    answered
        .healths()
        .inspect_err(|failed| tracing::warn!("model health: {failed}"))
        .ok()
        .and_then(|calls| suzatary_core::health::Limited::standing(&calls, now))
}

/// The one way the box asks a model it records: nothing while a limit stands in the model health
/// ([`suzatary_core::health::Limited::standing`]), and each call joined into it under `entry`.
#[derive(Clone)]
pub struct Door {
    answered: suzatary_io::model_health::Answered,
    entry: String,
}

impl Door {
    #[must_use]
    pub fn new(answered: suzatary_io::model_health::Answered, entry: String) -> Self {
        Self { answered, entry }
    }

    #[must_use]
    pub fn limited(&self) -> Option<suzatary_core::health::Limited> {
        limited_in(&self.answered)
    }

    /// `call`, unless a limit stands, its result joined into the register ([`answered_by`]).
    ///
    /// # Errors
    /// [`ModelError::Limited`] while a limit stands, with no call made; `call`'s own.
    pub fn ask<T>(&self, call: impl FnOnce() -> Result<T, ModelError>) -> Result<T, ModelError> {
        if let Some(limited) = self.limited() {
            return Err(ModelError::Limited(limited));
        }
        let result = call();
        answered_by(&self.answered, &self.entry, result.as_ref().err());
        result
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

/// Whether the subscription has room above the reserve. `RateLimited` and `Refused` are the
/// endpoint refusing, not the subscription full: either with no remembered reading is room.
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

/// The deny-all every call to the model's CLI is fenced by.
#[must_use]
pub fn fence(instance: &Instance) -> suzatary_io::model::Fence {
    suzatary_io::model::Fence::at(instance.secretary.settings.clone())
}

/// The `claude` CLI at `model`, fenced: the one spawner every caller shares.
#[must_use]
pub fn cli_of(
    instance: &Instance,
    model: &str,
    secrets: &suzatary_io::secrets::Secretspec,
) -> suzatary_io::model::ClaudeCli {
    spawner(instance, model)
        .with_token(suzatary_io::model::TokenSource::from_store(secrets.clone()))
        .with_uploads(upload_store(instance))
        .with_files(file_store(instance))
}

/// The CLI as the secretary runs it, before its token: where its sessions are.
#[must_use]
pub(crate) fn spawner(instance: &Instance, model: &str) -> suzatary_io::model::ClaudeCli {
    suzatary_io::model::ClaudeCli::new(model.to_owned(), fence(instance))
        .in_directory(instance.code().to_path_buf())
}

/// The web as the secretary may read it, by allow rules reread on every ask (none while the
/// instance file does not read).
#[derive(Debug, Clone)]
pub struct WebAccess {
    fetcher: suzatary_io::web::Fetcher,
    instance: std::path::PathBuf,
}

impl WebAccess {
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self {
            fetcher: suzatary_io::web::Fetcher::new(std::time::Duration::from_secs(10)),
            instance: instance.path.clone(),
        }
    }
}

impl suzatary_core::web::Web for WebAccess {
    fn allow_rules(&self) -> Vec<suzatary_core::web::AllowRule> {
        crate::instance::File::read(&self.instance)
            .map(|file| file.secretary.web_allow)
            .unwrap_or_default()
    }

    fn perform(
        &self,
        request: &suzatary_core::web::WebRequest,
    ) -> Result<suzatary_core::web::WebAnswer, suzatary_core::web::WebFailure> {
        self.fetcher.perform(request)
    }
}

/// The owner's Drive as the secretary may read it, by `[secretary] drive_allow` reread on every
/// ask (none while the instance file does not read).
#[derive(Debug, Clone)]
pub struct DriveAccess {
    drive: suzatary_io::drive::ReadOnly,
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
pub fn proton_drive(instance: &Instance) -> suzatary_io::drive::ProtonDrive {
    suzatary_io::drive::ProtonDrive::new(
        instance.paths.drive_cli.clone(),
        instance.paths.drive_scratch.clone(),
        instance.code().to_path_buf(),
    )
}

impl suzatary_core::drive::Drive for DriveAccess {
    fn allowed_folders(&self) -> Vec<suzatary_core::drive_path::DrivePath> {
        crate::instance::File::read(&self.instance)
            .map(|file| file.secretary.drive_allow)
            .unwrap_or_default()
    }

    fn port(&self) -> &dyn suzatary_core::drive::Port {
        &self.drive
    }

    fn read(
        &self,
        file: &suzatary_core::drive_path::DrivePath,
    ) -> Result<suzatary_core::file_read::FileRead, suzatary_core::drive_path::DriveFailure> {
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
    ) -> Result<suzatary_core::drive_path::DrivePath, suzatary_core::drive_path::DriveFailure> {
        self.0.keep(&suzatary_core::vault::folder(), name, bytes)
    }
}

#[must_use]
pub fn drive_vault(instance: &Instance) -> DriveVault {
    DriveVault(proton_drive(instance))
}

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

/// The mail servers an older search reaches: Microsoft IMAP signed in silently, Gmail's API on
/// lieer's grant or a Google account's own; leaves under the mail root, indexed as a sync does.
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
    suzatary_io::mail_sync::Timer::new(instance.paths.last_sync.clone())
}

/// The owner's uploads, skipped by the conversations' listing.
#[must_use]
pub fn upload_store(instance: &Instance) -> suzatary_io::attachments::AttachmentStore {
    suzatary_io::attachments::AttachmentStore::new(
        instance.secretary.uploads(),
        suzatary_io::pdftotext::Pdftotext::new(instance.code()),
    )
}

/// The upload index, `upload-index/` inside the conversations directory: box-local,
/// git-ignored with the uploads, never pushed.
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

/// The `claude` CLI at `[secretary] model`, each call joined into `answered`.
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
    let door = Door::new(answered, cli.name());
    Brain::Cli(Box::new(Recorded { cli, door }))
}

/// The `claude` subprocess behind its [`Door`].
pub struct Recorded {
    cli: suzatary_io::model::ClaudeCli,
    door: Door,
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
        self.door.ask(|| self.cli.turn(conversation, tools))
    }

    fn limited(&self) -> Option<suzatary_core::health::Limited> {
        self.door.limited()
    }
}

impl SessionModel for Recorded {
    fn open(
        &self,
        id: &suzatary_core::secretary::model::SessionId,
        conversation: &Conversation,
        tools: &serde_json::Value,
        halt: &suzatary_core::secretary::model::Halt,
    ) -> Result<suzatary_core::secretary::model::Sessioned, ModelError> {
        self.door
            .ask(|| self.cli.open(id, conversation, tools, halt))
    }

    fn resume(
        &self,
        system: &str,
        session: &suzatary_core::secretary::model::Session,
        delta: &suzatary_core::secretary::model::Delta,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &suzatary_core::secretary::model::Halt,
    ) -> Result<suzatary_core::secretary::model::Sessioned, ModelError> {
        self.door.ask(|| {
            self.cli
                .resume(system, session, delta, tools, on_delta, halt)
        })
    }
}

fn five_hour(allowance: &suzatary_io::usage::Allowance) -> suzatary_core::usage::Allowance {
    allowance
        .allowance()
        .into_iter()
        .find_map(|(window, sum)| (window == suzatary_core::usage::Window::FiveHour).then_some(sum))
        .unwrap_or(suzatary_core::usage::Allowance::Unknown)
}

/// One model of `[secretary] compactors` on the `claude` CLI's plain path (no tools),
/// from an empty directory and configuration, only while the subscription has room above the
/// reserve.
pub struct CompactorModel {
    asked: suzatary_io::model::ClaudeCli,
    door: Door,
    name: suzatary_core::model_name::ModelName,
    allowance: suzatary_io::usage::Allowance,
    reserve: Reserve,
}

impl CompactorModel {
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
        let asked = suzatary_io::model::ClaudeCli::new(name.as_str(), fence(instance))
            .in_directory(empty.clone())
            .with_config_dir(config.clone())
            .with_token(suzatary_io::model::TokenSource::from_store(secrets));
        Self {
            door: Door::new(
                suzatary_io::model_health::beside(&instance.mail.heartbeat),
                asked.name(),
            ),
            asked,
            name,
            allowance: suzatary_io::usage::Allowance::new(
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
    ///
    /// # Errors
    /// [`ModelError::Unreachable`] at the reserve, [`ModelError::Limited`] while a limit stands,
    /// or the CLI's own failure.
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
        self.door.ask(|| self.asked.plain(system, text))
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
