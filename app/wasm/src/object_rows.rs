//! The stores of objects as the tab's worker drives them, each at its own schema. Every move
//! answers its frame; a reading is made once per move, and an append extends it in place.

use std::cell::OnceCell;

use prodrome_data::event::{Envelope, Hash, parse_envelope};
use prodrome_data::schema::Schema;
use prodrome_wasm_exports::Replica;
use suzatary_lines::secretary::object_stores;
use suzatary_lines::wire::lines::{ObjectsPayload, ReceiptPayload};
use suzatary_store::replica::held::Node as _;
use suzatary_store::replica::objects::{self, Move, Object, Objects, Policy};
use suzatary_store::replica::writes::{Answer, Refusal};
use suzatary_store::{receipt, snapshot};
use suzatary_stores::asks::{AskEvent, AskVocabulary};
use wasm_bindgen::JsError;

use crate::line_rows::{Step, hashes, whole};
use crate::{json, refused};

/// `SIGNATURES`: whether a signature is read with what it signs. A todo chain's are, since who
/// is trusted is read off them; a store's are not, so a signing keeps its reading.
pub struct Store<E: Schema, const SIGNATURES: bool = false> {
    replica: Objects,
    read: OnceCell<Result<Replica<E>, String>>,
    version: u32,
    /// The version a move last said moved: a piece that moves the policy alone is said by the
    /// landing after it.
    told: u32,
}

impl<E: Schema<Vocabulary: Default>, const SIGNATURES: bool> Default for Store<E, SIGNATURES> {
    fn default() -> Self {
        Store {
            replica: Objects::default(),
            read: OnceCell::new(),
            version: 0,
            told: 0,
        }
    }
}

fn said(moved: bool) -> &'static str {
    if moved { "\"All\"" } else { "[]" }
}

impl<E: Schema<Vocabulary: Default>, const SIGNATURES: bool> Store<E, SIGNATURES> {
    /// The frontier a pull names as `since`, `""` for none.
    pub fn since(&self) -> String {
        self.replica
            .rows()
            .holding()
            .replica()
            .frontier()
            .digest()
            .map_or_else(String::new, |d| d.as_str().to_owned())
    }

    pub fn heads(&self) -> Result<String, JsError> {
        let heads = self.replica.rows().holding().replica().read().heads();
        json(&heads.iter().map(Hash::as_str).collect::<Vec<_>>())
    }

    pub fn heard(&self) -> bool {
        self.replica.rows().landed().replica().spoken().is_some()
    }

    pub fn behind(&self, heads: &str) -> Result<bool, JsError> {
        let heads = hashes(heads, "heads")?;
        Ok(self
            .replica
            .rows()
            .holding()
            .replica()
            .read()
            .behind(&heads))
    }

    /// Whether `epoch` is later than either row heard.
    pub fn overtaken(&self, epoch: f64) -> Result<bool, JsError> {
        Ok(self.replica.rows().overtaken(whole(epoch, "epoch")?))
    }

    /// Whether `epoch` is earlier than the word held.
    pub fn went_back(&self, epoch: f64) -> Result<bool, JsError> {
        Ok(self.replica.rows().went_back(whole(epoch, "epoch")?))
    }

    /// The box's answer kept as a piece beside the landed row: the frame. A store's answer and a
    /// chain's both hold its `ObjectsPayload`, which is all a piece reads.
    pub fn piece(&mut self, answer: &str, fetched_at: f64) -> Result<Vec<u8>, JsError> {
        let answer: ObjectsPayload =
            serde_json::from_str(answer).map_err(|e| refused(format!("answer: {e}")))?;
        let policy = Policy {
            untrusted: answer.untrusted.iter().cloned().collect(),
            root: answer.root.clone(),
        };
        let said = object_stores::said::<E>(&E::Vocabulary::default(), answer)
            .map_err(|e| refused(e.to_string()))?;
        let fetched_at = whole(fetched_at, "fetched_at")?;
        let m = self.moved(|r| r.piece(said, policy, fetched_at));
        framed(&m)
    }

    /// `{untrusted, root}` as the latest word heard said them, or `null` before the box spoke.
    pub fn policy(&self) -> Result<String, JsError> {
        json(
            &self
                .replica
                .policy()
                .map(|p| serde_json::json!({ "untrusted": p.untrusted, "root": p.root })),
        )
    }

    /// The frame of letting `names` go, each unsent write named with what rests on it, made and
    /// not taken: the replica takes it only through [`Store::apply`], once the caller kept the frame.
    pub fn let_go(&self, names: &str) -> Result<Vec<u8>, JsError> {
        let names = hashes(names, "names")?;
        framed(&self.replica.letting_go(&names))
    }

    /// The frame of `literals`, a JSON array, written as the tab's own, made and not taken, as
    /// [`Store::let_go`]'s is: what an earlier build kept unsent, or what [`Store::append`] made
    /// beside its signatures. What this replica already wrote is written once: its answer stands.
    pub fn writing(&self, literals: &str) -> Result<Vec<u8>, JsError> {
        let literals: Vec<String> =
            serde_json::from_str(literals).map_err(|e| refused(format!("literals: {e}")))?;
        let schema = E::Vocabulary::default();
        let own: std::collections::BTreeSet<&Hash> =
            self.replica.own().map(|o| o.node.name()).collect();
        let objects = literals
            .into_iter()
            .map(|l| Object::read::<E>(&schema, l))
            .filter(|o| !own.contains(o.name()))
            .collect();
        framed(&Objects::writing(objects))
    }

    /// Whether the landed rows hold all they say they hold: a log that lost its early frames does
    /// not, and is taken again.
    #[must_use]
    pub fn closed(&self) -> bool {
        self.replica.closed()
    }

    /// Whether the box holds all this replica holds and the landed row holds all the box said
    /// it holds: nothing is this tab's alone, and the box's heads are the landed row's.
    pub fn settled(&self) -> bool {
        let landed = self.replica.rows().landed();
        let reading = landed.replica().read();
        self.replica.untaken().is_empty()
            && reading.word().is_some_and(|w| w.heads == reading.heads())
    }

    /// The heads the box last said, as a JSON array, or `null` before it spoke.
    pub fn box_heads(&self) -> Result<String, JsError> {
        let landed = self.replica.rows().landed();
        let reading = landed.replica().read();
        json(
            &reading
                .word()
                .map(|w| w.heads.iter().map(Hash::as_str).collect::<Vec<_>>()),
        )
    }

    /// Every standing object, less each signature where a signature asserts nothing read here.
    fn read_objects(replica: &Objects) -> impl Iterator<Item = &Object> {
        replica
            .standing()
            .into_iter()
            .filter(|o| SIGNATURES || !o.is_signature())
    }

    pub(crate) fn replica(&self) -> &Objects {
        &self.replica
    }

    pub fn land(&mut self) -> Result<Step, JsError> {
        self.step(Objects::land)
    }

    /// Every row reset; what the tab wrote stays.
    pub fn start_over(&mut self) -> Result<Step, JsError> {
        self.step(Objects::start_over)
    }

    /// `[{hash, literal}]`: `events`, a JSON array of prints, each sealed as a `Change` over the
    /// kept reading and the changes before it, made and not taken; none for a twin. The replica
    /// takes them only through [`Store::writing`] and [`Store::apply`], once the caller kept the
    /// frame, so a move the device could not keep is not held either. A failed append leaves no
    /// reading kept, so the next question makes one afresh.
    pub fn append(&mut self, events: &str) -> Result<String, JsError> {
        let events: Vec<String> =
            serde_json::from_str(events).map_err(|e| refused(format!("events: {e}")))?;
        let mut read = match self.read.take() {
            Some(read) => read.map_err(refused)?,
            None => self.fresh().map_err(refused)?,
        };
        let mut changes = Vec::new();
        for event in &events {
            let appended =
                prodrome_wasm_exports::append(&mut read, event, None).map_err(refused)?;
            let appended: Appended =
                serde_json::from_str(&appended).map_err(|e| refused(format!("append: {e}")))?;
            changes.extend(appended.objects);
        }
        // The reading holds the changes, which the replica does not until their frame is kept:
        // the next reading is made afresh.
        if changes.is_empty() {
            self.read = OnceCell::from(Ok(read));
        }
        json(
            &changes
                .iter()
                .map(|o| serde_json::json!({ "hash": o.hash, "literal": o.text }))
                .collect::<Vec<_>>(),
        )
    }

    /// `[[{hash, literal, keys}]]`: the unsent, in posts the box takes whole; an object of a store
    /// that locks no words travels with no keys.
    pub fn unsent(&self) -> Result<String, JsError> {
        json(
            &self
                .replica
                .unsent_pieces::<E>(&E::Vocabulary::default(), crate::seal::POST_FITS, |o| {
                    serde_json::to_string(o.literal()).map_or(usize::MAX, |s| s.len() + 1)
                })
                .iter()
                .map(|piece| {
                    piece
                        .iter()
                        .map(|o| serde_json::json!({ "hash": o.name().as_str(), "literal": o.literal(), "keys": [] }))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
        )
    }

    pub fn answered(&mut self, receipt: &str) -> Result<Step, JsError> {
        let receipt: ReceiptPayload =
            serde_json::from_str(receipt).map_err(|e| refused(format!("receipt: {e}")))?;
        self.step(|r| r.answered(object_stores::answers(&receipt)))
    }

    /// `[<name>, …]`: the tab's unsent changes no signature it wrote covers.
    pub fn unsigned(&self) -> Result<String, JsError> {
        json(
            &self
                .replica
                .unsigned::<E>(&E::Vocabulary::default())
                .iter()
                .map(|o| o.name().as_str())
                .collect::<Vec<_>>(),
        )
    }

    /// `[{hash, kind, reason, literal, words, signature}]`: what the box refused for good, `kind`
    /// `Final` or `Outdated`; `literal` and `words` are the lines' alone, and null here; a
    /// signature tells nothing of its own, and is marked so.
    pub fn refusals(&self) -> Result<String, JsError> {
        json(
            &self
                .replica
                .refusals()
                .iter()
                .map(|(o, kind, reason)| serde_json::json!({ "hash": o.name().as_str(), "kind": kind, "reason": reason, "literal": null, "words": null, "signature": o.is_signature() }))
                .collect::<Vec<_>>(),
        )
    }

    pub fn signed(&mut self, literal: &str) -> Result<Step, JsError> {
        let signature = Object::read::<E>(&E::Vocabulary::default(), literal.to_owned());
        self.step(|r| r.write(vec![signature]))
    }

    /// The objects `names` refused for good in the box's words `reason`.
    pub fn refuse(&mut self, names: &str, reason: &str) -> Result<Step, JsError> {
        let names = hashes(names, "names")?;
        self.step(|r| {
            r.answered(names.into_iter().map(|n| {
                (
                    n,
                    Answer::Refused {
                        kind: Refusal::Final,
                        reason: reason.to_owned(),
                    },
                )
            }))
        })
    }

    /// A frame another tab appended taken: whether a reading moved.
    pub fn apply(&mut self, frame: &[u8]) -> Result<String, JsError> {
        let m = objects::unframe(frame).map_err(|e| refused(e.to_string()))?;
        self.moved(|r| r.apply(&m));
        Ok(self.said().to_owned())
    }

    pub fn snapshot(&self) -> Result<Vec<u8>, JsError> {
        snapshot::snapshot(&self.replica).map_err(|e| refused(e.to_string()))
    }

    /// The replica replaced by `bytes` and then `frames`: whether a reading moved. Refused,
    /// holding what it held, where one does not read.
    /// What `bytes` and then `frames` hold joined into what this replica holds, so a move it made
    /// meanwhile stays: whether a reading moved. Refused, holding what it held, where one does not
    /// read.
    pub fn load(&mut self, bytes: Option<Vec<u8>>, frames: &[u8]) -> Result<String, JsError> {
        let mut kept: Objects = match bytes {
            None => Objects::default(),
            Some(bytes) => snapshot::from_snapshot(&bytes).map_err(|e| refused(e.to_string()))?,
        };
        for frame in snapshot::frames(frames).map_err(|e| refused(e.to_string()))? {
            kept.apply(&objects::unframe(frame).map_err(|e| refused(e.to_string()))?);
        }
        self.moved(|r| r.join(&kept));
        Ok(self.said().to_owned())
    }

    /// Counts the moves of what a reading reads.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The Prodrome's replica of the standing objects, made once per move.
    pub fn reading(&self) -> Result<&Replica<E>, JsError> {
        self.read
            .get_or_init(|| self.fresh())
            .as_ref()
            .map_err(refused)
    }

    fn fresh(&self) -> Result<Replica<E>, String> {
        // No schema a tab holds as objects has husks: each print is read as a stranger's.
        self.standing()
            .and_then(|o| Replica::of(&o, prodrome_wasm_exports::Sender::Stranger))
    }

    /// `{excluded}`: what the reading leaves out.
    pub fn tips(&self) -> Result<String, JsError> {
        prodrome_wasm_exports::tips(self.reading()?).map_err(refused)
    }

    /// The read objects as the Prodrome's replica reads them, `[{hash, text}]`.
    pub(crate) fn standing(&self) -> Result<String, String> {
        serde_json::to_string(
            &Self::read_objects(&self.replica)
                .map(|o| serde_json::json!({ "hash": o.name().as_str(), "text": o.literal() }))
                .collect::<Vec<_>>(),
        )
        .map_err(|e| format!("could not print the objects: {e}"))
    }

    fn step(&mut self, f: impl FnOnce(&mut Objects) -> Move) -> Result<Step, JsError> {
        let m = self.moved(f);
        Ok(Step::of(framed(&m)?, self.said().to_owned()))
    }

    /// Whether a reading moved since a move last said so.
    fn said(&mut self) -> &'static str {
        let news = self.version != self.told;
        self.told = self.version;
        said(news)
    }

    /// `f` done to the replica; a move of the standing objects or of the policy is a new version.
    fn moved<T>(&mut self, f: impl FnOnce(&mut Objects) -> T) -> T {
        let read = |r: &Objects| -> (Vec<Hash>, Option<Policy>) {
            (
                Self::read_objects(r).map(|o| o.name().clone()).collect(),
                r.policy().cloned(),
            )
        };
        let was = read(&self.replica);
        let done = f(&mut self.replica);
        if read(&self.replica) != was {
            self.read = OnceCell::new();
            self.version = self.version.wrapping_add(1);
        }
        done
    }
}

impl Store<AskEvent> {
    pub(crate) fn standing_envelopes(&self) -> Vec<(Hash, Envelope<AskEvent>)> {
        let schema = AskVocabulary::default();
        self.replica
            .standing()
            .iter()
            .filter_map(|o| Some((o.name().clone(), parse_envelope(&schema, o.literal()).ok()?)))
            .collect()
    }
}

impl<E: suzatary_stores::stores::Store<Vocabulary: Default>> Store<E> {
    /// The asks' refusals of what the tab wrote here, held as refused; none where none is new.
    pub fn refused_in(&mut self, asks: &Store<AskEvent>) -> Result<Option<Step>, JsError> {
        let objects = asks.standing_envelopes();
        let refused = receipt::refusals(objects.iter().map(|(name, o)| (name, o)), &E::KIND);
        let m = self.moved(|r| r.refused(refused));
        match &m {
            Move::Own(owned) if owned.is_empty() => Ok(None),
            Move::Own(_) | Move::Rows(_) | Move::Piece { .. } => {
                Ok(Some(Step::of(framed(&m)?, self.said().to_owned())))
            }
        }
    }
}

impl<E: Schema<Vocabulary: Default>, const SIGNATURES: bool> Store<E, SIGNATURES> {
    /// `[{hash, reason, literal, words, at, signature}]`: what the box has not taken yet, in the
    /// rule's words; `literal`, `words` and `at` are the lines' alone, and null here; a signature
    /// tells nothing of its own, and is marked so.
    pub fn waiting(&self) -> Result<String, JsError> {
        json(
            &self
                .replica
                .waiting()
                .iter()
                .map(|(o, reason)| serde_json::json!({ "hash": o.name().as_str(), "reason": reason, "literal": null, "words": null, "at": null, "signature": o.is_signature() }))
                .collect::<Vec<_>>(),
        )
    }
}

fn framed(m: &Move) -> Result<Vec<u8>, JsError> {
    objects::frame(m).map_err(|e| refused(e.to_string()))
}

/// What the Prodrome's `append` answers.
#[derive(serde::Deserialize)]
struct Appended {
    objects: Vec<Text>,
}

#[derive(serde::Deserialize)]
struct Text {
    hash: String,
    text: String,
}

/// A store's exports at one schema: a macro because `#[wasm_bindgen]` exports no generic item. A
/// store `unread` exports no register readings, which only a schema with a JSON view answers.
/// Only a store the stores' table says a tab writes (`Written`) is `written` and exports
/// `waiting`; a store not named so is asserted the box's alone.
macro_rules! objects_store {
    ($name:ident = $schema:ty, written) => {
        $crate::object_rows::objects_store!(@base $name = $schema, false);
        $crate::object_rows::objects_store!(@refusable $name);
        $crate::object_rows::objects_store!(@readings $name);
        $crate::object_rows::objects_store!(@waiting $name);
    };
    // A todo chain: a tab writes it, under the policy the box serves it with; the asks name none
    // of its refusals, which its own receipts carry.
    ($name:ident = $schema:ty, chain) => {
        $crate::object_rows::objects_store!(@base $name = $schema, true);
        $crate::object_rows::objects_store!(@waiting $name);

        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $name {
            pub fn policy(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.policy()
            }
            pub fn let_go(&self, names: &str) -> Result<Vec<u8>, wasm_bindgen::JsError> {
                self.0.let_go(names)
            }
            #[must_use]
            pub fn settled(&self) -> bool {
                self.0.settled()
            }
            pub fn box_heads(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.box_heads()
            }
        }
    };
    (@waiting $name:ident) => {
        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $name {
            pub fn waiting(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.waiting()
            }
        }
    };
    (@refusable $name:ident) => {
        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $name {
            pub fn refused_in(
                &mut self,
                asks: &$crate::asks::Asks,
            ) -> Result<Option<$crate::line_rows::Step>, wasm_bindgen::JsError> {
                self.0.refused_in(&asks.0)
            }
        }
    };
    ($name:ident = $schema:ty) => {
        $crate::object_rows::objects_store!($name = $schema, unread);
        $crate::object_rows::objects_store!(@readings $name);
    };
    ($name:ident = $schema:ty, unread) => {
        $crate::object_rows::objects_store!(@base $name = $schema, false);
        $crate::object_rows::objects_store!(@refusable $name);
        const _: () = assert!(
            matches!(
                <$schema as suzatary_stores::stores::Store>::KIND.writer(),
                suzatary_stores::stores::Writer::Box
            ),
            "a store a tab writes is `written`"
        );
    };
    (@readings $name:ident) => {
        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $name {
            /// Every entity's registers at `at` (ISO, or `null`) under `untrusted`.
            pub fn readings(
                &self,
                at: Option<String>,
                untrusted: &str,
            ) -> Result<String, wasm_bindgen::JsError> {
                prodrome_wasm_exports::thrown(prodrome_wasm_exports::readings(
                    self.0.reading()?,
                    at,
                    untrusted,
                ))
            }
        }
    };
    (@base $name:ident = $schema:ty, $signatures:literal) => {
        #[doc = concat!("The store of objects at `", stringify!($schema), "` the tab holds.")]
        #[wasm_bindgen::prelude::wasm_bindgen]
        #[derive(Default)]
        pub struct $name(pub(crate) $crate::object_rows::Store<$schema, $signatures>);

        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $name {
            #[wasm_bindgen(constructor)]
            #[must_use]
            pub fn new() -> $name {
                $name::default()
            }
            #[must_use]
            pub fn since(&self) -> String {
                self.0.since()
            }
            pub fn heads(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.heads()
            }
            #[must_use]
            pub fn heard(&self) -> bool {
                self.0.heard()
            }
            pub fn behind(&self, heads: &str) -> Result<bool, wasm_bindgen::JsError> {
                self.0.behind(heads)
            }
            pub fn overtaken(&self, epoch: f64) -> Result<bool, wasm_bindgen::JsError> {
                self.0.overtaken(epoch)
            }
            pub fn went_back(&self, epoch: f64) -> Result<bool, wasm_bindgen::JsError> {
                self.0.went_back(epoch)
            }
            pub fn piece(
                &mut self,
                answer: &str,
                fetched_at: f64,
            ) -> Result<Vec<u8>, wasm_bindgen::JsError> {
                self.0.piece(answer, fetched_at)
            }
            pub fn land(&mut self) -> Result<$crate::line_rows::Step, wasm_bindgen::JsError> {
                self.0.land()
            }
            pub fn start_over(&mut self) -> Result<$crate::line_rows::Step, wasm_bindgen::JsError> {
                self.0.start_over()
            }
            pub fn append(&mut self, events: &str) -> Result<String, wasm_bindgen::JsError> {
                self.0.append(events)
            }
            pub fn writing(&self, literals: &str) -> Result<Vec<u8>, wasm_bindgen::JsError> {
                self.0.writing(literals)
            }
            #[must_use]
            pub fn closed(&self) -> bool {
                self.0.closed()
            }
            pub fn unsent(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.unsent()
            }
            pub fn unsigned(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.unsigned()
            }
            pub fn refusals(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.refusals()
            }
            pub fn signed(
                &mut self,
                literal: &str,
            ) -> Result<$crate::line_rows::Step, wasm_bindgen::JsError> {
                self.0.signed(literal)
            }
            pub fn answered(
                &mut self,
                receipt: &str,
            ) -> Result<$crate::line_rows::Step, wasm_bindgen::JsError> {
                self.0.answered(receipt)
            }
            pub fn refuse(
                &mut self,
                names: &str,
                reason: &str,
            ) -> Result<$crate::line_rows::Step, wasm_bindgen::JsError> {
                self.0.refuse(names, reason)
            }
            pub fn apply(&mut self, frame: &[u8]) -> Result<String, wasm_bindgen::JsError> {
                self.0.apply(frame)
            }
            pub fn snapshot(&self) -> Result<Vec<u8>, wasm_bindgen::JsError> {
                self.0.snapshot()
            }
            pub fn load(
                &mut self,
                bytes: Option<Vec<u8>>,
                frames: &[u8],
            ) -> Result<String, wasm_bindgen::JsError> {
                self.0.load(bytes, frames)
            }
            #[must_use]
            pub fn version(&self) -> u32 {
                self.0.version()
            }
            pub fn tips(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.tips()
            }
        }
    };
}
pub(crate) use objects_store;

#[cfg(test)]
#[path = "object_rows_signed.rs"]
mod signed;

/// Whether `bytes`, a snapshot or a frame printed under a replica's form, are of `form` (`"This"`),
/// of another (`"Other"`, an earlier build's), or too short to name one (`"None"`).
pub(crate) fn form_of(form: u32, bytes: &[u8]) -> &'static str {
    match bytes.first_chunk::<4>() {
        None => "None",
        Some(found) if u32::from_le_bytes(*found) == form => "This",
        Some(_) => "Other",
    }
}

/// `"This"`, `"Other"` or `"None"`: whether `bytes`, a snapshot or a frame of a store of objects (a
/// chain's among them), are of the form this build reads, of an earlier build's, or too short to
/// name one.
#[wasm_bindgen::prelude::wasm_bindgen]
#[must_use]
pub fn objects_form(bytes: &[u8]) -> String {
    form_of(<Objects as snapshot::Held>::FORM, bytes).to_owned()
}
