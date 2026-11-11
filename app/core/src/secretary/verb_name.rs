//! Each secretary verb's name and powers, read below the verbs; `SecretaryVerb` maps onto it.

use crate::secretary::taint::Taint;

/// What a verb may do to the world, and whether its answer can carry somebody else's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Powers {
    /// What it does.
    pub effect: Effect,
    /// What its answer can carry.
    pub taint: Taint,
}

/// What a verb does to the world, so a round scoped to fewer powers is a set of effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Reads, or runs something that writes neither a chain nor a proposal.
    Reads,
    /// Stores something the owner taps, binding nothing until he does.
    Proposes,
    /// Binds at once in a round holding the owner's words alone, and files a card elsewhere.
    Binds(Binding),
    /// Withdraws one of the triage agent's cards; performing one is the owner's tap alone.
    Settles,
    /// Starts the timer's own mail sync early: nothing the timer does not reach, and nothing a
    /// rewind could take back.
    Syncs,
    /// Searches a mail server for mail older than the routine sync's window and keeps what
    /// matched on the box, read-only on the server and nothing a rewind could take back.
    Pulls,
    /// Sets a watch on mail the owner expects, whose only act is a push to the owner's phone.
    Watches,
}

/// An effect that binds at once, so a round's taint decides whether it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    /// Appends to a chain as the trusted actor.
    Writes,
    /// Writes the secretary's own memory.
    Remembers,
}

const fn powers(effect: Effect, taint: Taint) -> Powers {
    Powers { effect, taint }
}

use Binding::{Remembers, Writes};
use Effect::{Binds, Proposes, Pulls, Reads, Settles, Syncs, Watches};
use Taint::{Clean, Tainted};

macro_rules! verb_names {
    (@taint) => { Tainted };
    (@taint $clean:ident) => { $clean };
    ($($(#[$doc:meta])* $Name:ident = $name:literal: $effect:expr $(, $clean:ident)?;)+) => {
        /// A verb of the secretary's surface, by the name a model calls it.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum VerbName {
            $($(#[$doc])* #[doc = concat!("`", $name, "`.")] $Name,)+
        }

        impl VerbName {
            /// Every verb, in the order the model is shown them.
            pub const ALL: &'static [Self] = &[$(Self::$Name),+];

            /// Its name.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$Name => $name,)+
                }
            }

            /// Its powers: [`Tainted`] unless its row says [`Clean`].
            #[must_use]
            pub const fn powers(self) -> Powers {
                match self {
                    $(Self::$Name => powers($effect, verb_names!(@taint $($clean)?)),)+
                }
            }
        }
    };
}

impl VerbName {
    /// The verb `name` names, or `None`.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|verb| verb.name() == name)
    }
}

verb_names! {
    Context = "context": Reads, Clean;
    Remember = "remember": Binds(Remembers), Clean;
    Forget = "forget": Binds(Remembers), Clean;
    Consolidate = "consolidate": Binds(Remembers), Clean;
    Recall = "recall": Reads;
    // Its answer carries the node's own taint, which `taint::turn` reads; a refusal stays here.
    Zoom = "zoom": Reads;
    ListOpenTodos = "list_open_todos": Reads;
    TodoShow = "todo_show": Reads;
    History = "history": Reads;
    Search = "search": Reads;
    Show = "show": Reads;
    Attachments = "attachments": Reads;
    UploadRead = "upload_read": Reads;
    SyncMail = "sync_mail": Syncs;
    SearchOlder = "search_older": Pulls;
    WatchMail = "watch_mail": Watches;
    BriefingDelta = "briefing_delta": Reads;
    Briefing = "briefing": Reads;
    GitDiffSummary = "git_diff_summary": Reads;
    TodoAdd = "todo_add": Binds(Writes);
    TodoUpdate = "todo_update": Binds(Writes);
    Record = "record": Binds(Writes);
    ProposeEmail = "propose_email": Proposes;
    ProposeForward = "propose_forward": Proposes;
    ProposeAct = "propose_act": Proposes;
    ProposeChanges = "propose_changes": Proposes;
    WithdrawProposal = "withdraw_proposal": Proposes;
    Weather = "weather": Reads, Clean;
    Calendar = "calendar": Reads;
    Decisions = "decisions": Reads;
    Web = "web": Proposes;
    DriveList = "drive_list": Reads;
    DriveRead = "drive_read": Proposes;
    JunkDigest = "junk_digest": Reads;
    JunkShow = "junk_show": Reads;
    ReposList = "repos_list": Reads;
    RepoTree = "repo_tree": Reads;
    RepoRead = "repo_read": Reads;
    RepoGrep = "repo_grep": Reads;
    ProposePatch = "propose_patch": Proposes;
    ProposeCodingSession = "propose_coding_session": Proposes;
    ProposeDecision = "propose_decision": Proposes;
    DraftNote = "draft_note": Proposes;
    TriageCards = "triage_cards": Reads;
    TriageWithdraw = "triage_withdraw": Settles;
    // Write the box-local topic store, no chain and no proposal; a topic they may create was
    // written in a round with no words but the owner's, so their answers carry none but his.
    AddTopics = "add_topics": Reads, Clean;
    ReviseTopics = "revise_topics": Reads, Clean;
}
