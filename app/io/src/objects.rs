//! A prodrome's objects on disk, as an interpreter of [`Prodrome`]: the chains, the agent's
//! replicas, the inbox, the conversations, the memory's history, the digest's, the recorded
//! decisions, the two topic stores and each plugin's store.
//!
//! `EventStore` does the work; around it this keeps two things the database does not.
//!
//! 1. A memo keyed by the head set. Every route wants the same objects, and a set of heads names
//!    one set of objects in one order, so the same heads answer from memory. Keyed on every head,
//!    because an adoption adds one beside the first and a rejection takes heads away.
//! 2. The store's hold. `EventStore::append` takes an `flock` against other processes; a
//!    `Hold` says what serialises a read-then-decide-then-append above it.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use prodrome_core::declared::{Declaration, Declared};
use prodrome_core::event::{Envelope, Hash, TodoEvent};
use prodrome_core::policy::Untrusted;
use prodrome_core::registers::Node;
use prodrome_core::schema::Schema;
use prodrome_core::store::{EventStore, Replica};
use suzatary_core::record::Record;
use suzatary_core::secretary::digest_event::DigestEvent;
use suzatary_core::store::{
    Hold, Kind, Prodrome, Reads, Reentrant, Store, StoreError, WriteLock, append_to,
};

use crate::durable;

/// A prodrome's objects, behind the memo and its hold.
pub struct Objects<E: Schema> {
    events: EventStore<E, Untrusted>,
    remembered: Mutex<Option<Remembered<E>>>,
    reentrant: Reentrant,
    /// What the store refuses to take, whoever offers it: an appended event, or a received
    /// object's. A store with none takes every event its schema parses.
    door: Option<Door<E>>,
}

/// A store's admission rule: `Err` names why an event may not be stored.
type Door<E> = Box<dyn Fn(&E) -> Result<(), String> + Send + Sync>;

/// What one head set's read produced.
struct Remembered<E> {
    heads: Vec<Hash>,
    objects: Vec<(Hash, Envelope<E>)>,
    nodes: Vec<Node<E>>,
    /// What the read left out.
    excluded: std::collections::BTreeSet<Hash>,
    /// The fsck's findings, filled on the first ask, since few callers ask.
    verified: Option<Vec<String>>,
}

impl<E: Store> std::fmt::Debug for Objects<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Objects")
            .field("kind", &E::KIND)
            .field("root", &self.events.root())
            .finish_non_exhaustive()
    }
}

impl Objects<TodoEvent<Record>> {
    /// A chain, or the agent's replica of one, held by this process's threads: a `Genesis`
    /// labelled [`suzatary_core::record::GENESIS`], then one `Change` per event.
    #[must_use]
    pub fn chain(events: EventStore<TodoEvent<Record>, Untrusted>) -> Self {
        Self::new(events)
    }
}

impl<E: Store<Vocabulary: Default>> Objects<E> {
    /// The store at `root`, every writer believed, held by its declared hold: a `Genesis`
    /// labelled with its name, then one `Change` per event.
    #[must_use]
    pub fn open(root: impl Into<std::path::PathBuf>) -> Self {
        Self::new(EventStore::new(root, Untrusted::none()))
    }
}

impl Objects<DigestEvent> {
    /// The digest's history at `root`, held by `digests.lock` across processes: a `Genesis`
    /// labelled `digests`, then one `Change` per event. Its summaries' words are a store of their
    /// own beside it, [`crate::texts::Texts`], and its writes are made under the memory's hold.
    /// Its door takes a retraction only from one of `owners`
    /// ([`suzatary_core::secretary::digest_policy::Owners::admits`]), so a forged one is never
    /// stored or pushed.
    #[must_use]
    pub fn digests(
        root: impl Into<std::path::PathBuf>,
        owners: std::collections::BTreeSet<prodrome_core::event::Actor>,
    ) -> Self {
        let mut store = Self::new(EventStore::new(root, Untrusted::none()));
        store.door = Some(Box::new(move |event| {
            suzatary_core::secretary::digest_policy::Owners(&owners)
                .admits(event)
                .map_err(|refused| refused.to_string())
        }));
        store
    }
}

impl Objects<Declared> {
    /// A plugin's store at `root`, opened at its admitted `schema`, a CACHE-tier store held by
    /// `store.lock` across processes: a `Genesis` labelled `plugin`, then one `Change` per item
    /// learned. [`crate::plugin_store::PluginStore`] keeps the schema's text beside it.
    #[must_use]
    pub fn plugin(root: impl Into<std::path::PathBuf>, schema: Declaration) -> Self {
        Self::new(EventStore::at(root, Untrusted::none(), schema))
    }
}

impl<E: Schema> Objects<E> {
    /// Open a store of its schema's kind. Nothing is read until something asks.
    fn new(events: EventStore<E, Untrusted>) -> Self {
        Self {
            events,
            remembered: Mutex::new(None),
            reentrant: Reentrant::default(),
            door: None,
        }
    }

    /// The database underneath.
    #[must_use]
    pub fn inner(&self) -> &EventStore<E, Untrusted> {
        &self.events
    }
}

impl<E: Store> Objects<E> {
    /// Its `Genesis` written when it holds nothing, so a replica's first change has a prodrome to
    /// rest on: a replica may not begin one. A begun store stays as it is.
    ///
    /// # Errors
    /// The store unreadable, or its `Genesis` not written.
    pub fn begin(&self) -> Result<(), StoreError> {
        let _held = self.locked()?;
        if self.tips()?.is_empty() {
            self.events
                .init(E::LABEL)
                .map_err(|e| Self::not_appended(e))?;
        }
        Ok(())
    }

    /// A chain's objects taken in, told to whatever request is being recorded on this thread.
    fn wrote(names: &[Hash]) {
        if E::KIND == Kind::Chain {
            names.iter().for_each(suzatary_core::store::wrote);
        }
    }

    fn unreadable(why: impl std::fmt::Display) -> StoreError {
        StoreError::unreadable(E::KIND.name(), why)
    }

    fn not_appended(why: impl std::fmt::Display) -> StoreError {
        StoreError::not_appended(E::KIND.name(), why)
    }

    /// `look` at what this head set reads: what the same heads already produced, or the store
    /// read again.
    fn with<T>(&self, look: impl FnOnce(&mut Remembered<E>) -> T) -> Result<T, StoreError> {
        let heads = self.tips()?;
        let mut slot = self
            .remembered
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(held) = slot.as_mut().filter(|held| held.heads == heads) {
            return Ok(look(held));
        }
        let dag = self.events.dag().map_err(|e| Self::unreadable(e))?;
        let nodes = dag.nodes().map_err(|e| Self::unreadable(e))?;
        let objects = nodes
            .iter()
            .map(|node| (node.name().clone(), dag.objects()[node.name()].clone()))
            .collect();
        Ok(look(slot.insert(Remembered {
            heads,
            objects,
            nodes,
            excluded: suzatary_core::store::left_out(&dag),
            verified: None,
        })))
    }

    fn file(&self, name: &str) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        let held =
            durable::hold(&self.events.root().join(name)).map_err(|e| Self::not_appended(e))?;
        Ok(Box::new(held))
    }
}

impl WriteLock for durable::Held {}

impl<E: Store> Reads<E> for Objects<E> {
    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError> {
        self.with(|held| held.objects.clone())
    }

    fn nodes(&self) -> Result<Vec<Node<E>>, StoreError> {
        self.with(|held| held.nodes.clone())
    }

    /// Each the bytes of its own file, checked against its name. A missing file, or one the
    /// store's history leaves out, is not held; a file that cannot be read or does not hash to
    /// its name makes the store unreadable.
    /// The file's own length, read from its metadata alone: what [`Self::literals`] answers of it.
    fn sizes(&self, names: &[Hash]) -> Result<Vec<Option<usize>>, StoreError> {
        names
            .iter()
            .map(|name| {
                let path = self
                    .events
                    .root()
                    .join("objects")
                    .join(format!("{}.py", name.as_str()));
                match std::fs::metadata(&path) {
                    Ok(meta) => usize::try_from(meta.len())
                        .map(Some)
                        .map_err(|e| Self::unreadable(format!("{}: {e}", path.display()))),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(Self::unreadable(format!("{}: {e}", path.display()))),
                }
            })
            .collect()
    }

    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        use sha2::Digest as _;
        let history = self.events.dag().map_err(|e| Self::unreadable(e))?;
        names
            .iter()
            .map(|name| {
                let path = self
                    .events
                    .root()
                    .join("objects")
                    .join(format!("{}.py", name.as_str()));
                let bytes = match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(e) => return Err(Self::unreadable(format!("{}: {e}", path.display()))),
                };
                if hex::encode(sha2::Sha256::digest(&bytes)) != name.as_str() {
                    return Err(Self::unreadable(format!(
                        "{} does not hash to its name",
                        path.display()
                    )));
                }
                if history.get(name).is_none() {
                    return Ok(None);
                }
                String::from_utf8(bytes)
                    .map(Some)
                    .map_err(|e| Self::unreadable(format!("{}: {e}", path.display())))
            })
            .collect()
    }

    fn excluded(&self) -> Result<std::collections::BTreeSet<Hash>, StoreError> {
        self.with(|held| held.excluded.clone())
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.events
            .tips()
            .map(|tips| tips.into_iter().collect())
            .map_err(|e| Self::unreadable(e))
    }

    /// Remembered by the head set too: the findings are a function of the objects, which only
    /// grow. A store tampered with under a running process is caught on the next cold read.
    fn verify(&self) -> Vec<String> {
        // History beneath the epoch predates signatures: its unsigned writes are not findings.
        let findings = || {
            let found = self.events.verify();
            let found = match self.events.dag() {
                Ok(dag) => suzatary_core::signatures::since_epoch(&dag, found),
                Err(_) => found,
            };
            found.iter().map(ToString::to_string).collect()
        };
        self.with(|held| held.verified.get_or_insert_with(findings).clone())
            .unwrap_or_else(|_| findings())
    }
}

impl<E: Store> Prodrome<E> for Objects<E> {
    fn append(&self, event: E) -> Result<Hash, StoreError> {
        if let Some(door) = &self.door {
            door(&event).map_err(|refused| Self::not_appended(refused))?;
        }
        let name = append_to(&self.events, E::LABEL, event).map_err(|e| Self::not_appended(e))?;
        Self::wrote(std::slice::from_ref(&name));
        Ok(name)
    }

    /// Where the store has a door, every object's event goes through it first, so a refusal
    /// takes nothing.
    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        if let Some(door) = &self.door {
            for (name, print) in objects {
                let object =
                    prodrome_core::event::parse_envelope::<E>(Replica::schema(&self.events), print)
                        .map_err(|e| Self::not_appended(format!("{}: {e}", name.as_str())))?;
                if let Some(event) = object.event() {
                    door(event).map_err(|refused| {
                        Self::not_appended(format!("{}: {refused}", name.as_str()))
                    })?;
                }
            }
        }
        let by_name: BTreeMap<&Hash, &String> =
            objects.iter().map(|(name, print)| (name, print)).collect();
        let taken = Replica::receive(
            &self.events,
            by_name.keys().map(|name| (*name).clone()).collect(),
            &|name| by_name.get(name).map(|print| print.as_bytes().to_vec()),
        )
        .map_err(|e| Self::not_appended(e))?;
        Self::wrote(&taken);
        Ok(taken)
    }

    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        match E::HOLD {
            Hold::Reentrant => Ok(self.reentrant.hold()),
            Hold::File(name) => self.file(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::event::{Actor, mk_completed, mk_created};
    use prodrome_core::literal::Datetime;
    use suzatary_core::model_name::ModelName;

    use super::*;
    use suzatary_core::secretary::inbox::{self, Response};
    use suzatary_core::secretary::responded::{Done, Performed};
    use suzatary_core::secretary::store::{Draft, Proposal, Reasons};

    /// The deployment's writers in these tests: ada, who taps.
    fn owners() -> std::collections::BTreeSet<prodrome_core::event::Actor> {
        [prodrome_core::event::Actor::new("ada").expect("an actor")].into()
    }

    fn store(name: &str) -> (tempfile::TempDir, Objects<TodoEvent<Record>>) {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let untrusted = Untrusted::of([Actor::new("triage").expect("valid")]);
        let store = Objects::chain(EventStore::new(dir.path().join(name), untrusted));
        (dir, store)
    }

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    /// The digest's door: a retraction written as the triage agent or a plugin is refused
    /// whether appended or received, and nothing is stored; the owner's is taken.
    #[test]
    fn the_digest_takes_a_retraction_only_from_an_owner() {
        use prodrome_core::event::{canonical_envelope, parse_event};
        use suzatary_core::secretary::digest_event::{DigestEvent, DigestVocabulary};

        let dir = tempfile::tempdir().expect("a scratch dir");
        let guarded = Objects::digests(dir.path().join("digests"), owners());
        let node = "a".repeat(64);
        let event = |actor: &str| {
            parse_event::<DigestEvent>(
                &DigestVocabulary::default(),
                &format!(
                    "Retracted(node='{node}', at=datetime(2026, 10, 6, 12, 0, 0), actor='{actor}')"
                ),
            )
            .expect("an event")
        };
        for actor in ["triage", "plugin"] {
            assert!(guarded.append(event(actor)).is_err(), "as {actor}");
        }
        assert!(
            guarded.objects().expect("reads").is_empty(),
            "nothing stored"
        );

        let open = Objects::digests(
            dir.path().join("elsewhere"),
            [Actor::new("triage").expect("an actor")].into(),
        );
        open.append(event("triage"))
            .expect("taken where triage owns");
        let prints: Vec<(Hash, String)> = open
            .objects()
            .expect("reads")
            .into_iter()
            .map(|(name, object)| (name, canonical_envelope(&object)))
            .collect();
        assert!(
            guarded.accept(&prints).is_err(),
            "a forged object is not received"
        );
        assert!(
            guarded.objects().expect("reads").is_empty(),
            "nothing received"
        );

        guarded.append(event("ada")).expect("the owner's is taken");
    }

    /// A damaged print is left out of the read with everything resting on it, and the store
    /// says which: the names a write that deletes must not decide over.
    #[test]
    fn a_damaged_object_is_named_as_left_out_with_what_rests_on_it() {
        let (dir, store) = store("damaged");
        let first = store
            .append(mk_created("alpha", at(1), "ada", "", "").expect("valid"))
            .expect("appends");
        let second = store
            .append(mk_completed("alpha", at(2), "ada", "").expect("valid"))
            .expect("appends");
        assert!(store.excluded().expect("reads").is_empty());
        std::fs::write(
            dir.path()
                .join("damaged/objects")
                .join(format!("{}.py", first.as_str())),
            "Created(not an object",
        )
        .expect("damaged");
        let untrusted = Untrusted::of([Actor::new("triage").expect("valid")]);
        let again = Objects::chain(EventStore::new(dir.path().join("damaged"), untrusted));
        let left_out = again.excluded().expect("reads");
        assert!(
            left_out.contains(&first) && left_out.contains(&second),
            "{left_out:?}"
        );
        assert!(
            again
                .objects()
                .expect("reads")
                .iter()
                .all(|(name, _)| !left_out.contains(name))
        );
    }

    /// A content answer is the file's own bytes, and only of the history: a missing file and
    /// one the history leaves out are not held, and a file that does not hash to its name is
    /// a failure, never an absence.
    #[test]
    fn a_literal_is_its_files_bytes_of_the_history_alone() {
        let (dir, store) = store("literals");
        let first = store
            .append(mk_created("alpha", at(1), "ada", "", "").expect("valid"))
            .expect("appends");
        let second = store
            .append(mk_completed("alpha", at(2), "ada", "").expect("valid"))
            .expect("appends");
        let file = |name: &Hash| {
            dir.path()
                .join("literals/objects")
                .join(format!("{}.py", name.as_str()))
        };
        let held = std::fs::read_to_string(file(&second)).expect("the file");
        assert_eq!(
            store
                .literals(std::slice::from_ref(&second))
                .expect("reads"),
            vec![Some(held.clone())]
        );
        assert_eq!(
            store.sizes(std::slice::from_ref(&second)).expect("reads"),
            vec![Some(held.len())],
            "a size is the file's own bytes, what a content answer sends"
        );

        let first_bytes = std::fs::read(file(&first)).expect("the file");
        std::fs::remove_file(file(&first)).expect("removed");
        assert_eq!(
            store
                .literals(&[first.clone(), second.clone()])
                .expect("reads"),
            vec![None, None],
            "a missing file, and one resting on it, are not held"
        );

        std::fs::write(file(&first), &first_bytes).expect("restored");
        std::fs::write(file(&second), "Created(not this object)").expect("tampered");
        assert!(matches!(
            store.literals(&[second]),
            Err(StoreError::Unreadable { .. })
        ));
    }

    /// The memo answers the store's answer, and an append invalidates it.
    #[test]
    fn the_memo_is_rebuilt_when_a_head_moves() {
        let (_dir, store) = store("memo");
        assert!(store.objects().expect("reads").is_empty());
        let first = store
            .append(mk_created("alpha", at(1), "ada", "", "").expect("valid"))
            .expect("appends");
        let objects = store.objects().expect("reads");
        assert_eq!(
            objects.len(),
            2,
            "the genesis the first write began, and the write"
        );
        // The genesis is no parent, so the linearisation may place it either side of the write.
        assert!(
            objects
                .iter()
                .any(|(_, object)| matches!(object, Envelope::Genesis(_)))
        );
        assert!(objects.iter().any(|(name, _)| *name == first));
        assert_eq!(store.objects().expect("reads"), objects);
        assert_eq!(store.nodes().expect("reads").len(), 2);
        store
            .append(mk_completed("alpha", at(2), "ada", "").expect("valid"))
            .expect("appends");
        assert_eq!(store.objects().expect("reads").len(), 3);
        assert_eq!(store.nodes().expect("reads").len(), 3);
    }

    /// A conversation on disk is a `Genesis` and then a `Change` per line, no object holding its
    /// words, a key per slot of them beside it, and the lines read back in the order written.
    #[test]
    fn a_conversation_is_written_as_changes_through_the_door() {
        use suzatary_core::capability::Turn;
        use suzatary_core::secretary::store::{ConversationId, Entry};
        use suzatary_core::secretary::transcript;

        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = lines_at(dir.path().join("conversations"));
        let id = ConversationId::new("2026-09-30-1").expect("an id");
        let at = |second| Datetime::new(2026, 9, 30, 9, 0, second, 0).expect("an instant");
        transcript::start(
            store.conversations(),
            &id,
            &Entry::Opened {
                at: at(0),
                title: suzatary_core::secretary::words::Words::kept("Morning".to_owned()),
                model: "a fake model".to_owned(),
                kind: suzatary_core::secretary::store::Kind::Topic,
            },
        )
        .expect("starts");
        for text in ["sign in at https://example.test/l?token=s3cret", "ok", "ok"] {
            transcript::push(
                store.conversations(),
                &id,
                &Entry::Said {
                    at: at(1),
                    turn: Turn::Person {
                        text: text.to_owned(),
                        references: Vec::new(),
                        attachments: Vec::new(),
                    },
                    by: None,
                },
            )
            .expect("pushes");
        }
        let objects = store.lines.objects().expect("reads");
        let genesis = |o: &Envelope<suzatary_core::secretary::line_event::LineEvent>| {
            matches!(o, Envelope::Genesis(_))
        };
        assert_eq!(objects.iter().filter(|(_, o)| genesis(o)).count(), 1);
        assert!(
            objects
                .iter()
                .all(|(_, o)| genesis(o) || matches!(o, Envelope::Change(_)))
        );
        assert_eq!(objects.len(), 5);
        assert!(
            store.lines.verify().is_empty(),
            "{:?}",
            store.lines.verify()
        );
        let read = transcript::read(store.conversations(), &id).expect("reads");
        assert_eq!(read.lines().len(), 4);
        for (_, object) in &objects {
            let print = prodrome_core::event::canonical_envelope(object);
            assert!(
                !print.contains("s3cret")
                    && !print.contains("sign in")
                    && !print.contains("Morning")
            );
        }
        assert_eq!(
            files_in(&dir.path().join("lines-keys")),
            4,
            "a key per slot of words"
        );
    }

    /// A fact's words that are there and cannot be read (a directory where the text belongs)
    /// are a failure of the memory's reading, never a fact quietly gone from it.
    #[test]
    fn words_that_cannot_be_read_fail_the_reading() {
        use suzatary_core::secretary::memories::Memories;
        use suzatary_core::secretary::memory::{Fact, Kind, Origin};
        use suzatary_core::secretary::store::ConversationId;

        let dir = tempfile::tempdir().expect("a scratch dir");
        let history = Objects::open(dir.path().join("memory"));
        let texts = crate::texts::Texts::open(dir.path().join("memory-texts"));
        let cards = Objects::open(dir.path().join("inbox"));
        let (lines, digests) = (
            lines_at(dir.path().join("lines")),
            Objects::digests(dir.path().join("digests"), owners()),
        );
        let digest_texts = crate::texts::Texts::open(dir.path().join("digest-texts"));
        let memories = Memories {
            history: &history,
            texts: &texts,
            cards: &cards,
            entropy: &crate::texts::SystemEntropy,
            digests: suzatary_core::secretary::digests::Digests {
                history: &digests,
                texts: &digest_texts,
                lines: lines.conversations(),
                owners: &std::collections::BTreeSet::new(),
                compacting: &suzatary_core::secretary::compacting::Compacting::Off,
            },
        };
        let said = Origin::Turn {
            conversation: ConversationId::new("default").expect("a slug"),
            turn: 0,
        };
        memories
            .remember(
                &Fact::new("He takes the 7:40 train").expect("a fact"),
                Kind::Owner,
                &said,
                Datetime::new(2026, 10, 2, 9, 0, 0, 0).expect("an instant"),
            )
            .expect("remembers");
        assert_eq!(memories.read().expect("reads").facts.len(), 1);
        let text = std::fs::read_dir(dir.path().join("memory-texts"))
            .expect("the texts")
            .next()
            .expect("one text")
            .expect("an entry")
            .path();
        std::fs::remove_file(&text).expect("removed");
        std::fs::create_dir(&text).expect("a directory where the text belongs");
        assert!(memories.read().is_err());
    }

    /// A triage write joining the open card about its todo files the new card and withdraws the
    /// old one under the inbox's one hold, on the box's own file lock, which one process cannot
    /// take twice: it returns.
    #[test]
    fn a_triage_card_joins_under_the_inbox_file_lock() {
        use suzatary_core::secretary::changes::{Act, Changeset};
        use suzatary_core::secretary::filed::{Carding, TriageCards};

        let dir = tempfile::tempdir().expect("a scratch dir");
        let root = dir.path().to_path_buf();
        let (sent, joined) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let inbox: Objects<suzatary_core::secretary::inbox_event::InboxEvent> =
                Objects::open(root.join("inbox"));
            let lines = lines_at(root.join("lines"));
            let cards = TriageCards::new(lines.conversations(), &inbox);
            let file = |note: &str, at: u32| {
                cards
                    .file(
                        Datetime::new(2026, 10, 24, 9, at, 0, 0).expect("an instant"),
                        Changeset::new(
                            "life",
                            vec![Act::Complete {
                                todo: "permit".to_owned(),
                                note: note.to_owned(),
                            }],
                        )
                        .expect("one act"),
                        Reasons::new(
                            "a message says so",
                            vec!["briefing:2026-10-24".parse().expect("a reference")],
                        )
                        .expect("reasons"),
                        None,
                        &|_| Ok(()),
                    )
                    .expect("filed")
                    .how
            };
            let _ = sent.send((file("filed", 0), file("filed again", 1)));
        });
        let (first, second) = joined
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the join returns rather than waiting on its own lock");
        assert_eq!((first, second), (Carding::New, Carding::Joined));
    }

    /// The memory on disk: a `Genesis` and a `Change` per event, no object holding a fact's
    /// words, the words in the text store until forgotten, and beside the objects only the two
    /// locks.
    #[test]
    fn the_memory_keeps_its_words_apart_and_forgetting_deletes_them() {
        use suzatary_core::secretary::memories::Memories;
        use suzatary_core::secretary::memory::{Fact, Kind, Origin};
        use suzatary_core::secretary::store::ConversationId;

        let dir = tempfile::tempdir().expect("a scratch dir");
        let history = Objects::open(dir.path().join("memory"));
        let texts = crate::texts::Texts::open(dir.path().join("memory-texts"));
        let cards = Objects::open(dir.path().join("inbox"));
        let (lines, digests) = (
            lines_at(dir.path().join("conversations")),
            Objects::digests(dir.path().join("digests"), owners()),
        );
        let digest_texts = crate::texts::Texts::open(dir.path().join("digest-texts"));
        let memories = Memories {
            history: &history,
            texts: &texts,
            cards: &cards,
            entropy: &crate::texts::SystemEntropy,
            digests: suzatary_core::secretary::digests::Digests {
                history: &digests,
                texts: &digest_texts,
                lines: lines.conversations(),
                owners: &std::collections::BTreeSet::new(),
                compacting: &suzatary_core::secretary::compacting::Compacting::Off,
            },
        };
        let said = Origin::Turn {
            conversation: ConversationId::new("default").expect("a slug"),
            turn: 0,
        };
        let at = |minute| Datetime::new(2026, 10, 2, 9, minute, 0, 0).expect("an instant");
        let kept = memories
            .remember(
                &Fact::new("He takes the 7:40 train").expect("a fact"),
                Kind::Owner,
                &said,
                at(0),
            )
            .expect("remembers");
        let gone = memories
            .remember(
                &Fact::new("Mira is the accountant").expect("a fact"),
                Kind::Person,
                &said,
                at(1),
            )
            .expect("remembers");
        memories
            .forget(gone.id, at(2), &Actor::new("ada").expect("an actor"))
            .expect("forgets");
        let read = memories.read().expect("reads");
        assert_eq!(read.facts, vec![kept]);
        assert_eq!(read.version, 3);
        assert!(history.verify().is_empty(), "{:?}", history.verify());
        let mut beside: Vec<String> = std::fs::read_dir(dir.path().join("memory"))
            .expect("the store")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        beside.sort();
        assert_eq!(
            beside,
            [".gitattributes", ".lock", "memory.lock", "objects"]
        );
        let every_byte = |dir: &std::path::Path| -> String {
            std::fs::read_dir(dir)
                .expect("a directory")
                .map(|entry| {
                    std::fs::read_to_string(entry.expect("an entry").path()).expect("text")
                })
                .collect()
        };
        let objects = every_byte(&dir.path().join("memory/objects"));
        assert!(
            !objects.contains("Mira") && !objects.contains("7:40"),
            "{objects}"
        );
        let words = every_byte(&dir.path().join("memory-texts"));
        assert!(words.contains("7:40") && !words.contains("Mira"), "{words}");
        assert_eq!(
            std::fs::read_dir(dir.path().join("memory-texts"))
                .expect("texts")
                .count(),
            1
        );
    }

    /// The conversations' lines at `root`, their keys beside them.
    fn lines_at(root: std::path::PathBuf) -> crate::conversations::ConversationStores {
        let keys = root.with_file_name("lines-keys");
        crate::conversations::ConversationStores::open(
            root,
            keys,
            std::sync::Arc::new(suzatary_core::capability::Nothing),
        )
    }

    /// A conversation of 48 lines on disk under `dir`, enough to settle a node of the digest.
    fn spoken(dir: &std::path::Path) -> crate::conversations::ConversationStores {
        use suzatary_core::capability::Turn;
        use suzatary_core::secretary::store::{ConversationId, Entry};
        use suzatary_core::secretary::transcript;

        let lines = lines_at(dir.join("conversations"));
        let id = ConversationId::new("default").expect("a slug");
        let minute = |m: u32| Datetime::new(2026, 10, 6, 9, m, 0, 0).expect("an instant");
        let opened = Entry::Opened {
            at: minute(0),
            title: suzatary_core::secretary::words::Words::kept("Morning".to_owned()),
            model: "a fake model".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Topic,
        };
        transcript::start(lines.conversations(), &id, &opened).expect("starts");
        for m in 1..48 {
            let said = Entry::Said {
                at: minute(m),
                turn: Turn::Person {
                    text: format!("line {m}"),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            };
            transcript::push(lines.conversations(), &id, &said).expect("pushes");
        }
        lines
    }

    /// `name`'s file in the store `store` under `dir`.
    fn object_file(dir: &std::path::Path, store: &str, name: &Hash) -> std::path::PathBuf {
        dir.join(store)
            .join("objects")
            .join(format!("{}.py", name.as_str()))
    }

    /// The files in `path`, none when it does not exist.
    fn files_in(path: &std::path::Path) -> usize {
        std::fs::read_dir(path).map_or(0, Iterator::count)
    }

    /// Nothing is deleted or summarised over a partial read: with a conversation line damaged, a
    /// forgetting stands and its sweep deletes nothing, a sweep refuses naming the line, and a
    /// summary is refused; read whole again, the sweep deletes what is dead.
    #[test]
    fn nothing_is_deleted_or_summarised_over_a_damaged_line() {
        use suzatary_core::secretary::memory::{Fact, Kind, Origin};
        use suzatary_core::secretary::store::ConversationId;
        use suzatary_core::secretary::{digest_line::Line, digests::Digests, memories::Memories};

        let dir = tempfile::tempdir().expect("a scratch dir");
        let lines = spoken(dir.path());
        let (history, cards, summaries) = (
            Objects::open(dir.path().join("memory")),
            Objects::open(dir.path().join("inbox")),
            Objects::digests(dir.path().join("digests"), owners()),
        );
        let (texts, digest_texts) = (
            crate::texts::Texts::open(dir.path().join("memory-texts")),
            crate::texts::Texts::open(dir.path().join("digest-texts")),
        );
        let memories = Memories {
            history: &history,
            texts: &texts,
            cards: &cards,
            entropy: &crate::texts::SystemEntropy,
            digests: Digests {
                history: &summaries,
                texts: &digest_texts,
                lines: lines.conversations(),
                owners: &std::collections::BTreeSet::new(),
                compacting: &suzatary_core::secretary::compacting::Compacting::Off,
            },
        };
        let said = Origin::Turn {
            conversation: ConversationId::new("default").expect("a slug"),
            turn: 0,
        };
        let at = Datetime::new(2026, 10, 6, 10, 0, 0, 0).expect("an instant");
        let fact = Fact::new("Mira is the accountant").expect("a fact");
        let gone = memories
            .remember(&fact, Kind::Person, &said, at)
            .expect("remembers");
        let ada = Actor::new("ada").expect("an actor");
        let node = memories.digests.read().expect("reads").pending()[0].clone();
        let (line, _) = lines
            .lines
            .objects()
            .expect("reads")
            .into_iter()
            .find(|(_, object)| object.event().is_some())
            .expect("a line");
        let print = std::fs::read(object_file(dir.path(), "conversations", &line)).expect("reads");
        std::fs::write(object_file(dir.path(), "conversations", &line), "Said(").expect("damaged");
        let damaged = lines_at(dir.path().join("conversations"));
        let partial = Memories {
            digests: Digests {
                lines: damaged.conversations(),
                ..memories.digests
            },
            ..memories
        };
        partial
            .forget(gone.id, at, &ada)
            .expect("the forgetting stands, whatever its sweep");
        assert_eq!(
            files_in(&dir.path().join("memory-texts")),
            1,
            "its sweep deleted nothing"
        );
        let refused = partial
            .sweep()
            .expect_err("a sweep over a partial read refuses");
        assert!(
            refused.message().contains(line.as_str()),
            "{}",
            refused.message()
        );
        let model = ModelName::parse("claude-sonnet-5-5").expect("a model");
        let summary = Line::new("the first lines").expect("a line");
        let over = vec![node.clone()];
        let refused = partial
            .hold()
            .expect("held")
            .summarize(&node, &summary, over, model, at)
            .expect_err("nothing summarised over a partial read");
        assert!(
            refused.message().contains("left out"),
            "{}",
            refused.message()
        );
        assert_eq!(files_in(&dir.path().join("digest-texts")), 0);
        std::fs::write(object_file(dir.path(), "conversations", &line), print).expect("restored");
        let whole = lines_at(dir.path().join("conversations"));
        let memories = Memories {
            digests: Digests {
                lines: whole.conversations(),
                ..memories.digests
            },
            ..memories
        };
        assert_eq!(memories.sweep().expect("reads whole"), 1);
        assert_eq!(
            files_in(&dir.path().join("memory-texts")),
            0,
            "the dead words go"
        );
    }

    /// A damaged digest event, a retraction here, leaves the sweep deleting nothing.
    #[test]
    fn nothing_is_deleted_over_a_damaged_retraction() {
        use prodrome_core::event::parse_event;
        use suzatary_core::secretary::digest_event::{DigestEvent, DigestVocabulary};
        use suzatary_core::secretary::digests::Digests;
        use suzatary_core::secretary::memories::Memories;

        let dir = tempfile::tempdir().expect("a scratch dir");
        let lines = spoken(dir.path());
        let (line, _) = lines
            .lines
            .objects()
            .expect("reads")
            .into_iter()
            .find(|(_, object)| object.event().is_some())
            .expect("a line");
        let summaries = Objects::digests(dir.path().join("digests"), owners());
        let retraction = parse_event::<DigestEvent>(
            &DigestVocabulary::default(),
            &format!(
                "Retracted(node='{}', at=datetime(2026, 10, 6, 10, 0, 0), actor='ada')",
                line.as_str()
            ),
        )
        .expect("a retraction");
        let retracted = summaries.append(retraction).expect("stored");
        std::fs::write(object_file(dir.path(), "digests", &retracted), "Retracted(")
            .expect("damaged");
        let (history, cards, damaged) = (
            Objects::open(dir.path().join("memory")),
            Objects::open(dir.path().join("inbox")),
            Objects::digests(dir.path().join("digests"), owners()),
        );
        let (texts, digest_texts) = (
            crate::texts::Texts::open(dir.path().join("memory-texts")),
            crate::texts::Texts::open(dir.path().join("digest-texts")),
        );
        let memories = Memories {
            history: &history,
            texts: &texts,
            cards: &cards,
            entropy: &crate::texts::SystemEntropy,
            digests: Digests {
                history: &damaged,
                texts: &digest_texts,
                lines: lines.conversations(),
                owners: &std::collections::BTreeSet::new(),
                compacting: &suzatary_core::secretary::compacting::Compacting::Off,
            },
        };
        let refused = memories
            .sweep()
            .expect_err("a damaged digest event refuses");
        assert!(
            refused.message().contains(retracted.as_str()),
            "{}",
            refused.message()
        );
    }

    fn now() -> Datetime {
        Datetime::new(2026, 9, 28, 9, 0, 0, 0).expect("an instant")
    }

    /// Two threads submitting one proposal on the real store send once: the
    /// hold is the file's, and it waits across threads as across processes.
    #[test]
    fn two_threads_on_the_store_send_once() {
        let dir = std::env::temp_dir().join(format!("suzatary-inbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let caps = Objects::open(&dir);
        let asked = inbox::Asked {
            proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
            proposal: Proposal::Email(Draft {
                account: "ada@example.com".to_owned(),
                to: vec!["mira@example.com".to_owned()],
                cc: Vec::new(),
                subject: "Thursday".to_owned(),
                body: "Thursday works.".to_owned(),
                attachments: Vec::new(),
            }),
            reasons: Reasons::new(
                "she asked",
                vec!["mail:abcdef012345".parse().expect("a ref")],
            )
            .expect("reasons"),
            asked: "proposal:default/0".parse().expect("a ref"),
            price: inbox::Price::unpriced(),
        };
        let assistant = Actor::new("assistant").expect("an actor");
        let id = inbox::ask(&caps, asked, &assistant, now()).expect("filed");
        let sends = std::sync::atomic::AtomicUsize::new(0);
        let owner = Actor::new("ada").expect("an actor");
        let responses: Vec<Response> = std::thread::scope(|scope| {
            let taps: Vec<_> = (0..2)
                .map(|_| {
                    scope.spawn(|| {
                        inbox::submit(&caps, &id, &owner, now(), |_| {
                            sends.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            Ok(Performed::Done(Done::Sent {
                                recipients: vec!["mira@example.com".to_owned()],
                                message_id: "<1@box>".to_owned(),
                            }))
                        })
                        .expect("answered")
                    })
                })
                .collect();
            taps.into_iter()
                .map(|tap| tap.join().expect("a thread"))
                .collect()
        });
        assert_eq!(sends.load(std::sync::atomic::Ordering::SeqCst), 1);
        let said: Vec<String> = responses
            .iter()
            .map(|response| match response {
                Response::Ran(ran) => ran.to_string(),
                Response::Recorded(reading) => reading
                    .shown()
                    .map_or_else(String::new, |answer| answer.response.to_string()),
            })
            .collect();
        assert_eq!(
            said[0], said[1],
            "the second tap answers what the first recorded"
        );
        assert!(
            said[0].starts_with("Sent("),
            "the one send is what both answer: {said:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
