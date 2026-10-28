//! The secretary's readings a page takes from its replicas of the inbox and the conversations:
//! `suzatary_core::secretary::readings` applied to the objects the classes hold, so a page reads
//! what the box would answer for the same objects. A conversation's reading takes the inbox's
//! class by reference: its objects are already in the module, and nothing crosses again. The lines
//! are a [`Lines`], held by what changed and read by what moved.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::dag::Unread;
use prodrome_core::event::Hash;
use prodrome_core::fpl::datetime_of;
use prodrome_wasm_exports::wire::parse_instant;
use suzatary_core::query;
use suzatary_core::secretary::held_lines::{Admitted, HeldLines};
use suzatary_core::secretary::inbox::Fold;
use suzatary_core::secretary::key_ring::KeyRing;
use suzatary_core::secretary::locked_text::KeyName;
use suzatary_core::secretary::outlines::{Outlines, Parsed};
use suzatary_core::secretary::readings::{self, DEFAULT_TURN_WINDOW, Dress, Window};
use suzatary_core::secretary::store::{ConversationId, Stored};
use suzatary_core::secretary::stream;
use suzatary_core::wire::MemoryPayload;
use wasm_bindgen::prelude::*;

use crate::{Chains, Proposals, json, refused};

#[wasm_bindgen]
impl Proposals {
    /// Every proposal, oldest first, with its state and its value at `at` (ISO), each priced
    /// through `chains` by the todo it serves: what the Inbox lists. A remember card's words are
    /// found in the memory as its route answers it, or read as unread for `undefined`.
    pub fn inbox(
        &self,
        at: &str,
        chains: &Chains,
        memory: Option<String>,
    ) -> Result<String, JsError> {
        let at = parse_instant("at", at).map_err(refused)?;
        let now = datetime_of(at).map_err(|e| refused(format!("at: {e}")))?;
        let memory = memory_of(memory)?;
        json(&readings::inbox(
            &self.fold()?,
            &chains.set(now, None),
            memory.as_ref(),
        ))
    }
}

/// The lines this replica holds, read through `HeldLines`: each print decoded once as it is
/// held, and each reading computing only the conversations and lines that moved since the last.
/// A slot whose key is not held reads as missing, and one an erasure names as erased. Beside the
/// prints, the skeleton's line nodes, whose outlines the stream chooses its rounds by.
#[wasm_bindgen]
pub struct Lines {
    held: HeldLines,
    outlines: Outlines,
}

#[wasm_bindgen]
impl Lines {
    /// Nothing held.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Lines {
        Lines {
            held: HeldLines::new(),
            outlines: Outlines::default(),
        }
    }

    /// `add`, `[{hash, text}]`, each hashed against its name and held, and `drop`,
    /// `[<name>, …]`, let go.
    pub fn hold(&mut self, add: &str, drop: &str) -> Result<(), JsError> {
        self.held.hold(objects(add)?, &names(drop, "drop")?);
        Ok(())
    }

    /// `add`, `[{hash, text}]`, each a literal the tab's content admission checked against its
    /// name, held without hashing it again, and `drop`, `[<name>, …]`, let go.
    pub fn hold_admitted(&mut self, add: &str, drop: &str) -> Result<(), JsError> {
        let add = objects(add)?
            .into_iter()
            .map(|(name, bytes)| Admitted::vouched(name, bytes))
            .collect();
        self.held.hold_admitted(add, &names(drop, "drop")?);
        Ok(())
    }

    /// The skeleton's nodes `add`, as the box sent them, held, and `drop`, `[<name>, …]`, let go.
    pub fn nodes(&mut self, add: &str, drop: &str) -> Result<(), JsError> {
        let add: Vec<suzatary_core::wire::SkeletonNode> =
            serde_json::from_str(add).map_err(|e| refused(format!("nodes: {e}")))?;
        let add = add
            .into_iter()
            .map(Parsed::of)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| refused(e.to_string()))?;
        self.outlines.hold(add, &names(drop, "drop")?);
        Ok(())
    }

    /// The keys `add`, `{<name>: <hex>}`, held, and `drop`, `[<name>, …]`, let go.
    pub fn keys(&mut self, add: &str, drop: &str) -> Result<(), JsError> {
        let add: BTreeMap<KeyName, String> =
            serde_json::from_str(add).map_err(|e| refused(format!("keys: {e}")))?;
        let drop: Vec<KeyName> =
            serde_json::from_str(drop).map_err(|e| refused(format!("keys: {e}")))?;
        self.held.keys(&add, &drop);
        Ok(())
    }

    /// Every conversation, newest first, with what of each stands in `inbox`.
    pub fn list(&mut self, inbox: &Proposals) -> Result<String, JsError> {
        json(&readings::conversations(&self.stored()?, &inbox.fold()?))
    }

    /// One conversation's window, as the box's route reads it: `before`, `limit` and `about` as
    /// its query spells them, and the memory as its route answers it, or `undefined` for a
    /// memory not read, which reads every receipt as unread. Nothing else beyond the two stores
    /// dresses it: blocks stay their source, and no changeset is drawn.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "wasm-bindgen hands an optional string over by value"
    )]
    pub fn conversation(
        &mut self,
        id: &str,
        inbox: &Proposals,
        before: Option<String>,
        limit: Option<String>,
        about: Option<String>,
        memory: Option<String>,
    ) -> Result<String, JsError> {
        let id = ConversationId::new(id).map_err(|e| refused(e.to_string()))?;
        let window = Window {
            before: query::turn_before(before.as_deref()).map_err(|r| said(&r))?,
            limit: query::turn_limit(limit.as_deref(), DEFAULT_TURN_WINDOW)
                .map_err(|r| said(&r))?,
            about: query::about(about.as_deref()).map_err(|r| said(&r))?,
        };
        let memory = memory_of(memory)?;
        let stored = self
            .held
            .conversation(&id)
            .ok_or_else(|| refused(format!("no conversation named {id}")))?
            .map_err(|e| refused(e.to_string()))?;
        let dress = Dress {
            memory: memory.as_ref(),
            ..Dress::BARE
        };
        json(&readings::conversation(
            &stored,
            &inbox.fold()?,
            &window,
            &dress,
        ))
    }

    /// `[<name>, …]`: every line whose words the newest `pages` pages of the stream read, chosen by
    /// the skeleton's outlines alone, which a page holds before it reads the stream.
    pub fn chosen(
        &self,
        topics: &crate::topics::TopicReading,
        chosen: &str,
        pages: u32,
    ) -> Result<String, JsError> {
        let chosen = crate::topics::topics_of(chosen)?;
        let choice = stream::choose(
            self.outlines.outlines(),
            &topics.0,
            &chosen,
            pages_of(pages)?,
        )
        .map_err(|e| refused(e.to_string()))?;
        json(
            &choice
                .objects()
                .iter()
                .map(Hash::as_str)
                .collect::<Vec<_>>(),
        )
    }

    /// The newest `pages` pages of the rounds of every conversation carrying each topic of
    /// `chosen` (a JSON array), oldest first, with how many the filter shows in all: chosen by
    /// the skeleton's outlines, each drawn from the lines held that it reads.
    pub fn stream(
        &mut self,
        topics: &crate::topics::TopicReading,
        chosen: &str,
        pages: u32,
    ) -> Result<String, JsError> {
        let chosen = crate::topics::topics_of(chosen)?;
        let pages = pages_of(pages)?;
        let held = &mut self.held;
        json(
            &stream::stream(
                self.outlines.outlines(),
                |id, turn, objects| held.round(id, turn, objects),
                &topics.0,
                &chosen,
                pages,
                &Dress::BARE,
            )
            .map_err(|e| refused(e.to_string()))?,
        )
    }

    /// Every conversation's turns holding `q`, newest first: only words this reading opened.
    pub fn search(&mut self, q: &str) -> Result<String, JsError> {
        json(&readings::search(&self.stored()?, q).map_err(|r| said(&r))?)
    }

    /// Every key an erasure of its own conversation names, and every key named in `told` (JSON
    /// `[<name>, …]`, the names the box's pull says an erasure anywhere in the store names, which
    /// may be in a piece not yet held), each with the answer keys of the blocks its words were
    /// rendered to where `keys` (`{<name>: <hex>}`) opens it, as JSON
    /// `{<name>: [<answer key>, …]}`: what a replica forgets, its rendered blocks first, then the
    /// key, as the box's sweep does.
    pub fn erased(&self, keys: &str, told: &str) -> Result<String, JsError> {
        let told: BTreeSet<KeyName> =
            serde_json::from_str(told).map_err(|e| refused(format!("told: {e}")))?;
        let erased: BTreeMap<KeyName, Vec<String>> = self
            .held
            .erased(&ring(keys)?, &told)
            .into_iter()
            .map(|(name, blocks)| {
                let blocks = blocks
                    .iter()
                    .map(|block| crate::rendering::answer_key(block.as_str()))
                    .collect();
                (name, blocks)
            })
            .collect();
        json(&erased)
    }

    /// `[<name>, …]`: every key an erasure line of its own conversation names.
    pub fn dead(&mut self) -> Result<String, JsError> {
        json(&self.held.dead())
    }

    /// `{excluded}`: what the history leaves out, `[{name, why, resting}]` in name order, as every
    /// class's `tips` answers it: a print that does not read, and a name a held line rests on that
    /// the skeleton does not hold either.
    pub fn excluded(&self) -> Result<String, JsError> {
        let excluded: Vec<serde_json::Value> = self
            .held
            .excluded(|name| self.outlines.holds(name))
            .into_iter()
            .map(|out| {
                serde_json::json!({
                    "name": out.name.as_str(),
                    "why": out.why.as_ref().map_or_else(|| "not held".to_owned(), |why| unread(&out.name, why)),
                    "resting": out.resting.iter().map(Hash::as_str).collect::<Vec<_>>(),
                })
            })
            .collect();
        json(&serde_json::json!({ "excluded": excluded }))
    }
}

impl Default for Lines {
    fn default() -> Self {
        Self::new()
    }
}

impl Lines {
    fn stored(&mut self) -> Result<Vec<std::sync::Arc<Stored>>, JsError> {
        self.held.read().0.map_err(|e| refused(e.to_string()))
    }
}

/// One object as a page hands it over.
#[derive(serde::Deserialize)]
struct Object {
    hash: String,
    text: String,
}

/// The objects a page hands over, each beside its name.
fn objects(text: &str) -> Result<Vec<(Hash, Vec<u8>)>, JsError> {
    let add: Vec<Object> = serde_json::from_str(text).map_err(|e| refused(format!("add: {e}")))?;
    add.into_iter()
        .map(|object| {
            Hash::new(object.hash)
                .map(|name| (name, object.text.into_bytes()))
                .map_err(|e| refused(format!("add: {e}")))
        })
        .collect()
}

fn names(text: &str, what: &str) -> Result<Vec<Hash>, JsError> {
    let names: Vec<String> =
        serde_json::from_str(text).map_err(|e| refused(format!("{what}: {e}")))?;
    names
        .into_iter()
        .map(|name| Hash::new(name).map_err(|e| refused(format!("{what}: {e}"))))
        .collect()
}

/// Why the print under `name` is not an object, in the words the Prodrome's exports use.
fn unread(name: &Hash, why: &Unread) -> String {
    match why {
        Unread::Tampered(computed) => format!(
            "does not hash to its own name (tampered/corrupt): its bytes hash to {}",
            computed.as_str()
        ),
        Unread::Io(_) | Unread::NotText | Unread::Unparsed(_) => {
            format!("failed to parse: {}", why.refusal(name))
        }
    }
}

/// What an erasure does, as the box's card says it before the owner confirms one.
#[wasm_bindgen]
#[must_use]
pub fn erase_warning() -> String {
    suzatary_core::secretary::erasure::Erasure::FOR_GOOD.to_owned()
}

/// The keys a pull gave, parsed once.
fn ring(keys: &str) -> Result<KeyRing, JsError> {
    serde_json::from_str(keys)
        .map(|given| KeyRing::of(&given))
        .map_err(|e| refused(format!("keys: {e}")))
}

/// The memory as its route answers it, parsed once.
fn memory_of(memory: Option<String>) -> Result<Option<MemoryPayload>, JsError> {
    memory
        .map(|text| serde_json::from_str(&text))
        .transpose()
        .map_err(|e| refused(format!("memory: {e}")))
}

impl Proposals {
    fn fold(&self) -> Result<Fold, JsError> {
        let read = self.0.read().map_err(refused)?;
        Ok(Fold::of(read.nodes))
    }
}

fn pages_of(pages: u32) -> Result<usize, JsError> {
    usize::try_from(pages).map_err(|e| refused(format!("pages: {e}")))
}

fn said(refusal: &suzatary_core::refusal::Refusal) -> JsError {
    refused(refusal.message())
}
