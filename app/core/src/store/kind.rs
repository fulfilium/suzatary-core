//! Every store the data tree keeps, declared once in [`stores!`]: its name, its tier and what its
//! directory holds, and for a prodrome its schema and its hold. The interpreters, the fsck and
//! the routes read these rows, and a guard derives the data push's and the vault's lists from
//! them, so a new store is one row and its schema.

use prodrome_core::declared::Declared;
use prodrome_core::schema::Schema;

use crate::decisions::DecisionEvent;
use crate::record::TodoEvent;
use crate::secretary::digest_event::DigestEvent;
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::line_event::LineEvent;
use crate::secretary::memory_event::MemoryEvent;
use crate::secretary::topics::alias_event::AliasEvent;
use crate::secretary::topics::membership_event::MembershipEvent;

/// Where a store's files go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Committed and pushed with the data repository.
    Pushed,
    /// Kept on the box and in the vault, never pushed: words a push could not erase, and keys.
    Local,
    /// Kept on the box only, and rebuilt by syncing again.
    Cache,
}

/// What a store's directory holds, as the data push stages and checks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Files {
    /// A todo chain's objects, appended and never edited, beside its tracked lock file.
    Chain,
    /// Objects appended and never edited.
    Objects,
    /// One record per performed request, named by its hash and written once.
    Records,
    /// One file, rewritten whole.
    File,
    /// Files its store writes and deletes by rules of its own.
    Directory,
}

/// What serialises a prodrome's writers above the database's own lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// This process's threads, reentrant, since a batch holds it while each write inside takes
    /// its own.
    Reentrant,
    /// An `flock` on this file beside the objects, opened afresh for each hold, so threads wait
    /// on each other as processes do; never taken twice by one thread.
    File(&'static str),
}

/// A schema kept as one of this application's prodromes: its row, its hold and its label.
pub trait Store: Schema {
    /// Its row.
    const KIND: Kind;
    /// What serialises its writers.
    const HOLD: Hold;
    /// The label of the `Genesis` an empty store's first write begins it with.
    const LABEL: &'static str = Self::KIND.name();
}

/// The rows: `Variant = name, tier, files`, and for a prodrome `: schema, hold`, then its
/// `Genesis` label where that is not its name. The name is what errors and the fsck say, a
/// prodrome's label and a tab's route segment, so it never changes for a store that holds
/// objects.
macro_rules! stores {
    ($($(#[doc = $doc:literal])* $variant:ident = $name:literal, $tier:ident, $files:ident
        $(: $event:ty, $hold:expr $(, $label:expr)?)?;)*) => {
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
        }

        $($(impl Store for $event {
            const KIND: Kind = Kind::$variant;
            const HOLD: Hold = $hold;
            $(const LABEL: &'static str = $label;)?
        })?)*
    };
}

stores! {
    /// A todo chain.
    Chain = "chain", Pushed, Chain: TodoEvent, Hold::Reentrant, crate::record::GENESIS;
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
    /// What was said of each node of the conversations' summary tree.
    Digests = "digests", Pushed, Objects: DigestEvent, Hold::File("digests.lock");
    /// Each summary's words; the sweep deletes the dead.
    DigestTexts = "digest texts", Local, Directory;
    /// The recorded decisions.
    Decisions = "decisions", Pushed, Objects: DecisionEvent, Hold::File("decisions.lock");
    /// The topics on the secretary's rounds and notes: a topic's name is words.
    Topics = "topics", Local, Objects: MembershipEvent, Hold::File("topics.lock");
    /// The topics' aliases.
    TopicAliases = "topic-aliases", Local, Objects: AliasEvent, Hold::File("topic-aliases.lock");
    /// What a plugin learned, at its declared schema.
    Plugin = "plugin", Cache, Objects: Declared, Hold::File("store.lock");
    /// The triage's dedup authority, decisions under keyed names.
    Triage = "triage", Pushed, File;
    /// The box-local tier: what the box keeps of its mail, the person's prose, the leak guard's
    /// terms.
    Local = "local", Local, Directory;
    /// The owner's uploads.
    Uploads = "uploads", Local, Directory;
    /// The files a verb kept for the model.
    Kept = "kept", Local, Directory;
    /// Each request a device signed and the server performed, as provenance.
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
