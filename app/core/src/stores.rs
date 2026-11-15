//! Every store the data tree keeps, declared once in `stores!`: a new store is one row and its
//! schema, and the interpreters, the fsck, the routes and the guards read the rows.

mod database;
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/in_memory.rs"]
mod in_memory;
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/scene.rs"]
mod scene;

use prodrome_core::declared::Declared;
use prodrome_core::schema::Schema;

use crate::asks::AskEvent;
use crate::secretary::digest_event::DigestEvent;
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::line_event::LineEvent;
use crate::secretary::memory_event::MemoryEvent;
use crate::secretary::topics::reading::AliasEvent;
use crate::secretary::topics::reading::MembershipEvent;
use suzatary_record::record::TodoEvent;
use suzatary_store::admission::{Admission, Shared};
use suzatary_store::store::Hold;
use suzatary_todo::decisions::DecisionEvent;

pub use database::Database;
#[cfg(any(test, feature = "nothing"))]
pub use in_memory::InMemory;
#[cfg(any(test, feature = "nothing"))]
pub use scene::{phone_trusted, scene};

/// A store a tab writes, by its schema: the rule its row names, which the box takes a tab's post by
/// after the rule every such store shares.
pub trait Written: Store {
    /// Its rule.
    type Rule: Admission<Schema = Self>;
}

/// Where a store's files go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Committed and pushed with the data repository.
    Pushed,
    /// Kept on the box and in the vault, never pushed.
    Local,
    /// Kept on the box only, and rebuilt by syncing again.
    Cache,
}

/// What a store's directory holds, as the data push stages and checks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Files {
    /// A todo chain's objects, beside its tracked lock file.
    Chain,
    /// Objects appended and never edited.
    Objects,
    /// One record per performed request, written once.
    Records,
    /// One file, rewritten whole.
    File,
    /// Files its store writes and deletes by rules of its own.
    Directory,
}

/// Who writes a store's objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer {
    /// The box alone: a post of objects to it is refused.
    Box,
    /// A tab too, each change beside its writer's signature, taken by the rule its row names.
    Tab,
}

/// A schema kept as one of this application's prodromes: its row, its hold and its label.
pub trait Store: Schema {
    /// Its row.
    const KIND: Kind;
    /// What serialises its writers.
    const HOLD: Hold;
    /// Its `Genesis` label.
    const LABEL: &'static str = Self::KIND.name();
}

/// The rows: `Variant = name, tier, files`, and for a prodrome `: schema, hold`, its `Genesis`
/// label where that is not its name, and `=> rule` where a tab writes it, the rule the box takes
/// a tab's post by. A name never changes for a store that holds objects.
macro_rules! stores {
    ($($(#[doc = $doc:literal])* $variant:ident = $name:literal, $tier:ident, $files:ident
        $(: $event:ty, $hold:expr $(, $label:expr)? $(=> $rule:ty)?)?;)*) => {
        /// A store the data tree keeps.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Kind {
            $($(#[doc = $doc])* $variant,)*
        }

        impl Kind {
            /// Every store, in the order the fsck reports them.
            pub const ALL: &'static [Kind] = &[$(Kind::$variant),*];

            /// Its name.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Kind::$variant => $name,)*
                }
            }

            /// Where its files go.
            #[must_use]
            pub const fn tier(self) -> Tier {
                match self {
                    $(Kind::$variant => Tier::$tier,)*
                }
            }

            /// What its directory holds.
            #[must_use]
            pub const fn files(self) -> Files {
                match self {
                    $(Kind::$variant => Files::$files,)*
                }
            }

            /// Who writes its objects.
            #[must_use]
            pub const fn writer(self) -> Writer {
                match self {
                    $(Kind::$variant => writer!($($($rule)?)?),)*
                }
            }
        }

        $($(impl Store for $event {
            const KIND: Kind = Kind::$variant;
            const HOLD: Hold = $hold;
            $(const LABEL: &'static str = $label;)?
        }
        $(impl Written for $event {
            type Rule = $rule;
        })?)?)*
    };
}

/// A row's writer: a tab where it names a rule, else the box alone.
macro_rules! writer {
    () => {
        Writer::Box
    };
    ($rule:ty) => {
        Writer::Tab
    };
}

stores! {
    /// A todo chain.
    Chain = "chain", Pushed, Chain: TodoEvent, Hold::Reentrant, suzatary_record::record::GENESIS;
    /// The proposals a person taps.
    Inbox = "inbox", Pushed, Objects: InboxEvent, Hold::File("submit.lock");
    /// The secretary's conversations, every word locked.
    Lines = "lines", Pushed, Objects: LineEvent, Hold::File("lines.lock");
    /// The keys of the lines' words, destroyed by the lines' sweep.
    LineKeys = "line keys", Local, Directory;
    /// Which facts the secretary remembered and forgot, by id.
    Memory = "memory", Pushed, Objects: MemoryEvent, Hold::File("memory.lock");
    /// Each standing fact's words; forgetting deletes them.
    MemoryTexts = "memory texts", Local, Directory;
    /// The conversations' summary tree.
    Digests = "digests", Pushed, Objects: DigestEvent, Hold::File("digests.lock");
    /// Each summary's words; the sweep deletes the dead.
    DigestTexts = "digest texts", Local, Directory;
    /// The recorded decisions.
    Decisions = "decisions", Pushed, Objects: DecisionEvent, Hold::File("decisions.lock");
    /// The topics on the secretary's rounds and notes: a topic's name is words.
    Topics = "topics", Local, Objects: MembershipEvent, Hold::File("topics.lock") => Shared<MembershipEvent>;
    /// The topics' aliases.
    TopicAliases = "topic-aliases", Local, Objects: AliasEvent, Hold::File("topic-aliases.lock") => Shared<AliasEvent>;
    /// The box's refusals of what a tab posted; a reason may quote a topic.
    Asks = "asks", Local, Objects: AskEvent, Hold::File("asks.lock");
    /// What a plugin learned, at its declared schema.
    Plugin = "plugin", Cache, Objects: Declared, Hold::File("store.lock");
    /// The triage's dedup authority, decisions under keyed names.
    Triage = "triage", Pushed, File;
    /// The box-local tier.
    Local = "local", Local, Directory;
    /// The owner's uploads.
    Uploads = "uploads", Local, Directory;
    /// The files a verb kept for the model.
    Kept = "kept", Local, Directory;
    /// Each performed signed request, as provenance.
    Requests = "requests", Pushed, Records;
}

impl Kind {
    /// Whether the data push refuses a change or deletion of a file it already holds.
    #[must_use]
    pub const fn append_only(self) -> bool {
        match self.files() {
            Files::Chain | Files::Objects | Files::Records => true,
            Files::File | Files::Directory => false,
        }
    }
}
