//! `suzatary serve`; `table` lists every route. It sends no CORS header, `gate` refuses any peer
//! whois cannot name, and the only HTML it serves is `dist`'s files and typst's rendering.
use crate::gate::Door;
use crate::portal::Portal;
use crate::secretary::routes::{Cards, Perform, Post};
use prodrome_core::memo::{Cache, Lru};
use prodrome_core::store::frontier::Frontiers;
use suzatary_api::clienterror::Limiter;
use suzatary_api::routes::Books;
use suzatary_base::capability;
use suzatary_cards::secretary::noticed;
use suzatary_cards::settings::overview;
use suzatary_health::health;
use suzatary_health::health::{Reading, Register};
use suzatary_held::held_value::Reveal;
use suzatary_held::vault::Vault;
use suzatary_io::attachments::AttachmentStore;
use suzatary_io::objects::Objects;
use suzatary_io::requests::Requests;
use suzatary_lines::parts::{Parts, part, scoped};
use suzatary_lines::secretary::transcript::Retry;
use suzatary_secretary::secretary::Bound;
use suzatary_services::plugin::Plugins;
use suzatary_signatures::chain_set;
use suzatary_signatures::epoch::Epochs;
use suzatary_signatures::signatures::{self, BoxKeys, Pairings};
use suzatary_stores::asks::AskEvent;
pub mod asked;
mod body;
mod call;
mod chain;
pub mod checks;
pub mod compaction;
mod conversation_worker;
mod fresh;
pub mod google;
mod guard;
mod held;
pub mod listening;
pub mod live;
pub(crate) mod locks;
mod mail;
pub mod models;
mod noting;
mod params;
mod patches;
mod proposals;
pub(crate) mod reply;
pub mod route;
mod secretary;
mod settings;
pub mod sign_ins;
mod stores;
mod stream;
pub mod table;
mod the_box;
mod token_expiry;
mod validated;
mod versioned;
mod watch;
mod watched;
use crate::server::Server;
use anyhow::Context;
use axum::Router;
use axum::extract::connect_info::IntoMakeServiceWithConnectInfo;
use axum::http::{HeaderValue, header};
pub use conversation_worker::Workers;
pub use live::Live;
use prodrome_core::fpl::instant_of;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use suzatary_base::refusal::{Refusal, shown};
use suzatary_connect::connect::Reach;
use suzatary_connect::deployment::Deployment;
use suzatary_connect::secrets::Remembering;
use suzatary_connect::settings::{Declares, Edits, Settables, Verify, Writes};
use suzatary_conversations::secretary::model;
use suzatary_digest::secretary::memories::Texts;
use suzatary_google::google::Endpoints;
use suzatary_inbox::secretary::sign_in;
use suzatary_io::google::Google;
use suzatary_lines::secretary::transcript;
use suzatary_release::release::{Plan, Released};
use tower_http::compression::{CompressionLayer, CompressionLevel};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
/// The owner's uploads; only his confirm reaches the vault.
pub struct Uploads {
    pub store: AttachmentStore,
    pub vault: Arc<dyn Vault>,
}

#[derive(Clone)]
pub struct State(Arc<Inner>);

impl std::ops::Deref for State {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

/// The settings page's stores, and the build it names.
pub struct Settings {
    pub live: live::Shared,
    pub models: Arc<models::Models>,
    pub secrets: Arc<dyn Writes + Send + Sync>,
    pub declares: Arc<dyn Declares + Send + Sync>,
    pub edits: Arc<dyn Edits + Send + Sync>,
    /// Deletes a removed account's data.
    pub forget: Arc<dyn suzatary_mail::mail::cache::Forget + Send + Sync>,
    pub stamp: Stamp,
    pub data: Arc<std::path::PathBuf>,
}

/// What a tap reaches, outside every record.
pub struct Performers {
    pub postman: Arc<dyn Post + Send + Sync>,
    pub acting: Arc<dyn Perform + Send + Sync>,
}

/// The signed requests kept, the box's refusals of signed objects, and the pairing codes.
pub struct Signed {
    pub requests: Arc<Requests>,
    pub asks: Arc<Objects<AskEvent>>,
    pub pairings: Arc<dyn Pairings>,
}

/// Everything a handler reads.
#[derive(Clone)]
pub struct Inner {
    /// Serves every bare `/api/…` path.
    pub first: Chained,
    pub chains: Arc<BTreeMap<String, Chained>>,
    pub deployment: Arc<Deployment>,
    pub door: Arc<Door>,
    /// Reached only with a `Tap`.
    pub postman: Arc<dyn Post + Send + Sync>,
    /// Reached only with a `Tap`.
    pub acting: Arc<dyn Perform + Send + Sync>,
    pub known: Arc<Mutex<crate::gate::Remembering>>,
    pub workers: Arc<Workers>,
    /// Only `State::reconnected` writes it.
    pub live: live::Shared,
    pub models: Arc<models::Models>,
    /// Reached only by a tapped card.
    pub secrets: Arc<dyn Writes + Send + Sync>,
    /// Refuses undeclared names.
    pub declares: Arc<dyn Declares + Send + Sync>,
    /// Edits the instance file, by a tapped card.
    pub edits: Arc<dyn Edits + Send + Sync>,
    /// Deletes a removed account's data.
    pub forget: Arc<dyn suzatary_mail::mail::cache::Forget + Send + Sync>,
    /// Reads a held value back for a tap; outside every record, so no agent verb can.
    pub reveal: Arc<dyn Reveal + Send + Sync>,
    /// Syncs a plugin after a tap.
    pub checks: checks::Checks,
    pub reach: Arc<Reach>,
    /// `None` refuses an upload by name.
    pub uploads: Option<Arc<Uploads>>,
    /// `None` names a topic with the record's model.
    pub titler: Option<Arc<dyn model::Sessions + Send + Sync>>,
    /// `None` while the compactor is off.
    pub compactor: Option<Arc<compaction::Compactor>>,
    pub presence: Presence,
    pub reports: Arc<Mutex<Limiter>>,
    pub stamp: Stamp,
    /// `None` declares no plugin.
    pub plugins: Option<Arc<Plugins>>,
    pub repos: Arc<suzatary_services::repos::Declared>,
    pub data: Arc<std::path::PathBuf>,
    /// Only a chain whose heads moved is reread.
    pub read: Arc<Mutex<chain_set::Read>>,
    /// Open connection cards, memoised.
    pub asking: Arc<Mutex<Cache<Vec<overview::Asking>, Lru>>>,
    pub built: Arc<Mutex<Cache<overview::Build, Lru>>>,
    /// Reached only by a tapped device card.
    pub keys: Option<Arc<BoxKeys>>,
    pub requests: Arc<Requests>,
    /// Written only by the acceptance of a tab's objects.
    pub asks: Arc<Objects<AskEvent>>,
    pub pairings: Arc<dyn Pairings>,
    /// So a request sent twice at once runs once.
    pub performing: asked::InFlight,
    /// Now, in UTC, since devices stamp UTC.
    pub wall: fn() -> chrono::DateTime<chrono::Utc>,
    pub answered: Arc<dyn Register<String> + Send + Sync>,
    /// Reached only with a `Tap`.
    pub signing: Arc<dyn sign_in::Sessions + Send + Sync>,
    pub verify: Arc<dyn Verify + Send + Sync>,
    /// Begun only with a `Tap`, finished only by its callback.
    pub consenting: Arc<google::Consenting>,
    /// Held across a filing, so none files a card twice.
    pub noticing: Arc<Mutex<Option<Heard>>>,
    /// Secret store answers since the last filing.
    pub secrets_answered: Arc<Mutex<suzatary_connect::secrets::SecretAnswers>>,
    pub epochs: Arc<Epochs>,
    pub frontiers: Arc<Frontiers>,
    pub portal: Arc<Portal>,
    /// Finds the plan store per request.
    pub projects: Projects,
}

/// A store that holds no secret.
pub struct Unresolved;

impl suzatary_io::secrets::Resolves for Unresolved {
    fn get(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<capability::Secret>, suzatary_io::secrets::SecretError> {
        Ok(None)
    }
}

/// No sign-in is held open.
pub(crate) struct Closed;

impl sign_in::Sessions for Closed {
    fn open(&self) -> Vec<sign_in::SignInUrl> {
        Vec::new()
    }

    fn finish(
        &self,
        _: &sign_in::SignInUrl,
        _: &capability::Secret,
    ) -> Option<(
        suzatary_inbox::secretary::inbox::ProposalId,
        Result<capability::Secret, String>,
    )> {
        None
    }
}

/// Keeps no model credential.
pub(crate) struct Unverified;

impl Verify for Unverified {
    fn verify(
        &self,
        name: &suzatary_connect::settings::Settable,
        _: &capability::Secret,
    ) -> Result<(), String> {
        Err(format!(
            "this server tests no credential, so {} is kept by no tap",
            name.name()
        ))
    }
}

/// The build `GET /api/about` names: the release this process runs as, and its build.
#[derive(Debug, Clone)]
pub struct Stamp {
    /// `None` for a build with no release file beside it; `Err` says why that file did not read.
    pub release: Result<Option<&'static Released>, String>,
    pub build: Build,
}

/// This process's build.
#[derive(Debug, Clone)]
pub enum Build {
    Compiled(&'static str),
    /// Read per request, the compiled build where it does not read; test only.
    #[cfg(feature = "fixture")]
    File {
        path: Arc<PathBuf>,
        compiled: &'static str,
    },
}

impl Stamp {
    #[must_use]
    pub(crate) fn build(&self) -> String {
        match &self.build {
            Build::Compiled(build) => (*build).to_owned(),
            #[cfg(feature = "fixture")]
            Build::File { path, compiled } => std::fs::read_to_string(path.as_ref())
                .map_or_else(|_| (*compiled).to_owned(), |t| t.trim().to_owned()),
        }
    }

    /// What `GET /api/about` answers.
    pub(crate) fn about(
        &self,
        projects: &Projects,
        set: &chain_set::ChainSet<'_>,
    ) -> Result<suzatary_cards::wire::cards::AboutPayload, Refusal> {
        let internal = |e: &dyn std::fmt::Display| Refusal::internal(e.to_string());
        let released = self.release.as_ref().map_err(|e| internal(e))?;
        let project = projects().map_err(|e| internal(&e))?;
        suzatary_api::routes::about(
            *released,
            &self.build(),
            project.as_ref().map(|project| &project.chain),
            set,
        )
    }
}

/// The plan store's chain and the plan it holds.
#[derive(Debug, Clone)]
pub struct Project {
    pub chain: suzatary_base::chains::ChainName,
    pub plan: Plan,
}

/// Reads the plan store again at each call, so a plan written while the box runs is found;
/// `None` where no mount holds a plan.
pub type Projects = Arc<dyn Fn() -> anyhow::Result<Option<Project>> + Send + Sync>;

/// Whether a writer's page pinged within `PRESENT_SECONDS`.
#[derive(Debug, Clone, Default)]
pub struct Presence(Arc<Mutex<Option<Instant>>>);

/// Two missed pings and a margin.
pub(crate) const PRESENT_SECONDS: u64 = 70;

impl Presence {
    pub(crate) fn seen(&self) {
        *locks::held(&self.0) = Some(Instant::now());
    }

    #[must_use]
    pub(crate) fn anyone(&self) -> bool {
        locks::held(&self.0).is_some_and(|at| at.elapsed() < Duration::from_secs(PRESENT_SECONDS))
    }
}

/// One chain's effects under its own trust rule.
#[derive(Clone)]
pub struct Chained {
    /// Effects whose `chain` is this chain's store.
    pub caps: &'static Server<'static>,
    pub policy: Arc<suzatary_base::chains::ChainPolicy>,
}

impl Chained {
    fn keeping<'a>(
        chains: impl IntoIterator<Item = &'a Chained>,
    ) -> signatures::Keeping<signatures::Keeper<'static>> {
        signatures::Keeping::of(chains, |chained| &chained.policy).map(|chained| {
            signatures::Keeper {
                name: chained.policy.name.clone(),
                store: chained.caps.secretary.chain,
            }
        })
    }
}

impl State {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn over(
        first: Chained,
        rest: impl IntoIterator<Item = Chained>,
        deployment: Arc<Deployment>,
        door: Door,
        Signed {
            requests,
            asks,
            pairings,
        }: Signed,
        Performers { postman, acting }: Performers,
        workers: Arc<Workers>,
        Settings {
            live,
            models,
            secrets,
            declares,
            edits,
            forget,
            stamp,
            data,
        }: Settings,
    ) -> Self {
        let answered = Mutex::new(health::Healths::one(
            models.recorded(),
            chrono::DateTime::UNIX_EPOCH.naive_utc(),
            health::Outcome::Succeeded,
            "",
        ));
        let consenting = Arc::new(google::Consenting::new(
            Google::new(Endpoints::google()),
            Arc::new(Unresolved),
            Arc::clone(&edits) as _,
            &deployment.app_url,
        ));
        let mut chains = BTreeMap::new();
        chains.insert(first.policy.name.as_str().to_owned(), first.clone());
        for chain in rest {
            chains.insert(chain.policy.name.as_str().to_owned(), chain);
        }
        Self(Arc::new(Inner {
            first,
            chains: Arc::new(chains),
            door: Arc::new(door),
            deployment,
            known: Arc::default(),
            workers,
            postman,
            acting,
            live,
            secrets,
            declares,
            edits,
            forget,
            reveal: Arc::new(suzatary_held::held_value::Unread),
            checks: Arc::new(checks::Unsynced),
            reach: Arc::new(suzatary_connect::connect::PLAIN.clone()),
            uploads: None,
            titler: None,
            compactor: None,
            presence: Presence::default(),
            reports: Arc::new(Mutex::new(Limiter::new(suzatary_api::clienterror::BUDGET))),
            stamp,
            plugins: None,
            repos: Arc::new(suzatary_services::repos::Declared::default()),
            data,
            read: Arc::default(),
            asking: Arc::new(Mutex::new(Cache::new(Lru::new(4)))),
            built: Arc::new(Mutex::new(Cache::new(Lru::new(4)))),
            keys: None,
            requests,
            asks,
            pairings,
            performing: Arc::default(),
            wall: chrono::Utc::now,
            answered: Arc::new(answered),
            signing: Arc::new(Closed),
            verify: Arc::new(Unverified),
            consenting,
            noticing: Arc::default(),
            secrets_answered: Arc::default(),
            epochs: Arc::new(Epochs::remembered()),
            frontiers: Arc::new(Frontiers::remembered(prodrome_core::store::frontier::KEPT)),
            portal: Arc::new(Portal::Quiet),
            projects: Arc::new(|| Ok(None)),
            models,
        }))
    }

    /// Copies only if already shared.
    fn building(&mut self) -> &mut Inner {
        Arc::make_mut(&mut self.0)
    }

    #[must_use]
    pub fn with_books(mut self, epochs: Arc<Epochs>, frontiers: Arc<Frontiers>) -> Self {
        let inner = self.building();
        inner.epochs = epochs;
        inner.frontiers = frontiers;
        self
    }

    #[must_use]
    pub(crate) fn books(&self) -> Books<'_> {
        Books {
            epochs: &self.epochs,
            frontiers: &self.frontiers,
        }
    }

    #[must_use]
    pub fn with_answered(mut self, answered: Arc<dyn Register<String> + Send + Sync>) -> Self {
        self.building().answered = answered;
        self
    }

    /// Reread the busy calendars; a failed reading holds their cards.
    pub(crate) fn hear(&self) {
        let caps = self.first.caps;
        let now = instant_of(caps.secretary.clock.now());
        let heard = heard(caps.secretary.calendar, now);
        *locks::held(&self.noticing) = heard;
    }

    #[must_use]
    pub fn with_sign_ins(
        mut self,
        signing: Arc<dyn sign_in::Sessions + Send + Sync>,
        verify: Arc<dyn Verify + Send + Sync>,
    ) -> Self {
        self.building().signing = signing;
        self.building().verify = verify;
        self
    }

    #[must_use]
    pub fn with_google(mut self, consenting: google::Consenting) -> Self {
        self.building().consenting = Arc::new(consenting);
        self
    }

    #[must_use]
    pub fn with_portal(mut self, portal: Portal) -> Self {
        self.building().portal = Arc::new(portal);
        self
    }

    #[must_use]
    pub fn with_wall(mut self, wall: fn() -> chrono::DateTime<chrono::Utc>) -> Self {
        self.building().wall = wall;
        self
    }

    #[must_use]
    pub fn with_keys(mut self, keys: Option<BoxKeys>) -> Self {
        self.building().keys = keys.map(Arc::new);
        self
    }

    /// What every question of a key's standing asks.
    fn keeping(&self) -> signatures::Keeping<signatures::Keeper<'static>> {
        let rest = self
            .chains
            .values()
            .filter(|chained| chained.policy.name != self.first.policy.name);
        Chained::keeping(std::iter::once(&self.first).chain(rest))
    }

    #[must_use]
    pub fn with_repos(mut self, repos: suzatary_services::repos::Declared) -> Self {
        self.building().repos = Arc::new(repos);
        self
    }

    #[must_use]
    pub fn with_plugins(mut self, plugins: Plugins) -> Self {
        locks::write(&self.live)
            .plugins
            .clone_from(&plugins.manifests);
        self.building().plugins = Some(Arc::new(plugins));
        self
    }

    #[must_use]
    pub fn with_projects(mut self, projects: Projects) -> Self {
        self.building().projects = projects;
        self
    }

    /// The plan store's chain and plan, `None` where no mount holds a plan: the one question
    /// "Build it", `GET /api/releases` and `GET /api/about` ask.
    ///
    /// # Errors
    /// A plan that does not read, or two.
    pub(crate) fn project(&self) -> Result<Option<Project>, Refusal> {
        (self.projects)().map_err(|failed| Refusal::internal(format!("{failed:#}")))
    }

    #[must_use]
    pub fn with_reach(mut self, reach: Reach) -> Self {
        self.building().reach = Arc::new(reach);
        self
    }

    #[must_use]
    pub fn with_checks(mut self, checks: checks::Checks) -> Self {
        self.building().checks = checks;
        self
    }

    #[must_use]
    pub fn with_reveal(mut self, reveal: Arc<dyn Reveal + Send + Sync>) -> Self {
        self.building().reveal = reveal;
        self
    }

    /// Settable secret names, as of now.
    #[must_use]
    pub(crate) fn settables(&self) -> Settables {
        locks::read(&self.live).settables.clone()
    }

    /// The instance file's declarations, read once.
    #[must_use]
    pub(crate) fn live(&self) -> live::Live {
        locks::read(&self.live).clone()
    }

    /// `work` given the instance file's editor, and the file reread before this returns if
    /// `work` moved it, so no request answered after it reads the declarations it replaced.
    fn editing<T>(&self, who: &str, work: impl FnOnce(&noting::Noting<'_>) -> T) -> T {
        let edits = noting::Noting::new(self.edits.as_ref());
        let done = work(&edits);
        if edits.moved() {
            self.reconnected(who);
        }
        done
    }

    /// Swap a reread [`live::Live`] in whole under the lock; a failure changes nothing.
    pub(crate) fn reconnected(&self, who: &str) {
        let mut live = locks::write(&self.live);
        let read = self.edits.declared().and_then(|declared| {
            let accounts = suzatary_mail::mail::account::Roster::new(declared.accounts.clone())
                .map_err(|refused| {
                    suzatary_connect::settings::EditError::Malformed(format!(
                        "[[mail.accounts]]: {refused}"
                    ))
                })?;
            Ok((declared, accounts))
        });
        let (declared, accounts) = match read {
            Ok(read) => read,
            Err(failed) => {
                tracing::warn!("reconnected: the instance file: {failed}");
                return;
            }
        };
        let next = live::Live {
            settables: Settables::of_deployment(
                suzatary_io::model::TOKEN,
                declared.channels.iter().copied(),
                self.deployment.coding.as_ref(),
                (&declared.plugins, &declared.accounts),
                &self.repos,
            ),
            channels: suzatary_conversations::nonempty::NonEmpty::of(declared.channels),
            accounts,
            plugins: declared.plugins,
        };
        if let Err(failed) = self.declares.declare(&next.settables) {
            tracing::warn!("reconnected: declaring the secret names: {failed}");
            return;
        }
        logged(&live, &next, who);
        *live = next;
    }

    /// `caps`' secret store, as last answered.
    #[must_use]
    pub(crate) fn secrets<'a>(&'a self, caps: &'a Server<'_>) -> Remembering<'a> {
        Remembering {
            store: caps.secretary.secrets,
            answers: &self.secrets_answered,
        }
    }

    #[must_use]
    pub fn with_titler(mut self, model: Arc<dyn model::Sessions + Send + Sync>) -> Self {
        self.building().titler = Some(model);
        self
    }

    #[must_use]
    pub fn with_uploads(mut self, store: AttachmentStore, vault: Arc<dyn Vault>) -> Self {
        self.building().uploads = Some(Arc::new(Uploads { store, vault }));
        self
    }

    #[must_use]
    pub fn with_compactor(mut self, compactor: compaction::Compactor) -> Self {
        self.building().compactor = Some(Arc::new(compactor));
        self
    }

    /// Whether a round a lost run left may be claimed again: not while the model's last observed
    /// call failed, nor while its health does not read.
    pub(crate) fn retry(&self) -> Retry {
        match self.answered.healths() {
            Ok(answered) => match health::reading_of(&answered, self.models.recorded().as_str()) {
                Some(Reading::Failing { .. }) => Retry::Held,
                Some(Reading::Ok { .. }) | None => Retry::Allowed,
            },
            Err(failed) => {
                tracing::warn!("secretary: the model health did not read, so no retry: {failed}");
                Retry::Held
            }
        }
    }

    pub(crate) fn heard(&self, id: &suzatary_base::conversation_id::ConversationId) {
        self.workers.nudge(id, Arc::new(self.clone()));
        if let Some(compactor) = &self.compactor {
            compactor.nudge();
        }
    }

    /// File owed cards, withdraw the rest; failures are logged.
    pub fn notice(&self) {
        let heard = part("lock", || locks::held(&self.noticing));
        let live::Live {
            settables,
            channels: _,
            accounts: _,
            plugins: _,
        } = self.live();
        let answered = match part("models", || self.answered.healths()) {
            Ok(answered) => answered,
            Err(failed) => {
                tracing::warn!(
                    "noticed: the model health: {}",
                    suzatary_held::excerpt::Excerpt::of(&failed.to_string())
                );
                return;
            }
        };
        let open = part("sign_ins", || self.signing.open());
        let caps = self.first.caps;
        let declared = part("declared", || self.edits.declared())
            .inspect_err(|failed| tracing::warn!("noticed: the instance file: {failed}"))
            .ok();
        let health = part("sources", || caps.secretary.health.healths())
            .inspect_err(|failed| tracing::warn!("noticed: the sources' health: {failed}"))
            .ok();
        let now = instant_of(caps.secretary.clock.now());
        let model = part("model", || self.models.recorded());
        locks::held(&self.secrets_answered).forget();
        let secrets = self.secrets(caps);
        let seen = noticed::Seen {
            settables: &settables,
            secrets: &secrets,
            model: &model,
            answered: &answered,
            open: &open,
            declared: declared.as_ref(),
            health: health.as_ref(),
            heard: heard.as_ref(),
            pushed: &part("pushed", || settings::pushed(self)),
            now,
            redirect: suzatary_cards::secretary::google_sign_in::Consents::redirect(
                self.consenting.as_ref(),
            ),
        };
        match noticed::notice(
            &seen,
            caps.secretary.conversations,
            caps.secretary.inbox,
            caps.secretary.clock.now(),
        ) {
            Ok(filed) if !filed.withdrawn.is_empty() => tracing::info!(
                owed = filed.owed.len(),
                withdrawn = filed.withdrawn.len(),
                "noticed: the box's own cards"
            ),
            Ok(_) => {}
            Err(failed) => tracing::warn!("noticed: {}", failed.message()),
        }
        // A key trusted while no server ran leaves its card.
        if let Some(keys) = &self.keys {
            match part("trusted", || {
                crate::secretary::routes::withdraw_trusted(
                    caps.secretary.inbox,
                    caps.secretary.chain,
                    &keys.root().public(),
                    caps.secretary.clock.now(),
                )
            }) {
                Ok(withdrawn) if !withdrawn.is_empty() => {
                    tracing::info!(
                        withdrawn = withdrawn.len(),
                        "noticed: trusted devices' cards"
                    );
                    self.heard(&suzatary_base::conversation_id::DEFAULT);
                }
                Ok(_) => {}
                Err(failed) => {
                    tracing::warn!("noticed: trusted devices' cards: {}", failed.message());
                }
            }
        }
    }

    #[must_use]
    pub(crate) fn every_chain(&self) -> Vec<Bound<'_>> {
        self.chains
            .iter()
            .map(|(name, chained)| Bound {
                name,
                caps: chained.caps.secretary,
                policy: &chained.policy,
            })
            .collect()
    }

    /// Runs secretary work with the cards built.
    pub(crate) fn with_cards<T>(
        &self,
        work: impl FnOnce(&Server<'_>, &Cards<'_>) -> Result<T, Refusal>,
    ) -> Result<T, Refusal> {
        let chains = self.every_chain();
        let declared = self
            .edits
            .declared()
            .inspect_err(|failed| tracing::warn!("cards: the instance file: {failed}"))
            .ok();
        let settables = self.settables();
        let cards = Cards {
            deployment: &self.deployment,
            chains: &chains,
            plugins: declared
                .as_ref()
                .map(|declared| declared.plugins.as_slice()),
            reach: &self.reach,
            settables: &settables,
        };
        work(self.first.caps, &cards)
    }

    /// Every chain; only moved heads are reread.
    #[must_use]
    pub(crate) fn chains_read(&self) -> chain_set::Read {
        let lock = || locks::held(&self.read);
        // Read unlocked; of two racing requests the last to finish wins.
        let last = lock().clone();
        let now = last.moved(
            self.chains
                .values()
                .map(|chained| (chained.policy.as_ref(), chained.caps.secretary.chain)),
        );
        *lock() = now.clone();
        now
    }

    /// The named chain, or the first.
    pub(crate) fn bound(&self, named: Option<&str>) -> Result<Chained, Refusal> {
        let Some(name) = named else {
            return Ok(self.first.clone());
        };
        self.chains.get(name).cloned().ok_or_else(|| {
            Refusal::not_found(format!(
                "no chain named {} — this deployment serves {}",
                shown(name),
                self.deployment.chains.names().join(", ")
            ))
        })
    }
}

fn logged(from: &live::Live, to: &live::Live, who: &str) {
    if (&from.channels, &from.accounts, &from.plugins) != (&to.channels, &to.accounts, &to.plugins)
    {
        tracing::info!(who, "settings: the connections' tables changed");
    }
}

const NOT_BUILT: &str = "suzatary serve — the API is up, the frontend is not built.

The JSON API is live at /api/chain, /api/fold, /api/todo/<id>, /api/tree,
/api/mail/inbox, /api/mail/search, /api/mail/message/<id>,
/api/mail/thread/<id>, /api/briefings, /api/briefings/<date> and /events.

To build the page this server would otherwise hand you:

    scripts/build-web.sh

It writes web/dist. Restart `suzatary serve` afterwards — the static mount is
decided when the server starts.
";

/// The routes and the frontend behind the gate, as only [`app`] builds them: what a listener
/// serves.
pub struct Gated(Router);

impl Gated {
    /// The service a listener runs, each request carrying its peer's address for the gate.
    #[must_use]
    pub fn service(self) -> IntoMakeServiceWithConnectInfo<Router, SocketAddr> {
        self.0.into_make_service_with_connect_info::<SocketAddr>()
    }
}

#[allow(clippy::too_many_lines)]
pub fn app(state: State, dist: &std::path::Path) -> Gated {
    let api = table::table()
        .into_iter()
        .fold(Router::new(), |api, route| route.mount(api));

    // No SPA fallback: routes live in the fragment.
    let frontend: Router<State> = if dist.is_dir() {
        let tag = fresh::tag_of(dist);
        Router::new()
            .fallback_service(ServeDir::new(dist).precompressed_br())
            .layer(axum::middleware::from_fn(move |request, next| {
                fresh::revalidated(Arc::clone(&tag), request, next)
            }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                versioned::serve,
            ))
    } else {
        Router::new().fallback(not_built)
    };

    // Browsers cache no API answer but an object's literal; fastest compression on a tailnet.
    Gated(
        api.layer(axum::middleware::from_fn(validated::revalidated))
            .layer(SetResponseHeaderLayer::overriding(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-store"),
            ))
            .layer(axum::middleware::from_fn(reply::kept))
            .layer(
                CompressionLayer::new()
                    .br(true)
                    .gzip(true)
                    .quality(CompressionLevel::Fastest),
            )
            .merge(frontend)
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::gate::admit,
            ))
            // An attachment's stricter policy wins.
            .layer(SetResponseHeaderLayer::if_not_present(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static(POLICY),
            ))
            // So no other origin can pose as the Android app (`web/src/app/pushtoken.ts`).
            .layer(SetResponseHeaderLayer::overriding(
                header::HeaderName::from_static("cross-origin-opener-policy"),
                HeaderValue::from_static("same-origin"),
            ))
            .with_state(state),
    )
}

/// Every page's CSP; inline `style` is for typst's layout and reaches no other origin.
pub const POLICY: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; \
    style-src-attr 'unsafe-inline'; worker-src 'self'; object-src 'none'; base-uri 'none'; \
    form-action 'none'; frame-ancestors 'none'";

/// Everything serve does before it listens, each step timed (`Started`); none folds the
/// conversations, whose first fold reads the whole store.
pub async fn started_up(first: &Chained, rest: &[Chained]) {
    let starting = Instant::now();
    let mut started = Started::default();
    started.step("warm", || {
        for chain in std::iter::once(first).chain(rest) {
            watch::warm_now(chain.caps, &chain.policy);
        }
    });
    let caps = first.caps;
    started
        .blocking("reconcile", move || reconcile_orphaned_acts(caps))
        .await;
    // In order, so a key the sweep kept keeps no summary that read its words. Each step reads
    // only stored state, so a restart resumes it.
    started
        .blocking("erase.forgotten", move || {
            erased_at_startup(
                "erased by forgetting",
                caps.secretary
                    .memories
                    .hold()
                    .and_then(|held| held.erase_forgotten()),
            );
        })
        .await;
    started
        .blocking("erase.lines", move || {
            erased_at_startup(
                "keys destroyed",
                transcript::sweep(caps.secretary.conversations).map_err(Refusal::from),
            );
        })
        .await;
    started
        .blocking("erase.texts", move || {
            erased_at_startup(
                "texts deleted",
                caps.secretary.memories.sweep_only(Texts::Facts),
            );
        })
        .await;
    started
        .blocking("default", move || open_default(caps))
        .await;
    started.log("serve start", starting.elapsed());
}

/// Steps and their parts, one line at `suzatary::start`: `<step>.<part> <ms>`, then the time no part
/// took, so a line sums to its whole. It names no store's content.
#[derive(Default)]
struct Started(Vec<(&'static str, Parts, Duration)>);

impl Started {
    fn step<T>(&mut self, step: &'static str, run: impl FnOnce() -> T) -> T {
        let at = Instant::now();
        let (answer, parts) = scoped(run);
        self.took(step, at.elapsed(), parts);
        answer
    }

    /// [`Started::step`] on a blocking thread, so the runtime's threads stay free; a step that
    /// did not finish is logged and adds nothing.
    async fn blocking(&mut self, step: &'static str, run: impl FnOnce() + Send + 'static) {
        let at = Instant::now();
        match tokio::task::spawn_blocking(move || scoped(run).1).await {
            Ok(parts) => self.took(step, at.elapsed(), parts),
            Err(join_error) => tracing::warn!("{step} did not finish: {join_error}"),
        }
    }

    fn took(&mut self, step: &'static str, spent: Duration, parts: Parts) {
        let own = spent.saturating_sub(parts.sum());
        self.0.push((step, parts, own));
    }

    fn log(&self, said: &str, whole: Duration) {
        let steps = self
            .0
            .iter()
            .flat_map(|(step, parts, own)| {
                [
                    parts.line(&format!("{step}.")),
                    format!("{step} {}", own.as_millis()),
                ]
            })
            .filter(|said| !said.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        tracing::info!(
            target: "suzatary::start",
            whole = whole.as_millis(),
            "{said}: {steps}"
        );
    }
}

/// The listener, bound before the markup warms, so no other server takes the port meanwhile.
pub fn bound(listening: &listening::Listening) -> anyhow::Result<std::net::TcpListener> {
    let address = listening.at();
    let listener =
        std::net::TcpListener::bind(address).with_context(|| format!("binding {address}"))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// Serves `gated` on `listener` until the process ends.
pub async fn serve_on(
    listening: listening::Listening,
    listener: std::net::TcpListener,
    gated: Gated,
) -> anyhow::Result<()> {
    let service = gated.service();
    let address = listening.at();
    match listening {
        listening::Listening::Loopback(_) => {
            tracing::info!(
                "serve: http://{address} (reads GET, writes POST /api/act/*, gated by whois)"
            );
            axum_server::from_tcp(listener)?
                .acceptor(axum_server::accept::NoDelayAcceptor::new())
                .serve(service)
                .await
                .context("serving")
        }
        listening::Listening::Tls { cert, key, .. } => {
            let config = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(
                tls_config(&cert, &key)
                    .with_context(|| format!("reading {} and {}", cert.display(), key.display()))?,
            ));
            tracing::info!(
                "serve: https://{address} (reads GET, writes POST /api/act/*, gated by whois)"
            );
            // TCP_NODELAY, or the h2 preface waits on Nagle and a delayed ACK (~45 ms).
            axum_server::tls_rustls::from_tcp_rustls(listener, config)?
                .map(|tls| tls.acceptor(axum_server::accept::NoDelayAcceptor::new()))
                .serve(service)
                .await
                .context("serving")
        }
    }
}

async fn not_built() -> axum::response::Response {
    use axum::response::IntoResponse;
    (
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
        )],
        NOT_BUILT,
    )
        .into_response()
}

/// Open the default conversation; no read writes it.
fn open_default(caps: &Server<'_>) {
    if let Err(failed) = suzatary_lines::secretary::conversations::open_default(
        caps.secretary.conversations,
        caps.secretary.model,
        caps.secretary.clock.now(),
    ) {
        tracing::warn!("secretary: the default conversation: {}", failed.message());
    }
}

/// The work after the start, spawned: the first fold alone on the box's cores, then what else
/// folds or files.
pub fn after_started(state: &State, moved: Arc<tokio::sync::Notify>) {
    let (after, caps) = (state.clone(), state.first.caps);
    tokio::spawn(async move {
        let waiting = after.clone();
        after_start("waiting", move || resume_waiting(&waiting)).await;
        erase_after_start(caps);
        notice_from_now_on(&after, moved);
    });
}

/// The patch sweep over `mirrors`, the token watch over `tokens` and each chain's watch; only the
/// first chain pushes.
pub fn watching(
    state: &State,
    first: &Chained,
    rest: Vec<Chained>,
    mirrors: Arc<suzatary_io::repos::Mirrors>,
    tokens: Arc<suzatary_io::github_tokens::GithubTokens>,
) {
    tokio::spawn(patches::sweeping(state.clone(), mirrors));
    tokio::spawn(token_expiry::watching(state.clone(), tokens));
    tokio::spawn(watch::watcher(
        first.caps,
        (*state.deployment).clone(),
        (*first.policy).clone(),
        state.presence.clone(),
    ));
    for chain in rest {
        tokio::spawn(watch::warming(chain.caps, (*chain.policy).clone()));
    }
}

/// Calendars busy within the quiet span.
pub(crate) type Heard = std::collections::BTreeSet<suzatary_google::calendar::CalendarName>;

/// `None`, logged, on a failed read.
fn heard(
    calendar: &dyn suzatary_google::calendar::effect::Calendar,
    now: health::Instant,
) -> Option<Heard> {
    use suzatary_cards::secretary::failing::{QUIET, quiet};
    calendar
        .days((now - quiet()).date(), 2 * QUIET + 1)
        .inspect_err(|failed| tracing::warn!("noticed: the calendars: {failed}"))
        .ok()
        .map(|days| {
            days.iter()
                .flat_map(|day| &day.entries)
                .map(|entry| entry.id.calendar().clone())
                .collect()
        })
}

/// File owed cards hourly, the first at once, and on `moved`; the start waits for none.
fn notice_from_now_on(state: &State, moved: Arc<tokio::sync::Notify>) {
    let ringing = state.clone();
    tokio::spawn(async move {
        loop {
            moved.notified().await;
            let noticing = ringing.clone();
            if let Err(join_error) = tokio::task::spawn_blocking(move || noticing.notice()).await {
                tracing::warn!(
                    "noticed: a filing after a model moved did not finish: {join_error}"
                );
            }
        }
    });
    let state = state.clone();
    tokio::spawn(async move {
        let mut every = tokio::time::interval(Duration::from_hours(1));
        every.tick().await;
        let first = state.clone();
        after_start("notice", move || {
            first.hear();
            first.notice();
        })
        .await;
        loop {
            every.tick().await;
            let noticing = state.clone();
            if let Err(join_error) = tokio::task::spawn_blocking(move || {
                noticing.hear();
                noticing.notice();
            })
            .await
            {
                tracing::warn!("noticed: an hourly filing did not finish: {join_error}");
            }
        }
    });
}

/// `work` once the server listens, logged as `step` on an `after start` line.
async fn after_start(step: &'static str, work: impl FnOnce() + Send + 'static) {
    let at = Instant::now();
    let mut after = Started::default();
    after.blocking(step, work).await;
    after.log("after start", at.elapsed());
}

const INSTANCE_POLL: Duration = Duration::from_secs(1);

/// Reconnect and file when the instance file's bytes move.
pub async fn instance_watched(state: State, path: PathBuf) {
    let mut last = tokio::fs::read(&path).await.ok();
    let mut every = tokio::time::interval(INSTANCE_POLL);
    loop {
        every.tick().await;
        let Ok(now) = tokio::fs::read(&path).await else {
            continue;
        };
        if last.as_ref() == Some(&now) {
            continue;
        }
        last = Some(now);
        let moved = state.clone();
        if let Err(join_error) = tokio::task::spawn_blocking(move || {
            moved.reconnected("the instance file");
            moved.notice();
        })
        .await
        {
            tracing::warn!(
                "reconnected: reading the moved instance file did not finish: {join_error}"
            );
        }
    }
}

fn resume_waiting(state: &State) {
    let caps = state.first.caps;
    match suzatary_topics::secretary::set_up::owing(caps.secretary.conversations) {
        Ok(owing) => {
            for id in &owing {
                state.heard(id);
            }
        }
        Err(failed) => tracing::warn!("secretary: the openings owed did not read: {failed}"),
    }
    match transcript::resumed(
        caps.secretary.conversations,
        caps.secretary.clock.now(),
        state.retry(),
    ) {
        Ok(resumed) => {
            if !resumed.is_empty() {
                tracing::info!(
                    "secretary: resuming {} unanswered conversation(s)",
                    resumed.len()
                );
            }
            for id in &resumed {
                state.heard(id);
            }
        }
        Err(failed) => tracing::warn!("secretary: unanswered conversations: {failed}"),
    }
}

fn reconcile_orphaned_acts(caps: &Server<'_>) {
    match suzatary_lines::secretary::act_lines::reconcile(
        caps.secretary.conversations,
        caps.secretary.inbox,
        caps.secretary.clock.now(),
    ) {
        Ok(0) => {}
        Ok(reconciled) => {
            tracing::info!("secretary: reconciled {reconciled} orphaned act(s) at startup");
        }
        Err(failed) => tracing::warn!("secretary: reconcile at startup: {}", failed.message()),
    }
}

/// `ring` is passed, not installed process-wide.
fn tls_config(cert: &Path, key: &Path) -> anyhow::Result<rustls::ServerConfig> {
    use rustls::pki_types::pem::PemObject as _;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let chain = CertificateDer::pem_file_iter(cert)?.collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file(key)?;
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(chain, key)?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

/// The erasures that fold the conversations, in order after the start's. No read serves
/// what they remove meanwhile: a dead summary ([`Texts`]), an erased upload (search walks live
/// conversations).
fn erase_after_start(caps: &'static Server<'static>) {
    tokio::spawn(async move {
        after_start("erase.summaries", move || {
            erased_at_startup(
                "summaries' texts deleted",
                caps.secretary.memories.sweep_only(Texts::Summaries),
            );
        })
        .await;
        after_start("erase.uploads", move || {
            erased_at_startup(
                "uploads in the search index",
                suzatary_secretary::secretary::file_search::reindex(
                    caps.secretary.conversations,
                    caps.secretary.upload_index,
                ),
            );
        })
        .await;
    });
}

fn erased_at_startup(what: &str, step: Result<usize, Refusal>) {
    match step {
        Ok(0) => {}
        Ok(n) => tracing::info!("secretary: at startup, {n} {what}"),
        Err(failed) => {
            tracing::warn!("secretary: at startup, {what} failed: {}", failed.message());
        }
    }
}

#[cfg(test)]
mod not_built_tests {
    /// The page a server without a built frontend answers, paragraph by paragraph, the command
    /// alone and indented.
    #[test]
    fn the_unbuilt_page_says_what_to_run_in_paragraphs() {
        let answer = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
            .block_on(async {
                let response = super::not_built().await;
                axum::body::to_bytes(response.into_body(), usize::MAX).await
            })
            .expect("a body");
        let text = std::str::from_utf8(&answer).expect("utf-8");
        let paragraphs: Vec<&str> = text.split("\n\n").collect();
        assert_eq!(
            paragraphs,
            [
                "suzatary serve — the API is up, the frontend is not built.",
                "The JSON API is live at /api/chain, /api/fold, /api/todo/<id>, /api/tree,\n\
                 /api/mail/inbox, /api/mail/search, /api/mail/message/<id>,\n\
                 /api/mail/thread/<id>, /api/briefings, /api/briefings/<date> and /events.",
                "To build the page this server would otherwise hand you:",
                "    scripts/build-web.sh",
                "It writes web/dist. Restart `suzatary serve` afterwards — the static mount is\n\
                 decided when the server starts.\n",
            ]
        );
    }
}

#[cfg(test)]
mod tls_tests {
    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../io/tests/fixtures/{name}"))
    }

    #[test]
    fn the_listener_builds_its_tls_with_no_installed_provider() {
        let config = super::tls_config(&fixture("bridge-cert.pem"), &fixture("bridge-key.pem"))
            .expect("a pair");
        assert_eq!(
            config.alpn_protocols,
            [b"h2".to_vec(), b"http/1.1".to_vec()]
        );
        assert!(rustls::crypto::CryptoProvider::get_default().is_none());
        assert!(
            super::tls_config(&fixture("bridge-cert.pem"), &fixture("bridge-cert.pem")).is_err()
        );
    }
}
