use std::collections::BTreeMap;

use prodrome_core::declared::Declared;

use super::{Doc, Kind, Log, Prodrome, Store};
use crate::attachment::AttachmentId;
use crate::chains::ChainName;
use crate::decisions::DecisionEvent;
use crate::devices::Device;
use crate::mail::local_state::LocalState;
use crate::mail::state::TriageState;
use crate::plugin::PluginName;
use crate::record::TodoEvent;
use crate::secretary::digest_event::DigestEvent;
use crate::secretary::digest_line::Line;
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::line_event::LineEvent;
use crate::secretary::locked_text::{KeyName, TextKey};
use crate::secretary::memory::Fact;
use crate::secretary::memory_event::MemoryEvent;
use crate::secretary::salted_text::{Text, TextName};
use crate::secretary::topics::alias_event::AliasEvent;
use crate::secretary::topics::membership_event::MembershipEvent;
use crate::upload::Upload;

/// The whole store, as one value: every chain, and the box's own stores.
#[derive(Clone)]
pub struct Database<'a> {
    /// Each chain by name, in the instance file's order of meaning.
    pub chains: BTreeMap<ChainName, &'a dyn Prodrome<TodoEvent>>,
    /// The proposals a person taps.
    pub inbox: &'a dyn Prodrome<InboxEvent>,
    /// The secretary's conversations, one prodrome keyed by conversation, every word locked.
    pub conversations: &'a dyn Prodrome<LineEvent>,
    /// The key of each slot of the conversations' words not erased: box-local, vaulted, never
    /// pushed, and destroyed by the lines' sweep.
    pub line_keys: &'a dyn Doc<KeyName, TextKey>,
    /// The secretary's memory: which facts were remembered and forgotten, by id.
    pub memory: &'a dyn Prodrome<MemoryEvent>,
    /// Each standing fact's words, by the name its remembering gives them; forgetting deletes.
    pub memory_texts: &'a dyn Doc<TextName, Text<Fact>>,
    /// The digest: what was said of each node of the conversations' summary tree, by node.
    pub digests: &'a dyn Prodrome<DigestEvent>,
    /// Each summary's words, by the name its event gives them; the sweep deletes the dead.
    pub digest_texts: &'a dyn Doc<TextName, Text<Line>>,
    /// The recorded decisions: designs and plans as items, keyed by decision.
    pub decisions: &'a dyn Prodrome<DecisionEvent>,
    /// The topics on the secretary's rounds and notes.
    pub topics: &'a dyn Prodrome<MembershipEvent>,
    /// The topics' aliases.
    pub topic_aliases: &'a dyn Prodrome<AliasEvent>,
    /// What each plugin learned, by its name: a cache-tier replica at the plugin's declared
    /// schema, rebuilt by syncing again.
    pub plugins: BTreeMap<PluginName, &'a dyn Prodrome<Declared>>,
    /// The triage's dedup authority, pushed: decisions under keyed names.
    pub triage: &'a dyn Doc<(), TriageState>,
    /// What the box keeps of its mail beside the decisions, box-local and never pushed.
    pub triage_local: &'a dyn Doc<(), LocalState>,
    /// Each actor's phones.
    pub devices: &'a dyn Log<String, Device>,
    /// The owner's uploads.
    pub uploads: &'a dyn Doc<AttachmentId, Upload>,
    /// The files a verb kept for the model.
    pub kept: &'a dyn Doc<AttachmentId, Vec<u8>>,
}

impl Database<'_> {
    /// Every prodrome's fsck findings, each under the name [`Kind::name`] gives its store, a
    /// chain's and a plugin's after their own names.
    #[must_use]
    pub fn verify(&self) -> Vec<(String, Vec<String>)> {
        self.chains
            .iter()
            .map(|(name, chain)| (format!("{name} {}", Kind::Chain.name()), chain.verify()))
            .chain([
                found(self.inbox, crate::secretary::inbox::findings(self.inbox)),
                found(
                    self.conversations,
                    crate::secretary::transcript::findings(self.conversations, self.inbox),
                ),
                found(self.memory, self.memory.verify()),
                found(self.digests, self.digests.verify()),
                found(self.decisions, self.decisions.verify()),
                found(self.topics, self.topics.verify()),
                found(self.topic_aliases, self.topic_aliases.verify()),
            ])
            .chain(
                self.plugins.iter().map(|(name, store)| {
                    (format!("{} {name}", Kind::Plugin.name()), store.verify())
                }),
            )
            .collect()
    }
}

/// `findings` under the name of `store`'s kind.
fn found<E: Store>(_: &dyn Prodrome<E>, findings: Vec<String>) -> (String, Vec<String>) {
    (E::KIND.name().to_owned(), findings)
}
