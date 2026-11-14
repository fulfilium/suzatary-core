//! `suzatary serve`; `table` lists every route. It sends no CORS header, `gate` refuses any peer
//! whois cannot name, and the only HTML it serves is `dist`'s files and typst's rendering.

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

use suzatary_api::server::Server;
use suzatary_base::refusal::{Refusal, shown};
use suzatary_core::deployment::Deployment;
use suzatary_core::google::Endpoints;
use suzatary_core::secretary::compacting::Compacting;
use tower_http::compression::{CompressionLayer, CompressionLevel};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::instance::Instance;
pub use conversation_worker::Workers;
pub use interpreters::{Brain, Peers};
pub use live::Live;
pub use settings::pushed_at;
pub use world::World;

/// The owner's uploads; only his confirm reaches the vault.
pub struct Uploads {
    pub store: suzatary_io::attachments::AttachmentStore,
    pub vault: Arc<dyn suzatary_core::vault::Vault>,
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
    pub secrets: Arc<dyn suzatary_core::settings::Writes + Send + Sync>,
    pub declares: Arc<dyn suzatary_core::settings::Declares + Send + Sync>,
    pub edits: Arc<dyn suzatary_core::settings::Edits + Send + Sync>,
    pub data: Arc<std::path::PathBuf>,
}

/// What a tap reaches, outside every record.
pub struct Performers {
    pub postman: Arc<dyn suzatary_api::secretary::routes::Post + Send + Sync>,
    pub acting: Arc<dyn suzatary_api::secretary::routes::Perform + Send + Sync>,
}

/// The signed requests kept, the box's refusals of signed objects, and the pairing codes.
pub struct Signed {
    pub requests: Arc<suzatary_io::requests::Requests>,
    pub asks: Arc<suzatary_io::objects::Objects<suzatary_core::asks::AskEvent>>,
    pub pairings: Arc<dyn suzatary_core::signatures::Pairings>,
}

/// Everything a handler reads.
#[derive(Clone)]
pub struct Inner {
    /// Serves every bare `/api/…` path.
    pub first: Chained,
    pub chains: Arc<BTreeMap<String, Chained>>,
    pub deployment: Arc<Deployment>,
    pub door: Arc<suzatary_api::gate::Door>,
    /// Reached only with a `Tap`.
    pub postman: Arc<dyn suzatary_api::secretary::routes::Post + Send + Sync>,
    /// Reached only with a `Tap`.
    pub acting: Arc<dyn suzatary_api::secretary::routes::Perform + Send + Sync>,
    pub known: Arc<Mutex<gate::Remembering>>,
    pub workers: Arc<Workers>,
    /// Only [`State::reconnected`] writes it.
    pub live: live::Shared,
    pub models: Arc<models::Models>,
    /// Reached only by a tapped card.
    pub secrets: Arc<dyn suzatary_core::settings::Writes + Send + Sync>,
    /// Refuses undeclared names.
    pub declares: Arc<dyn suzatary_core::settings::Declares + Send + Sync>,
    /// Edits the instance file, by a tapped card.
    pub edits: Arc<dyn suzatary_core::settings::Edits + Send + Sync>,
    /// Deletes a removed account's data.
    pub forget: Arc<dyn suzatary_core::mail::cache::Forget + Send + Sync>,
    /// Reads a held value back for a tap; outside every record, so no agent verb can.
    pub reveal: Arc<dyn suzatary_core::held_value::Reveal + Send + Sync>,
    /// Syncs a plugin after a tap.
    pub checks: checks::Checks,
    pub reach: Arc<suzatary_core::connect::Reach>,
    /// `None` refuses an upload by name.
    pub uploads: Option<Arc<Uploads>>,
    /// `None` names a topic with the record's model.
    pub titler: Option<Arc<dyn suzatary_core::secretary::model::Sessions + Send + Sync>>,
    /// `None` while the compactor is off.
    pub compactor: Option<Arc<compaction::Compactor>>,
    pub presence: Presence,
    pub reports: Arc<Mutex<suzatary_api::clienterror::Limiter>>,
    pub stamp: Stamp,
    /// `None` declares no plugin.
    pub plugins: Option<Arc<crate::instance::Plugins>>,
    pub repos: Arc<suzatary_core::repos::Declared>,
    pub data: Arc<std::path::PathBuf>,
    /// Only a chain whose heads moved is reread.
    pub read: Arc<Mutex<suzatary_core::chain_set::Read>>,
    /// Open connection cards, memoised.
    pub asking: Arc<
        Mutex<
            prodrome_core::memo::Cache<
                Vec<suzatary_core::settings::overview::Asking>,
                prodrome_core::memo::Lru,
            >,
        >,
    >,
    pub built: Arc<
        Mutex<
            prodrome_core::memo::Cache<
                suzatary_core::settings::overview::Build,
                prodrome_core::memo::Lru,
            >,
        >,
    >,
    /// Reached only by a tapped device card.
    pub keys: Option<Arc<suzatary_core::signatures::BoxKeys>>,
    pub requests: Arc<suzatary_io::requests::Requests>,
    /// Written only by the acceptance of a tab's objects.
    pub asks: Arc<suzatary_io::objects::Objects<suzatary_core::asks::AskEvent>>,
    pub pairings: Arc<dyn suzatary_core::signatures::Pairings>,
    /// So a request sent twice at once runs once.
    pub performing: asked::InFlight,
    /// Now, in UTC, since devices stamp UTC.
    pub wall: fn() -> chrono::DateTime<chrono::Utc>,
    pub answered: Arc<dyn suzatary_core::health::Register<String> + Send + Sync>,
    /// Reached only with a `Tap`.
    pub signing: Arc<dyn suzatary_core::secretary::sign_in::Sessions + Send + Sync>,
    pub verify: Arc<dyn suzatary_core::settings::Verify + Send + Sync>,
    /// Begun only with a `Tap`, finished only by its callback.
    pub consenting: Arc<google::Consenting>,
    /// Held across a filing, so none files a card twice.
    pub noticing: Arc<Mutex<Option<Heard>>>,
    /// Secret store answers since the last filing.
    pub secrets_answered: Arc<Mutex<suzatary_core::secrets::SecretAnswers>>,
    pub epochs: Arc<suzatary_core::epoch::Epochs>,
    pub frontiers: Arc<prodrome_core::store::frontier::Frontiers>,
    pub portal: Arc<suzatary_api::portal::Portal>,
}

/// A store that holds no secret.
pub struct Unresolved;

impl suzatary_io::secrets::Resolves for Unresolved {
    fn get(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<suzatary_base::capability::Secret>, suzatary_io::secrets::SecretError> {
        Ok(None)
    }
}

/// No sign-in is held open.
pub struct Closed;

impl suzatary_core::secretary::sign_in::Sessions for Closed {
    fn open(&self) -> Vec<suzatary_core::secretary::sign_in::SignInUrl> {
        Vec::new()
    }

    fn finish(
        &self,
        _: &suzatary_core::secretary::sign_in::SignInUrl,
        _: &suzatary_base::capability::Secret,
    ) -> Option<(
        suzatary_core::secretary::inbox::ProposalId,
        Result<suzatary_base::capability::Secret, String>,
    )> {
        None
    }
}

/// Keeps no model credential.
pub struct Unverified;

impl suzatary_core::settings::Verify for Unverified {
    fn verify(
        &self,
        name: &suzatary_core::settings::Settable,
        _: &suzatary_base::capability::Secret,
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
        set: &suzatary_core::chain_set::ChainSet<'_>,
    ) -> Result<suzatary_core::wire::cards::AboutPayload, Refusal> {
        let internal = |e: &dyn std::fmt::Display| Refusal::internal(e.to_string());
        let released = match crate::stamp::release().map_err(|e| internal(&e))? {
            crate::stamp::Release::Released(released) => Some(released),
            crate::stamp::Release::Dev => None,
        };
        let plan = suzatary_core::release::Plan::declared().map_err(|e| internal(&e))?;
        suzatary_api::routes::about(released, &self.build(), &plan, set)
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
        door: suzatary_api::gate::Door,
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
        let answered = Mutex::new(suzatary_core::health::Healths::one(
            models.recorded(),
            chrono::DateTime::UNIX_EPOCH.naive_utc(),
            suzatary_core::health::Outcome::Succeeded,
            "",
        ));
        let consenting = Arc::new(google::Consenting::new(
            suzatary_io::google::Google::new(Endpoints::google()),
            Arc::new(Unresolved),
            Arc::clone(&edits),
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
            reveal: Arc::new(suzatary_core::held_value::Unread),
            checks: Arc::new(checks::Unsynced),
            reach: Arc::new(suzatary_core::connect::PLAIN.clone()),
            uploads: None,
            titler: None,
            compactor: None,
            presence: Presence::default(),
            reports: Arc::new(Mutex::new(suzatary_api::clienterror::Limiter::new(
                suzatary_api::clienterror::BUDGET,
            ))),
            stamp: Stamp::Compiled,
            plugins: None,
            repos: Arc::new(suzatary_core::repos::Declared::default()),
            data,
            read: Arc::default(),
            asking: Arc::new(Mutex::new(prodrome_core::memo::Cache::new(
                prodrome_core::memo::Lru::new(4),
            ))),
            built: Arc::new(Mutex::new(prodrome_core::memo::Cache::new(
                prodrome_core::memo::Lru::new(4),
            ))),
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
            epochs: Arc::new(suzatary_core::epoch::Epochs::remembered()),
            frontiers: Arc::new(prodrome_core::store::frontier::Frontiers::remembered(
                prodrome_core::store::frontier::KEPT,
            )),
            portal: Arc::new(suzatary_api::portal::Portal::Quiet),
            models,
        }))
    }

    /// Copies only if already shared.
    fn building(&mut self) -> &mut Inner {
        Arc::make_mut(&mut self.0)
    }

    #[must_use]
    pub fn with_books(
        mut self,
        epochs: Arc<suzatary_core::epoch::Epochs>,
        frontiers: Arc<prodrome_core::store::frontier::Frontiers>,
    ) -> Self {
        let inner = self.building();
        inner.epochs = epochs;
        inner.frontiers = frontiers;
        self
    }

    #[must_use]
    pub fn books(&self) -> suzatary_api::routes::Books<'_> {
        suzatary_api::routes::Books {
            epochs: &self.epochs,
            frontiers: &self.frontiers,
        }
    }

    #[must_use]
    pub fn with_answered(
        mut self,
        answered: Arc<dyn suzatary_core::health::Register<String> + Send + Sync>,
    ) -> Self {
        self.building().answered = answered;
        self
    }

    /// Reread the busy calendars; a failed reading holds their cards.
    pub fn hear(&self) {
        let caps = self.first.caps;
        let now = prodrome_core::fpl::instant_of(caps.secretary.clock.now());
        let heard = heard(caps.secretary.calendar, now);
        *locks::held(&self.noticing) = heard;
    }

    #[must_use]
    pub fn with_sign_ins(
        mut self,
        signing: Arc<dyn suzatary_core::secretary::sign_in::Sessions + Send + Sync>,
        verify: Arc<dyn suzatary_core::settings::Verify + Send + Sync>,
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
    pub fn with_portal(mut self, portal: suzatary_api::portal::Portal) -> Self {
        self.building().portal = Arc::new(portal);
        self
    }

    #[must_use]
    pub fn with_wall(mut self, wall: fn() -> chrono::DateTime<chrono::Utc>) -> Self {
        self.building().wall = wall;
        self
    }

    #[must_use]
    pub fn with_keys(mut self, keys: Option<suzatary_core::signatures::BoxKeys>) -> Self {
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
    pub fn with_repos(mut self, repos: suzatary_core::repos::Declared) -> Self {
        self.building().repos = Arc::new(repos);
        self
    }

    #[must_use]
    pub fn with_plugins(mut self, plugins: crate::instance::Plugins) -> Self {
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
    pub fn with_reach(mut self, reach: suzatary_core::connect::Reach) -> Self {
        self.building().reach = Arc::new(reach);
        self
    }

    #[must_use]
    pub fn with_checks(mut self, checks: checks::Checks) -> Self {
        self.building().checks = checks;
        self
    }

    #[must_use]
    pub fn with_reveal(
        mut self,
        reveal: Arc<dyn suzatary_core::held_value::Reveal + Send + Sync>,
    ) -> Self {
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
    pub fn settables(&self) -> suzatary_core::settings::Settables {
        locks::read(&self.live).settables.clone()
    }

    /// The instance file's declarations, read once.
    #[must_use]
    pub fn live(&self) -> live::Live {
        locks::read(&self.live).clone()
    }

    /// Swap a reread [`live::Live`] in whole under the lock; a failure changes nothing.
    pub fn reconnected(&self, who: &str) {
        let mut live = locks::write(&self.live);
        let read = self.edits.declared().and_then(|declared| {
            let accounts = suzatary_core::mail::account::Roster::new(declared.accounts.clone())
                .map_err(|refused| {
                    suzatary_core::settings::EditError::Malformed(format!(
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
            settables: suzatary_core::settings::Settables::of_deployment(
                suzatary_io::model::TOKEN,
                declared.channels.iter().copied(),
                self.deployment.coding.as_ref(),
                (&declared.plugins, &declared.accounts),
                &self.repos,
            ),
            channels: suzatary_core::nonempty::NonEmpty::of(declared.channels),
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
    pub fn secrets<'a>(
        &'a self,
        caps: &'a suzatary_api::server::Server<'_>,
    ) -> suzatary_core::secrets::Remembering<'a> {
        suzatary_core::secrets::Remembering {
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
    pub fn with_titler(
        mut self,
        model: Arc<dyn suzatary_core::secretary::model::Sessions + Send + Sync>,
    ) -> Self {
        self.building().titler = Some(model);
        self
    }

    #[must_use]
    pub fn with_uploads(
        mut self,
        store: suzatary_io::attachments::AttachmentStore,
        vault: Arc<dyn suzatary_core::vault::Vault>,
    ) -> Self {
        self.building().uploads = Some(Arc::new(Uploads { store, vault }));
        self
    }

    #[must_use]
    pub fn with_compactor(mut self, compactor: compaction::Compactor) -> Self {
        self.building().compactor = Some(Arc::new(compactor));
        self
    }

    /// Nudge this conversation's task and the compactor.
    pub fn heard(&self, id: &suzatary_base::conversation_id::ConversationId) {
        self.workers.nudge(id, Arc::new(self.clone()));
        if let Some(compactor) = &self.compactor {
            compactor.nudge();
        }
    }

    /// File owed cards, withdraw the rest; failures are logged.
    pub fn notice(&self) {
        let heard = locks::held(&self.noticing);
        let live::Live {
            settables,
            channels: _,
            accounts: _,
            plugins: _,
        } = self.live();
        let answered = match self.answered.healths() {
            Ok(answered) => answered,
            Err(failed) => {
                tracing::warn!(
                    "noticed: the model health: {}",
                    suzatary_core::excerpt::Excerpt::of(&failed.to_string())
                );
                return;
            }
        };
        let open = self.signing.open();
        let caps = self.first.caps;
        let declared = self
            .edits
            .declared()
            .inspect_err(|failed| tracing::warn!("noticed: the instance file: {failed}"))
            .ok();
        let health = caps
            .secretary
            .health
            .healths()
            .inspect_err(|failed| tracing::warn!("noticed: the sources' health: {failed}"))
            .ok();
        let now = prodrome_core::fpl::instant_of(caps.secretary.clock.now());
        let model = self.models.recorded();
        locks::held(&self.secrets_answered).forget();
        let secrets = self.secrets(caps);
        let seen = suzatary_core::secretary::noticed::Seen {
            settables: &settables,
            secrets: &secrets,
            model: &model,
            answered: &answered,
            open: &open,
            declared: declared.as_ref(),
            health: health.as_ref(),
            heard: heard.as_ref(),
            pushed: &settings::pushed(self),
            now,
            redirect: suzatary_core::secretary::google_sign_in::Consents::redirect(
                self.consenting.as_ref(),
            ),
        };
        match suzatary_core::secretary::noticed::notice(
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
            match suzatary_api::secretary::routes::withdraw_trusted(
                caps.secretary.inbox,
                caps.secretary.chain,
                &keys.root().public(),
                caps.secretary.clock.now(),
            ) {
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
    pub fn every_chain(&self) -> Vec<suzatary_core::secretary::Bound<'_>> {
        self.chains
            .iter()
            .map(|(name, chained)| suzatary_core::secretary::Bound {
                name,
                caps: chained.caps.secretary,
                policy: &chained.policy,
            })
            .collect()
    }

    /// Runs secretary work with the cards built.
    pub fn with_cards<T>(
        &self,
        work: impl FnOnce(
            &Server<'_>,
            &suzatary_api::secretary::routes::Cards<'_>,
        ) -> Result<T, Refusal>,
    ) -> Result<T, Refusal> {
        let chains = self.every_chain();
        let declared = self
            .edits
            .declared()
            .inspect_err(|failed| tracing::warn!("cards: the instance file: {failed}"))
            .ok();
        let settables = self.settables();
        let cards = suzatary_api::secretary::routes::Cards {
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
    pub fn chains_read(&self) -> suzatary_core::chain_set::Read {
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
    pub door: suzatary_api::gate::Door,
    pub dist: PathBuf,
    pub live: live::Shared,
    /// Names a peer, loopback included.
    pub peers: Peers,
    pub brain: Brain,
    pub titler: Brain,
    /// Off under every fixture.
    pub compacting: Compacting,
    pub postman: crate::commands::secretary::Postman,
    pub acting: Performing,
    /// Shared with the performer for a tap's announcements.
    pub workers: Arc<Workers>,
    pub stamp: Stamp,
    pub signing: Arc<sign_ins::SignIns>,
    pub verify: Arc<dyn suzatary_core::settings::Verify + Send + Sync>,
    /// Rings `moved` when a reading moves.
    pub answered: suzatary_io::model_health::Answered,
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
pub type Performing = Box<
    dyn FnOnce(
            crate::commands::secretary::ActingStores,
            Arc<Workers>,
        ) -> crate::commands::secretary::Acting
        + Send,
>;

pub async fn listen(instance: &Instance, serving: Serving) -> anyhow::Result<()> {
    use suzatary_io::{epochs, frontiers};
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
    if let Err(failed) = world.begin_topics() {
        tracing::warn!("serve: the topic stores are not begun: {failed}");
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
            requests: Arc::new(suzatary_io::requests::Requests::open(
                instance.requests(),
                instance.paths.answers.clone(),
            )),
            asks: Arc::new(suzatary_io::objects::Objects::open(instance.asks())),
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
            edits: Arc::new(crate::instance::InstanceEdits::new(instance.path.clone())),
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
        suzatary_io::google::Google::new(serving.google),
        Arc::new(instance.secrets()),
        Arc::new(crate::instance::InstanceEdits::new(instance.path.clone())),
        &instance.deployment.app_url,
    ));
    let state = compaction::started(state, instance, serving.compacting);

    for chain in std::iter::once(&first).chain(&rest) {
        watch::warm_now(chain.caps, &chain.policy);
    }
    reconcile_orphaned_acts(&first).await;
    erase_at_startup(&first).await;
    open_default(&first).await;
    notice_from_now_on(state.clone(), serving.moved).await;
    resume_waiting(&state).await;
    tokio::spawn(instance_watched(state.clone(), instance.path.clone()));
    watch_from_now_on(instance, stores, &state, &first, rest);

    let router = app(state, &serving.dist);
    let service = router.into_make_service_with_connect_info::<SocketAddr>();

    serve_on(serving.listening, service).await
}

async fn serve_on(
    listening: listening::Listening,
    service: axum::extract::connect_info::IntoMakeServiceWithConnectInfo<Router, SocketAddr>,
) -> anyhow::Result<()> {
    let address = listening.at();
    // Bound before the line a caller waits on says so.
    let listener =
        std::net::TcpListener::bind(address).with_context(|| format!("binding {address}"))?;
    listener.set_nonblocking(true)?;
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

/// Refused for plain http anywhere but loopback, and for a fixture flag behind a certificate.
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
    #[cfg(feature = "fixture")]
    let secure = tls.is_some();
    let (listening, door) = listening_of(instance, args.host, args.port, tls)?;
    #[cfg(feature = "fixture")]
    let stamp = stamp_of(build_fixture, secure)?;
    #[cfg(not(feature = "fixture"))]
    let stamp = Stamp::Compiled;
    #[cfg(feature = "fixture")]
    let google = match google_fixture {
        None => Endpoints::google(),
        // A fixture behind a real certificate is reachable.
        Some(_) if secure => {
            anyhow::bail!("--google-fixture is test-only and refuses to run with a certificate")
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
            if secure {
                anyhow::bail!("--whois-fixture is test-only and refuses to run with a certificate");
            }
            tracing::warn!("serve: --whois-fixture {login} — every peer is this login");
            Peers::Fixture(suzatary_base::capability::Login::from_whois(login))
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
    let (brain, postman) =
        fixture_talking(instance, &answered, (model_fixture, smtp_fixture), secure)?;
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
) -> anyhow::Result<(listening::Listening, suzatary_api::gate::Door)> {
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

/// Fixtures only; never behind a certificate.
#[cfg(feature = "fixture")]
fn fixture_talking(
    instance: &Instance,
    answered: &suzatary_io::model_health::Answered,
    (model_fixture, smtp_fixture): (Option<PathBuf>, Option<PathBuf>),
    certified: bool,
) -> anyhow::Result<(Brain, crate::commands::secretary::Postman)> {
    Ok(match (model_fixture, smtp_fixture) {
        (None, None) => (real_brain(instance, answered), real_postman(instance)),
        (script, recording) => {
            if certified {
                anyhow::bail!("the model and smtp fixtures refuse to run with a certificate");
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
                        crate::commands::secretary::Postman::Recording(path)
                    }
                },
            )
        }
    })
}

fn performers(
    instance: &Instance,
    answered: &suzatary_io::model_health::Answered,
    moved: &Arc<tokio::sync::Notify>,
    #[cfg(feature = "fixture")] (scripted, google): (bool, &Endpoints),
) -> (
    Performing,
    Arc<sign_ins::SignIns>,
    Arc<dyn suzatary_core::settings::Verify + Send + Sync>,
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

/// `--build-fixture`; refused behind a certificate.
#[cfg(feature = "fixture")]
fn stamp_of(build_fixture: Option<PathBuf>, tls: bool) -> anyhow::Result<Stamp> {
    let Some(path) = build_fixture else {
        return Ok(Stamp::Compiled);
    };
    if tls {
        anyhow::bail!("--build-fixture is test-only and refuses to run with a certificate");
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

fn real_brain(instance: &Instance, answered: &suzatary_io::model_health::Answered) -> Brain {
    interpreters::brain(instance, answered.clone())
}

fn real_postman(instance: &Instance) -> crate::commands::secretary::Postman {
    crate::commands::secretary::Postman::Real(Arc::new(instance.clone()))
}

/// Rings `moved` when a device flow ends.
fn real_acting(
    instance: &Instance,
    signing: &Arc<sign_ins::SignIns>,
    moved: &Arc<tokio::sync::Notify>,
) -> Performing {
    let (instance, signing, moved) = (instance.clone(), Arc::clone(signing), Arc::clone(moved));
    Box::new(move |stores, workers| {
        crate::commands::secretary::Acting::real(
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
    Box::new(move |stores, workers| {
        crate::commands::secretary::Acting::recording(&instance, stores, workers, signing, &google)
    })
}

/// Open the default conversation; no read writes it.
async fn open_default(chain: &Chained) {
    let caps = chain.caps;
    let now = caps.secretary.clock.now();
    match tokio::task::spawn_blocking(move || {
        suzatary_core::secretary::conversations::read_or_open_default(
            caps.secretary.conversations,
            caps.secretary.model,
            &suzatary_base::conversation_id::DEFAULT,
            now,
        )
    })
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(failed)) => {
            tracing::warn!("secretary: the default conversation: {}", failed.message());
        }
        Err(join_error) => {
            tracing::warn!(
                "secretary: opening the default conversation did not finish: {join_error}"
            );
        }
    }
}

/// Only the first chain pushes.
fn watch_from_now_on(
    instance: &Instance,
    stores: crate::commands::secretary::ActingStores,
    state: &State,
    first: &Chained,
    rest: Vec<Chained>,
) {
    crate::commands::coding::spawn_polling(instance, stores.conversations, &state.workers);
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
pub type Heard = std::collections::BTreeSet<suzatary_core::calendar::CalendarName>;

/// `None`, logged, on a failed read.
fn heard(
    calendar: &dyn suzatary_core::calendar::effect::Calendar,
    now: suzatary_core::health::Instant,
) -> Option<Heard> {
    use suzatary_core::secretary::failing::{QUIET, quiet};
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

/// File owed cards now, hourly and on `moved`.
async fn notice_from_now_on(state: State, moved: Arc<tokio::sync::Notify>) {
    let first = state.clone();
    if let Err(join_error) = tokio::task::spawn_blocking(move || first.notice()).await {
        tracing::warn!("noticed: the first filing did not finish: {join_error}");
    }
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
    tokio::spawn(async move {
        // The first tick is at once.
        let mut every = tokio::time::interval(Duration::from_hours(1));
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

async fn resume_waiting(state: &State) {
    let caps = state.first.caps;
    let answered = match state.answered.healths() {
        Ok(answered) => answered,
        Err(failed) => {
            tracing::warn!(
                "secretary: the model health did not read, so nothing is resumed: {failed}"
            );
            return;
        }
    };
    let model = state.models.recorded();
    match tokio::task::spawn_blocking(move || {
        suzatary_core::secretary::transcript::waiting(
            caps.secretary.conversations,
            suzatary_core::health::reading_of(&answered, model.as_str()).as_ref(),
            caps.secretary.clock.now(),
        )
    })
    .await
    {
        Ok(Ok(waiting)) => {
            if !waiting.is_empty() {
                tracing::info!(
                    "secretary: resuming {} unanswered conversation(s)",
                    waiting.len()
                );
            }
            for id in &waiting {
                state.heard(id);
            }
        }
        Ok(Err(failed)) => tracing::warn!("secretary: unanswered conversations: {failed}"),
        Err(join_error) => {
            tracing::warn!(
                "secretary: reading unanswered conversations did not finish: {join_error}"
            );
        }
    }
}

async fn reconcile_orphaned_acts(chain: &Chained) {
    let caps = chain.caps;
    let now = caps.secretary.clock.now();
    match tokio::task::spawn_blocking(move || {
        suzatary_core::secretary::act_lines::reconcile(
            caps.secretary.conversations,
            caps.secretary.inbox,
            now,
        )
    })
    .await
    {
        Ok(Ok(reconciled)) if reconciled > 0 => {
            tracing::info!("secretary: reconciled {reconciled} orphaned act(s) at startup");
        }
        Ok(Ok(_)) => {}
        Ok(Err(failed)) => {
            tracing::warn!("secretary: reconcile at startup: {}", failed.message());
        }
        Err(join_error) => {
            tracing::warn!("secretary: reconcile at startup did not finish: {join_error}");
        }
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

/// Each step reads only stored state, so a restart resumes it.
async fn erase_at_startup(chain: &Chained) {
    let caps = chain.caps;
    let done = tokio::task::spawn_blocking(move || {
        // In order, so a key the sweep kept keeps no summary that read its words.
        let erased = caps
            .secretary
            .memories
            .hold()
            .and_then(|held| held.erase_forgotten());
        let lines = suzatary_core::secretary::transcript::sweep(caps.secretary.conversations)
            .map_err(Refusal::from);
        let texts = caps.secretary.memories.sweep();
        let indexed = suzatary_core::secretary::file_search::reindex(
            caps.secretary.conversations,
            caps.secretary.upload_index,
        );
        [
            ("erased by forgetting", erased),
            ("keys destroyed", lines),
            ("texts deleted", texts),
            ("uploads in the search index", indexed),
        ]
    })
    .await;
    match done {
        Ok(steps) => {
            for (what, step) in steps {
                match step {
                    Ok(0) => {}
                    Ok(n) => tracing::info!("secretary: at startup, {n} {what}"),
                    Err(failed) => {
                        tracing::warn!(
                            "secretary: at startup, {what} failed: {}",
                            failed.message()
                        );
                    }
                }
            }
        }
        Err(join_error) => {
            tracing::warn!("secretary: erasing at startup did not finish: {join_error}");
        }
    }
}

pub fn run(path: &Path, args: Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
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
