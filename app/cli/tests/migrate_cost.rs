//! What `suzatary secretary migrate-scrub --apply` costs, counted.
//!
//! The ignored measurement migrates [`SLOTS`] stale slots once and prints reseals per second, the
//! keys' syncs (through `suzatary_os::durable`; the lines' are not counted), scrubs and decryptions
//! per stale slot, and the lines' lock time, tried every millisecond, from counters, never a clock.
//!
//! `cargo test --release -p suzatary --features fixture --test all migrate_cost -- --ignored
//! --nocapture`

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use std::collections::BTreeSet;

use clap::Parser as _;
use prodrome_core::event::{Envelope, Hash};
use suzatary_lines::secretary::conversations::Conversations;
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_memory::secretary::locked_text::{KeyName, TextKey};
use suzatary_store::store::{Doc, Gets, Graph, Prodrome, Reads, StoreError, WriteLock};

use crate::secretary_cli;

const SLOTS: usize = 10_000;

#[derive(Debug, clap::Parser)]
struct Wrapper {
    #[command(flatten)]
    bulk: suzatary_cli::commands::fixture::Bulk,
}

/// A fresh instance holding `lines` generated lines over `conversations`, sealed unversioned,
/// `reseals` of their slots sealed again under the current rules.
pub(crate) fn stale_store(
    name: &str,
    lines: usize,
    conversations: usize,
    reseals: usize,
) -> suzatary_cli::instance::Instance {
    let instance = secretary_cli::fresh_instance(name);
    let (lines, conversations, reseals) = (
        lines.to_string(),
        conversations.to_string(),
        reseals.to_string(),
    );
    Wrapper::parse_from([
        "bulk",
        "--lines",
        &lines,
        "--conversations",
        &conversations,
        "--sealed",
        "unversioned",
        "--reseals",
        &reseals,
    ])
    .bulk
    .write(&instance)
    .expect("a generated store");
    instance
}

/// How long the file lock at `path` is held by anyone else while `run` runs.
fn held_while(path: &std::path::Path, run: impl FnOnce()) -> Duration {
    let done = Arc::new(AtomicBool::new(false));
    let probe = {
        let (done, path) = (Arc::clone(&done), path.to_path_buf());
        std::thread::spawn(move || {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
                .expect("the lock file");
            let mut held = Duration::ZERO;
            while !done.load(Ordering::SeqCst) {
                let tried = Instant::now();
                if file.try_lock().is_ok() {
                    file.unlock().expect("let go");
                    std::thread::sleep(Duration::from_millis(1));
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                    held += tried.elapsed();
                }
            }
            held
        })
    };
    run();
    done.store(true, Ordering::SeqCst);
    probe.join().expect("the probe")
}

#[test]
#[allow(clippy::cast_precision_loss)]
#[ignore = "a measurement: thousands of generated slots, and the disk's syncs"]
fn migrate_cost() {
    let instance = stale_store("migrate-cost", SLOTS, 50, 0);
    let stores = instance.conversation_stores();
    let at = prodrome_core::literal::Datetime::new(2026, 11, 20, 10, 0, 0, 0).expect("an instant");
    let (syncs, scrubs, opens) = (
        suzatary_os::durable::syncs(),
        suzatary_held::redact::scrubs(),
        suzatary_memory::secretary::locked_text::opens(),
    );
    let mut done = None;
    let started = Instant::now();
    let held = held_while(&instance.secretary.line_store().join("lines.lock"), || {
        done = Some(
            suzatary_lines::secretary::rescrub::run(
                stores.conversations(),
                at,
                suzatary_lines::secretary::rescrub::BATCH,
            )
            .expect("a run"),
        );
    });
    let took = started.elapsed();
    let done = done.expect("ran");
    let after = suzatary_os::durable::syncs();
    let stale = done.census.stale;
    assert!(stale >= SLOTS / 2, "the store is stale: {:?}", done.census);
    let per = |n: usize| n as f64 / stale as f64;
    println!(
        "{stale} stale slot(s), {} resealed, {} key(s) destroyed, in {took:?}: {:.0} slot(s)/s\n\
         lock held {held:?}\n\
         syncs: {} of a file, {} of a filesystem, {} of a directory: {:.4} per slot\n\
         scrubs {:.2} and decryptions {:.2} per slot",
        done.resealed,
        done.destroyed,
        stale as f64 / took.as_secs_f64(),
        after.files - syncs.files,
        after.filesystems - syncs.filesystems,
        after.dirs - syncs.dirs,
        per(
            after.files - syncs.files + after.filesystems - syncs.filesystems + after.dirs
                - syncs.dirs
        ),
        per(suzatary_held::redact::scrubs() - scrubs),
        per(suzatary_memory::secretary::locked_text::opens() - opens),
    );
}

/// Where a run is stopped, as a crash stops it.
#[derive(Debug, Clone, Copy)]
enum Stop {
    /// In batch `batch`, after `written` of its keys reached the disk, before any is durable.
    Keys { batch: usize, written: usize },
    /// In batch `batch`, after its keys are durable and `placed` of its lines are placed.
    Lines { batch: usize, placed: usize },
    /// After the sweep destroyed this many keys.
    Sweep(usize),
    /// Nowhere.
    Never,
}

fn stopped() -> StoreError {
    StoreError::unwritable("a stopped run", "stopped")
}

/// A key store that stops where `stop` says, counting the batches it is asked to write.
struct StoppingKeys<'a> {
    keys: &'a dyn Doc<KeyName, TextKey>,
    stop: Stop,
    batches: std::sync::atomic::AtomicUsize,
}

impl Gets<KeyName, TextKey> for StoppingKeys<'_> {
    fn get(&self, name: &KeyName) -> Result<Option<TextKey>, StoreError> {
        self.keys.get(name)
    }
}

impl Doc<KeyName, TextKey> for StoppingKeys<'_> {
    fn set(&self, name: &KeyName, key: &TextKey) -> Result<(), StoreError> {
        self.set_all(&[(name, key)])
    }

    fn remove(&self, name: &KeyName) -> Result<(), StoreError> {
        self.remove_all(&[name])
    }

    fn set_all(&self, values: &[(&KeyName, &TextKey)]) -> Result<(), StoreError> {
        let this = self.batches.fetch_add(1, Ordering::SeqCst);
        match self.stop {
            Stop::Keys { batch, written } if batch == this => {
                self.keys.set_all(&values[..written.min(values.len())])?;
                Err(stopped())
            }
            Stop::Keys { .. } | Stop::Lines { .. } | Stop::Sweep(_) | Stop::Never => {
                self.keys.set_all(values)
            }
        }
    }

    fn remove_all(&self, names: &[&KeyName]) -> Result<(), StoreError> {
        match self.stop {
            Stop::Sweep(removed) => {
                self.keys.remove_all(&names[..removed.min(names.len())])?;
                Err(stopped())
            }
            Stop::Keys { .. } | Stop::Lines { .. } | Stop::Never => self.keys.remove_all(names),
        }
    }

    fn tidy(&self) -> Result<(), StoreError> {
        self.keys.tidy()
    }
}

/// A lines store that stops where `stop` says, and appends `between` the moment the lines' lock
/// is asked for, as another writer does that takes the lock first.
struct Interrupted<'a> {
    lines: &'a dyn Prodrome<LineEvent>,
    stop: Stop,
    between: std::sync::Mutex<Option<LineEvent>>,
    batches: std::sync::atomic::AtomicUsize,
}

impl<'a> Interrupted<'a> {
    fn new(lines: &'a dyn Prodrome<LineEvent>, stop: Stop, between: Option<LineEvent>) -> Self {
        Self {
            lines,
            stop,
            between: std::sync::Mutex::new(between),
            batches: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl Reads<LineEvent> for Interrupted<'_> {
    fn objects(&self) -> Result<Vec<(Hash, Envelope<LineEvent>)>, StoreError> {
        self.lines.objects()
    }
    fn graph(&self) -> Result<Arc<Graph<LineEvent>>, StoreError> {
        self.lines.graph()
    }
    fn excluded(&self) -> Result<BTreeSet<Hash>, StoreError> {
        self.lines.excluded()
    }
    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.lines.tips()
    }
    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        self.lines.literals(names)
    }
    fn verify(&self) -> Vec<String> {
        self.lines.verify()
    }
}

impl Prodrome<LineEvent> for Interrupted<'_> {
    fn append(&self, event: LineEvent) -> Result<Hash, StoreError> {
        self.lines.append(event)
    }

    fn append_each(
        &self,
        next: &mut dyn FnMut() -> Result<Option<LineEvent>, StoreError>,
    ) -> Result<Vec<Hash>, StoreError> {
        let this = self.batches.fetch_add(1, Ordering::SeqCst);
        match self.stop {
            Stop::Lines { batch, placed } if batch == this => {
                let mut left = placed;
                self.lines.append_each(&mut || {
                    if left == 0 {
                        return Err(stopped());
                    }
                    left -= 1;
                    next()
                })
            }
            Stop::Keys { .. } | Stop::Lines { .. } | Stop::Sweep(_) | Stop::Never => {
                self.lines.append_each(next)
            }
        }
    }

    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        self.lines.accept(objects)
    }

    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        let between = self
            .between
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(line) = between {
            self.lines.append(line)?;
        }
        self.lines.locked()
    }
}

/// Every step of every batch of a run over the store's three stale slots, in batches of one, two
/// and a thousand, each beside its batches' size; then every step of the sweep.
fn stops() -> Vec<(usize, Stop)> {
    let mut stops = Vec::new();
    for size in [1, 2, 1000] {
        let mut left = 3;
        let mut batch = 0;
        while left > 0 {
            let this = left.min(size);
            for step in 0..=this {
                stops.push((
                    size,
                    Stop::Keys {
                        batch,
                        written: step,
                    },
                ));
                stops.push((
                    size,
                    Stop::Lines {
                        batch,
                        placed: step,
                    },
                ));
            }
            left -= this;
            batch += 1;
        }
    }
    stops.extend((0..=4).map(|removed| (1000, Stop::Sweep(removed))));
    stops
}

/// The crash order, "a line is never durable before its key": a run stopped at any step of a batch
/// or its sweep keeps every old key and reads the same words, and the next run reaches the store
/// an unstopped run reaches.
#[test]
fn migrate_scrub_stopped_at_any_step_of_a_batch_and_run_again_reads_the_same() {
    use suzatary_lines::secretary::rescrub::{self, Census};
    for (n, (size, stop)) in stops().into_iter().enumerate() {
        let instance = secretary_cli::fresh_instance(&format!("migrate-scrub-stop-{n}"));
        let id = secretary_cli::written_before_the_rules(&instance);
        let before = secretary_cli::read_lines(&instance, &id);
        let keys_before = secretary_cli::kept_keys(&instance);
        let stores = instance.conversation_stores();
        let whole = stores.conversations();
        let keys = StoppingKeys {
            keys: whole.keys,
            stop,
            batches: std::sync::atomic::AtomicUsize::new(0),
        };
        let lines = Interrupted::new(whole.lines, stop, None);
        let at =
            prodrome_core::literal::Datetime::new(2026, 10, 25, 10, 0, 0, 0).expect("an instant");
        let stopping = Conversations {
            keys: &keys,
            lines: &lines,
            ..whole
        };
        let batch = std::num::NonZeroUsize::new(size).expect("not zero");
        assert!(
            rescrub::run(stopping, at, batch).is_err(),
            "{stop:?} in batches of {size} stops the run"
        );
        let kept = secretary_cli::kept_keys(&instance);
        match stop {
            Stop::Keys { .. } | Stop::Lines { .. } => assert!(
                keys_before.is_subset(&kept),
                "{stop:?} in batches of {size}: every old key is kept"
            ),
            Stop::Sweep(_) | Stop::Never => {}
        }
        assert_eq!(
            secretary_cli::read_lines(&instance, &id),
            before,
            "stopped at {stop:?} in batches of {size}, every slot reads the same"
        );
        // What a batch stopped between its staging and its renames leaves.
        let staged = instance.secretary.line_keys().join(".stopped.writing");
        std::fs::write(&staged, b"half a key").expect("staged");

        let (done, said) = secretary_cli::migrated(&instance, true);
        assert!(done.is_some(), "{said}");
        assert!(
            !staged.exists(),
            "the run removes what a stopped batch staged"
        );
        assert_eq!(
            secretary_cli::read_lines(&instance, &id),
            before,
            "{stop:?} in batches of {size}"
        );
        let stores = instance.conversation_stores();
        assert_eq!(
            rescrub::census(stores.conversations()).expect("reads"),
            Census {
                lines: 6,
                current: 4,
                stale: 0,
                erased: 1,
                unopened: Vec::new(),
                withheld: Vec::new(),
            },
            "{stop:?} in batches of {size}"
        );
        assert_eq!(
            keys_before
                .difference(&secretary_cli::kept_keys(&instance))
                .count(),
            4,
            "{stop:?} in batches of {size}: the three replaced seals' keys and the erased slot's \
             are destroyed"
        );
        let (done, _) = secretary_cli::migrated(&instance, true);
        let done = done.expect("a run");
        assert_eq!((done.resealed, done.destroyed), (0, 0), "{stop:?}");
    }
}

/// A crash inside a hold may keep a reseal and lose the line it rests on, so the next run's sweep
/// destroys the waiting reseal's key and, once the slot is erased, no key on disk opens its files.
#[test]
fn migrate_scrub_destroys_the_key_of_a_reseal_kept_without_its_parent() {
    use suzatary_lines::secretary::line_shape::LineShape;
    use suzatary_lines::secretary::{rescrub, transcript};
    use suzatary_memory::secretary::erasure::Erasure;

    let instance = secretary_cli::fresh_instance("migrate-scrub-orphan");
    let id = secretary_cli::written_before_the_rules(&instance);
    let before = secretary_cli::read_lines(&instance, &id);
    let at = |minute: u32| {
        prodrome_core::literal::Datetime::new(2026, 10, 25, 10, minute, 0, 0).expect("an instant")
    };
    let reseals: Vec<(Hash, LineEvent)> = {
        let stores = instance.conversation_stores();
        let whole = stores.conversations();
        let lines = Interrupted::new(
            whole.lines,
            Stop::Lines {
                batch: 0,
                placed: 3,
            },
            None,
        );
        let stopping = Conversations {
            lines: &lines,
            ..whole
        };
        assert!(rescrub::run(stopping, at(0), rescrub::BATCH).is_err());
        let graph = whole.lines.graph().expect("reads");
        graph
            .objects()
            .filter_map(|(name, object)| Some((name.clone(), object.event()?.clone())))
            .filter(|(_, line)| matches!(line.shape(), LineShape::Resealed { .. }))
            .collect()
    };
    assert_eq!(reseals.len(), 3, "placed, parents first");
    // The first reseal's rename lost: the two placed after it rest on it, and wait.
    let objects = instance.secretary.line_store().join("objects");
    std::fs::remove_file(objects.join(format!("{}.py", reseals[0].0.as_str())))
        .expect("the parent's rename undone");
    let heads = instance.secretary.line_store().join(".heads");
    if heads.exists() {
        std::fs::remove_file(&heads).expect("the record of heads goes with it");
    }
    let LineShape::Resealed { of: slot, .. } = reseals[1].1.shape() else {
        unreachable!("only reseals were kept");
    };
    let slot = slot.clone();
    let waiting = reseals[1].1.keys().next().expect("a reseal's key").clone();
    assert!(secretary_cli::kept_keys(&instance).contains(waiting.as_str()));

    let (done, said) = secretary_cli::migrated(&instance, true);
    assert!(done.is_some(), "{said}");
    assert_eq!(secretary_cli::read_lines(&instance, &id), before);
    assert!(
        !secretary_cli::kept_keys(&instance).contains(waiting.as_str()),
        "the sweep destroys the waiting reseal's key"
    );

    let stores = instance.conversation_stores();
    let actor = prodrome_core::event::Actor::new("assistant").expect("an actor");
    transcript::erase(
        stores.conversations(),
        &id,
        [slot.clone()].into_iter().collect(),
        Erasure::Asked { by: actor },
        at(5),
    )
    .expect("erased");
    let seals = no_key_opens_a_seal_of(&instance, &slot);
    assert!(
        objects
            .join(format!("{}.py", reseals[1].0.as_str()))
            .exists(),
        "the waiting reseal is still on disk"
    );
    assert!(
        seals >= 2,
        "the waiting reseal and the slot's new one: {seals}"
    );
}

/// The counts, for generated stores of several shapes: the census opens and scrubs each stale
/// slot once and opens no other slot's words (a reseal already in the store is opened once, to
/// see that it supersedes); a run seals each once, and its sweep opens and scrubs nothing more.
#[test]
fn migrate_scrub_scrubs_each_stale_slot_once() {
    use suzatary_held::redact::scrubs;
    use suzatary_lines::secretary::rescrub;
    use suzatary_memory::secretary::locked_text::opens;
    for (n, (lines, conversations, reseals)) in [(4, 1, 0), (13, 2, 0), (30, 3, 0), (20, 2, 4)]
        .into_iter()
        .enumerate()
    {
        let instance = stale_store(
            &format!("migrate-scrub-once-{n}"),
            lines,
            conversations,
            reseals,
        );
        let stores = instance.conversation_stores();
        let (scrubbed, opened) = (scrubs(), opens());
        let census = rescrub::census(stores.conversations()).expect("a census");
        assert!(census.stale > 0, "{census:?}");
        assert_eq!(scrubs() - scrubbed, census.stale, "the census: {census:?}");
        assert_eq!(
            opens() - opened,
            census.stale + reseals,
            "the census: {census:?}"
        );

        let at =
            prodrome_core::literal::Datetime::new(2026, 11, 20, 10, 0, 0, 0).expect("an instant");
        let (scrubbed, opened) = (scrubs(), opens());
        let done = rescrub::run(stores.conversations(), at, rescrub::BATCH).expect("a run");
        assert_eq!(done.census, census, "one reading, the same store");
        assert_eq!(done.resealed, census.stale, "one seal per stale slot");
        if reseals == 0 {
            assert_eq!(scrubs() - scrubbed, census.stale, "a run: {done:?}");
            assert_eq!(opens() - opened, census.stale, "a run: {done:?}");
        }
    }
}

/// A run opens the stale slots under no lock, then reads again under the lock and trusts that
/// reading, so a slot erased between the two stays erased, never sealed again, and counts erased.
#[test]
fn migrate_scrub_trusts_the_reading_under_the_lock() {
    use suzatary_lines::secretary::rescrub;
    use suzatary_memory::secretary::erasure::Erasure;

    let instance = secretary_cli::fresh_instance("migrate-scrub-between");
    let id = secretary_cli::written_before_the_rules(&instance);
    let stores = instance.conversation_stores();
    let whole = stores.conversations();
    let held = whole.lines.events().expect("reads");
    let first = held
        .iter()
        .find(|line| line.conversation() == &id)
        .and_then(|line| line.keys().next())
        .expect("the first line's slot, stale")
        .clone();
    let actor = prodrome_core::event::Actor::new("assistant").expect("an actor");
    let at = |minute: u32| {
        prodrome_core::literal::Datetime::new(2026, 10, 25, 10, minute, 0, 0).expect("an instant")
    };
    let erasure = LineEvent::erasure(
        id.clone(),
        at(0),
        actor.clone(),
        u32::try_from(held.len()).expect("a count"),
        [first].into_iter().collect(),
        Erasure::Asked { by: actor },
    );
    let lines = Interrupted::new(whole.lines, Stop::Never, Some(erasure));
    let done = rescrub::run(
        Conversations {
            lines: &lines,
            ..whole
        },
        at(1),
        rescrub::BATCH,
    )
    .expect("a run");
    assert_eq!(
        (done.census.stale, done.census.erased, done.resealed),
        (2, 2, 2),
        "{done:?}"
    );
    let (done, _) = secretary_cli::migrated(&instance, true);
    let done = done.expect("a run");
    assert_eq!((done.resealed, done.destroyed), (0, 0));
}

/// Whatever file of a resealed store stops reading for a while and whatever runs meanwhile, every
/// slot not erased opens again once the file is back: a sweep destroys a key only where what it
/// opens is held elsewhere or erased.
#[test]
fn migrate_scrub_keeps_every_slot_through_a_file_that_does_not_read() {
    use suzatary_lines::secretary::{rescrub, transcript};
    use suzatary_memory::secretary::erasure::Erasure;

    let at = |minute: u32| {
        prodrome_core::literal::Datetime::new(2026, 10, 25, 11, minute, 0, 0).expect("an instant")
    };
    let files = |instance: &suzatary_cli::instance::Instance| {
        let mut files: Vec<std::path::PathBuf> =
            std::fs::read_dir(instance.secretary.line_store().join("objects"))
                .expect("the objects")
                .map(|entry| entry.expect("a file").path())
                .collect();
        files.sort();
        files
    };
    // Resealed and swept, as a completed migration leaves it; or resealed with the old keys
    // still standing, as a run stopped before its sweep leaves it.
    let resealed = |instance: &suzatary_cli::instance::Instance, swept: bool| {
        if swept {
            secretary_cli::migrated(instance, true);
        } else {
            let stores = instance.conversation_stores();
            rescrub::reseal(stores.conversations(), at(0)).expect("resealed");
        }
    };
    for swept in [true, false] {
        let count = {
            let instance =
                secretary_cli::fresh_instance(&format!("migrate-scrub-unread-count-{swept}"));
            secretary_cli::written_before_the_rules(&instance);
            resealed(&instance, swept);
            files(&instance).len()
        };
        for (n, garbled) in (0..count).flat_map(|n| [(n, true), (n, false)]) {
            let instance = secretary_cli::fresh_instance(&format!(
                "migrate-scrub-unread-{swept}-{n}-{garbled}"
            ));
            let id = secretary_cli::written_before_the_rules(&instance);
            resealed(&instance, swept);
            let file = files(&instance)[n].clone();
            let bytes = std::fs::read(&file).expect("the file");
            if garbled {
                std::fs::write(&file, b"\xff\xfe not a line").expect("garbled");
            } else {
                std::fs::remove_file(&file).expect("gone");
            }
            {
                // Each may refuse while the file does not read; none may lose a slot.
                let stores = instance.conversation_stores();
                let whole = stores.conversations();
                let _ = transcript::sweep(whole);
                let first = whole
                    .lines
                    .events()
                    .ok()
                    .and_then(|lines| lines.into_iter().find(|line| line.conversation() == &id))
                    .and_then(|line| line.keys().next().cloned());
                if let Some(first) = first {
                    let actor = prodrome_core::event::Actor::new("assistant").expect("an actor");
                    let _ = transcript::erase(
                        whole,
                        &id,
                        [first].into_iter().collect(),
                        Erasure::Asked { by: actor },
                        at(1),
                    );
                }
                let _ = rescrub::run(whole, at(2), rescrub::BATCH);
                let _ = transcript::sweep(whole);
            }
            std::fs::write(&file, &bytes).expect("restored");
            let heads = instance.secretary.line_store().join(".heads");
            if heads.exists() {
                std::fs::remove_file(&heads).expect("the record of heads, derived again");
            }
            let stores = instance.conversation_stores();
            let census = rescrub::census(stores.conversations()).expect("reads once restored");
            assert!(
                census.unopened.is_empty() && census.withheld.is_empty(),
                "file {n} {}, {}: {census:?}",
                if garbled { "garbled" } else { "gone" },
                if swept { "swept" } else { "unswept" }
            );
        }
    }
}

/// A render cache that forwards every forgetting and keeps a note of it.
struct Recording<'a> {
    renders: &'a dyn suzatary_todo::render_cache::RenderCache,
    forgot: std::sync::Mutex<Vec<suzatary_base::answer::Renderable>>,
}

impl suzatary_todo::render_cache::RenderCache for Recording<'_> {
    fn forget(&self, block: &suzatary_base::answer::Renderable) -> Result<(), String> {
        self.forgot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(block.clone());
        self.renders.forget(block)
    }
}

/// Erasing a slot whose reseal waits outside the history destroys the reseal's key and the slot's
/// and forgets every block rendered from the reseal's words, so no key on disk opens a seal of it.
#[test]
fn migrate_scrub_erasing_a_slot_whose_reseal_waits_empties_every_holder() {
    use suzatary_lines::secretary::line_shape::LineShape;
    use suzatary_lines::secretary::transcript;
    use suzatary_memory::secretary::erasure::Erasure;

    let instance = secretary_cli::fresh_instance("migrate-scrub-erase-waiting");
    let id = secretary_cli::written_before_the_rules(&instance);
    secretary_cli::migrated(&instance, true);
    let objects = instance.secretary.line_store().join("objects");
    // A reseal of a slot whose words were rendered to blocks, its key, and those blocks.
    let (reseal, slot, rendered) = {
        let stores = instance.conversation_stores();
        let whole = stores.conversations();
        let graph = whole.lines.graph().expect("reads");
        graph
            .objects()
            .filter_map(|(name, object)| Some((name.clone(), object.event()?.clone())))
            .find_map(|(name, line)| {
                let LineShape::Resealed { of, .. } = line.shape() else {
                    return None;
                };
                let key = whole.keys.get(line.keys().next()?).ok()??;
                let rendered = transcript::fed(&line, 0, &key);
                (!rendered.is_empty()).then(|| ((name, line.clone()), of.clone(), rendered))
            })
            .expect("a resealed answer")
    };
    // The last line the reseals rest on that is not the slot's own: it goes for a while.
    let between = {
        let stores = instance.conversation_stores();
        let graph = stores.conversations().lines.graph().expect("reads");
        graph
            .objects()
            .filter_map(|(name, object)| Some((name.clone(), object.event()?.clone())))
            .filter(|(_, line)| {
                !matches!(line.shape(), LineShape::Resealed { .. })
                    && !line.keys().any(|key| key == &slot)
            })
            .last()
            .expect("a line after the slot's")
            .0
    };
    let file = objects.join(format!("{}.py", between.as_str()));
    let bytes = std::fs::read(&file).expect("the line");
    // Gone, not garbled: a file that does not parse stops every write, an erasure's too.
    std::fs::remove_file(&file).expect("gone");

    let stores = instance.conversation_stores();
    let whole = stores.conversations();
    let recording = Recording {
        renders: whole.renders,
        forgot: std::sync::Mutex::new(Vec::new()),
    };
    let actor = prodrome_core::event::Actor::new("assistant").expect("an actor");
    let at = prodrome_core::literal::Datetime::new(2026, 10, 25, 12, 0, 0, 0).expect("an instant");
    let erased = transcript::erase(
        Conversations {
            renders: &recording,
            ..whole
        },
        &id,
        [slot.clone()].into_iter().collect(),
        Erasure::Asked { by: actor },
        at,
    );
    assert!(erased.is_ok(), "the erasure is written: {erased:?}");
    let reseal_key = reseal.1.keys().next().expect("its key").clone();
    let on_disk = secretary_cli::kept_keys(&instance);
    assert!(
        !on_disk.contains(reseal_key.as_str()) && !on_disk.contains(slot.as_str()),
        "the waiting reseal's key and the slot's own are destroyed"
    );
    let forgot = recording
        .forgot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for block in &rendered {
        assert!(
            forgot.contains(block),
            "a block rendered from the reseal is kept"
        );
    }

    std::fs::write(&file, &bytes).expect("restored");
    let heads = instance.secretary.line_store().join(".heads");
    if heads.exists() {
        std::fs::remove_file(&heads).expect("the record of heads, derived again");
    }
    let stores = instance.conversation_stores();
    transcript::sweep(stores.conversations()).expect("sweeps");
    no_key_opens_a_seal_of(&instance, &slot);
}

/// Assert that no key on disk opens any line file on disk that seals `slot` (its own line, or a
/// reseal of it, in the history or waiting outside it); answers how many such files there are.
fn no_key_opens_a_seal_of(instance: &suzatary_cli::instance::Instance, slot: &KeyName) -> usize {
    use suzatary_lines::secretary::line_shape::LineShape;
    let on_disk = secretary_cli::kept_keys(instance);
    let vocabulary = suzatary_lines::secretary::line_event::LineVocabulary;
    let mut seals = 0;
    for file in
        std::fs::read_dir(instance.secretary.line_store().join("objects")).expect("the objects")
    {
        let print = std::fs::read_to_string(file.expect("a file").path()).expect("a print");
        let Some(line) = prodrome_core::event::parse_envelope::<LineEvent>(&vocabulary, &print)
            .ok()
            .and_then(|object| object.event().cloned())
        else {
            continue;
        };
        let of_slot = if let LineShape::Resealed { of, .. } = line.shape() {
            of == slot
        } else {
            line.keys().any(|key| key == slot)
        };
        if of_slot {
            seals += 1;
            for key in line.keys() {
                assert!(
                    !on_disk.contains(key.as_str()),
                    "{key} opens a seal of the erased slot"
                );
            }
        }
    }
    seals
}
