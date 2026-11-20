//! `suzatary serve`; `table` lists every route. It sends no CORS header, `gate` refuses any peer
//! whois cannot name, and the only HTML it serves is `dist`'s files and typst's rendering.

use crate::commands::secretary::Acting;
use crate::commands::secretary::ActingStores;
use crate::commands::secretary::Postman;
use crate::instance::InstanceEdits;
use crate::instance::Mounts;
use crate::instance::Plugins;
use crate::stamp::Release;
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
use suzatary_io::model_health::Answered;
use suzatary_io::objects::Objects;
use suzatary_io::requests::Requests;
use suzatary_lines::parts::{Parts, part, scoped};
use suzatary_lines::secretary::transcript::Retry;
use suzatary_secretary::secretary::Bound;
use suzatary_server::gate::Door;
use suzatary_server::portal::Portal;
use suzatary_server::secretary::routes::{Cards, Perform, Post};
use suzatary_signatures::chain_set;
use suzatary_signatures::epoch::Epochs;
use suzatary_signatures::signatures::{BoxKeys, Pairings};
use suzatary_stores::asks::AskEvent;
pub(crate) mod asked;
mod body;
mod call;
mod chain;
pub mod checks;
mod compaction;
mod conversation_worker;
mod fresh;
mod gate;
pub use gate::door;
pub mod google;
mod guard;
mod held;
pub(crate) mod interpreters;
pub mod listening;
pub mod live;
mod locks;
mod mail;
pub mod mail_cache;
pub mod models;
mod noting;
mod params;
mod patches;
mod proposals;
mod reply;
pub(crate) mod route;
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
pub mod world;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use axum::Router;
use axum::http::{HeaderValue, header};

use suzatary_base::refusal::{Refusal, shown};
use suzatary_connect::deployment::Deployment;
use suzatary_digest::secretary::compacting::Compacting;
use suzatary_digest::secretary::memories::Texts;
use suzatary_google::google::Endpoints;
use suzatary_server::server::Server;
use tower_http::compression::{CompressionLayer, CompressionLevel};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::instance::Instance;
pub use conversation_worker::Workers;
pub use interpreters::{Brain, Peers};
pub use live::Live;
use prodrome_core::fpl::instant_of;
pub use settings::pushed_at;
use suzatary_connect::connect::Reach;
use suzatary_connect::secrets::Remembering;
use suzatary_connect::settings::{Declares, Edits, Settables, Verify, Writes};
use suzatary_conversations::secretary::model;
use suzatary_inbox::secretary::sign_in;
use suzatary_io::google::Google;
use suzatary_lines::secretary::transcript;
pub use world::World;

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

/// The settings page's stores.
pub struct Settings {
    pub live: live::Shared,
    pub models: Arc<models::Models>,
    pub secrets: Arc<dyn Writes + Send + Sync>,
    pub declares: Arc<dyn Declares + Send + Sync>,
    pub edits: Arc<dyn Edits + Send + Sync>,
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
    pub known: Arc<Mutex<gate::Remembering>>,
    pub workers: Arc<Workers>,
    /// Only [`State::reconnected`] writes it.
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
    /// Where each chain's store is, so the plan store is found per request.
    pub mounts: Arc<Mounts>,
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
pub struct Closed;

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
pub struct Unverified;

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

/// The build `GET /api/about` names.
#[derive(Debug, Clone, Default)]
pub enum Stamp {
    #[default]
    Compiled,
    /// Read per request; test only.
    #[cfg(feature = "fixture")]
    File(Arc<PathBuf>),
}

impl Stamp {
    #[must_use]
    pub fn build(&self) -> String {
        match self {
            Stamp::Compiled => crate::stamp::build().to_owned(),
            #[cfg(feature = "fixture")]
            Stamp::File(path) => std::fs::read_to_string(path.as_ref()).map_or_else(
                |_| crate::stamp::build().to_owned(),
                |t| t.trim().to_owned(),
            ),
        }
    }

    /// What `GET /api/about` answers.
    pub fn about(
        &self,
        mounts: &Mounts,
        set: &chain_set::ChainSet<'_>,
    ) -> Result<suzatary_cards::wire::cards::AboutPayload, Refusal> {
        let internal = |e: &dyn std::fmt::Display| Refusal::internal(e.to_string());
        let released = match crate::stamp::release().map_err(|e| internal(&e))? {
            Release::Released(released) => Some(released),
            Release::Dev => None,
        };
        let project = mounts.project().map_err(|e| internal(&e))?;
        suzatary_api::routes::about(
            released,
            &self.build(),
            project.as_ref().map(|project| &project.chain),
            set,
        )
    }
}

/// Whether a writer's page pinged within [`PRESENT_SECONDS`].
#[derive(Debug, Clone, Default)]
pub struct Presence(Arc<Mutex<Option<Instant>>>);

/// Two missed pings and a margin.
pub const PRESENT_SECONDS: u64 = 70;

impl Presence {
    pub fn seen(&self) {
        *locks::held(&self.0) = Some(Instant::now());
    }

    #[must_use]
    pub fn anyone(&self) -> bool {
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
            forget: Arc::new(mail_cache::Unkept(None)),
            reveal: Arc::new(suzatary_held::held_value::Unread),
            checks: Arc::new(checks::Unsynced),
            reach: Arc::new(suzatary_connect::connect::PLAIN.clone()),
            uploads: None,
            titler: None,
            compactor: None,
            presence: Presence::default(),
            reports: Arc::new(Mutex::new(Limiter::new(suzatary_api::clienterror::BUDGET))),
            stamp: Stamp::Compiled,
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
            mounts: Arc::default(),
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
    pub fn books(&self) -> Books<'_> {
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
    pub fn hear(&self) {
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

    /// Every chain's writer.
    #[must_use]
    pub fn chain_stores(
        &self,
    ) -> Vec<&'static dyn suzatary_store::store::Prodrome<suzatary_record::record::TodoEvent>> {
        std::iter::once(self.first.caps.secretary.chain)
            .chain(
                self.chains
                    .values()
                    .filter(|chained| chained.policy.name != self.first.policy.name)
                    .map(|chained| chained.caps.secretary.chain),
            )
            .collect()
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
    pub fn with_grants(mut self, tokens: suzatary_io::oauth::TokenStore) -> Self {
        self.building().forget = Arc::new(mail_cache::Unkept(Some(tokens)));
        self
    }

    #[must_use]
    pub fn with_mounts(mut self, mounts: Mounts) -> Self {
        self.building().mounts = Arc::new(mounts);
        self
    }

    /// The plan store's chain and plan, `None` where no mount holds a plan: the one question
    /// "Build it", `GET /api/releases` and `GET /api/about` ask.
    ///
    /// # Errors
    /// A plan that does not read, or two.
    pub fn project(&self) -> Result<Option<crate::instance::Project>, Refusal> {
        self.mounts
            .project()
            .map_err(|failed| Refusal::internal(format!("{failed:#}")))
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

    #[must_use]
    pub fn with_mail_cache(mut self, cache: mail_cache::MailCache) -> Self {
        self.building().forget = Arc::new(cache);
        self
    }

    /// Settable secret names, as of now.
    #[must_use]
    pub fn settables(&self) -> Settables {
        locks::read(&self.live).settables.clone()
    }

    /// The instance file's declarations, read once.
    #[must_use]
    pub fn live(&self) -> live::Live {
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
    pub fn reconnected(&self, who: &str) {
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
    pub fn secrets<'a>(&'a self, caps: &'a Server<'_>) -> Remembering<'a> {
        Remembering {
            store: caps.secretary.secrets,
            answers: &self.secrets_answered,
        }
    }

    #[must_use]
    pub fn with_stamp(mut self, stamp: Stamp) -> Self {
        self.building().stamp = stamp;
        self
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
    pub fn retry(&self) -> Retry {
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

    pub fn heard(&self, id: &suzatary_base::conversation_id::ConversationId) {
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
                suzatary_server::secretary::routes::withdraw_trusted(
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
    pub fn every_chain(&self) -> Vec<Bound<'_>> {
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
    pub fn with_cards<T>(
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
    pub fn chains_read(&self) -> chain_set::Read {
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
    pub fn bound(&self, named: Option<&str>) -> Result<Chained, Refusal> {
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

/// What `suzatary serve` was told.
pub struct Serving {
    pub listening: listening::Listening,
    /// `app_url`'s host at the `listening` port.
    pub door: Door,
    pub dist: PathBuf,
    pub live: live::Shared,
    /// Names a peer, loopback included.
    pub peers: Peers,
    pub brain: Brain,
    pub titler: Brain,
    /// Off under every fixture.
    pub compacting: Compacting,
    pub postman: Postman,
    pub acting: Performing,
    /// Shared with the performer for a tap's announcements.
    pub workers: Arc<Workers>,
    pub stamp: Stamp,
    pub signing: Arc<sign_ins::SignIns>,
    pub verify: Arc<dyn Verify + Send + Sync>,
    /// Rings `moved` when a reading moves.
    pub answered: Answered,
    /// Rung when a model starts or stops answering.
    pub moved: Arc<tokio::sync::Notify>,
    /// Google's endpoints, or a fake.
    pub google: Endpoints,
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

#[allow(clippy::too_many_lines)]
pub fn app(state: State, dist: &std::path::Path) -> Router {
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
            gate::admit,
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
        .with_state(state)
}

/// Every page's CSP; inline `style` is for typst's layout and reaches no other origin.
pub const POLICY: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; \
    style-src-attr 'unsafe-inline'; worker-src 'self'; object-src 'none'; base-uri 'none'; \
    form-action 'none'; frame-ancestors 'none'";

/// Performs a tapped act through the pages' handles.
pub type Performing = Box<dyn FnOnce(ActingStores, Arc<Workers>) -> Acting + Send>;

pub async fn listen(instance: &Instance, serving: Serving) -> anyhow::Result<()> {
    use suzatary_io::{epochs, frontiers};
    let listener = bound(&serving.listening)?;
    // Declared first, so a server that cannot write them does not start.
    instance
        .declarations
        .opened()
        .context("declaring this instance's secret names")?;
    // Missing keys leave the server read-only.
    let signers = instance.signers();
    if let Err(failed) = instance.adopt(&signers).whole() {
        tracing::warn!("{failed:#}");
    }
    let world: &'static World = Box::leak(Box::new(World::assemble(
        instance,
        signers,
        serving.peers,
        serving.brain,
    )));
    if let Err(failed) = world.begin_written() {
        tracing::warn!("serve: the stores a tab writes are not begun: {failed}");
    }
    let mut bound = world.chains.iter().map(|stores| Chained {
        caps: Box::leak(Box::new(world.server(stores))),
        policy: Arc::new(stores.policy.clone()),
    });
    let first = bound.next().context("an instance names a chain")?;
    let rest: Vec<Chained> = bound.collect();
    let stores = world.acting_stores();
    let workers = serving.workers;
    let state = State::over(
        first.clone(),
        rest.clone(),
        Arc::new(instance.deployment.clone()),
        serving.door,
        Signed {
            requests: Arc::new(Requests::open(
                instance.requests(),
                instance.paths.answers.clone(),
            )),
            asks: Arc::new(Objects::open(instance.asks())),
            pairings: Arc::new(suzatary_io::pairing::Pairings::at(
                instance.paths.pairing.clone(),
            )),
        },
        Performers {
            postman: Arc::new(serving.postman),
            acting: Arc::new((serving.acting)(stores, Arc::clone(&workers))),
        },
        workers,
        Settings {
            // Never `secretspec.toml`, which names unsettable secrets.
            live: Arc::clone(&serving.live),
            models: Arc::new(models::Models::of(instance)),
            secrets: Arc::new(instance.secrets()),
            declares: Arc::new(instance.declarations.clone()),
            edits: Arc::new(InstanceEdits::new(instance.path.clone())),
            data: Arc::new(instance.data().to_path_buf()),
        },
    )
    .with_uploads(
        interpreters::upload_store(instance),
        Arc::new(interpreters::drive_vault(instance)),
    )
    .with_titler(Arc::new(serving.titler))
    .with_stamp(serving.stamp)
    .with_keys(world.signers.held().cloned())
    .with_plugins(instance.plugins.clone())
    .with_reach(instance.reach())
    .with_mounts(instance.mounts())
    .with_repos(instance.repos.clone())
    .with_portal(instance.portal.clone())
    .with_mail_cache(mail_cache::MailCache::of(instance))
    .with_reveal(Arc::new(crate::commands::mail::notmuch(instance)))
    .with_checks(Arc::new(checks::Synced::of(instance, &serving.google)))
    .with_answered(Arc::new(serving.answered))
    .with_books(
        Arc::new(epochs::open(instance.paths.epochs.clone())),
        Arc::new(frontiers::open(instance.paths.frontiers.clone())),
    )
    .with_sign_ins(serving.signing, serving.verify)
    .with_google(google::Consenting::new(
        Google::new(serving.google),
        Arc::new(instance.secrets()),
        Arc::new(InstanceEdits::new(instance.path.clone())),
        &instance.deployment.app_url,
    ));
    started_up(&first, &rest).await;
    // Its first round folds the conversations, which a start step would wait for.
    let state = compaction::started(state, instance, serving.compacting);
    let (after, caps) = (state.clone(), first.caps);
    // The first fold alone on the box's cores; what else folds or files starts once it holds.
    tokio::spawn(async move {
        let waiting = after.clone();
        after_start("waiting", move || resume_waiting(&waiting)).await;
        erase_after_start(caps);
        notice_from_now_on(&after, serving.moved);
    });
    tokio::spawn(instance_watched(state.clone(), instance.path.clone()));
    watch_from_now_on(instance, stores, &state, &first, rest);

    let router = app(state, &serving.dist);
    let service = router.into_make_service_with_connect_info::<SocketAddr>();

    serve_on(serving.listening, listener, service).await
}

/// Everything serve does before it listens, each step timed ([`Started`]); none folds the
/// conversations, whose first fold reads the whole store.
async fn started_up(first: &Chained, rest: &[Chained]) {
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
fn bound(listening: &listening::Listening) -> anyhow::Result<std::net::TcpListener> {
    let address = listening.at();
    let listener =
        std::net::TcpListener::bind(address).with_context(|| format!("binding {address}"))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

async fn serve_on(
    listening: listening::Listening,
    listener: std::net::TcpListener,
    service: axum::extract::connect_info::IntoMakeServiceWithConnectInfo<Router, SocketAddr>,
) -> anyhow::Result<()> {
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

/// The flags `suzatary serve` takes, each an override of the instance file.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The address to bind, overriding the instance file's.
    #[arg(long)]
    host: Option<String>,
    /// The port.
    #[arg(long)]
    port: Option<u16>,
    /// A TLS certificate (PEM). With `--key`, serve https.
    #[arg(long, requires = "key")]
    cert: Option<PathBuf>,
    /// A TLS key (PEM).
    #[arg(long, requires = "cert")]
    key: Option<PathBuf>,
    /// The built frontend directory, served in place of the instance file's `dist`.
    #[arg(long)]
    dist: Option<PathBuf>,
    /// Test only, compiled out of the shipped binary: answer every whois with this login.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "LOGIN")]
    whois_fixture: Option<String>,
    /// Test only: answer every model turn from this script.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "FILE")]
    model_fixture: Option<PathBuf>,
    /// Test only: record what a tap would have sent into this file, and send nothing.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "FILE")]
    smtp_fixture: Option<PathBuf>,
    /// Test only: answer `GET /api/about`'s build with this file, read per request.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "FILE")]
    build_fixture: Option<PathBuf>,
    /// Test only: ask the fake Google at this base URL instead of Google's own.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "URL")]
    google_fixture: Option<String>,
}

fn tls(
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
    tls_dir: &std::path::Path,
) -> Option<(PathBuf, PathBuf)> {
    if let (Some(cert), Some(key)) = (cert, key) {
        return Some((cert, key));
    }
    let (cert, key) = (tls_dir.join("cert.pem"), tls_dir.join("key.pem"));
    (cert.is_file() && key.is_file()).then_some((cert, key))
}

/// Refused for plain http anywhere but loopback, and for a fixture flag anywhere but loopback.
fn serving(instance: &Instance, args: Args) -> anyhow::Result<Serving> {
    #[cfg(feature = "fixture")]
    let (whois_fixture, model_fixture, smtp_fixture, build_fixture, google_fixture) = (
        args.whois_fixture,
        args.model_fixture,
        args.smtp_fixture,
        args.build_fixture,
        args.google_fixture,
    );
    let tls = tls(args.cert, args.key, &instance.serving.tls_dir);
    let (listening, door) = listening_of(instance, args.host, args.port, tls)?;
    // A fixture runs only where no other machine reaches it: loopback, over http or https.
    #[cfg(feature = "fixture")]
    let reachable = listening.reachable();
    #[cfg(feature = "fixture")]
    let stamp = stamp_of(build_fixture, reachable)?;
    #[cfg(not(feature = "fixture"))]
    let stamp = Stamp::Compiled;
    #[cfg(feature = "fixture")]
    let google = match google_fixture {
        None => Endpoints::google(),
        Some(_) if reachable => {
            anyhow::bail!(
                "--google-fixture is test-only and refuses to run where another machine reaches it"
            )
        }
        Some(base) => Endpoints::at(&base),
    };
    #[cfg(not(feature = "fixture"))]
    let google = Endpoints::google();
    let live = live::Live::of(instance).shared();
    let moved = Arc::new(tokio::sync::Notify::new());
    let answered = suzatary_io::model_health::beside(&instance.mail.heartbeat).ringing({
        let moved = Arc::clone(&moved);
        move || moved.notify_one()
    });

    #[cfg(feature = "fixture")]
    let any_fixture = whois_fixture.is_some();
    #[cfg(feature = "fixture")]
    let peers = match whois_fixture {
        None => daemon_peers(&instance.paths.whois_socket),
        Some(login) => {
            if reachable {
                anyhow::bail!(
                    "--whois-fixture is test-only and refuses to run where another machine reaches it"
                );
            }
            tracing::warn!("serve: --whois-fixture {login} — every peer is this login");
            Peers::Fixture(capability::Login::from_whois(login))
        }
    };
    #[cfg(not(feature = "fixture"))]
    let peers = daemon_peers(&instance.paths.whois_socket);

    // Records taps too, never reaching Microsoft.
    #[cfg(feature = "fixture")]
    let scripted = model_fixture.is_some();
    #[cfg(feature = "fixture")]
    let titler = match &model_fixture {
        Some(path) => Brain::Scripted(crate::commands::fixture::Scripted::reading(path)?),
        None => real_brain(instance, &answered),
    };
    #[cfg(not(feature = "fixture"))]
    let titler = real_brain(instance, &answered);
    #[cfg(feature = "fixture")]
    let (brain, postman) = fixture_talking(
        instance,
        &answered,
        (model_fixture, smtp_fixture),
        reachable,
    )?;
    #[cfg(not(feature = "fixture"))]
    let (brain, postman) = (real_brain(instance, &answered), real_postman(instance));
    let (acting, signing, verify) = performers(
        instance,
        &answered,
        &moved,
        #[cfg(feature = "fixture")]
        (scripted, &google),
    );
    #[cfg(feature = "fixture")]
    let fixture = any_fixture || scripted;
    #[cfg(not(feature = "fixture"))]
    let fixture = false;
    let compacting = quiet(instance, fixture);

    Ok(Serving {
        listening,
        door,
        dist: args.dist.unwrap_or_else(|| instance.serving.dist.clone()),
        live,
        peers,
        brain,
        titler,
        compacting,
        postman,
        acting,
        workers: Arc::new(Workers::new()),
        stamp,
        signing,
        verify,
        answered,
        moved,
        google,
    })
}

fn listening_of(
    instance: &Instance,
    host: Option<String>,
    port: Option<u16>,
    tls: Option<(PathBuf, PathBuf)>,
) -> anyhow::Result<(listening::Listening, Door)> {
    let host = host.unwrap_or_else(|| instance.serving.host.clone());
    let at = SocketAddr::new(
        host.parse()
            .with_context(|| format!("{} is not an address", shown(&host)))?,
        port.unwrap_or(instance.serving.port),
    );
    let door = gate::door_at(&instance.deployment.app_url, at)?;
    let listening = listening::Listening::new(at, tls).with_context(|| {
        format!(
            "no readable {}/{{cert,key}}.pem",
            instance.serving.tls_dir.display()
        )
    })?;
    Ok((listening, door))
}

fn quiet(instance: &Instance, fixture: bool) -> Compacting {
    if fixture {
        Compacting::Off
    } else {
        instance.secretary.compacting()
    }
}

/// Fixtures only; never where another machine reaches them.
#[cfg(feature = "fixture")]
fn fixture_talking(
    instance: &Instance,
    answered: &Answered,
    (model_fixture, smtp_fixture): (Option<PathBuf>, Option<PathBuf>),
    reachable: bool,
) -> anyhow::Result<(Brain, Postman)> {
    Ok(match (model_fixture, smtp_fixture) {
        (None, None) => (real_brain(instance, answered), real_postman(instance)),
        (script, recording) => {
            if reachable {
                anyhow::bail!(
                    "the model and smtp fixtures refuse to run where another machine reaches them"
                );
            }
            (
                match script {
                    None => real_brain(instance, answered),
                    Some(path) => {
                        tracing::warn!(
                            "serve: --model-fixture {} — scripted replies",
                            path.display()
                        );
                        Brain::Scripted(crate::commands::fixture::Scripted::reading(&path)?)
                    }
                },
                match recording {
                    None => real_postman(instance),
                    Some(path) => {
                        tracing::warn!(
                            "serve: --smtp-fixture {} — nothing is sent",
                            path.display()
                        );
                        Postman::Recording(path)
                    }
                },
            )
        }
    })
}

fn performers(
    instance: &Instance,
    answered: &Answered,
    moved: &Arc<tokio::sync::Notify>,
    #[cfg(feature = "fixture")] (scripted, google): (bool, &Endpoints),
) -> (
    Performing,
    Arc<sign_ins::SignIns>,
    Arc<dyn Verify + Send + Sync>,
) {
    let signing = Arc::new(sign_ins::SignIns::new("claude").ringing({
        let moved = Arc::clone(moved);
        move || moved.notify_one()
    }));
    let answered = || answered.clone();
    #[cfg(feature = "fixture")]
    if scripted {
        return (
            recording_acting(instance, &signing, google),
            Arc::clone(&signing),
            Arc::new(interpreters::Verifying::Fixture {
                instance: Arc::new(instance.clone()),
                answered: answered(),
            }),
        );
    }
    (
        real_acting(instance, &signing, moved),
        Arc::clone(&signing),
        Arc::new(interpreters::Verifying::Real {
            instance: Arc::new(instance.clone()),
            answered: answered(),
        }),
    )
}

/// `--build-fixture`; refused where another machine reaches it.
#[cfg(feature = "fixture")]
fn stamp_of(build_fixture: Option<PathBuf>, reachable: bool) -> anyhow::Result<Stamp> {
    let Some(path) = build_fixture else {
        return Ok(Stamp::Compiled);
    };
    if reachable {
        anyhow::bail!(
            "--build-fixture is test-only and refuses to run where another machine reaches it"
        );
    }
    tracing::warn!(
        "serve: --build-fixture {} — the build is that file's",
        path.display()
    );
    Ok(Stamp::File(Arc::new(path)))
}

/// The local daemon names every peer, loopback included: an address is never an identity.
fn daemon_peers(socket: &Path) -> Peers {
    Peers::Daemon(suzatary_io::whois::Tailscaled::new(socket))
}

fn real_brain(instance: &Instance, answered: &Answered) -> Brain {
    interpreters::brain(instance, answered.clone())
}

fn real_postman(instance: &Instance) -> Postman {
    Postman::Real(Arc::new(instance.clone()))
}

/// Rings `moved` when a device flow ends.
fn real_acting(
    instance: &Instance,
    signing: &Arc<sign_ins::SignIns>,
    moved: &Arc<tokio::sync::Notify>,
) -> Performing {
    let (instance, signing, moved) = (instance.clone(), Arc::clone(signing), Arc::clone(moved));
    Box::new(move |stores, workers| {
        Acting::real(
            &instance,
            stores,
            workers,
            signing,
            Arc::new(move || moved.notify_one()),
        )
    })
}

/// Never dials Microsoft.
#[cfg(feature = "fixture")]
fn recording_acting(
    instance: &Instance,
    signing: &Arc<sign_ins::SignIns>,
    google: &Endpoints,
) -> Performing {
    let (instance, signing, google) = (instance.clone(), Arc::clone(signing), google.clone());
    Box::new(move |stores, workers| Acting::recording(&instance, stores, workers, signing, &google))
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

/// Only the first chain pushes.
fn watch_from_now_on(
    instance: &Instance,
    stores: ActingStores,
    state: &State,
    first: &Chained,
    rest: Vec<Chained>,
) {
    crate::commands::coding::spawn_polling(instance, stores.conversations, &state.workers);
    let upkeep = crate::commands::doctor::upkeep_line(instance.deployment.upkeep.as_ref());
    if instance.deployment.upkeep.is_some() {
        tracing::info!("serve: upkeep {upkeep}");
    } else {
        tracing::warn!("serve: upkeep {upkeep}");
    }
    tokio::spawn(patches::sweeping(
        state.clone(),
        interpreters::mirrors(instance),
    ));
    tokio::spawn(token_expiry::watching(
        state.clone(),
        Arc::new(suzatary_io::github_tokens::GithubTokens::new(
            instance.paths.github_tokens.clone(),
            Box::new(instance.secrets()),
        )),
    ));
    tokio::spawn(watch::watcher(
        first.caps,
        instance.deployment.clone(),
        (*first.policy).clone(),
        state.presence.clone(),
    ));
    for chain in rest {
        tokio::spawn(watch::warming(chain.caps, (*chain.policy).clone()));
    }
}

/// Calendars busy within the quiet span.
pub type Heard = std::collections::BTreeSet<suzatary_google::calendar::CalendarName>;

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

pub fn run(path: &Path, args: Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    // Installed already where `RUST_LOG` asked `main` for it, with the same filter.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .try_init();
    let serving = serving(&instance, args)?;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(listen(&instance, serving))
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
