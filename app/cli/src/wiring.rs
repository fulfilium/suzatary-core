//! The box's interpreters, opened once from the instance: what every record borrows. Each
//! effect's interpreter is built in one place.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use prodrome_core::event::Actor;
use suzatary_core::capability::{Agent, RenderCache, Secretary};
use suzatary_core::chains::ChainPolicy;
use suzatary_core::decisions::{DecisionEvent, Decisions};
use suzatary_core::mail::account::Roster;
use suzatary_core::persona::Persona;
use suzatary_core::record::TodoEvent;
use suzatary_core::secretary::compacting::Compacting;
use suzatary_core::secretary::digest_event::DigestEvent;
use suzatary_core::secretary::digest_line::Line;
use suzatary_core::secretary::digests::{Digests, ReadDigests};
use suzatary_core::secretary::inbox_event::InboxEvent;
use suzatary_core::secretary::memories::{Memories, ReadMemories};
use suzatary_core::secretary::memory::Fact;
use suzatary_core::secretary::memory_event::MemoryEvent;
use suzatary_core::secretary::model::Model;
use suzatary_core::secretary::recollection::Recollection;
use suzatary_core::signatures::Signers;
use suzatary_core::store::{Only, Reads, Signing};
use suzatary_core::zone::Zone;
use suzatary_io::conversations::ConversationStores;
use suzatary_io::objects::Objects;
use suzatary_io::texts::Texts;
use suzatary_io::unrendered::Unrendered;

use crate::commands::serve::interpreters::{Declared, DriveAccess, Forecast, Junk, WebAccess};
use crate::instance::{ChainDir, Instance};

/// The files a [`Grounds`] reads and writes, and the accounts its sync pulls.
pub struct Ground {
    mail_root: PathBuf,
    notmuch: PathBuf,
    /// `None` is a box with no mail index, which every read refuses.
    notmuch_config: Option<PathBuf>,
    pub(crate) ledger: suzatary_io::state::Ledger,
    /// Where each listed message lives, by canonical hash.
    mail_hashes: PathBuf,
    /// The accounts a message's folder is read as, and the accounts a sync pulls.
    accounts: Roster,
    mbsyncrc: PathBuf,
    imap_state: PathBuf,
    tokens: suzatary_io::oauth::TokenStore,
    grants: suzatary_io::google::Grants,
    heartbeat: PathBuf,
    last_sync: PathBuf,
    watches: PathBuf,
    pub(crate) data: PathBuf,
    markup_cache: PathBuf,
}

impl Ground {
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self {
            mail_root: instance.mail.root.clone(),
            notmuch: instance.mail.notmuch.clone(),
            notmuch_config: instance.mail.notmuch_config.clone(),
            ledger: crate::commands::mail::ledger(instance),
            mail_hashes: instance.paths.mail_hashes.clone(),
            accounts: instance.mail.accounts.clone(),
            mbsyncrc: instance.mail.mbsyncrc.clone(),
            imap_state: instance.paths.imap_state.clone(),
            tokens: crate::commands::mail::tokens(instance),
            grants: crate::commands::mail::google_grants(instance),
            heartbeat: instance.mail.heartbeat.clone(),
            last_sync: instance.paths.last_sync.clone(),
            watches: instance.local.watches(),
            data: instance.data().to_path_buf(),
            markup_cache: instance.paths.markup_cache.clone(),
        }
    }

    #[must_use]
    pub fn mail_root(&self) -> &Path {
        &self.mail_root
    }

    /// The mail this ground keeps, held against its sync.
    #[must_use]
    pub fn mail_cache(&self) -> crate::commands::serve::mail_cache::MailCache {
        crate::commands::serve::mail_cache::MailCache::new(
            self.mail_root.clone(),
            self.notmuch.clone(),
            self.notmuch_config.clone(),
            suzatary_io::sync::lock(&self.imap_state),
            self.tokens.clone(),
        )
    }

    /// Where each listed message lives, by canonical hash.
    #[must_use]
    pub fn located(&self) -> suzatary_io::mailbox::located::Located {
        suzatary_io::mailbox::located::Located::open(self.mail_hashes.clone())
    }

    /// The corpus, read-only, `pdftotext` falling back through `code`.
    #[must_use]
    pub fn notmuch(&self, code: &Path) -> suzatary_io::mailbox::Notmuch {
        let notmuch = suzatary_io::mailbox::Notmuch::new(
            self.mail_root.clone(),
            self.ledger.clone(),
            self.located(),
            self.accounts.clone(),
        )
        .with_binary(self.notmuch.clone())
        .with_pdftotext_repo(code.to_path_buf());
        match &self.notmuch_config {
            Some(config) => notmuch.with_config(config.clone()),
            None => notmuch,
        }
    }

    #[must_use]
    pub fn syncer(&self) -> suzatary_io::sync::Syncer {
        let syncer = suzatary_io::sync::Syncer::new(
            self.accounts.clone(),
            self.mail_root.clone(),
            self.notmuch.clone(),
            self.mbsyncrc.clone(),
            self.imap_state.clone(),
            (self.tokens.clone(), self.grants.clone()),
        );
        match &self.notmuch_config {
            Some(config) => syncer.with_config(config.clone()),
            None => syncer,
        }
    }

    /// The mechanical operations over this ground, the sync's window counted in `zone`.
    #[must_use]
    pub fn operator(&self, code: &Path, zone: Zone) -> suzatary_io::corpus::Operator {
        suzatary_io::corpus::Operator::new(
            self.notmuch(code),
            self.ledger.clone(),
            self.syncer(),
            self.heartbeat.clone(),
            self.last_sync.clone(),
            zone,
            suzatary_io::watches::Watches::open(self.watches.clone()),
        )
    }
}

/// What every record reads through, the triage agent's included: its six effects beside a chain.
pub struct Grounds {
    pub(crate) corpus: suzatary_io::corpus::Operator,
    pub(crate) repository: suzatary_io::repository::Checkout,
    pub(crate) manuals: suzatary_io::manuals::Handbook,
    pub(crate) mail: suzatary_io::mailbox::Notmuch,
    pub(crate) render: suzatary_io::render::Typst,
    pub(crate) clock: suzatary_io::clock::SystemClock,
}

impl Grounds {
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self::over(
            &Ground::of(instance),
            instance.code(),
            &instance.persona,
            suzatary_io::typeset::Closed::this_binary(),
        )
    }

    /// The six over `ground`, reading manuals and templates from the code root `code`.
    #[must_use]
    pub fn over(
        ground: &Ground,
        code: &Path,
        persona: &Persona,
        typeset: suzatary_io::typeset::Closed,
    ) -> Self {
        Self {
            corpus: ground.operator(code, persona.zone),
            repository: suzatary_io::repository::Checkout::new(ground.data.clone()),
            manuals: suzatary_io::manuals::Handbook::new(code.to_path_buf(), persona.clone()),
            mail: ground.notmuch(code),
            render: suzatary_io::render::Typst::new(ground.markup_cache.clone(), typeset),
            clock: suzatary_io::clock::SystemClock,
        }
    }

    /// The agent's record over `chain`, reading `memory`.
    #[must_use]
    pub fn agent<'a>(
        &'a self,
        chain: &'a dyn Reads<TodoEvent>,
        memory: Recollection<'a>,
    ) -> Agent<'a> {
        Agent {
            chain,
            corpus: &self.corpus,
            mail: &self.mail,
            repository: &self.repository,
            manuals: &self.manuals,
            render: &self.render,
            clock: &self.clock,
            memory,
        }
    }
}

/// The secretary's memory and every store it reads: what the secretary's record writes and the
/// triage agent's reads through a [`Recollection`].
pub struct Remembering {
    pub(crate) conversations: ConversationStores,
    pub(crate) memory: Objects<MemoryEvent>,
    pub(crate) memory_texts: Texts<Fact>,
    pub(crate) digests: Objects<DigestEvent>,
    pub(crate) digest_texts: Texts<Line>,
    /// Whose retraction or erasure the digest believes.
    pub(crate) owners: BTreeSet<Actor>,
    pub(crate) compacting: Compacting,
    // One store for every chain: a proposal is the box's question, not a chain's.
    pub(crate) inbox: Objects<InboxEvent>,
}

impl Remembering {
    /// The memory's stores; `renders` is the answers' cache an erasure empties.
    #[must_use]
    pub fn of(instance: &Instance, renders: Arc<dyn RenderCache + Send + Sync>) -> Self {
        Self {
            conversations: instance.secretary.conversation_stores(
                renders,
                Arc::new(crate::serve::interpreters::spawner(instance, "").sessions()),
            ),
            memory: instance.memory_store(),
            memory_texts: Texts::open(instance.secretary.memory_texts()),
            digests: instance.digest_store(),
            digest_texts: Texts::open(instance.secretary.digest_texts()),
            owners: instance.deployment.writers(),
            compacting: instance.secretary.compacting(),
            inbox: instance.inbox_store(),
        }
    }

    /// The memory's stores without the answers' cache, which sweeps its directory when opened.
    #[must_use]
    pub fn reader(instance: &Instance) -> Self {
        Self::of(instance, Arc::new(Unrendered))
    }

    /// The memory and the digest, as one record reads and erases them.
    #[must_use]
    pub fn memories(&self) -> Memories<'_> {
        Memories {
            history: &self.memory,
            texts: &self.memory_texts,
            cards: &self.inbox,
            entropy: &suzatary_io::texts::SystemEntropy,
            digests: Digests {
                history: &self.digests,
                texts: &self.digest_texts,
                lines: self.conversations.conversations(),
                owners: &self.owners,
                compacting: &self.compacting,
            },
        }
    }
}

/// The secretary's memory as the triage agent's process opens it: memory and digest inside
/// [`Only`], so no write handle exists in the process. The conversations and the inbox stay
/// writable, since the agent's cards are filed there.
pub struct Recalling {
    pub(crate) conversations: ConversationStores,
    memory: Only<Objects<MemoryEvent>>,
    memory_texts: Only<Texts<Fact>>,
    digests: Only<Objects<DigestEvent>>,
    digest_texts: Only<Texts<Line>>,
    owners: BTreeSet<Actor>,
    compacting: Compacting,
    pub(crate) inbox: Objects<InboxEvent>,
}

impl Recalling {
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Remembering::reader(instance).into()
    }

    /// The memory, read within `budget` bytes.
    #[must_use]
    pub fn memory(&self, budget: usize) -> Recollection<'_> {
        Recollection::of(
            ReadMemories {
                history: &self.memory,
                texts: &self.memory_texts,
                digests: ReadDigests {
                    history: &self.digests,
                    texts: &self.digest_texts,
                    lines: self.conversations.conversations().reads(),
                    owners: &self.owners,
                    compacting: &self.compacting,
                },
            },
            budget,
        )
    }
}

impl From<Remembering> for Recalling {
    fn from(opened: Remembering) -> Self {
        Self {
            conversations: opened.conversations,
            memory: Only::new(opened.memory),
            memory_texts: Only::new(opened.memory_texts),
            digests: Only::new(opened.digests),
            digest_texts: Only::new(opened.digest_texts),
            owners: opened.owners,
            compacting: opened.compacting,
            inbox: opened.inbox,
        }
    }
}

/// [`Grounds`] and every store and effect a secretary's record adds, one for all chains: a
/// second chain is a second [`Stores`], never a second inbox, cache or mail handle.
pub struct Opened {
    pub(crate) grounds: Grounds,
    /// The answers' cache of typst blocks, opened once: the pages draw through the same cache
    /// an erasure empties.
    pub(crate) answers: Arc<suzatary_io::render::Typst>,
    pub(crate) remembering: Remembering,
    pub(crate) decisions: Objects<DecisionEvent>,
    /// The topics, box-local and never pushed.
    pub(crate) topics: suzatary_io::topics::Topics,
    pub(crate) briefings: suzatary_io::briefings::BriefingFiles,
    pub(crate) weather: Forecast,
    pub(crate) calendar: suzatary_io::calendar::Clocked,
    pub(crate) secrets: Declared,
    pub(crate) web: WebAccess,
    pub(crate) drive: DriveAccess,
    pub(crate) junk: Junk,
    pub(crate) sync: suzatary_io::mail_sync::Timer,
    pub(crate) health: suzatary_io::source_health::Health,
    pub(crate) older: crate::commands::serve::interpreters::Older,
    pub(crate) uploads: suzatary_io::attachments::AttachmentStore,
    pub(crate) upload_index: suzatary_io::upload_index::UploadIndexFiles,
    pub(crate) kept: suzatary_io::blobs::Blobs,
    pub(crate) pdf: suzatary_io::pdftotext::Pdftotext,
    pub(crate) tapped: suzatary_io::docs::Docs<
        suzatary_core::held_value::HeldFile,
        suzatary_core::secretary::attached::Attached,
    >,
    pub(crate) configuration: crate::instance::InstanceEdits,
    pub(crate) watches: suzatary_io::watches::Watches,
    pub(crate) repos: Arc<suzatary_io::repos::Mirrors>,
}

impl Opened {
    /// Every interpreter the instance file chooses, the secret store's audit saying `asking`.
    #[must_use]
    pub fn of(instance: &Instance, asking: &'static str) -> Self {
        use crate::commands::serve::interpreters as chosen;
        let answers = Arc::new(instance.answers());
        Self {
            grounds: Grounds::of(instance),
            remembering: Remembering::of(instance, Arc::clone(&answers) as _),
            answers,
            decisions: Objects::open(instance.decisions()),
            topics: instance.secretary.topics.open(),
            briefings: suzatary_io::briefings::BriefingFiles::new(
                instance.serving.briefings.clone(),
            ),
            weather: Forecast(instance.persona.clone()),
            calendar: suzatary_io::calendar::Clocked::new(
                instance.calendar_stores(),
                instance.persona.zone,
            ),
            secrets: Declared {
                secrets: instance.secrets(),
                tokens: crate::commands::mail::tokens(instance),
                asking,
            },
            web: WebAccess::of(instance),
            drive: DriveAccess::of(instance),
            junk: chosen::junk(instance),
            sync: chosen::mail_sync(instance),
            health: suzatary_io::source_health::beside(&instance.mail.heartbeat),
            older: chosen::older(instance),
            uploads: chosen::upload_store(instance),
            upload_index: chosen::upload_index(instance),
            kept: chosen::file_store(instance),
            pdf: suzatary_io::pdftotext::Pdftotext::new(instance.code()),
            tapped: suzatary_io::tapped::beside(&chosen::file_store(instance)),
            configuration: crate::instance::InstanceEdits::new(instance.path.clone()),
            watches: suzatary_io::watches::Watches::open(instance.local.watches()),
            repos: chosen::mirrors(instance),
        }
    }

    #[must_use]
    pub fn memories(&self) -> Memories<'_> {
        self.remembering.memories()
    }

    #[must_use]
    pub fn decisions(&self) -> Decisions<'_> {
        Decisions {
            store: &self.decisions,
        }
    }

    /// The secretary's record over one chain's `stores`, answered by `model`.
    #[must_use]
    pub fn secretary<'a>(&'a self, stores: &'a Stores, model: &'a dyn Model) -> Secretary<'a> {
        let grounds = &self.grounds;
        Secretary {
            chain: &stores.chain,
            corpus: &grounds.corpus,
            mail: &grounds.mail,
            repository: &grounds.repository,
            manuals: &grounds.manuals,
            render: &grounds.render,
            clock: &grounds.clock,
            model,
            conversations: self.remembering.conversations.conversations(),
            inbox: &self.remembering.inbox,
            memories: self.memories(),
            decisions: self.decisions(),
            topics: self.topics.stores(),
            briefings: &self.briefings,
            weather: &self.weather,
            calendar: &self.calendar,
            secrets: &self.secrets,
            web: &self.web,
            repositories: self.repos.as_ref(),
            drive: &self.drive,
            junk: &self.junk,
            sync: &self.sync,
            health: &self.health,
            older: &self.older,
            uploads: &self.uploads,
            upload_index: &self.upload_index,
            kept: &self.kept,
            pdf: &self.pdf,
            tapped: &self.tapped,
            configuration: &self.configuration,
            watches: &self.watches,
        }
    }
}

/// One chain's stores and the policy they are read under, together.
pub struct Stores {
    pub policy: ChainPolicy,
    /// Every append the box makes is signed.
    pub(crate) chain: Signing<Objects<TodoEvent>>,
}

impl Stores {
    /// One chain's stores under its own trust rule, written as its owner.
    #[must_use]
    pub fn of(chain: &ChainDir, signers: &Signers) -> Self {
        Self {
            policy: chain.policy.clone(),
            chain: Instance::writer_of(chain, signers, &chain.owner),
        }
    }
}
