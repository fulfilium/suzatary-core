//! The inbox and the two topic stores as the tab's worker drives them: each a replica of objects
//! the core holds (`suzatary_core::replica::objects`), at its own schema. Every move answers the
//! frame the worker appends to the device's log, and whether it moved what a reading reads; a
//! warm load hands the snapshot and the frames after it back. The worker keeps nothing of a store
//! but the handle: every question is asked here.
//!
//! A reading reads the store's standing objects through the Prodrome's own replica at the
//! schema, made once per move of them and kept until the next ([`Store::reading`]), so every
//! reading of one holding reads one replica, and the store's `version` names it.

use std::cell::OnceCell;

use prodrome_core::schema::Schema;
use prodrome_wasm_exports::Replica;
use suzatary_core::replica::held::Node as _;
use suzatary_core::replica::objects::{self, Answer, Move, Object, Objects, Refusal};
use suzatary_core::secretary::object_stores;
use suzatary_core::snapshot;
use suzatary_core::wire::{ReceiptPayload, StorePayload};
use wasm_bindgen::JsError;

use crate::line_rows::{Step, hashes, whole};
use crate::{json, refused};

/// A store of objects at the schema `E`, and its reading.
pub struct Store<E: Schema> {
    replica: Objects,
    read: OnceCell<Result<Replica<E>, String>>,
    version: u32,
}

impl<E: Schema<Vocabulary: Default>> Default for Store<E> {
    fn default() -> Self {
        Store {
            replica: Objects::default(),
            read: OnceCell::new(),
            version: 0,
        }
    }
}

/// `moved` as a step says it, the way the lines say a move that touched every conversation.
fn said(moved: bool) -> &'static str {
    if moved { "\"All\"" } else { "[]" }
}

impl<E: Schema<Vocabulary: Default>> Store<E> {
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

    /// `[<name>, …]`: the heads of what is held, landed and not.
    pub fn heads(&self) -> Result<String, JsError> {
        let heads = self.replica.rows().holding().replica().read().heads();
        json(
            &heads
                .iter()
                .map(prodrome_core::event::Hash::as_str)
                .collect::<Vec<_>>(),
        )
    }

    /// Whether the landed row has heard the box.
    pub fn heard(&self) -> bool {
        self.replica.rows().landed().replica().spoken().is_some()
    }

    /// Whether the box's heads `heads` name an object what is held does not read.
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

    /// Whether an answer of `epoch` is of a later epoch than either row heard.
    pub fn overtaken(&self, epoch: f64) -> Result<bool, JsError> {
        Ok(self.replica.rows().overtaken(whole(epoch, "epoch")?))
    }

    /// Whether an answer of `epoch` is of an earlier epoch than the word held.
    pub fn went_back(&self, epoch: f64) -> Result<bool, JsError> {
        Ok(self.replica.rows().went_back(whole(epoch, "epoch")?))
    }

    /// The box's answer `answer` (`StorePayload` JSON) kept as a piece beside the landed row,
    /// heard at `fetched_at`: the frame. It moves no reading.
    pub fn piece(&mut self, answer: &str, fetched_at: f64) -> Result<Vec<u8>, JsError> {
        let answer: StorePayload =
            serde_json::from_str(answer).map_err(|e| refused(format!("answer: {e}")))?;
        let said = object_stores::said::<E>(&E::Vocabulary::default(), answer.objects)
            .map_err(|e| refused(e.to_string()))?;
        let m = self.replica.piece(said, whole(fetched_at, "fetched_at")?);
        framed(&m)
    }

    /// The pending pieces landed.
    pub fn land(&mut self) -> Result<Step, JsError> {
        self.step(Objects::land)
    }

    /// Every row reset, in the generation after both; what the tab wrote stays.
    pub fn start_over(&mut self) -> Result<Step, JsError> {
        self.step(Objects::start_over)
    }

    /// `event`, its print, sealed as a `Change` over the reading's own fold and kept as this
    /// tab's, unsent: the frame, or none for an event the store already holds.
    pub fn append(&mut self, event: &str) -> Result<Option<Step>, JsError> {
        let mut read = Replica::<E>::of(&self.standing().map_err(refused)?).map_err(refused)?;
        let appended: Appended = serde_json::from_str(
            &prodrome_wasm_exports::append(&mut read, event, None).map_err(refused)?,
        )
        .map_err(|e| refused(format!("append: {e}")))?;
        if appended.objects.is_empty() {
            return Ok(None);
        }
        let schema = E::Vocabulary::default();
        let objects = appended
            .objects
            .into_iter()
            .map(|o| Object::read::<E>(&schema, o.text))
            .collect();
        self.step(|r| r.write(objects)).map(Some)
    }

    /// `[{hash, literal}]`: what the tab wrote that the box may take and has not.
    pub fn unsent(&self) -> Result<String, JsError> {
        json(
            &self
                .replica
                .unsent()
                .iter()
                .map(|o| serde_json::json!({ "hash": o.name().as_str(), "literal": o.literal() }))
                .collect::<Vec<_>>(),
        )
    }

    /// The box's receipt `receipt` (`ReceiptPayload` JSON) for a post of what the tab wrote.
    pub fn answered(&mut self, receipt: &str) -> Result<Step, JsError> {
        let receipt: ReceiptPayload =
            serde_json::from_str(receipt).map_err(|e| refused(format!("receipt: {e}")))?;
        self.step(|r| r.answered(object_stores::answers(&receipt)))
    }

    /// The objects `names` (`[<name>, …]`) refused for good in the box's words `reason`: a post
    /// the box refused whole.
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

    /// A frame another tab appended taken: whether a reading moved. A frame of another form
    /// moves nothing.
    pub fn apply(&mut self, frame: &[u8]) -> Result<String, JsError> {
        match objects::unframe(frame).map_err(|e| refused(e.to_string()))? {
            Some(m) => Ok(said(self.moved(|r| r.apply(&m)).1).to_owned()),
            None => Ok(said(false).to_owned()),
        }
    }

    /// The replica whole, in the core's form.
    pub fn snapshot(&self) -> Result<Vec<u8>, JsError> {
        snapshot::snapshot(&self.replica).map_err(|e| refused(e.to_string()))
    }

    /// The replica replaced by the snapshot `bytes` (none for the empty one) and then each frame
    /// of `frames` (each after its length as four little-endian bytes) taken in order: whether a
    /// reading moved. Refused, holding what it held, for a snapshot of another form or a frame
    /// that does not read; a frame of another form is passed over.
    pub fn load(&mut self, bytes: Option<Vec<u8>>, frames: &[u8]) -> Result<String, JsError> {
        let mut kept: Objects = match bytes {
            None => Objects::default(),
            Some(bytes) => snapshot::from_snapshot(&bytes).map_err(|e| refused(e.to_string()))?,
        };
        for frame in snapshot::frames(frames).map_err(|e| refused(e.to_string()))? {
            if let Some(m) = objects::unframe(frame).map_err(|e| refused(e.to_string()))? {
                kept.apply(&m);
            }
        }
        Ok(said(self.moved(|r| *r = kept).1).to_owned())
    }

    /// Counts the moves of what a reading reads: a reading made at one count is the reading of
    /// every later question until it moves.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The Prodrome's replica of the standing objects, made once per move of them.
    ///
    /// # Errors
    /// Why the objects have no reading at the schema.
    pub fn reading(&self) -> Result<&Replica<E>, JsError> {
        self.read
            .get_or_init(|| self.standing().and_then(|o| Replica::of(&o)))
            .as_ref()
            .map_err(refused)
    }

    /// `{excluded}`: what the reading leaves out of the objects.
    pub fn tips(&self) -> Result<String, JsError> {
        prodrome_wasm_exports::tips(self.reading()?).map_err(refused)
    }

    /// The standing objects as the Prodrome's replica reads them, `[{hash, text}]`.
    fn standing(&self) -> Result<String, String> {
        serde_json::to_string(
            &self
                .replica
                .standing()
                .iter()
                .map(|o| serde_json::json!({ "hash": o.name().as_str(), "text": o.literal() }))
                .collect::<Vec<_>>(),
        )
        .map_err(|e| format!("could not print the objects: {e}"))
    }

    fn step(&mut self, f: impl FnOnce(&mut Objects) -> Move) -> Result<Step, JsError> {
        let (m, moved) = self.moved(f);
        Ok(Step::of(framed(&m)?, said(moved).to_owned()))
    }

    /// `f` done to the replica: whether the standing objects moved, the reading made again
    /// at the next question where they did.
    fn moved<T>(&mut self, f: impl FnOnce(&mut Objects) -> T) -> (T, bool) {
        let names = |r: &Objects| -> Vec<prodrome_core::event::Hash> {
            r.standing().iter().map(|o| o.name().clone()).collect()
        };
        let was = names(&self.replica);
        let done = f(&mut self.replica);
        let moved = names(&self.replica) != was;
        if moved {
            self.read = OnceCell::new();
            self.version = self.version.wrapping_add(1);
        }
        (done, moved)
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
    text: String,
}

/// A store's exports at one schema: the class `$name` over a [`Store`]. A macro because
/// `#[wasm_bindgen]` exports no generic item; each method is one call of the store's.
macro_rules! objects_store {
    ($name:ident = $schema:ty) => {
        #[doc = concat!("The store of objects at `", stringify!($schema), "` the tab holds.")]
        #[wasm_bindgen::prelude::wasm_bindgen]
        #[derive(Default)]
        pub struct $name(pub(crate) $crate::object_rows::Store<$schema>);

        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $name {
            /// Nothing held.
            #[wasm_bindgen(constructor)]
            #[must_use]
            pub fn new() -> $name {
                $name::default()
            }
            /// See `Store::since`.
            #[must_use]
            pub fn since(&self) -> String {
                self.0.since()
            }
            /// See `Store::heads`.
            pub fn heads(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.heads()
            }
            /// See `Store::heard`.
            #[must_use]
            pub fn heard(&self) -> bool {
                self.0.heard()
            }
            /// See `Store::behind`.
            pub fn behind(&self, heads: &str) -> Result<bool, wasm_bindgen::JsError> {
                self.0.behind(heads)
            }
            /// See `Store::overtaken`.
            pub fn overtaken(&self, epoch: f64) -> Result<bool, wasm_bindgen::JsError> {
                self.0.overtaken(epoch)
            }
            /// See `Store::went_back`.
            pub fn went_back(&self, epoch: f64) -> Result<bool, wasm_bindgen::JsError> {
                self.0.went_back(epoch)
            }
            /// See `Store::piece`.
            pub fn piece(
                &mut self,
                answer: &str,
                fetched_at: f64,
            ) -> Result<Vec<u8>, wasm_bindgen::JsError> {
                self.0.piece(answer, fetched_at)
            }
            /// See `Store::land`.
            pub fn land(&mut self) -> Result<$crate::line_rows::Step, wasm_bindgen::JsError> {
                self.0.land()
            }
            /// See `Store::start_over`.
            pub fn start_over(&mut self) -> Result<$crate::line_rows::Step, wasm_bindgen::JsError> {
                self.0.start_over()
            }
            /// See `Store::append`.
            pub fn append(
                &mut self,
                event: &str,
            ) -> Result<Option<$crate::line_rows::Step>, wasm_bindgen::JsError> {
                self.0.append(event)
            }
            /// See `Store::unsent`.
            pub fn unsent(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.unsent()
            }
            /// See `Store::answered`.
            pub fn answered(
                &mut self,
                receipt: &str,
            ) -> Result<$crate::line_rows::Step, wasm_bindgen::JsError> {
                self.0.answered(receipt)
            }
            /// See `Store::refuse`.
            pub fn refuse(
                &mut self,
                names: &str,
                reason: &str,
            ) -> Result<$crate::line_rows::Step, wasm_bindgen::JsError> {
                self.0.refuse(names, reason)
            }
            /// See `Store::apply`.
            pub fn apply(&mut self, frame: &[u8]) -> Result<String, wasm_bindgen::JsError> {
                self.0.apply(frame)
            }
            /// See `Store::snapshot`.
            pub fn snapshot(&self) -> Result<Vec<u8>, wasm_bindgen::JsError> {
                self.0.snapshot()
            }
            /// See `Store::load`.
            pub fn load(
                &mut self,
                bytes: Option<Vec<u8>>,
                frames: &[u8],
            ) -> Result<String, wasm_bindgen::JsError> {
                self.0.load(bytes, frames)
            }
            /// See `Store::version`.
            #[must_use]
            pub fn version(&self) -> u32 {
                self.0.version()
            }
            /// See `Store::tips`.
            pub fn tips(&self) -> Result<String, wasm_bindgen::JsError> {
                self.0.tips()
            }
            /// §6: every entity's registers in the store's reading, each its maximal writes,
            /// at `at` (ISO, or `null` for everything) under `untrusted`.
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
}
pub(crate) use objects_store;
