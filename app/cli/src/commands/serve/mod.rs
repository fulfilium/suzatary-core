//! `suzatary serve`: the app's HTTP surface over axum. Route logic lives in
//! `suzatary_core`; `table` lists every route and `route` mounts each one.
//!
//! It sends no CORS header, and `gate` asks the local daemon (whois) who owns each
//! peer, refusing with 403 any it cannot name.
//! It authors no markup: the only HTML it serves is `dist`'s files and typst's
//! rendering of content a trusted actor wrote.

mod asked;
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
mod proposals;
mod reply;
mod route;
mod secretary;
mod settings;
pub mod sign_ins;
mod stores;
mod stream;
pub mod table;
mod the_box;
mod validated;
mod versioned;
mod watch;
mod watched;
pub mod world;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use axum::http::{HeaderValue, header};

use suzatary_core::capability::Server;
use suzatary_core::deployment::Deployment;
use suzatary_core::refusal::{Refusal, shown};
use suzatary_core::secretary::compacting::Compacting;
use tower_http::compression::{CompressionLayer, CompressionLevel};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::instance::Instance;
pub use conversation_worker::Workers;
pub use interpreters::{Brain, Ledger, Peers};
pub use live::Live;
pub use settings::pushed_at;
pub use world::World;

/// The owner's uploads: the store on the box, and his Drive for the ones he keeps private.
/// The vault is here and in no record, so only his confirm on an upload reaches it.
pub struct Uploads {
    /// Photos and PDFs on the box, and the notes of private ones.
    pub store: suzatary_io::attachments::AttachmentStore,
    /// Where a private file goes.
    pub vault: Arc<dyn suzatary_core::vault::Vault>,
}

/// What every handler is handed: one shared [`Inner`], so handing it to a request is one count.
#[derive(Clone)]
pub struct State(Arc<Inner>);

impl std::ops::Deref for State {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

/// What the settings page reads and what its writes swap: the instance file's declarations
/// about secrets, models, channels, accounts and plugins under their one lock, and the writers
/// outside every record.
pub struct Settings {
    /// The settable names, the channels, the accounts, the plugins.
    pub live: live::Shared,
    /// The Claude models the deployment runs.
    pub models: Arc<models::Models>,
    /// What writes a secret.
    pub secrets: Arc<dyn suzatary_core::settings::Writes + Send + Sync>,
    /// What declares `settables` to the store `secrets` writes to.
    pub declares: Arc<dyn suzatary_core::settings::Declares + Send + Sync>,
    /// What declares a new provider in the instance file.
    pub edits: Arc<dyn suzatary_core::settings::Edits + Send + Sync>,
    /// The data repository.
    pub data: Arc<std::path::PathBuf>,
}

/// What a tap reaches, outside every record: the postman and the act performer.
pub struct Performers {
    /// What puts a tapped proposal on a wire.
    pub postman: Arc<dyn suzatary_core::secretary::routes::Post + Send + Sync>,
    /// What performs a tapped act.
    pub acting: Arc<dyn suzatary_core::secretary::routes::Perform + Send + Sync>,
}

/// Everything a handler reads, held once per server.
#[derive(Clone)]
pub struct Inner {
    /// The first chain, which every bare `/api/…` path serves.
    pub first: Chained,
    /// Every chain by name, the first included, as a `/api/<chain>/…` segment looks it up.
    pub chains: Arc<BTreeMap<String, Chained>>,
    /// This deployment.
    pub deployment: Arc<Deployment>,
    /// Where its `app_url` says the app answers, read once when `serve` starts.
    pub door: Arc<suzatary_core::gate::Door>,
    /// What puts a tapped proposal on a wire; outside every record, reached only with a `Tap`.
    pub postman: Arc<dyn suzatary_core::secretary::routes::Post + Send + Sync>,
    /// What performs a tapped act; outside every record for the postman's reason.
    pub acting: Arc<dyn suzatary_core::secretary::routes::Perform + Send + Sync>,
    /// What tailscaled said about each peer, and until when; per server, never global.
    pub known: Arc<std::sync::Mutex<gate::Remembering>>,
    /// Every conversation currently being answered.
    pub workers: Arc<Workers>,
    /// The settable names, the channels, the accounts and the plugins, under the one lock
    /// that, once serving,
    /// [`State::reconnected`] alone writes, so a tap's edit or a hand edit takes effect with no
    /// restart.
    pub live: live::Shared,
    /// The Claude models the deployment runs.
    pub models: Arc<models::Models>,
    /// What writes a secret; outside every record, reached only by a tapped card.
    pub secrets: Arc<dyn suzatary_core::settings::Writes + Send + Sync>,
    /// What declares `settables` to the store, which refuses a name it was never told of;
    /// reached only through [`State::reconnected`].
    pub declares: Arc<dyn suzatary_core::settings::Declares + Send + Sync>,
    /// What edits the instance file (a setting, a connection, an allow rule); outside every
    /// record for `secrets`'s reason, reached only by a tapped card.
    pub edits: Arc<dyn suzatary_core::settings::Edits + Send + Sync>,
    /// What deletes what the box kept for a removed account, its sign-in and, on its card's
    /// wider yes, its mail; outside every record.
    pub forget: Arc<dyn suzatary_core::mail::cache::Forget + Send + Sync>,
    /// What reads a held value back for the owner's tap (`POST /api/held/reveal`); outside every
    /// record, so no verb of the secretary or the agent can.
    pub reveal: Arc<dyn suzatary_core::held_value::Reveal + Send + Sync>,
    /// What syncs a plugin once a tap stored its link; outside every record.
    pub checks: checks::Checks,
    /// How the person reaches the app at a terminal, which a step's command is spelled in.
    pub reach: Arc<suzatary_core::connect::Reach>,
    /// Where the owner's uploaded photos and PDFs go; `None` refuses an upload by name.
    pub uploads: Option<Arc<Uploads>>,
    /// The title job's model; `None` names a topic with the record's own model.
    pub titler: Option<Arc<dyn suzatary_core::capability::Sessions + Send + Sync>>,
    /// The compactor's task, which a moved conversation nudges; `None` while it is off.
    pub compactor: Option<Arc<compaction::Compactor>>,
    /// Who has the app open (see [`Presence`]).
    pub presence: Presence,
    /// Each login's budget of browser error reports
    /// (`suzatary_core::clienterror`), one table per server like `known`.
    pub reports: Arc<std::sync::Mutex<suzatary_core::clienterror::Limiter>>,
    /// Where `GET /api/about`'s build comes from.
    pub stamp: Stamp,
    /// The plugins this box runs and their cache tier, for the settings page's link rows;
    /// `None` declares none.
    pub plugins: Option<Arc<crate::instance::Plugins>>,
    /// The repositories this deployment reaches, whose GitHub tokens it may set.
    pub repos: Arc<suzatary_core::repos::Declared>,
    /// The data repository, which Settings names with where it is pushed.
    pub data: Arc<std::path::PathBuf>,
    /// The chains as the last request read them, so the next reads again only a chain whose
    /// heads moved.
    pub read: Arc<std::sync::Mutex<suzatary_core::chain_set::Read>>,
    /// The Inbox's open connection cards, a tabulation by the inbox's heads, so they are read
    /// again only when the inbox moves.
    pub asking: Arc<
        std::sync::Mutex<
            prodrome_core::memo::Cache<
                Vec<suzatary_core::settings::overview::Asking>,
                prodrome_core::memo::Lru,
            >,
        >,
    >,
    /// The box's signing keys, which only a tapped device card reaches (to register a key, signed
    /// by the root); `None` for a deployment that declares no root.
    pub keys: Option<Arc<suzatary_core::signatures::BoxKeys>>,
    /// Where each request a device signed and the box performed is kept with its answer;
    /// `None` keeps none.
    pub requests: Option<Arc<suzatary_io::requests::Requests>>,
    /// The last code `suzatary device pair` printed, as the box keeps it; `None` pairs no
    /// device.
    pub pairings: Option<Arc<dyn suzatary_core::signatures::Pairings>>,
    /// The requests being performed now, so one sent twice at once is performed once.
    pub performing: asked::InFlight,
    /// Now, in UTC, as a request's window reads it: a device stamps UTC, so the window is never
    /// the box's local clock.
    pub wall: fn() -> chrono::DateTime<chrono::Utc>,
    /// Whether each model entry answered its calls: the register the box reads to know whether
    /// any model answers.
    pub answered: Arc<dyn suzatary_core::health::Register<String> + Send + Sync>,
    /// The sign-ins the box holds open; outside every record, reached only with a `Tap`.
    pub signing: Arc<dyn suzatary_core::secretary::sign_in::Sessions + Send + Sync>,
    /// The one test call before a model's credential is kept; outside every record likewise.
    pub verify: Arc<dyn suzatary_core::settings::Verify + Send + Sync>,
    /// The sign-ins with Google the box holds open; outside every record, begun only with a
    /// `Tap` and finished only by the callback that names one.
    pub consenting: Arc<google::Consenting>,
    /// Held across one filing: two filings at once would each read the inbox before the other
    /// wrote, and file the same card twice. It guards the calendars last heard, which only the
    /// hourly pass reads again (`hear`): nothing a tap does moves an occurrence, so a tap's
    /// filing reads the calendars' stores not at all; `None` until the first hearing, which
    /// holds the quiet calendars' cards.
    pub noticing: Arc<std::sync::Mutex<Option<Heard>>>,
    /// Each store's epoch, which every objects answer carries.
    pub epochs: Arc<suzatary_core::epoch::Epochs>,
    /// Each store's recorded frontiers, which every pull names and every answer records.
    pub frontiers: Arc<prodrome_core::store::frontier::Frontiers>,
    /// What `GET /api/portal` answers, read from `[notify]` when the server starts.
    pub portal: Arc<suzatary_core::portal::Portal>,
}

/// A store that holds no secret: a test's server keeps no Google client until it is given one.
pub struct Unresolved;

impl suzatary_io::secrets::Resolves for Unresolved {
    fn get(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<suzatary_core::capability::Secret>, suzatary_io::secrets::SecretError> {
        Ok(None)
    }
}

/// No sign-in is held open: a server that drives none, and every test's default.
pub struct Closed;

impl suzatary_core::secretary::sign_in::Sessions for Closed {
    fn open(&self) -> Vec<suzatary_core::secretary::sign_in::SignInUrl> {
        Vec::new()
    }

    fn finish(
        &self,
        _: &suzatary_core::secretary::sign_in::SignInUrl,
        _: &suzatary_core::capability::Secret,
    ) -> Option<(
        suzatary_core::secretary::inbox::ProposalId,
        Result<suzatary_core::capability::Secret, String>,
    )> {
        None
    }
}

/// Keeps no credential of a model: what a server not handed a verifier answers, so a key is
/// never kept untested.
pub struct Unverified;

impl suzatary_core::settings::Verify for Unverified {
    fn verify(
        &self,
        name: &suzatary_core::settings::Settable,
        _: &suzatary_core::capability::Secret,
    ) -> Result<(), String> {
        Err(format!(
            "this server tests no credential, so {} is kept by no tap",
            name.name()
        ))
    }
}

/// Where the build `GET /api/about` names comes from.
#[derive(Debug, Clone, Default)]
pub enum Stamp {
    /// What the binary was stamped with ([`crate::stamp`]).
    #[default]
    Compiled,
    /// The file's trimmed contents, read on every request; test only.
    #[cfg(feature = "fixture")]
    File(Arc<PathBuf>),
}

impl Stamp {
    /// The build `GET /api/about` names.
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

    /// What `GET /api/about` answers, each completed todo as the project chain reads it in
    /// `set`.
    ///
    /// # Errors
    /// A release file beside the binary that does not read (which the flake's package refuses
    /// before it ships), or the project chain held and not read.
    pub fn about(
        &self,
        set: &suzatary_core::chain_set::ChainSet<'_>,
    ) -> Result<suzatary_core::wire::AboutPayload, suzatary_core::refusal::Refusal> {
        let internal =
            |e: &dyn std::fmt::Display| suzatary_core::refusal::Refusal::internal(e.to_string());
        let released = match crate::stamp::release().map_err(|e| internal(&e))? {
            crate::stamp::Release::Released(released) => Some(released),
            crate::stamp::Release::Dev => None,
        };
        let plan = suzatary_core::release::Plan::declared().map_err(|e| internal(&e))?;
        suzatary_core::routes::about(released, &self.build(), &plan, set)
    }
}

/// Whether anyone has the app open: a ping heard within [`PRESENT_SECONDS`]; the phone is
/// pushed only when nobody is.
#[derive(Debug, Clone, Default)]
pub struct Presence(Arc<std::sync::Mutex<Option<std::time::Instant>>>);

/// How long a ping counts: two missed pings and a margin.
pub const PRESENT_SECONDS: u64 = 70;

impl Presence {
    /// A page said it is shown, now.
    pub fn seen(&self) {
        *locks::held(&self.0) = Some(std::time::Instant::now());
    }

    /// Whether a page said so within [`PRESENT_SECONDS`].
    #[must_use]
    pub fn anyone(&self) -> bool {
        locks::held(&self.0)
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(PRESENT_SECONDS))
    }
}

/// One chain's effects and its policy, together, so a store is never read under
/// another chain's trust rule.
#[derive(Clone)]
pub struct Chained {
    /// The effects whose `chain` is this chain's store; the rest is shared.
    pub caps: &'static Server<'static>,
    /// Whose events bind here, and which actors this store knows.
    pub policy: Arc<suzatary_core::chains::ChainPolicy>,
}

impl State {
    /// Build a state over one chain and however many others, sharing the given `workers`.
    #[must_use]
    pub fn over(
        first: Chained,
        rest: impl IntoIterator<Item = Chained>,
        deployment: Arc<Deployment>,
        door: suzatary_core::gate::Door,
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
        // A test's server: its model answers, as a scripted model does; `listen` reads the
        // box's own register instead.
        let answered = std::sync::Mutex::new(suzatary_core::health::Healths::one(
            models.recorded(),
            chrono::DateTime::UNIX_EPOCH.naive_utc(),
            suzatary_core::health::Outcome::Succeeded,
            "",
        ));
        let consenting = Arc::new(google::Consenting::new(
            suzatary_io::google::Google::new(suzatary_core::google::Endpoints::google()),
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
            reports: Arc::new(std::sync::Mutex::new(
                suzatary_core::clienterror::Limiter::new(suzatary_core::clienterror::BUDGET),
            )),
            stamp: Stamp::Compiled,
            plugins: None,
            repos: Arc::new(suzatary_core::repos::Declared::default()),
            data,
            read: Arc::default(),
            asking: Arc::new(std::sync::Mutex::new(prodrome_core::memo::Cache::new(
                prodrome_core::memo::Lru::new(4),
            ))),
            keys: None,
            requests: None,
            pairings: None,
            performing: Arc::default(),
            wall: chrono::Utc::now,
            answered: Arc::new(answered),
            signing: Arc::new(Closed),
            verify: Arc::new(Unverified),
            consenting,
            noticing: Arc::default(),
            // A test's server keeps its epochs in memory; `listen` keeps the box's on disk.
            epochs: Arc::new(suzatary_core::epoch::Epochs::remembered()),
            frontiers: Arc::new(prodrome_core::store::frontier::Frontiers::remembered(
                prodrome_core::store::frontier::KEPT,
            )),
            portal: Arc::new(suzatary_core::portal::Portal::Quiet),
            models,
        }))
    }

    /// The state being built, before any request shares it: copied only if it already is.
    fn building(&mut self) -> &mut Inner {
        Arc::make_mut(&mut self.0)
    }

    /// The same state keeping each store's epoch in `epochs` and recording its frontiers in
    /// `frontiers`, both under the state directory.
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

    /// What every pull records beside its answer.
    #[must_use]
    pub fn books(&self) -> suzatary_core::routes::Books<'_> {
        suzatary_core::routes::Books {
            epochs: &self.epochs,
            frontiers: &self.frontiers,
        }
    }

    /// The same state reading whether a model answers from `answered`.
    #[must_use]
    pub fn with_answered(
        mut self,
        answered: Arc<dyn suzatary_core::health::Register<String> + Send + Sync>,
    ) -> Self {
        self.building().answered = answered;
        self
    }

    /// Read again which calendars hold an occurrence within the quiet span, for the filings
    /// from now on; a reading that fails keeps none, which holds the quiet calendars' cards.
    pub fn hear(&self) {
        let caps = self.first.caps;
        let now = prodrome_core::fpl::instant_of(caps.secretary.clock.now());
        let heard = heard(caps.secretary.calendar, now);
        *locks::held(&self.noticing) = heard;
    }

    /// The same state holding its sign-ins in `signing`, and testing a model's credential with
    /// `verify` before a tap keeps it.
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

    /// The same state signing Google accounts in through `consenting`.
    #[must_use]
    pub fn with_google(mut self, consenting: google::Consenting) -> Self {
        self.building().consenting = Arc::new(consenting);
        self
    }

    /// The same state answering the Android portal with `portal`.
    #[must_use]
    pub fn with_portal(mut self, portal: suzatary_core::portal::Portal) -> Self {
        self.building().portal = Arc::new(portal);
        self
    }

    /// The same state reading a request's window against `wall`.
    #[must_use]
    pub fn with_wall(mut self, wall: fn() -> chrono::DateTime<chrono::Utc>) -> Self {
        self.building().wall = wall;
        self
    }

    /// The same state keeping each performed request, with its answer, in `requests`.
    #[must_use]
    pub fn with_requests(mut self, requests: suzatary_io::requests::Requests) -> Self {
        self.building().requests = Some(Arc::new(requests));
        self
    }

    /// The same state trusting a device that types the code kept in `pairings`.
    #[must_use]
    pub fn with_pairings(mut self, pairings: Arc<dyn suzatary_core::signatures::Pairings>) -> Self {
        self.building().pairings = Some(pairings);
        self
    }

    /// The same state signing as `keys` say.
    #[must_use]
    pub fn with_keys(mut self, keys: Option<suzatary_core::signatures::BoxKeys>) -> Self {
        self.building().keys = keys.map(Arc::new);
        self
    }

    /// Every chain's writer, the first first: what a tapped device card registers a key on.
    #[must_use]
    pub fn chain_stores(
        &self,
    ) -> Vec<&'static dyn suzatary_core::store::Prodrome<suzatary_core::record::TodoEvent>> {
        std::iter::once(self.first.caps.secretary.chain)
            .chain(
                self.chains
                    .values()
                    .filter(|chained| chained.policy.name != self.first.policy.name)
                    .map(|chained| chained.caps.secretary.chain),
            )
            .collect()
    }

    /// The same state whose repositories are `repos`, whose tokens it may set.
    #[must_use]
    pub fn with_repos(mut self, repos: suzatary_core::repos::Declared) -> Self {
        self.building().repos = Arc::new(repos);
        self
    }

    /// The same state with these plugins declared, and their cache tier where it is.
    #[must_use]
    pub fn with_plugins(mut self, plugins: crate::instance::Plugins) -> Self {
        locks::write(&self.live)
            .plugins
            .clone_from(&plugins.manifests);
        self.building().plugins = Some(Arc::new(plugins));
        self
    }

    /// The same state keeping no mail and the sign-ins `tokens` keeps, as a test's server does.
    #[must_use]
    pub fn with_grants(mut self, tokens: suzatary_io::oauth::TokenStore) -> Self {
        self.building().forget = Arc::new(mail_cache::Unkept(Some(tokens)));
        self
    }

    /// The same state spelling a terminal's step as `reach` reaches the app.
    #[must_use]
    pub fn with_reach(mut self, reach: suzatary_core::connect::Reach) -> Self {
        self.building().reach = Arc::new(reach);
        self
    }

    /// The same state checking a stored link as `checks` syncs its plugin.
    #[must_use]
    pub fn with_checks(mut self, checks: checks::Checks) -> Self {
        self.building().checks = checks;
        self
    }

    /// The same state reading held values back through `reveal`.
    #[must_use]
    pub fn with_reveal(
        mut self,
        reveal: Arc<dyn suzatary_core::held_value::Reveal + Send + Sync>,
    ) -> Self {
        self.building().reveal = reveal;
        self
    }

    /// The same state deleting a removed account's mail as `cache` keeps it.
    #[must_use]
    pub fn with_mail_cache(mut self, cache: mail_cache::MailCache) -> Self {
        self.building().forget = Arc::new(cache);
        self
    }

    /// The secret names the deployment may set as of now: the one live list every reader takes,
    /// which [`Self::reconnected`] derives again after a tap's edit.
    #[must_use]
    pub fn settables(&self) -> suzatary_core::settings::Settables {
        locks::read(&self.live).settables.clone()
    }

    /// What the instance file declares as of now, read once so its parts agree.
    #[must_use]
    pub fn live(&self) -> live::Live {
        locks::read(&self.live).clone()
    }

    /// `who` moved the instance file (a tap, or a hand edit the watcher saw): everything
    /// [`live::Live`] holds is read from the file again and swapped in whole with no restart, so
    /// a connected calendar's line is drawn and its link asked for at once, and a removed
    /// connection's card is withdrawn. The file is read under the lock, so of two moves the one that swaps last read
    /// the file last. The names are declared to the secret store before the swap, so no card is
    /// filed for a name the store would refuse. A file that does not read, or names the store
    /// will not take, keeps all of it as it was, and says so. Reading a file that did not move
    /// changes nothing.
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

    /// The same state with `/api/about`'s build read from `stamp`.
    #[must_use]
    pub fn with_stamp(mut self, stamp: Stamp) -> Self {
        self.building().stamp = stamp;
        self
    }

    /// The same state with topics named by `model`.
    #[must_use]
    pub fn with_titler(
        mut self,
        model: Arc<dyn suzatary_core::capability::Sessions + Send + Sync>,
    ) -> Self {
        self.building().titler = Some(model);
        self
    }

    /// The same state with uploads kept in `store`, and private ones in `vault`.
    #[must_use]
    pub fn with_uploads(
        mut self,
        store: suzatary_io::attachments::AttachmentStore,
        vault: Arc<dyn suzatary_core::vault::Vault>,
    ) -> Self {
        self.building().uploads = Some(Arc::new(Uploads { store, vault }));
        self
    }

    /// The same state with the compactor's task nudged by every moved conversation.
    #[must_use]
    pub fn with_compactor(mut self, compactor: compaction::Compactor) -> Self {
        self.building().compactor = Some(Arc::new(compactor));
        self
    }

    /// A write moved this conversation's heads: its task tries the conversation's rules on the
    /// new reading, starting if none is running, and the compactor steps again.
    pub fn heard(&self, id: &suzatary_core::secretary::store::ConversationId) {
        self.workers.nudge(id, Arc::new(self.clone()));
        if let Some(compactor) = &self.compactor {
            compactor.nudge();
        }
    }

    /// File what the box owes now with no model asked (a fix card for each declared secret
    /// that does not resolve, the sign-in when no model answers, an open sign-in's code), and
    /// withdraw what it no longer owes; a failure is a log line, never the request's, since the
    /// next write asks again.
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
        let seen = suzatary_core::secretary::noticed::Seen {
            settables: &settables,
            secrets: caps.secretary.secrets,
            model: &model,
            answered: &answered,
            open: &open,
            declared: declared.as_ref(),
            health: health.as_ref(),
            heard: heard.as_ref(),
            pushed: &settings::pushed(self),
            now,
            redirect: self.consenting.redirect(),
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
        // A key trusted while no server ran, or before cards were withdrawn, leaves its card.
        if let Some(keys) = &self.keys {
            match suzatary_core::secretary::routes::withdraw_trusted(
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
                    self.heard(&suzatary_core::secretary::store::DEFAULT);
                }
                Ok(_) => {}
                Err(failed) => {
                    tracing::warn!("noticed: trusted devices' cards: {}", failed.message());
                }
            }
        }
    }

    /// Every chain this server serves, as the secretary binds one.
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

    /// Secretary work with every chain bound and the cards built.
    ///
    /// # Errors
    /// The work's own [`Refusal`].
    pub fn with_cards<T>(
        &self,
        work: impl FnOnce(
            &Server<'_>,
            &suzatary_core::secretary::routes::Cards<'_>,
        ) -> Result<T, Refusal>,
    ) -> Result<T, Refusal> {
        let chains = self.every_chain();
        let declared = self.edits.declared().ok();
        let settables = self.settables();
        let cards = suzatary_core::secretary::routes::Cards {
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

    /// Every chain this server serves, read where its heads moved since the last request and
    /// kept where they did not: the set every row and proposal of one request is priced through.
    #[must_use]
    pub fn chains_read(&self) -> suzatary_core::chain_set::Read {
        let lock = || locks::held(&self.read);
        // Read the stores outside the lock, so one request's disk read holds up no other; two
        // racing requests each read what moved, and the last to finish is kept.
        let last = lock().clone();
        let now = last.moved(
            self.chains
                .values()
                .map(|chained| (chained.policy.as_ref(), chained.caps.secretary.chain)),
        );
        *lock() = now.clone();
        now
    }

    /// The chain a path segment named, or the first when the path had none.
    ///
    /// # Errors
    /// [`Refusal::not_found`] on a name nothing is bound to.
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

/// One log line when `to` moves the connections' tables.
fn logged(from: &live::Live, to: &live::Live, who: &str) {
    if (&from.channels, &from.accounts, &from.plugins) != (&to.channels, &to.accounts, &to.plugins)
    {
        tracing::info!(who, "settings: the connections' tables changed");
    }
}

/// What `suzatary serve` was told.
pub struct Serving {
    /// Where to listen, and whether over https.
    pub listening: listening::Listening,
    /// Where the gate answers: `app_url`'s host, at the port `listening` binds.
    pub door: suzatary_core::gate::Door,
    /// The frontend build.
    pub dist: PathBuf,
    /// What the instance file declares, under the one lock a tap's edit swaps.
    pub live: live::Shared,
    /// Who names a peer, loopback included.
    pub peers: Peers,
    /// Which model answers a conversation.
    pub brain: Brain,
    /// Which model names a topic: `[secretary] model`, as the conversation's.
    pub titler: Brain,
    /// Whether the compactor runs, and with what; off under every fixture, so no test asks a
    /// model.
    pub compacting: Compacting,
    /// What puts a tapped proposal on a wire.
    pub postman: crate::commands::secretary::Postman,
    /// What performs a tapped act.
    pub acting: crate::commands::secretary::Acting,
    /// Where the usage page reads its transcripts from.
    pub ledger: interpreters::Ledger,
    /// Where `GET /api/about`'s build comes from.
    pub stamp: Stamp,
    /// The sign-ins the performer begins and the code cards finish.
    pub signing: Arc<sign_ins::SignIns>,
    /// The one test call before a model's credential is kept.
    pub verify: Arc<dyn suzatary_core::settings::Verify + Send + Sync>,
    /// Whether each model answered, ringing `moved` when a reading moves.
    pub answered: suzatary_io::model_health::Answered,
    /// Rung when a model starts or stops answering, so the box files at once.
    pub moved: Arc<tokio::sync::Notify>,
    /// Where a sign-in with Google and a Google calendar are asked: Google's own, or the
    /// fixture's fake.
    pub google: suzatary_core::google::Endpoints,
}

/// The only page this server authors, as plain text.
const NOT_BUILT: &str = "suzatary serve — the API is up, the frontend is not built.

The JSON API is live at /api/chain, /api/fold, /api/todo/<id>, /api/tree,
/api/mail/inbox, /api/mail/search, /api/mail/message/<id>,
/api/mail/thread/<id>, /api/briefings, /api/briefings/<date> and /events.

To build the page this server would otherwise hand you:

    scripts/build-web.sh

It writes web/dist. Restart `suzatary serve` afterwards — the static mount is
decided when the server starts.
";

/// The routes, flat so the table reads against the path strings.
#[allow(clippy::too_many_lines)]
pub fn app(state: State, dist: &std::path::Path) -> Router {
    let api = table::table()
        .into_iter()
        .fold(Router::new(), |api, route| route.mount(api));

    // No SPA fallback: routes live in the fragment, so one would only answer 200 to typos.
    // The frontend is compressed when it is built (`scripts/build-web.sh`), at a quality no
    // request could wait for, so it is served as its `.br` or as itself, never compressed here.
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

    // Every answer of the API is the owner's mail or todos, so none is kept by a browser's cache,
    // but an object's literal asked for by its name, which never changes and is marked again
    // outside this layer (`reply::kept`); the page keeps each other read's last answer itself and
    // asks with its tag (`validated`). Brotli
    // where the request accepts it, gzip otherwise, each at its fastest: on a private tailnet
    // bytes cost less than compression time, and brotli's fastest still beats gzip's on JSON.
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
        // A response with a stricter policy of its own (an attachment's sandbox) keeps it.
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(POLICY),
        ))
        // A page of another origin that opened this one keeps no handle on it, so it cannot post
        // a message that reads as the Android app's (`web/src/app/pushtoken.ts`).
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("cross-origin-opener-policy"),
            HeaderValue::from_static("same-origin"),
        ))
        .with_state(state)
}

/// What a page this server answers may load and run: its own origin's scripts, workers and
/// WebAssembly, no inline script and nothing from elsewhere, no plugin, no `<base>`, no form, and
/// no frame around it. Behind the tripwire and the href door, a script that slipped both still
/// does not run. An inline `style` attribute is the one inline thing allowed: typst's HTML lays
/// its boxes out with them, and an attribute's CSS reaches no other origin under `default-src`.
pub const POLICY: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; \
    style-src-attr 'unsafe-inline'; worker-src 'self'; object-src 'none'; base-uri 'none'; \
    form-action 'none'; frame-ancestors 'none'";

/// Bind and run.
pub async fn listen(instance: &Instance, serving: Serving) -> anyhow::Result<()> {
    use suzatary_io::{epochs, frontiers};
    // The names the server starts with, declared before any store is asked, so a server that
    // cannot write their file does not start.
    instance
        .declarations
        .opened()
        .context("declaring this instance's secret names")?;
    // Held for the life of the process, so every record borrows them for 'static. Keys not at
    // hand do not stop the server: it reads, and every write says why it cannot be made.
    let signers = instance.signers();
    // The server adopts each chain as it starts; a chain that does not is said, and the server
    // still reads, since a refused write names why it was refused.
    if let Err(failed) = instance.adopt(&signers).whole() {
        tracing::warn!("{failed:#}");
    }
    let world: &'static World = Box::leak(Box::new(World::assemble(
        instance,
        signers,
        serving.peers,
        serving.brain,
        serving.ledger,
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
    // Share the acting's table, so a tap's announcements reach this state's subscribers.
    let workers = serving
        .acting
        .workers()
        .unwrap_or_else(|| Arc::new(Workers::new()));
    let state = State::over(
        first.clone(),
        rest.clone(),
        Arc::new(instance.deployment.clone()),
        serving.door,
        Performers {
            postman: Arc::new(serving.postman),
            acting: Arc::new(serving.acting),
        },
        workers,
        Settings {
            // Settable names come from the instance file, never `secretspec.toml`, which also
            // declares secrets the app may not set.
            live: Arc::clone(&serving.live),
            models: Arc::new(models::Models::of(instance)),
            // The same store `secret_resolves` reads, so a write and its check agree.
            secrets: Arc::new(instance.secrets()),
            // The names `instance.settables` holds, declared above; every later move goes
            // through `State::reconnected`.
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
    .with_requests(suzatary_io::requests::Requests::open(
        instance.requests(),
        instance.paths.answers.clone(),
    ))
    .with_pairings(Arc::new(suzatary_io::pairing::Pairings::at(
        instance.paths.pairing.clone(),
    )))
    .with_plugins(instance.plugins.clone())
    .with_reach(instance.reach())
    .with_repos(instance.repos.clone())
    .with_portal(instance.portal.clone())
    .with_mail_cache(mail_cache::MailCache::of(instance))
    .with_reveal(Arc::new(crate::commands::mail::notmuch(instance)))
    .with_checks(Arc::new(checks::Synced::of(instance.path.clone())))
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

    // Warmed here rather than in `app`, which every test calls.
    for chain in std::iter::once(&first).chain(&rest) {
        watch::warm_now(chain.caps, &chain.policy);
    }
    warm_usage(&first);
    // Before the watcher: at startup no task owns a device flow, so any standing act
    // was orphaned by this restart.
    reconcile_orphaned_acts(&first).await;
    erase_at_startup(&first).await;
    open_default(&first).await;
    notice_from_now_on(state.clone(), serving.moved).await;
    resume_waiting(&state).await;
    tokio::spawn(instance_watched(state.clone(), instance.path.clone()));
    watch_from_now_on(instance, &state, &first, rest);

    let router = app(state, &serving.dist);
    let service = router.into_make_service_with_connect_info::<SocketAddr>();

    serve_on(serving.listening, service).await
}

/// Serve `service` where `listening` says, over the protocol it says.
async fn serve_on(
    listening: listening::Listening,
    service: axum::extract::connect_info::IntoMakeServiceWithConnectInfo<Router, SocketAddr>,
) -> anyhow::Result<()> {
    let address = listening.at();
    match listening {
        listening::Listening::Loopback(_) => {
            tracing::info!(
                "serve: http://{address} (reads GET, writes POST /api/act/*, gated by whois)"
            );
            axum_server::bind(address)
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
            axum_server::bind_rustls(address, config)
                .map(|tls| tls.acceptor(axum_server::accept::NoDelayAcceptor::new()))
                .serve(service)
                .await
                .context("serving")
        }
    }
}

/// The one page this server authors, when there is no frontend to serve.
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
    /// The built frontend directory, in place of the instance file's `dist`, so an
    /// immutable store path is served rather than a checkout a dev build overwrites.
    #[arg(long)]
    dist: Option<PathBuf>,
    /// Test only, compiled out of the shipped binary: answer every whois with this
    /// login.
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
    /// Test only: ask the fake Google at this base URL, for its sign-in and its calendars,
    /// instead of Google's own.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "URL")]
    google_fixture: Option<String>,
}

/// The certificate pair the flags name, or else the one in `tls_dir`; clap already refuses half
/// a pair.
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

/// Build what `serve` was told, out of the flags and the instance file.
///
/// Refused for plain http anywhere but loopback, and under the `fixture` feature for its flags
/// behind a certificate.
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
        None => suzatary_core::google::Endpoints::google(),
        // Test only: a fixture behind a real certificate is one somebody could reach.
        Some(_) if secure => {
            anyhow::bail!("--google-fixture is test-only and refuses to run with a certificate")
        }
        Some(base) => suzatary_core::google::Endpoints::at(&base),
    };
    #[cfg(not(feature = "fixture"))]
    let google = suzatary_core::google::Endpoints::google();
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
            // Test only: a fixture behind a real certificate is one somebody could reach.
            if secure {
                anyhow::bail!("--whois-fixture is test-only and refuses to run with a certificate");
            }
            tracing::warn!("serve: --whois-fixture {login} — every peer is this login");
            Peers::Fixture(suzatary_core::capability::Login::from_whois(login))
        }
    };
    #[cfg(not(feature = "fixture"))]
    let peers = daemon_peers(&instance.paths.whois_socket);

    // A scripted model also swaps in the recording performer, so a browser test never
    // reaches Microsoft; read before the match below moves `model_fixture`.
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
    // Any fixture reads no transcripts (a cold walk of the box's saturates the blocking pool
    // and times out browser scenes) and asks no compactor.
    #[cfg(feature = "fixture")]
    let fixture = any_fixture || scripted;
    #[cfg(not(feature = "fixture"))]
    let fixture = false;
    let (ledger, compacting) = quiet(instance, fixture);

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
        ledger,
        stamp,
        signing,
        verify,
        answered,
        moved,
        google,
    })
}

/// Where `serve` listens, the flags over the instance file, and where its gate answers.
fn listening_of(
    instance: &Instance,
    host: Option<String>,
    port: Option<u16>,
    tls: Option<(PathBuf, PathBuf)>,
) -> anyhow::Result<(listening::Listening, suzatary_core::gate::Door)> {
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

/// Where the usage page reads and whether the compactor runs: nowhere and off under a fixture.
fn quiet(instance: &Instance, fixture: bool) -> (interpreters::Ledger, Compacting) {
    if fixture {
        (interpreters::Ledger::nowhere(), Compacting::Off)
    } else {
        (interpreters::Ledger::Box, instance.secretary.compacting())
    }
}

/// The secretary's model and postman under the fixtures: scripted replies and a recording
/// postman where the flags name them, the real ones otherwise; never behind a certificate.
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

/// What performs a tapped act and tests a model's credential, sharing one table of sign-ins: the
/// fixture's recording and scripted pair under a model fixture, the box's own otherwise.
fn performers(
    instance: &Instance,
    answered: &suzatary_io::model_health::Answered,
    moved: &Arc<tokio::sync::Notify>,
    #[cfg(feature = "fixture")] (scripted, google): (bool, &suzatary_core::google::Endpoints),
) -> (
    crate::commands::secretary::Acting,
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
        real_acting(instance, &signing),
        Arc::clone(&signing),
        Arc::new(interpreters::Verifying::Real {
            instance: Arc::new(instance.clone()),
            answered: answered(),
        }),
    )
}

/// `--build-fixture`, resolved: test-only, and refused behind a certificate
/// like every other fixture.
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

/// The `claude` CLI at `[secretary] model`, its calls joined into `answered`: the secretary's
/// model and the one that names a topic.
fn real_brain(instance: &Instance, answered: &suzatary_io::model_health::Answered) -> Brain {
    interpreters::brain(instance, answered.clone())
}

fn real_postman(instance: &Instance) -> crate::commands::secretary::Postman {
    crate::commands::secretary::Postman::Real(Arc::new(instance.clone()))
}

/// The device flow, for real, over a fresh [`Workers`] table `listen` reads back.
fn real_acting(
    instance: &Instance,
    signing: &Arc<sign_ins::SignIns>,
) -> crate::commands::secretary::Acting {
    crate::commands::secretary::Acting::real(
        instance,
        Arc::new(Workers::new()),
        Arc::clone(signing),
    )
}

/// The fixture's recording device flow, so a browser suite never dials Microsoft.
#[cfg(feature = "fixture")]
fn recording_acting(
    instance: &Instance,
    signing: &Arc<sign_ins::SignIns>,
    google: &suzatary_core::google::Endpoints,
) -> crate::commands::secretary::Acting {
    crate::commands::secretary::Acting::recording(
        instance,
        Arc::new(Workers::new()),
        Arc::clone(signing),
        google,
    )
}

/// Warm the usage page's transcript cache in a blocking task, since a cold first view
/// takes tens of seconds; the cache is shared, so the first chain warms it for all.
fn warm_usage(first: &Chained) {
    let caps = first.caps;
    tokio::task::spawn_blocking(move || {
        let began = std::time::Instant::now();
        match caps.usage.warm_transcripts() {
            Ok(files) => tracing::info!(
                "warmed {files} transcript(s) for the usage page in {:.2}s",
                began.elapsed().as_secs_f64()
            ),
            Err(failed) => tracing::warn!("usage warm: {failed}"),
        }
    });
}

/// Open the default conversation if nobody has, since a tab reads it from its replica and no
/// read writes; a failure is logged, and the next act noted there opens it instead.
async fn open_default(chain: &Chained) {
    let caps = chain.caps;
    let now = caps.secretary.clock.now();
    match tokio::task::spawn_blocking(move || {
        suzatary_core::secretary::read_or_open_default(
            caps.secretary.conversations,
            caps.secretary.model,
            &suzatary_core::secretary::store::DEFAULT,
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

/// The coding poll and each chain's watcher; only the first chain pushes, the others are warmed
/// but notify nobody.
fn watch_from_now_on(instance: &Instance, state: &State, first: &Chained, rest: Vec<Chained>) {
    crate::commands::coding::spawn_polling(instance, &state.workers);
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

/// The calendars holding an occurrence within the quiet span of an instant, before or after it.
pub type Heard = std::collections::BTreeSet<suzatary_core::calendar::CalendarName>;

/// The calendars holding an occurrence within the quiet span of `now`, before or after it;
/// `None` when the calendars could not be read, which is said.
fn heard(
    calendar: &dyn suzatary_core::capability::Calendar,
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

/// File what the box owes before the first request, so a fresh deployment's missing secrets are
/// cards before anyone opens Settings, and once an hour from then on: a secret can stop
/// resolving, a sign-in lapse and a model stop answering with no request to say so; and at
/// once whenever `moved` rings (a model starts or stops answering, a sign-in lapses). The
/// calendars are heard before each hourly filing, the first at once and beside the server,
/// never before it listens; the Drive's probe is the source timer's (`suzatary plugin sync`).
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
        let mut every = tokio::time::interval(std::time::Duration::from_hours(1));
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

/// How often the server reads the instance file to see whether it moved.
const INSTANCE_POLL: std::time::Duration = std::time::Duration::from_secs(1);

/// Read the instance file at `path` every second, and whenever its bytes moved (a
/// calendar, an account or a Drive folder added by hand), read it again through the one
/// parser ([`State::reconnected`]) and file what the box owes for it, so the settings page and
/// the Inbox follow the edit with no restart. A tap's own edit is reconnected by the tap, so
/// seeing it here changes nothing. A file that cannot be read for a moment (an editor's
/// rename) is read again on the next tick.
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

/// Answer what a restart left unanswered (`transcript::waiting`): each such conversation is
/// nudged, as a write would nudge it. A model health that does not read resumes nothing; the
/// owner's next write asks again. A failure is logged, never fatal.
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
            answered.reading(model.as_str()).as_ref(),
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

/// Close acts orphaned by a restart; a failure is logged, never fatal, since a later
/// restart can try again.
async fn reconcile_orphaned_acts(chain: &Chained) {
    let caps = chain.caps;
    let now = caps.secretary.clock.now();
    match tokio::task::spawn_blocking(move || {
        suzatary_core::secretary::acts::reconcile(
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

/// The listener's TLS over the one provider (`ring`), passed rather than installed process-wide,
/// as every client of this workspace passes it; HTTP/2 offered first.
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

/// Owe what was erased: every forgotten fact's erasures appended, then every key an erasure
/// names destroyed, then the upload index rebuilt to the uploads a live turn names, which takes
/// out an erased upload's words and indexes uploads stored before the index existed. Each is a
/// function of what is stored, so a restart finishes what a failure left; a failure is logged,
/// never fatal.
async fn erase_at_startup(chain: &Chained) {
    let caps = chain.caps;
    let done = tokio::task::spawn_blocking(move || {
        // Each step runs whatever the one before it did, so a key the lines' sweep kept never
        // keeps a summary that read its words, and each failure is said on its own.
        let erased = caps
            .secretary
            .memories
            .hold()
            .and_then(|held| held.erase_forgotten());
        let lines = suzatary_core::secretary::transcript::sweep(caps.secretary.conversations)
            .map_err(suzatary_core::refusal::Refusal::from);
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

/// `suzatary serve`: resolve the flags and run the router.
///
/// # Errors
/// Flags that do not resolve, and the server's own.
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
    /// A pair the mail tests already hold, so this asks for no generator.
    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../io/tests/fixtures/{name}"))
    }

    /// The listener's TLS builds over the provider it is handed, with no process-wide default
    /// installed, offers HTTP/2 first, and refuses a key that is not one.
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
