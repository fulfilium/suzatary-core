//! The Prodrome's store, `root/objects/<name>.py`, where a file that does not hash to its name
//! and reads as a [`HuskObject`] is the object it reads as. A write is the Prodrome's own
//! decision, an [`Overlay`] over the files as last read whole, placed under `root/.lock`; a name
//! the store holds is never taken, so no whole is written over its husk. A husk is a function of
//! its whole, so the store cannot tell one its prune wrote from one put under the name with other
//! clear fields: it holds nothing sealed and rests where the history says, and the rest is checked
//! once, against the committed whole, by the data push (`verify --husked`). It is `EventStore`
//! learning husks, and belongs there.

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::{DirEntryExt as _, MetadataExt as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use prodrome_core::dag::{Dag, Finding, Heads};
use prodrome_core::event::{Envelope, Hash, parse_envelope};
use prodrome_core::literal::ProdromeError;
use prodrome_core::policy::{Policy, Untrusted};
use prodrome_core::store::{EventStore, Held, Overlay, Replica};
use suzatary_store::store::{Hashed, HuskObject, Husking};

use crate::durable;
use crate::heads_record;
use crate::object_files::ObjectFiles;

/// A store whose names hold wholes or husks.
pub struct Husked<E: Husking> {
    root: PathBuf,
    schema: E::Vocabulary,
    memory: Mutex<Memory<E>>,
}

enum Kept<E: Husking> {
    /// A print that hashes to its name.
    Whole(Envelope<E>),
    Husk(HuskObject<E>),
    /// Neither, and why.
    Unread(String),
}

/// Each file by its inode, as its directory lists it. A whole is written once and a husk replaces
/// it by a rename, so either moves the inode; a file written over in place is read again by the
/// next process, and `verify` reads every file afresh.
struct Memory<E: Husking> {
    files: BTreeMap<Hash, (u64, Kept<E>)>,
    /// The files as last read whole, and what this store wrote or took since.
    looked: Option<Looked<E>>,
}

/// The files' history, and the overlay a write decides over, made at the first write or `held`.
struct Looked<E: Husking> {
    dag: Arc<Dag<E>>,
    writes: Option<Overlay<E, Read<E>>>,
}

impl<E: Husking<Vocabulary: Clone>> Looked<E> {
    fn writes(&mut self, schema: &E::Vocabulary) -> Result<&Overlay<E, Read<E>>, ProdromeError> {
        let writes = if let Some(writes) = self.writes.take() {
            writes
        } else {
            let dag = Arc::clone(&self.dag);
            let nodes = suzatary_core::parts::part("history", || dag.nodes())?;
            let folded = suzatary_core::parts::part("registers", || {
                Arc::new(prodrome_core::registers::fold(&nodes))
            });
            let held = Held {
                tips: dag.tips(),
                dag,
                folded,
            };
            Overlay::new(Read {
                held,
                schema: schema.clone(),
            })
        };
        Ok(self.writes.insert(writes))
    }
}

struct Read<E: Husking> {
    held: Held<E>,
    schema: E::Vocabulary,
}

/// The objects a prune husked, and the bytes they no longer take.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LetGo {
    pub objects: usize,
    pub bytes: u64,
}

impl<E: Husking<Vocabulary: Clone + Default>> Husked<E> {
    pub(crate) fn open(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            schema: E::Vocabulary::default(),
            memory: Mutex::new(Memory {
                files: BTreeMap::new(),
                looked: None,
            }),
        }
    }
}

impl<E: Husking<Vocabulary: Clone>> Husked<E> {
    fn path(&self, name: &Hash) -> PathBuf {
        self.root
            .join("objects")
            .join(format!("{}.py", name.as_str()))
    }

    fn lock(&self) -> Result<durable::Held, ProdromeError> {
        durable::hold(&self.root.join(".lock")).map_err(|e| failed(&self.root, &e))
    }

    /// The files, each read again where its inode moved, and their reading.
    fn memory(&self) -> Result<MutexGuard<'_, Memory<E>>, ProdromeError> {
        let mut memory = self.memory.lock().unwrap_or_else(PoisonError::into_inner);
        let objects = suzatary_core::parts::open().then(std::time::Instant::now);
        let listed = self.listing()?.0;
        let before = memory.files.len();
        memory.files.retain(|name, _| listed.contains_key(name));
        // An unread file is read on every look, as the Prodrome's store reads it, so one mended
        // under the inode it had is seen.
        let fresh: Vec<(Hash, u64)> = listed
            .into_iter()
            .filter(|(name, ino)| {
                memory
                    .files
                    .get(name)
                    .is_none_or(|(was, kept)| was != ino || matches!(kept, Kept::Unread(_)))
            })
            .collect();
        let read = spread(fresh, |(name, ino)| {
            let kept = self.kept(&name);
            (name, (ino, kept))
        });
        if let Some(started) = objects {
            // The listing and every file read, hashed and parsed, as a wall time: they run spread.
            suzatary_core::parts::add_part("objects", started.elapsed());
        }
        let moved = read.iter().any(|(name, (_, kept))| {
            !matches!(kept, Kept::Unread(_))
                || !matches!(memory.files.get(name), Some((_, Kept::Unread(_))))
        });
        if memory.files.len() != before || moved {
            memory.looked = None;
        }
        suzatary_core::parts::part("files", || memory.files.extend(read));
        if memory.looked.is_none() {
            let dag =
                suzatary_core::parts::part("dag", || Arc::new(Self::history(&memory.files).0));
            memory.looked = Some(Looked { dag, writes: None });
        }
        Ok(memory)
    }

    /// Each `<name>.py` in `objects/` by its inode, and every other entry's file name.
    fn listing(&self) -> Result<(BTreeMap<Hash, u64>, Vec<String>), ProdromeError> {
        let dir = self.root.join("objects");
        let entries = match std::fs::read_dir(&dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
            entries => entries.map_err(|e| failed(&dir, &e))?,
        };
        let (mut objects, mut garbage) = (BTreeMap::new(), Vec::new());
        for entry in entries {
            let entry = entry.map_err(|e| failed(&dir, &e))?;
            let file = entry.file_name().to_string_lossy().into_owned();
            match file.strip_suffix(".py").map(Hash::new) {
                Some(Ok(name)) => {
                    objects.insert(name, entry.ino());
                }
                _ => garbage.push(file),
            }
        }
        Ok((objects, garbage))
    }

    fn kept(&self, name: &Hash) -> Kept<E> {
        let path = self.path(name);
        let text = match std::fs::read(&path).map(String::from_utf8) {
            Ok(Ok(text)) => text,
            Ok(Err(_)) => return Kept::Unread("not text".to_owned()),
            Err(e) => return Kept::Unread(failed(&path, &e).to_string()),
        };
        match Hashed::of(name, &text) {
            Hashed::Whole(whole) => parse_envelope(&self.schema, whole)
                .map_or_else(|e| Kept::Unread(e.to_string()), Kept::Whole),
            Hashed::Other(other) => HuskObject::of_other(other).map_or_else(
                |e| Kept::Unread(format!("does not hash to its name, and {e}")),
                Kept::Husk,
            ),
        }
    }

    /// The history of `files`, and a finding for each that is neither a whole nor a husk.
    fn history(files: &BTreeMap<Hash, (u64, Kept<E>)>) -> (Dag<E>, Vec<Finding>) {
        let mut unread = Vec::new();
        let objects = files.iter().filter_map(|(name, (_, kept))| match kept {
            Kept::Whole(object) => Some((name.clone(), object.clone())),
            Kept::Husk(husk) => Some((name.clone(), husk.object())),
            Kept::Unread(why) => {
                unread.push(Finding::Garbage(format!(
                    "objects/{}.py: {why}",
                    name.as_str()
                )));
                None
            }
        });
        (objects.collect::<Dag<_>>().interior(), unread)
    }

    /// Each whole `names` names replaced by its husk, one file at a time under the lock. Every
    /// file is whole or husk at each instant, so a stop between two writes leaves a store that
    /// reads the same; a husk is passed over, so a second prune writes nothing.
    ///
    /// # Errors
    /// The lock not taken, or a file not replaced; what was replaced stays.
    pub(crate) fn husk(&self, names: &BTreeSet<Hash>) -> Result<LetGo, ProdromeError> {
        let _locked = self.lock()?;
        let mut let_go = LetGo::default();
        for name in names {
            let path = self.path(name);
            let Kept::Whole(Envelope::Change(whole)) = self.kept(name) else {
                continue;
            };
            let Some(husk) = HuskObject::of(&whole) else {
                continue;
            };
            let was = std::fs::metadata(&path).map_err(|e| failed(&path, &e))?;
            let print = husk.print();
            durable::replace(&path, print.as_bytes(), durable::Mode::Kept)
                .map_err(|e| failed(&path, &e))?;
            let_go.objects += 1;
            let_go.bytes += was.len().saturating_sub(print.len() as u64);
        }
        Ok(let_go)
    }

    /// `write` of the overlay, under the lock, with what it took placed in its files, parents
    /// first. A decision may not guess, so it refuses while a file is unread.
    fn writing(
        &self,
        write: impl FnOnce(&Overlay<E, Read<E>>) -> Result<Vec<Hash>, ProdromeError>,
    ) -> Result<Vec<Hash>, ProdromeError> {
        let _locked = self.lock()?;
        let mut memory = self.memory()?;
        if let Some((name, (_, Kept::Unread(why)))) = memory
            .files
            .iter()
            .find(|(_, (_, kept))| matches!(kept, Kept::Unread(_)))
        {
            return Err(ProdromeError::Store(format!(
                "objects/{}.py is unread, so no write decides over this store: {why}",
                name.as_str()
            )));
        }
        let Memory { files, looked } = &mut *memory;
        let looked = looked.as_mut().expect("made by `memory`");
        let read = looked.writes(&self.schema)?;
        let taken = write(read)?;
        let held = read.held()?;
        // The record of heads goes before any object is placed, and is written again once all
        // of them are, as the Prodrome's store keeps it: a stop between leaves no record.
        let placing = taken.iter().any(|name| !files.contains_key(name));
        if placing {
            heads_record::remove(&self.root)?;
        }
        let mut placed = Vec::new();
        for name in &taken {
            let (Some(object), Some(print)) = (held.dag.get(name), read.print(name)) else {
                continue;
            };
            let path = self.path(name);
            durable::replace(&path, &print, durable::Mode::Readable)
                .map_err(|e| failed(&path, &e))?;
            let ino = std::fs::metadata(&path)
                .map_err(|e| failed(&path, &e))?
                .ino();
            placed.push((name.clone(), (ino, Kept::Whole(object.clone()))));
        }
        files.extend(placed);
        looked.dag = held.dag;
        if placing {
            let heads = looked.dag.heads();
            // A record not written leaves none, so the next reader derives the heads.
            let _ = heads_record::write(&self.root, &heads);
        }
        Ok(taken)
    }
}

impl<E: Husking<Vocabulary: Clone>> Replica<E> for Husked<E> {
    fn schema(&self) -> &E::Vocabulary {
        &self.schema
    }

    fn held(&self) -> Result<Held<E>, ProdromeError> {
        let mut memory = self.memory()?;
        let looked = memory.looked.as_mut().expect("made by `memory`");
        looked.writes(&self.schema)?.held()
    }

    fn tips(&self) -> Result<BTreeSet<Hash>, ProdromeError> {
        Ok(ObjectFiles::dag(self)?.tips())
    }

    /// The record, `root/.heads`, with one `stat` per recorded tip and no listing, where it reads
    /// and every recorded tip's file is there; the heads derived otherwise (the Prodrome's rule).
    fn heads(&self) -> Result<Heads, ProdromeError> {
        match ObjectFiles::recorded(self)? {
            Some(recorded) => Ok(recorded),
            None => Ok(ObjectFiles::dag(self)?.heads()),
        }
    }

    /// A husk holds no bytes its name is the hash of.
    fn print(&self, name: &Hash) -> Option<Vec<u8>> {
        std::fs::read(self.path(name))
            .ok()
            .filter(|bytes| hashes_to(name, bytes))
    }

    fn append(&self, event: E) -> Result<Hash, ProdromeError> {
        let mut name = None;
        self.writing(|overlay| {
            let written = overlay.append(event)?;
            name = Some(written.clone());
            Ok(vec![written])
        })?;
        name.ok_or_else(|| ProdromeError::Store("nothing was decided".to_owned()))
    }

    /// The overlay walks no further than a name the store holds, whole or husk.
    fn receive(
        &self,
        seeds: BTreeSet<Hash>,
        print_of: &dyn Fn(&Hash) -> Option<Vec<u8>>,
    ) -> Result<Vec<Hash>, ProdromeError> {
        self.writing(|overlay| overlay.receive(seeds, print_of))
    }
}

impl<E: Husking<Vocabulary: Clone + Send + Sync>> ObjectFiles<E> for Husked<E> {
    fn root(&self) -> &Path {
        &self.root
    }

    fn dag(&self) -> Result<Arc<Dag<E>>, ProdromeError> {
        let memory = self.memory()?;
        Ok(Arc::clone(
            &memory.looked.as_ref().expect("made by `memory`").dag,
        ))
    }

    /// The record, `root/.heads`, with one `stat` per recorded tip, where it reads and every
    /// recorded tip's file is there.
    fn recorded(&self) -> Result<Option<Heads>, ProdromeError> {
        Ok(heads_record::read(&self.root)
            .filter(|recorded| recorded.tips().iter().all(|tip| self.path(tip).exists())))
    }

    /// Of a cold read: each whole rehashed and each husk read, and a record of heads that is
    /// not the derived heads, read before the cold read and after it (no lock is taken, so a
    /// record both reads found is one no write of the store crossed).
    fn findings(&self) -> Vec<Finding> {
        let recorded = heads_record::read(&self.root);
        let (listed, garbage) = match self.listing() {
            Ok(listing) => listing,
            Err(e) => return vec![Finding::Io(e)],
        };
        let files = spread(listed.into_iter().collect(), |(name, ino)| {
            let kept = self.kept(&name);
            (name, (ino, kept))
        })
        .into_iter()
        .collect();
        let (dag, mut findings) = Self::history(&files);
        let policy = Untrusted::none();
        findings.extend(dag.verify(policy.at(&dag).as_ref().unwrap_or(&policy)));
        findings.extend(
            garbage
                .into_iter()
                .map(|file| Finding::Garbage(format!("objects/{file}"))),
        );
        let derived = dag.heads();
        if let Some(recorded) =
            recorded.filter(|before| heads_record::read(&self.root).as_ref() == Some(before))
            && recorded != derived
        {
            findings.push(Finding::StaleHeads { recorded, derived });
        }
        findings
    }

    /// A store with no genesis holds no husk, so the Prodrome's own store begins it.
    fn init(&self, label: &str) -> Result<Hash, ProdromeError> {
        EventStore::<E, Untrusted>::at(&self.root, Untrusted::none(), self.schema.clone())
            .init(label)
    }

    /// Decided over one look and placed at the end, those decided before a refusal too.
    fn append_each(
        &self,
        label: &str,
        next: &mut dyn FnMut() -> Result<Option<E>, String>,
        written: &mut Vec<Hash>,
    ) -> Result<(), String> {
        let tips = || Replica::tips(self).map_err(|e| e.to_string());
        if tips()?.is_empty() {
            let begun = self.init(label);
            if tips()?.is_empty() {
                begun.map_err(|e| e.to_string())?;
            }
        }
        let mut refused = None;
        let taken = self
            .writing(|overlay| {
                let mut names = Vec::new();
                while refused.is_none() {
                    match next().map(|event| event.map(|event| overlay.append(event))) {
                        Ok(Some(Ok(name))) => names.push(name),
                        Ok(None) => break,
                        Ok(Some(Err(why))) => refused = Some(why.to_string()),
                        Err(why) => refused = Some(why),
                    }
                }
                Ok(names)
            })
            .map_err(|e| e.to_string())?;
        written.extend(taken);
        refused.map_or(Ok(()), Err)
    }
}

/// What the overlay over it takes is the store's to place.
impl<E: Husking> Replica<E> for Read<E> {
    fn schema(&self) -> &E::Vocabulary {
        &self.schema
    }

    fn held(&self) -> Result<Held<E>, ProdromeError> {
        Ok(self.held.clone())
    }

    fn tips(&self) -> Result<BTreeSet<Hash>, ProdromeError> {
        Ok(self.held.tips.clone())
    }

    fn print(&self, _: &Hash) -> Option<Vec<u8>> {
        None
    }

    fn append(&self, _: E) -> Result<Hash, ProdromeError> {
        Err(ProdromeError::Store("a reading takes nothing".to_owned()))
    }

    fn receive(
        &self,
        _: BTreeSet<Hash>,
        _: &dyn Fn(&Hash) -> Option<Vec<u8>>,
    ) -> Result<Vec<Hash>, ProdromeError> {
        Err(ProdromeError::Store("a reading takes nothing".to_owned()))
    }
}

fn hashes_to(name: &Hash, bytes: &[u8]) -> bool {
    hex::encode(<sha2::Sha256 as sha2::Digest>::digest(bytes)) == name.as_str()
}

fn failed(path: &Path, error: &std::io::Error) -> ProdromeError {
    ProdromeError::Store(format!("{}: {error}", path.display()))
}

/// `f` of each item, over the machine's cores, as the Prodrome's store reads its files.
fn spread<T: Send, U: Send>(items: Vec<T>, f: impl Fn(T) -> U + Sync) -> Vec<U> {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut chunks: Vec<Vec<T>> = (0..cores).map(|_| Vec::new()).collect();
    for (at, item) in items.into_iter().enumerate() {
        chunks[at % cores].push(item);
    }
    std::thread::scope(|scope| {
        let f = &f;
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| scope.spawn(move || chunk.into_iter().map(f).collect::<Vec<U>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|e| std::panic::resume_unwind(e))
            })
            .collect()
    })
}
