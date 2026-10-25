//! The box's interpreters, opened once from the instance: what every record borrows, whichever
//! process holds it. The agent's [`Grounds`] are part of the secretary's [`Opened`], and the
//! server's `World` is [`Opened`] plus what only a server holds, so one interpreter of an effect
//! is built in one place.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use prodrome_core::event::Actor;
use suzatary_core::capability::{Agent, Model, RenderCache, Secretary};
use suzatary_core::chains::ChainPolicy;
use suzatary_core::decisions::{DecisionEvent, Decisions};
use suzatary_core::record::TodoEvent;
use suzatary_core::secretary::compacting::Compacting;
use suzatary_core::secretary::digest_event::DigestEvent;
use suzatary_core::secretary::digest_line::Line;
use suzatary_core::secretary::digests::Digests;
use suzatary_core::secretary::inbox_event::InboxEvent;
use suzatary_core::secretary::memories::Memories;
use suzatary_core::secretary::memory::Fact;
use suzatary_core::secretary::memory_event::MemoryEvent;
use suzatary_core::secretary::recollection::Recollection;
use suzatary_core::signatures::Signers;
use suzatary_core::store::{Prodrome, Signing};
use suzatary_io::conversations::ConversationStores;
use suzatary_io::objects::Objects;
use suzatary_io::texts::Texts;
use suzatary_io::unrendered::Unrendered;

use crate::commands::serve::interpreters::{Declared, DriveAccess, Forecast, Junk, WebAccess};
use crate::instance::{ChainDir, Instance};

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
    /// The box's six, from the instance file.
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self {
            corpus: crate::commands::mail::operator(instance),
            repository: suzatary_io::repository::Checkout::new(
                instance.data().to_path_buf(),
                instance.chain_roster(),
            ),
            manuals: suzatary_io::manuals::Handbook::new(
                instance.code().to_path_buf(),
                instance.persona.clone(),
            ),
            mail: crate::commands::mail::notmuch(instance),
            render: suzatary_io::render::Typst::new(
                instance.code().to_path_buf(),
                instance.paths.markup_cache.clone(),
                suzatary_io::typeset::Closed::this_binary(),
            ),
            clock: suzatary_io::clock::SystemClock,
        }
    }

    /// The agent's record over `chain`, reading `memory`.
    #[must_use]
    pub fn agent<'a>(
        &'a self,
        chain: &'a dyn Prodrome<TodoEvent>,
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
    /// Whether the compactor runs, and within what.
    pub(crate) compacting: Compacting,
    // One store for every chain, as a proposal is the box's question and not a chain's.
    pub(crate) inbox: Objects<InboxEvent>,
}

impl Remembering {
    /// The memory's stores, from the instance file, the conversations' answers drawn through
    /// `renders`, the cache an erasure empties.
    #[must_use]
    pub fn of(instance: &Instance, renders: Arc<dyn RenderCache + Send + Sync>) -> Self {
        Self {
            conversations: instance.secretary.conversation_stores(renders),
            memory: instance.memory_store(),
            memory_texts: Texts::open(instance.secretary.memory_texts()),
            digests: instance.digest_store(),
            digest_texts: Texts::open(instance.secretary.digest_texts()),
            owners: instance.deployment.writers(),
            compacting: instance.secretary.compacting(),
            inbox: instance.inbox_store(),
        }
    }

    /// The memory's stores as a reader that draws no answer opens them: the secretary's answers'
    /// cache, which sweeps its directory when it opens, is not opened.
    #[must_use]
    pub fn reader(instance: &Instance) -> Self {
        Self::of(instance, Arc::new(Unrendered))
    }

    /// A memory that holds nothing, its stores under `dir`, which holds nothing either: what the
    /// bench is told, so no case reaches a vendor with the box's memory or today's.
    #[must_use]
    pub fn empty_in(dir: &Path, instance: &Instance) -> Self {
        Self {
            conversations: ConversationStores::open(
                dir.join("lines"),
                dir.join("keys"),
                Arc::new(Unrendered),
            ),
            memory: Objects::open(dir.join("memory")),
            memory_texts: Texts::open(dir.join("memory-texts")),
            digests: Objects::digests(dir.join("digests"), instance.deployment.writers()),
            digest_texts: Texts::open(dir.join("digest-texts")),
            owners: instance.deployment.writers(),
            compacting: Compacting::Off,
            inbox: Objects::open(dir.join("inbox")),
        }
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

/// [`Grounds`] and every store and effect a secretary's record adds, one of each for every
/// chain: a second chain is a second [`Stores`], never a second inbox, cache or mail handle.
pub struct Opened {
    pub(crate) grounds: Grounds,
    /// The answers' cache of typst blocks: the one the pages draw through is the one an
    /// erasure empties, so it is opened once and the conversations hold it.
    pub(crate) answers: Arc<suzatary_io::render::Typst>,
    pub(crate) remembering: Remembering,
    pub(crate) decisions: Objects<DecisionEvent>,
    /// The topics, box-local and never pushed; a server also serves them to a tab.
    pub(crate) topics: suzatary_io::topics::Topics,
    pub(crate) briefings: suzatary_io::briefings::BriefingFiles,
    pub(crate) weather: Forecast,
    pub(crate) calendar: suzatary_io::calendar::Clocked,
    pub(crate) secrets: Declared,
    pub(crate) web: WebAccess,
    pub(crate) drive: DriveAccess,
    pub(crate) junk: Junk,
    pub(crate) sync: suzatary_io::mail_sync::Timer,
    pub(crate) older: crate::commands::serve::interpreters::Older,
    pub(crate) uploads: suzatary_io::attachments::AttachmentStore,
    pub(crate) upload_index: suzatary_io::upload_index::UploadIndexFiles,
    pub(crate) kept: suzatary_io::blobs::Blobs,
    /// The model settings the instance file declares, read as a card is filed.
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
            older: chosen::older(instance),
            uploads: chosen::upload_store(instance),
            upload_index: chosen::upload_index(instance),
            kept: chosen::file_store(instance),
            configuration: crate::instance::InstanceEdits::new(instance.path.clone()),
            watches: suzatary_io::watches::Watches::open(instance.local.watches()),
            repos: chosen::mirrors(instance),
        }
    }

    /// The memory and the digest, as one record reads and erases them.
    #[must_use]
    pub fn memories(&self) -> Memories<'_> {
        self.remembering.memories()
    }

    /// The recorded decisions.
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
            older: &self.older,
            uploads: &self.uploads,
            upload_index: &self.upload_index,
            kept: &self.kept,
            configuration: &self.configuration,
            watches: &self.watches,
        }
    }
}

/// One chain's stores and the policy they are read under, together.
pub struct Stores {
    /// Whose events bind here.
    pub policy: ChainPolicy,
    /// Written through the box's key: every append the box makes is signed.
    pub(crate) chain: Signing<Objects<TodoEvent>>,
}

impl Stores {
    /// Open one chain's stores under that chain's own trust rule, written as its owner.
    #[must_use]
    pub fn of(chain: &ChainDir, signers: &Signers) -> Self {
        Self {
            policy: chain.policy.clone(),
            chain: Instance::writer_of(chain, signers, &chain.owner),
        }
    }
}
