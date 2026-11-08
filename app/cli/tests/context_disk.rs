//! THE AGENT'S `context` OVER THE DISK STORES, at the live shape in bytes as well as in count:
//! lines of about 8.4 kB on average and a digest history of about 9,000 objects, the store the
//! box's `suzatary mcp` opens cold for every session. It prints how long each phase took
//! (`suzatary_core::timings`), a measurement and not a law:
//!
//! `cargo test --release -p suzatary --test all context_disk -- --ignored --nocapture`

use std::path::Path;
use std::sync::Arc;

use prodrome_core::event::{Hash, canonical_envelope};
use suzatary_core::secretary::compacting::Compacting;
use suzatary_core::secretary::digests::ReadDigests;
use suzatary_core::secretary::live_shape::{self, Shape, Summaries};
use suzatary_core::secretary::memories::ReadMemories;
use suzatary_core::secretary::recollection::Recollection;
use suzatary_core::store::{Doc as _, Gets as _, Prodrome, Reads};
use suzatary_io::conversations::ConversationStores;
use suzatary_io::objects::Objects;
use suzatary_io::texts::Texts;

/// Every object of `from` handed to `to`, as a replica hands its objects over.
fn handed<E: suzatary_core::store::Store>(from: &dyn Reads<E>, to: &dyn Prodrome<E>) {
    let prints: Vec<(Hash, String)> = from
        .objects()
        .expect("the objects read")
        .into_iter()
        .map(|(name, object)| (name, canonical_envelope(&object)))
        .collect();
    to.accept(&prints).expect("the store takes them");
}

/// The disk stores under `root`, opened as a fresh process opens them: nothing read yet.
struct Disk {
    lines: ConversationStores,
    digests: Objects<suzatary_core::secretary::digest_event::DigestEvent>,
    summaries: Texts<suzatary_core::secretary::digest_line::Line>,
    facts: Objects<suzatary_core::secretary::memory_event::MemoryEvent>,
    words: Texts<suzatary_core::secretary::memory::Fact>,
    owners: std::collections::BTreeSet<prodrome_core::event::Actor>,
}

impl Disk {
    fn open(root: &Path, owners: std::collections::BTreeSet<prodrome_core::event::Actor>) -> Self {
        Self {
            lines: ConversationStores::open(
                root.join("lines"),
                root.join("keys"),
                Arc::new(suzatary_core::capability::Nothing),
                Arc::new(suzatary_core::capability::Nothing),
            ),
            digests: Objects::digests(root.join("digests"), owners.clone()),
            summaries: Texts::open(root.join("digest-texts")),
            facts: Objects::open(root.join("facts")),
            words: Texts::open(root.join("fact-texts")),
            owners,
        }
    }

    fn recollection(&self) -> Recollection<'_> {
        Recollection::of(
            ReadMemories {
                history: &self.facts,
                texts: &self.words,
                digests: ReadDigests {
                    history: &self.digests,
                    texts: &self.summaries,
                    lines: self.lines.conversations().reads(),
                    owners: &self.owners,
                    compacting: &Compacting::Off,
                },
            },
            16 * 1024,
        )
    }
}

/// The size of every file under `dir`, in bytes, and how many there are.
fn footprint(dir: &Path) -> (u64, usize) {
    let mut total = (0, 0);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("a directory").flatten() {
            let meta = entry.metadata().expect("its metadata");
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total = (total.0 + meta.len(), total.1 + 1);
            }
        }
    }
    total
}

#[test]
#[ignore = "a measurement, printed, not a law"]
fn measure_the_context_over_the_disk_stores_at_the_live_size() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let started = std::time::Instant::now();
    let memory = Shape {
        lines: 15_900,
        conversations: 100,
        answer_bytes: 16_000,
        summaries: Summaries::First(4_000),
    }
    .build();
    let written = Disk::open(root.path(), memory.owners.clone());
    let conversations = written.lines.conversations();
    handed(&memory.lines.lines, conversations.lines);
    for name in memory.lines.keys.names() {
        let key = memory.lines.keys.get(&name).expect("read").expect("held");
        conversations.keys.set(&name, &key).expect("kept");
    }
    handed(&memory.digests, &written.digests);
    for name in memory.summaries.keys() {
        let text = memory.summaries.get(&name).expect("read").expect("held");
        written.summaries.set(&name, &text).expect("kept");
    }
    eprintln!("built and written in {:?}", started.elapsed());
    for store in ["lines", "keys", "digests", "digest-texts"] {
        let (bytes, files) = footprint(&root.path().join(store));
        eprintln!("{store}: {files} files, {} kB", bytes / 1_000);
    }
    let started = std::time::Instant::now();
    let read: u64 = ["lines", "digests"]
        .iter()
        .map(|store| read_every_file(&root.path().join(store)))
        .sum();
    eprintln!(
        "every line and digest file read raw, {} kB, in {:?}: what `whole` spends beyond this is parsing",
        read / 1_000,
        started.elapsed()
    );
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter("suzatary::timings=info")
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        // The second read finds the files in the page cache and the words this process opened
        // in its cache of them, as a second session's would not.
        for run in ["first read", "second read"] {
            let cold = Disk::open(root.path(), memory.owners.clone());
            eprintln!("-- {run}: stores opened afresh");
            let started = std::time::Instant::now();
            let answer = live_shape::context(&cold.recollection());
            eprintln!(
                "-- {run}: context in {:?}, {} bytes",
                started.elapsed(),
                answer.to_string().len()
            );
        }
        let record = Disk::open(root.path(), memory.owners.clone())
            .recollection()
            .record()
            .expect("the record reads");
        let started = std::time::Instant::now();
        let lines = record.lines(16 * 1024);
        eprintln!(
            "-- one view within 16 kB: {} lines in {:?}",
            lines.len(),
            started.elapsed()
        );
    });
}

/// The bytes of every file under `dir`, each read whole, as a store's first read reads them.
fn read_every_file(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("a directory").flatten() {
            if entry.metadata().expect("its metadata").is_dir() {
                stack.push(entry.path());
            } else {
                total += std::fs::read(entry.path()).expect("a file").len() as u64;
            }
        }
    }
    total
}
