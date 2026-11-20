//! The lines this tab writes, as the worker drives them: each sealed as the box seals it, its
//! words through the redaction door and locked under keys drawn from bytes the worker draws
//! (`crypto.getRandomValues`), and handed over by the engine every store a tab writes shares
//! (`web/src/prodrome/written.ts`), each slot's key beside its line.
//!
//! Every move of the own lines answers a [`Step`] whose `delta` is the device's record of them
//! (`[{literal, keys, answer}]`, UTF-8 JSON), which the worker keeps apart from the rows' log, in
//! a form no build changes, so no snapshot of another build, reset or upgrade drops an unsent line.

use std::collections::BTreeMap;
use std::sync::Mutex;

use prodrome_core::event::{Actor, Hash};
use prodrome_core::fpl::datetime_of;
use prodrome_wasm_exports::wire::parse_instant;
use suzatary_base::capability::Entropy;
use suzatary_base::conversation_id::ConversationId;
use suzatary_connect::connect::TemplateId;
use suzatary_conversations::secretary::store::Title;
use suzatary_lines::secretary::held_replica::{Moved, Sealing};
use suzatary_lines::secretary::object_stores;
use suzatary_lines::secretary::own_line::OwnLine;
use suzatary_lines::wire::lines::{NodeHeader, ReceiptPayload};
use suzatary_memory::secretary::locked_text::KeyName;
use suzatary_store::receipt;
use suzatary_store::replica::held::Node as _;
use suzatary_store::replica::writes::{Answer, Own, Refusal};
use wasm_bindgen::prelude::*;

use crate::line_rows::{Step, hashes};
use crate::secretary::Lines;
use crate::{json, refused};

/// Bytes the worker drew, handed out once each; a seal that needs more is refused.
struct Drawn(Mutex<Vec<u8>>);

impl Entropy for Drawn {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        let mut drawn = self
            .0
            .lock()
            .map_err(|_| "the drawn bytes are poisoned".to_owned())?;
        if drawn.len() < bytes.len() {
            return Err(format!(
                "{} bytes drawn, {} wanted",
                drawn.len(),
                bytes.len()
            ));
        }
        let rest = drawn.split_off(bytes.len());
        bytes.copy_from_slice(&drawn);
        *drawn = rest;
        Ok(())
    }
}

/// A line a page asks the tab to seal, as the page spells it.
#[derive(serde::Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum Asked {
    Rename {
        conversation: String,
        title: String,
    },
    Open {
        title: String,
    },
    SetUp {
        template: String,
    },
    Say {
        conversation: String,
        text: String,
        references: Vec<String>,
        files: Vec<Filed>,
    },
    Stop {
        conversation: String,
    },
}

/// A file of a question: the upload's answer, and the name the person gave it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Filed {
    attachment: suzatary_held::attachment::Attachment,
    name: String,
}

/// One own write as the device keeps it.
#[derive(serde::Serialize, serde::Deserialize)]
struct Record {
    literal: String,
    keys: BTreeMap<KeyName, String>,
    answer: Answer,
}

/// A line sealed: its move (as a [`Step`] reads), its name and its conversation. Free it once
/// read.
#[wasm_bindgen(getter_with_clone)]
pub struct Sealed {
    /// The device's record of the own lines it joined in.
    pub delta: Vec<u8>,
    /// What it moved.
    pub moved: String,
    /// The line's name.
    pub name: String,
    /// Its conversation: a new one's id is its opening line's.
    pub conversation: String,
}

#[wasm_bindgen]
impl Lines {
    /// `asked` (`{type: "Rename", conversation, title}`, `{type: "Open", title}`,
    /// `{type: "SetUp", template}`, `{type: "Say", conversation, text, references, files}` or
    /// `{type: "Stop", conversation}`) sealed by `actor` at `at` (ISO), its keys drawn from
    /// `entropy`, bytes the worker drew: kept unsent and read at once.
    pub fn seal(
        &mut self,
        asked: &str,
        at: &str,
        actor: &str,
        entropy: &[u8],
    ) -> Result<Sealed, JsError> {
        let asked: Asked =
            serde_json::from_str(asked).map_err(|e| refused(format!("a seal: {e}")))?;
        let sealing = match asked {
            Asked::Rename {
                conversation,
                title,
            } => Sealing::Rename {
                conversation: ConversationId::new(conversation)
                    .map_err(|e| refused(e.to_string()))?,
                title: Title::new(title).map_err(|e| refused(e.to_string()))?,
            },
            Asked::Open { title } if title.trim().is_empty() => Sealing::Open { title: None },
            Asked::Open { title } => Sealing::Open {
                title: Some(Title::new(title).map_err(|e| refused(e.to_string()))?),
            },
            Asked::SetUp { template } => {
                Sealing::SetUp(TemplateId::parse(&template).map_err(|e| refused(e.to_string()))?)
            }
            Asked::Say {
                conversation,
                text,
                references,
                files,
            } => Sealing::Say {
                conversation: ConversationId::new(conversation)
                    .map_err(|e| refused(e.to_string()))?,
                text,
                references: references
                    .iter()
                    .map(|r| r.parse().map_err(|e| refused(format!("a reference: {e}"))))
                    .collect::<Result<_, _>>()?,
                // A file on the box takes the person's name for it; a private one is held as
                // it was kept.
                attachments: files
                    .into_iter()
                    .map(|Filed { attachment, name }| match attachment.private {
                        Some(_) => Ok(attachment),
                        None => attachment
                            .named(&name)
                            .map_err(|e| refused(format!("a file's name: {e}"))),
                    })
                    .collect::<Result<_, _>>()?,
                seat: suzatary_signatures::signatures::Seat::Device,
            },
            Asked::Stop { conversation } => Sealing::Stop {
                conversation: ConversationId::new(conversation)
                    .map_err(|e| refused(e.to_string()))?,
            },
        };
        let at = datetime_of(parse_instant("at", at).map_err(refused)?)
            .map_err(|e| refused(format!("at: {e}")))?;
        let actor = Actor::new(actor).map_err(|e| refused(format!("actor: {e}")))?;
        let drawn = Drawn(Mutex::new(entropy.to_vec()));
        let (owned, name, moved) = self
            .0
            .seal(sealing, at, &actor, &drawn)
            .map_err(|e| refused(e.to_string()))?;
        let conversation = owned
            .iter()
            .find_map(|own| match &own.node {
                OwnLine::Line { node, .. } => match &node.given().header {
                    NodeHeader::Line { conversation, .. } => Some(conversation.clone()),
                    NodeHeader::Structure { .. } => None,
                },
                OwnLine::Signature { .. } => None,
            })
            .unwrap_or_default();
        Ok(Sealed {
            delta: records(&owned)?,
            moved: said(&moved)?,
            name: name.as_str().to_owned(),
            conversation,
        })
    }

    /// The device's record of own writes, `[{literal, keys, answer}]`, taken: what it kept, or
    /// another tab's. A record that does not read is passed over and named in the answer's
    /// `unread`: `{moved, unread, taken, fresh}`, `fresh` how many records were new here or raised
    /// an answer, `taken` each own write the box took as
    /// `{hash, literal, line, free}`: a line's print is content the device keeps, and `free` says
    /// its record may be let go, once the landed rows hold its keys.
    pub fn take_own(&mut self, records: &str) -> Result<String, JsError> {
        let records: Vec<serde_json::Value> =
            serde_json::from_str(records).map_err(|e| refused(format!("own lines: {e}")))?;
        let mut owned = Vec::new();
        let mut unread = Vec::new();
        for record in records {
            let read = serde_json::from_value::<Record>(record)
                .map_err(|e| e.to_string())
                .and_then(|r| {
                    OwnLine::read(r.literal, &r.keys).map(|node| Own {
                        node,
                        answer: r.answer,
                    })
                });
            match read {
                Ok(own) => owned.push(own),
                Err(why) => unread.push(why),
            }
        }
        let (moved, fresh) = self.0.take_own(&owned);
        json(&serde_json::json!({
            "moved": reading(&moved),
            "unread": unread,
            "taken": self.taken(),
            "fresh": fresh,
        }))
    }

    /// `[{hash, literal, line, free}]`: each own write the box took, as `take_own` answers it.
    pub fn own_taken(&self) -> Result<String, JsError> {
        json(&self.taken())
    }

    fn taken(&self) -> Vec<serde_json::Value> {
        self.0
            .own_taken()
            .into_iter()
            .map(|(own, free)| {
                serde_json::json!({
                    "hash": own.name().as_str(),
                    "literal": own.print(),
                    "line": matches!(own, OwnLine::Line { .. }),
                    "free": free,
                })
            })
            .collect()
    }

    /// `[[{hash, literal, keys}]]`: the own writes the box may take, in posts it takes whole,
    /// each line beside its signatures, `keys` each slot's key in hex in slot order.
    pub fn unsent(&self) -> Result<String, JsError> {
        json(
            &self
                .0
                .unsent(POST_FITS, |own| posted(own).to_string().len() + 1)
                .iter()
                .map(|piece| piece.iter().map(|own| posted(own)).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
        )
    }

    /// `[<name>, …]`: the own lines no signature written here covers.
    pub fn unsigned(&self) -> Result<String, JsError> {
        json(
            &self
                .0
                .unsigned()
                .iter()
                .map(|own| own.name().as_str())
                .collect::<Vec<_>>(),
        )
    }

    /// The device's `Signed` of an own line, kept unsent.
    pub fn signed(&mut self, literal: &str) -> Result<Step, JsError> {
        let own = OwnLine::read(literal.to_owned(), &BTreeMap::new()).map_err(refused)?;
        let (owned, moved) = self.0.answer_own(|w| w.write(vec![own]));
        own_step(&owned, &moved)
    }

    /// The box's `ReceiptPayload` for a post of own writes.
    pub fn answered(&mut self, receipt: &str) -> Result<Step, JsError> {
        let receipt: ReceiptPayload =
            serde_json::from_str(receipt).map_err(|e| refused(format!("receipt: {e}")))?;
        let (owned, moved) = self.0.answered(object_stores::answers(&receipt));
        own_step(&owned, &moved)
    }

    /// The own writes `names` refused for good in the box's words `reason`.
    pub fn refuse(&mut self, names: &str, reason: &str) -> Result<Step, JsError> {
        let names = hashes(names, "names")?;
        let answers: Vec<(Hash, Answer)> = names
            .into_iter()
            .map(|name| {
                let answer = Answer::Refused {
                    kind: Refusal::Final,
                    reason: reason.to_owned(),
                };
                (name, answer)
            })
            .collect();
        let (owned, moved) = self.0.answered(answers);
        own_step(&owned, &moved)
    }

    /// The box's answer to the own write `name`: `"Unsent"`, `{"Refused": {kind, reason}}` or
    /// `"Taken"`; `"Taken"` too for one this tab no longer holds as its own, which only the box's
    /// rows hold.
    pub fn answer(&self, name: &str) -> Result<String, JsError> {
        let name = Hash::new(name).map_err(|e| refused(e.to_string()))?;
        let answer = self
            .0
            .own()
            .iter()
            .find(|own| *own.node.name() == name)
            .map_or(Answer::Taken, |own| own.answer.clone());
        json(&answer)
    }

    /// `[{hash, kind, reason, literal, words, signature}]`: every own write a refusal for good
    /// holds back, `kind` `Final` or `Outdated`, `literal` its print as the device's row keeps it,
    /// `words` what the person wrote in it, so they can copy them once the box refused them, and
    /// whether it is a signature, which tells nothing of its own.
    pub fn refusals(&self) -> Result<String, JsError> {
        json(
            &self
                .0
                .own()
                .refusals()
                .iter()
                .map(|(own, kind, reason)| {
                    serde_json::json!({
                        "hash": own.name().as_str(),
                        "kind": kind,
                        "reason": reason,
                        "literal": own.print(),
                        "words": words(own),
                        "signature": matches!(own.role(), suzatary_store::replica::writes::Role::Signs(_)),
                    })
                })
                .collect::<Vec<_>>(),
        )
    }

    /// `[{hash, reason, literal, words, signature}]`: every own write the box has not taken yet,
    /// in the rule's words, with its print and the words it says; a signature tells nothing of its
    /// own, and is marked so.
    pub fn waiting(&self) -> Result<String, JsError> {
        json(
            &self
                .0
                .own()
                .waiting()
                .iter()
                .map(|(own, reason)| {
                    serde_json::json!({
                        "hash": own.name().as_str(),
                        "reason": reason,
                        "literal": own.print(),
                        "words": words(own),
                        "signature": matches!(own.role(), suzatary_store::replica::writes::Role::Signs(_)),
                    })
                })
                .collect::<Vec<_>>(),
        )
    }

    /// The asks' refusals of the own writes, held as refused; none where none is new.
    pub fn refused_in(&mut self, asks: &crate::asks::Asks) -> Result<Option<Step>, JsError> {
        let refused = receipt::refusals(
            asks.0
                .standing_envelopes()
                .iter()
                .map(|(name, o)| (name, o)),
            &suzatary_stores::stores::Kind::Lines,
        );
        let (owned, moved) = self.0.answer_own(|w| w.refused(refused));
        if owned.is_empty() {
            return Ok(None);
        }
        own_step(&owned, &moved).map(Some)
    }
}

/// One own write as a post carries it.
fn posted(own: &OwnLine) -> serde_json::Value {
    let keys = own.keys();
    let in_order: Vec<&String> = match own {
        OwnLine::Line { node, .. } => match &node.given().header {
            NodeHeader::Line { keys: names, .. } => names
                .iter()
                .filter_map(|name| {
                    keys.iter()
                        .find(|(k, _)| k.as_str() == name)
                        .map(|(_, v)| v)
                })
                .collect(),
            NodeHeader::Structure { .. } => Vec::new(),
        },
        OwnLine::Signature { .. } => Vec::new(),
    };
    serde_json::json!({ "hash": own.name().as_str(), "literal": own.print(), "keys": in_order })
}

/// What one post of a tab's writes may carry: the route's count, and its body's bytes less room
/// for what a post holds beside its objects (its `since`, its brackets, each key's name).
pub(crate) const POST_FITS: suzatary_store::replica::writes::Fit =
    suzatary_store::replica::writes::Fit {
        most: suzatary_api::routes::MAX_POSTED,
        bytes: suzatary_api::routes::MAX_POSTED_BYTES / 2,
    };

/// A move of the own writes: the device's record of what it joined in, and what it moved.
fn own_step(owned: &[Own<OwnLine>], moved: &Moved) -> Result<Step, JsError> {
    Ok(Step::of(records(owned)?, said(moved)?))
}

/// The device's record of `owned`, as UTF-8 JSON.
fn records(owned: &[Own<OwnLine>]) -> Result<Vec<u8>, JsError> {
    let records: Vec<Record> = owned
        .iter()
        .map(|own| Record {
            literal: own.node.print().to_owned(),
            keys: own.node.keys(),
            answer: own.answer.clone(),
        })
        .collect();
    Ok(json(&records)?.into_bytes())
}

fn said(moved: &Moved) -> Result<String, JsError> {
    json(&reading(moved))
}

/// What `moved` moved, as a JSON value: `"All"`, or the conversations' ids.
fn reading(moved: &Moved) -> serde_json::Value {
    match moved {
        Moved::All => serde_json::json!("All"),
        Moved::These(ids) => {
            serde_json::json!(ids.iter().map(ConversationId::as_str).collect::<Vec<_>>())
        }
    }
}

/// What the person wrote in `own`, opened with its own keys: a question's words or a title.
fn words(own: &OwnLine) -> Option<String> {
    use suzatary_conversations::secretary::model::Turn;
    use suzatary_conversations::secretary::store::Entry;
    let OwnLine::Line { print, keys, .. } = own else {
        return None;
    };
    let envelope =
        prodrome_core::event::parse_envelope::<suzatary_lines::secretary::line_event::LineEvent>(
            &suzatary_lines::secretary::line_event::LineVocabulary,
            print,
        )
        .ok()?;
    let line = envelope.event()?;
    match line.entry(&|name: &KeyName| keys.key(name)) {
        Entry::Said {
            turn: Turn::Person { text, .. },
            ..
        } => Some(text),
        Entry::Titled { title, .. } => title.get().map(|title| title.as_str().to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a move moved is a JSON value inside an answer, never a JSON string of one: the tab
    /// reads `"All"` or the ids, and a string of them failed its decoder.
    #[test]
    fn a_move_reads_as_a_value() {
        let ids = std::collections::BTreeSet::from([ConversationId::new("garden").expect("an id")]);
        assert_eq!(reading(&Moved::These(ids)), serde_json::json!(["garden"]));
        assert_eq!(reading(&Moved::All), serde_json::json!("All"));
    }
}
