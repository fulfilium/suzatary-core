//! The secretary's readings a page takes from its replicas, so a page reads what the box would
//! answer for the same objects. A conversation's reading takes the inbox's class by reference:
//! its objects are already in the module, and nothing crosses again.

use crate::topics::TopicReading;
use std::collections::{BTreeMap, BTreeSet};

use prodrome_dag::dag::Unread;
use prodrome_data::event::Hash;
use prodrome_data::instant::datetime_of;
use prodrome_wasm_exports::wire::parse_instant;
use suzatary_api::query;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::store::Stored;
use suzatary_inbox::secretary::inbox::Fold;
use suzatary_lines::secretary::held_lines::{Admitted, Part};
use suzatary_lines::secretary::held_replica::HeldReplica;
use suzatary_lines::secretary::held_replica::Ruling;
use suzatary_lines::secretary::held_replica::Whole;
use suzatary_lines::secretary::outlines::{Nodes, Scope};
use suzatary_memory::secretary::key_ring::KeyRing;
use suzatary_memory::secretary::locked_text::KeyName;
use suzatary_topics::secretary::readings::{self, DEFAULT_TURN_WINDOW, Dress, Window};
use suzatary_topics::secretary::stream;
use suzatary_topics::wire::topics::{MemoryPayload, Speaker};
use wasm_bindgen::prelude::*;

use crate::{Chains, Proposals, Read, json, read, refused};

/// A block of a model's answer as a page asks the box for its HTML.
#[derive(serde::Serialize)]
struct AnsweredBlock<'a> {
    conversation: &'a str,
    source: &'a str,
    key: String,
}

fn answered_block<'a>(
    conversation: &'a str,
    source: &'a str,
) -> Result<AnsweredBlock<'a>, JsError> {
    Ok(AnsweredBlock {
        conversation,
        source,
        key: crate::rendering::answer_key(source)?,
    })
}

#[wasm_bindgen]
impl Proposals {
    /// Every proposal, oldest first, with its state and its value at `at` (ISO), each priced
    /// through `chains` by the todo it serves. A remember card's words are found in the memory as
    /// its route answers it, or read as unread for `undefined`. Said beside it: the chains it was
    /// priced without (`failed`).
    pub fn inbox(
        &self,
        at: &str,
        chains: &Chains,
        memory: Option<String>,
    ) -> Result<Read, JsError> {
        let at = parse_instant("at", at).map_err(refused)?;
        let now = datetime_of(at).map_err(|e| refused(format!("at: {e}")))?;
        let memory = memory_of(memory)?;
        let inbox = readings::inbox(&self.fold()?, &chains.set(now, None), memory.as_ref());
        read(&inbox, &inbox.failed)
    }
}

/// The lines this replica holds, each print decoded once as it is held. A slot whose key is not
/// held reads as missing, and one an erasure names as erased.
#[wasm_bindgen]
pub struct Lines(pub(crate) HeldReplica);

#[wasm_bindgen]
impl Lines {
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Lines {
        Lines(HeldReplica::default())
    }

    /// `add`, `[{hash, text}]`, each hashed against its name and held, and `drop`,
    /// `[<name>, …]`, let go.
    pub fn hold(&mut self, add: &str, drop: &str) -> Result<(), JsError> {
        self.0.lines.hold(objects(add)?, &names(drop, "drop")?);
        Ok(())
    }

    /// `add`, `[{hash, text}]`, each a literal the tab's content admission checked against its
    /// name, held without hashing it again, and `drop`, `[<name>, …]`, let go. Answers the
    /// conversations of the lines held, `[<id>, …]`: their readings moved.
    pub fn hold_admitted(&mut self, add: &str, drop: &str) -> Result<String, JsError> {
        let add = objects(add)?;
        let moved: BTreeSet<String> = add
            .iter()
            .filter_map(|(name, _)| self.0.outlines().node(name))
            .filter_map(|node| match &node.header {
                suzatary_lines::wire::lines::NodeHeader::Line { conversation, .. } => {
                    Some(conversation.clone())
                }
                suzatary_lines::wire::lines::NodeHeader::Structure { .. } => None,
            })
            .collect();
        self.0.lines.hold_admitted(
            add.into_iter()
                .map(|(name, bytes)| Admitted::vouched(name, bytes))
                .collect(),
            &names(drop, "drop")?,
        );
        json(&moved)
    }

    /// Of the conversations `ids` (`[<id>, …]`), those whose every line, and every node they rest
    /// on, is held: what a search over the words held reads, asked of the conversations a
    /// landing touched.
    pub fn whole_of(&mut self, ids: &str) -> Result<String, JsError> {
        let ids: Vec<String> =
            serde_json::from_str(ids).map_err(|e| refused(format!("ids: {e}")))?;
        let mut whole = Vec::new();
        for id in ids {
            let conversation = ConversationId::new(id).map_err(|e| refused(e.to_string()))?;
            let scope = Scope::Nodes(Nodes::Conversations(BTreeSet::from([conversation.clone()])));
            if self.0.lacking(&scope).is_empty() {
                whole.push(conversation.as_str().to_owned());
            }
        }
        json(&whole)
    }

    /// `[<name>, …]`: every print these lines hold.
    pub fn held(&self) -> Result<String, JsError> {
        json(&self.0.lines.names().map(Hash::as_str).collect::<Vec<_>>())
    }

    /// Moves with each change of the prints held: a reading of which are held is named by it.
    #[must_use]
    pub fn version(&self) -> u32 {
        self.0.lines.moves()
    }

    /// `[{hash, parents, size, header}, …]`: every node `scope` reads, of the landed row and a
    /// pull's pieces, whose print is not held, as it was given.
    pub fn lacking(&mut self, scope: &str) -> Result<String, JsError> {
        json(&self.0.lacking(&scope_of(scope)?))
    }

    /// `{nodes, lines}`: how many of the nodes `scope` reads, and of its line nodes, lack their
    /// print.
    pub fn lacks(&mut self, scope: &str) -> Result<String, JsError> {
        let lacking = self.0.lacking(&scope_of(scope)?);
        let lines = lacking
            .iter()
            .filter(|node| {
                matches!(
                    node.header,
                    suzatary_lines::wire::lines::NodeHeader::Line { .. }
                )
            })
            .count();
        json(&serde_json::json!({ "nodes": lacking.len(), "lines": lines }))
    }

    /// Whether the skeleton holds a line of conversation `id`.
    pub fn holds(&self, id: &str) -> Result<bool, JsError> {
        let id = ConversationId::new(id).map_err(|e| refused(e.to_string()))?;
        Ok(self.0.outlines().frontier(&id).is_some())
    }

    /// `{<name>: [<answer key>, …]}`: every key held that is dead (the prints name it erased, the
    /// box's rule retires it, or `told` names it), with the answer keys of the blocks its words
    /// fed: what a purge forgets, the blocks first.
    pub fn owed(&mut self, told: &str) -> Result<String, JsError> {
        let told: BTreeSet<KeyName> =
            serde_json::from_str(told).map_err(|e| refused(format!("told: {e}")))?;
        let owed: BTreeMap<KeyName, Vec<String>> = self
            .0
            .owed(&told)
            .into_iter()
            .map(|(name, blocks)| {
                let blocks = blocks
                    .iter()
                    .map(|block| crate::rendering::answer_key(block.as_str()))
                    .collect::<Result<_, _>>()?;
                Ok((name, blocks))
            })
            .collect::<Result<_, JsError>>()?;
        json(&owed)
    }

    /// `{keep, ahead}`: the names whose content the device keeps within `budget` bytes, the plan
    /// and every print held, and the plan's nodes whose print is not held, newest first.
    pub fn plan(&self, budget: usize) -> Result<String, JsError> {
        let plan = self.0.plan(budget);
        json(&serde_json::json!({
            "keep": plan.keep.iter().map(|name| name.as_str()).collect::<Vec<_>>(),
            "ahead": plan.ahead,
        }))
    }

    /// Every conversation the skeleton holds, newest first, each row's words as far as the prints
    /// held read them, with what of each stands in `inbox`.
    pub fn list(&mut self, inbox: &Proposals) -> Result<String, JsError> {
        let fold = inbox.fold()?;
        let listed = readings::listed(self.0.listed(|id, read| {
            read.map(|(rounds, row)| readings::listed_row(id, rounds, row, &fold))
                .map_err(Clone::clone)
        }))
        .map_err(|e| refused(e.to_string()))?;
        json(&listed)
    }

    /// Whether a round of conversation `id` touches the reference `about`: one boolean, whatever
    /// the conversation holds. False where the device does not hold it whole.
    pub fn talks(&mut self, id: &str, about: &str) -> Result<bool, JsError> {
        let id = ConversationId::new(id).map_err(|e| refused(e.to_string()))?;
        let Some(about) = query::about(Some(about)).map_err(|r| said(&r))? else {
            return Ok(false);
        };
        Ok(match self.0.transcript(&id) {
            Some(Whole::Read(Ok(read))) => readings::talks(&read, &about),
            Some(Whole::Read(Err(_)) | Whole::Lacking(_)) | None => false,
        })
    }

    /// One conversation's window, as the box's route reads it; a memory `undefined` reads every
    /// receipt as unread. Blocks stay their source, and no changeset is drawn.
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
        let read = match self
            .0
            .transcript(&id)
            .ok_or_else(|| refused(format!("no conversation named {id}")))?
        {
            Whole::Read(read) => read.map_err(|e| refused(e.to_string()))?,
            Whole::Lacking(lacking) => {
                return Err(refused(format!(
                    "{id} lacks {lacking} of its lines here: a reading asks for them first"
                )));
            }
        };
        let dress = Dress {
            memory: memory.as_ref(),
            ..Dress::BARE
        };
        let mut payload = readings::conversation(&read, &inbox.fold()?, &window, &dress);
        // Own lines the box does not hold: one of a turn drawn on the line it names, one refused
        // for good beside the transcript.
        let drawn: BTreeSet<&str> = payload.lines.iter().map(String::as_str).collect();
        let waiting = self
            .0
            .rulings(&id)
            .into_iter()
            .filter(|(own, ruling)| {
                matches!(ruling, Ruling::Refused { .. }) || drawn.contains(own.drawn_on().as_str())
            })
            .map(|(own, ruling)| suzatary_topics::wire::topics::Waiting {
                line: own.drawn_on().as_str().to_owned(),
                ruling: suzatary_topics::wire::topics::Ruling::of(ruling),
            })
            .collect();
        payload.waiting = waiting;
        json(&payload)
    }

    /// The box's sentence where `id` is a topic this device began and the box refused, else
    /// `undefined`: its page says so before it asks the box for the conversation.
    #[must_use]
    pub fn refused_topic(&self, id: &str) -> Option<String> {
        let id = ConversationId::new(id).ok()?;
        self.0.refused_topics().remove(&id)
    }

    /// `[{line, ruling}]`: each own line refused for good, with words the person has not acted on,
    /// of a topic this device began that the box refused, which the page that begins topics draws.
    pub fn refused_elsewhere(&self) -> Result<String, JsError> {
        json(
            &self
                .0
                .refused_elsewhere()
                .into_iter()
                .map(|(line, ruling)| suzatary_topics::wire::topics::Waiting {
                    line: line.as_str().to_owned(),
                    ruling: suzatary_topics::wire::topics::Ruling::of(ruling),
                })
                .collect::<Vec<_>>(),
        )
    }

    /// `[{hash, parents, size, header}, …]`: the node of every line whose words the newest `pages`
    /// pages of the stream read, chosen by the skeleton's outlines alone, which a page holds before
    /// it reads the stream.
    pub fn chosen(
        &self,
        topics: &TopicReading,
        chosen: &str,
        pages: u32,
    ) -> Result<String, JsError> {
        let chosen = crate::topics::topics_of(chosen)?;
        let choice = stream::choose(
            self.0.outlines().outlines(),
            &topics.0,
            &chosen,
            pages_of(pages)?,
        )
        .map_err(|e| refused(e.to_string()))?;
        json(
            &choice
                .objects()
                .iter()
                .filter_map(|name| self.0.outlines().node(name))
                .collect::<Vec<_>>(),
        )
    }

    /// The newest `pages` pages of the rounds of every conversation carrying each topic of
    /// `chosen` (a JSON array), oldest first, with how many the filter shows in all. Said beside
    /// it: each block of the rounds' answers, `{conversation, source, key}`, `key` the one the box
    /// keeps its HTML under.
    pub fn stream(
        &mut self,
        topics: &TopicReading,
        chosen: &str,
        pages: u32,
    ) -> Result<Read, JsError> {
        let chosen = crate::topics::topics_of(chosen)?;
        let pages = pages_of(pages)?;
        let (outlines, held) = self.0.reading();
        let stream = stream::stream(
            outlines.outlines(),
            |id, turn, objects| held.part(id, Part::Round(turn), objects),
            &topics.0,
            &chosen,
            pages,
            &Dress::BARE,
        )
        .map_err(|e| refused(e.to_string()))?;
        let blocks = stream
            .rounds
            .iter()
            .flat_map(|r| r.answer.iter().map(move |a| (r.conversation.as_str(), a)))
            .flat_map(|(conversation, a)| match &a.who {
                Speaker::Model { blocks } => blocks
                    .iter()
                    .map(|b| answered_block(conversation, &b.source))
                    .collect(),
                Speaker::Person { .. }
                | Speaker::Verb { .. }
                | Speaker::Act { .. }
                | Speaker::Box { .. } => Vec::new(),
            })
            .collect::<Result<Vec<_>, _>>()?;
        read(&stream, &blocks)
    }

    /// The turns holding `q`, newest first, of every conversation whose every line is held, said
    /// as read from those of all the skeleton names; `whole` where the device's pull of the
    /// skeleton ended whole, so the skeleton names every conversation.
    pub fn search(&mut self, q: &str, whole: bool) -> Result<String, JsError> {
        let counted = if whole {
            suzatary_topics::wire::topics::Counted::Every
        } else {
            suzatary_topics::wire::topics::Counted::SoFar
        };
        let (whole, of) = self.0.held_whole();
        let read: Vec<std::sync::Arc<suzatary_lines::secretary::transcript::Transcript>> = whole
            .into_iter()
            .collect::<Result<_, _>>()
            .map_err(|e| refused(e.to_string()))?;
        let stored: Vec<&Stored> = read.iter().map(|read| read.stored()).collect();
        json(&readings::search(&stored, of, counted, q).map_err(|r| said(&r))?)
    }

    /// Of the block keys `keys` (`[<key>, …]`), those a device may keep: every key of another
    /// template, and an answer key only while words a held key opens feed it. A block of words
    /// an erasure let go is kept nowhere, whenever its answer lands.
    pub fn admits(&mut self, keys: &str) -> Result<String, JsError> {
        let keys: Vec<String> =
            serde_json::from_str(keys).map_err(|e| refused(format!("keys: {e}")))?;
        let answer = format!("{} ", suzatary_todo::rendering::ANSWER.function().as_str());
        let fed = self.0.lines.fed();
        json(
            &keys
                .iter()
                .filter(|key| !key.starts_with(&answer) || fed.contains(*key))
                .collect::<Vec<_>>(),
        )
    }

    /// Every key an erasure of its own conversation names, and every key named in `told` (the
    /// names the box's pull says an erasure anywhere in the store names, which may be in a piece
    /// not yet held), each with the answer keys of the blocks its words were rendered to where
    /// `keys` (`{<name>: <hex>}`) opens it: what a replica forgets, its rendered blocks first,
    /// then the key, as the box's sweep does.
    pub fn erased(&self, keys: &str, told: &str) -> Result<String, JsError> {
        let told: BTreeSet<KeyName> =
            serde_json::from_str(told).map_err(|e| refused(format!("told: {e}")))?;
        let erased: BTreeMap<KeyName, Vec<String>> = self
            .0
            .lines
            .erased(&ring(keys)?, &told)
            .into_iter()
            .map(|(name, blocks)| {
                let blocks = blocks
                    .iter()
                    .map(|block| crate::rendering::answer_key(block.as_str()))
                    .collect::<Result<_, _>>()?;
                Ok((name, blocks))
            })
            .collect::<Result<_, JsError>>()?;
        json(&erased)
    }

    /// `{excluded}`: what the history leaves out, `[{name, why, resting}]` in name order, as every
    /// class's `tips` answers it.
    pub fn excluded(&self) -> Result<String, JsError> {
        let excluded: Vec<serde_json::Value> = self
            .0
            .lines
            .excluded(|name| self.0.outlines().holds(name))
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

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum Spelled {
    Titles,
    Whole,
    Conversations { ids: Vec<String> },
    Objects { names: Vec<String> },
    Naming { keys: Vec<String> },
}

fn scope_of(text: &str) -> Result<Scope, JsError> {
    let spelled: Spelled =
        serde_json::from_str(text).map_err(|e| refused(format!("scope: {e}")))?;
    let fail = |e: String| refused(format!("scope: {e}"));
    Ok(match spelled {
        Spelled::Titles => Scope::Titles,
        Spelled::Whole => Scope::Nodes(Nodes::Whole),
        Spelled::Conversations { ids } => Scope::Nodes(Nodes::Conversations(
            ids.into_iter()
                .map(|id| ConversationId::new(id).map_err(|e| fail(e.to_string())))
                .collect::<Result<_, _>>()?,
        )),
        Spelled::Objects { names } => Scope::Nodes(Nodes::Objects(
            names
                .into_iter()
                .map(|name| Hash::new(name).map_err(|e| fail(e.to_string())))
                .collect::<Result<_, _>>()?,
        )),
        Spelled::Naming { keys } => Scope::Nodes(Nodes::Naming(
            keys.into_iter()
                .map(|key| KeyName::new(key).map_err(|e| fail(e.to_string())))
                .collect::<Result<_, _>>()?,
        )),
    })
}

impl Default for Lines {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct Object {
    hash: String,
    text: String,
}

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
    suzatary_memory::secretary::erasure::Erasure::for_good()
}

fn ring(keys: &str) -> Result<KeyRing, JsError> {
    serde_json::from_str(keys)
        .map(|given| KeyRing::of(&given))
        .map_err(|e| refused(format!("keys: {e}")))
}

fn memory_of(memory: Option<String>) -> Result<Option<MemoryPayload>, JsError> {
    memory
        .map(|text| serde_json::from_str(&text))
        .transpose()
        .map_err(|e| refused(format!("memory: {e}")))
}

impl Proposals {
    fn fold(&self) -> Result<Fold, JsError> {
        let read = self.0.reading()?.read().map_err(refused)?;
        Ok(Fold::of(read.nodes))
    }
}

fn pages_of(pages: u32) -> Result<usize, JsError> {
    usize::try_from(pages).map_err(|e| refused(format!("pages: {e}")))
}

fn said(refusal: &suzatary_base::refusal::Refusal) -> JsError {
    refused(refusal.message())
}
