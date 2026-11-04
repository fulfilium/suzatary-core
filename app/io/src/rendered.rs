//! Typst's HTML by what it renders: one tabulation in this process over a replica on disk.

use std::path::{Path, PathBuf};

use prodrome_core::event::Hash;
use prodrome_core::memo::{Finding, Key};
use suzatary_core::table::Table;
use suzatary_core::tripwire::suspicious;

use crate::present::Present as _;

/// What a renderer has computed (the Prodrome's design §6.2): (function, source) keys to HTML,
/// in a [`Table`] and, optionally, one file per key in a cache directory. Neither is a source of
/// truth. A key whose two replicas hold two values is a [`Finding`]: memory keeps both and answers
/// neither, the file is deleted, and the source is compiled again.
#[derive(Debug)]
pub struct Rendered {
    table: Table<String>,
    disk: Option<Disk>,
    writes: std::sync::atomic::AtomicUsize,
}

#[derive(Debug)]
struct Disk {
    dir: PathBuf,
    current: Vec<Hash>,
}

/// How many files the directory keeps for one function, the oldest going first.
pub const KEPT_ON_DISK: usize = 8192;

const TRIM_EVERY: usize = 256;

impl Rendered {
    /// At most `capacity` keys in memory, over `disk`; folders of functions not in `current` go.
    #[must_use]
    pub fn on_disk(disk: impl Into<PathBuf>, capacity: usize, current: &[&Hash]) -> Rendered {
        let disk = Disk {
            dir: disk.into(),
            current: current.iter().map(|f| (*f).clone()).collect(),
        };
        // A failed sweep is retried, and reported, by the next forget.
        let _ = sweep(&disk);
        Rendered {
            table: Table::new(capacity),
            disk: Some(disk),
            writes: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// A tabulation in memory alone.
    #[must_use]
    pub fn in_memory(capacity: usize) -> Rendered {
        Rendered {
            table: Table::new(capacity),
            disk: None,
            writes: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// The value kept under `key`, from memory or else from the disk, tripwire-checked on the
    /// way in. A missing file is a miss; an unreadable one is an error, never a miss.
    pub fn recall(&self, key: &Key) -> std::io::Result<Option<String>> {
        if let Some(kept) = self.table.kept(key) {
            return Ok(Some(kept));
        }
        let Some(file) = self.file(key) else {
            return Ok(None);
        };
        let Some(html) = std::fs::read_to_string(&file).present()? else {
            return Ok(None);
        };
        if suspicious(&html).is_some() {
            let _ = std::fs::remove_file(&file);
            return Ok(None);
        }
        // A file that disagrees with memory is a finding: nothing answers it, and the file goes.
        if self.table.keep(key.clone(), html.clone()).is_err() {
            let _ = std::fs::remove_file(&file);
            return Ok(None);
        }
        Ok(Some(html))
    }

    /// `html` kept under `key`, in memory and on disk.
    ///
    /// # Errors
    /// A [`Finding`] when the key already held another value.
    pub fn remember(&self, key: Key, html: &str) -> Result<(), Finding> {
        // An unreadable file is left for `recall` to report: neither joined nor written over.
        let file = self
            .file(&key)
            .map(|file| (std::fs::read_to_string(&file).present(), file));
        // Another writer's value is joined first, so a disagreement is a finding, not an overwrite.
        if let Some((Ok(Some(there)), _)) = &file {
            let _ = self.table.keep(key.clone(), there.clone());
        }
        let kept = self.table.keep(key, html.to_owned());
        if let Some((Ok(_), file)) = file {
            // A second value is no file's: the last writer never wins.
            if kept.is_err() {
                let _ = std::fs::remove_file(&file);
            } else {
                write(&file, html);
                if self
                    .writes
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    .is_multiple_of(TRIM_EVERY)
                {
                    trim(file.parent().unwrap_or(&file), KEPT_ON_DISK);
                }
            }
        }
        kept
    }

    /// Forget what `key` keeps, in memory and on disk (synced), and every function's folder that
    /// is not current: an older build may have written the same words under its own name since.
    ///
    /// # Errors
    /// A removal or its sync failing: the file may still be there.
    pub fn forget(&self, key: &Key) -> std::io::Result<()> {
        self.table.forget(key);
        let Some(disk) = &self.disk else {
            return Ok(());
        };
        crate::durable::remove(&disk.file(key))?;
        sweep(disk)?;
        std::fs::File::open(&disk.dir)
            .present()?
            .map_or(Ok(()), |dir| dir.sync_all())
    }

    #[must_use]
    pub fn findings(&self) -> Vec<Finding> {
        self.table.findings()
    }

    fn file(&self, key: &Key) -> Option<PathBuf> {
        Some(self.disk.as_ref()?.file(key))
    }
}

impl Disk {
    fn file(&self, key: &Key) -> PathBuf {
        self.dir
            .join(key.function.as_str())
            .join(format!("{}.html", key.argument.as_str()))
    }
}

/// Remove flat `<sha>.html`/`.tmp` files and every function folder not in `disk.current`.
fn sweep(disk: &Disk) -> std::io::Result<()> {
    let Some(entries) = std::fs::read_dir(&disk.dir).present()? else {
        return Ok(());
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "html" || e == "tmp") {
            std::fs::remove_file(path).present()?;
        } else if path.is_dir()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && Hash::new(name).is_ok()
            && !disk.current.iter().any(|f| f.as_str() == name)
        {
            std::fs::remove_dir_all(path).present()?;
        }
    }
    Ok(())
}

fn trim(dir: &Path, kept: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= kept {
        return;
    }
    files.sort();
    for (_, path) in files.iter().take(files.len() - kept) {
        let _ = std::fs::remove_file(path);
    }
}

/// Replaced whole, since a reader may read while a warm writes; a failure costs only a compile.
fn write(file: &Path, html: &str) {
    let _ = crate::durable::replace(file, html.as_bytes(), crate::durable::Mode::Readable);
}

#[cfg(test)]
mod tests {
    use super::*;
    use prodrome_core::memo;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-io-rendered-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn function() -> Hash {
        memo::name(b"a renderer under test", [])
    }

    fn key(function: &Hash, source: &str) -> Key {
        Key {
            function: function.clone(),
            argument: suzatary_core::template::named(source),
        }
    }

    #[test]
    fn a_value_remembered_is_recalled_from_memory_and_from_a_cold_process_on_the_same_disk() {
        let dir = scratch("cold");
        let key = key(&function(), "= A heading");
        Rendered::on_disk(&dir, 8, &[&function()])
            .remember(key.clone(), "<h2>A heading</h2>")
            .expect("no finding");
        let cold = Rendered::on_disk(&dir, 8, &[&function()]);
        assert_eq!(
            cold.recall(&key).expect("read").as_deref(),
            Some("<h2>A heading</h2>")
        );
        std::fs::remove_dir_all(&dir).expect("the replica on disk");
        assert_eq!(
            cold.recall(&key).expect("read").as_deref(),
            Some("<h2>A heading</h2>"),
            "the memory answers once the disk is gone"
        );
    }

    #[test]
    fn an_unreadable_file_is_an_error_never_a_miss() {
        let dir = scratch("unreadable");
        let key = key(&function(), "= Unreadable");
        let rendered = Rendered::on_disk(&dir, 8, &[&function()]);
        std::fs::create_dir_all(rendered.file(&key).expect("on disk")).expect("a directory");
        assert!(rendered.recall(&key).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_suspicious_file_is_no_value() {
        let dir = scratch("suspicious");
        let key = key(&function(), "= Suspicious");
        let rendered = Rendered::on_disk(&dir, 8, &[&function()]);
        write(
            &rendered.file(&key).expect("on disk"),
            "<p onclick=\"steal()\">click</p>",
        );
        assert_eq!(rendered.recall(&key).expect("read"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_values_at_one_key_are_a_finding_and_neither_is_recalled() {
        let rendered = Rendered::in_memory(8);
        let key = key(&function(), "x");
        rendered.remember(key.clone(), "<p>one</p>").expect("first");
        assert_eq!(
            rendered.remember(key.clone(), "<p>two</p>"),
            Err(Finding { key: key.clone() })
        );
        assert_eq!(rendered.recall(&key).expect("read"), None);
        assert_eq!(rendered.findings(), vec![Finding { key }]);
    }

    #[test]
    fn a_second_value_deletes_the_file_and_no_writer_wins() {
        let dir = scratch("second");
        let key = key(&function(), "y");
        let one = Rendered::on_disk(&dir, 8, &[&function()]);
        one.remember(key.clone(), "<p>one</p>").expect("first");
        assert!(one.remember(key.clone(), "<p>two</p>").is_err());
        assert!(!one.file(&key).expect("on disk").exists(), "the file went");
        assert_eq!(
            Rendered::on_disk(&dir, 8, &[&function()])
                .recall(&key)
                .expect("read"),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_value_another_process_left_that_disagrees_is_a_finding_and_the_file_goes() {
        let dir = scratch("disagrees");
        let key = key(&function(), "z");
        Rendered::on_disk(&dir, 8, &[&function()])
            .remember(key.clone(), "<p>theirs</p>")
            .expect("first");
        let here = Rendered::on_disk(&dir, 8, &[&function()]);
        assert!(
            here.remember(key.clone(), "<p>mine</p>").is_err(),
            "a finding, never an overwrite"
        );
        assert!(!here.file(&key).expect("on disk").exists());
        assert_eq!(here.recall(&key).expect("read"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_flat_layout_goes_and_the_directory_is_bounded() {
        let dir = scratch("flat");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join(format!("{}.html", "a".repeat(64))), "<p>old</p>")
            .expect("old file");
        let rendered = Rendered::on_disk(&dir, 4, &[&function()]);
        assert_eq!(
            std::fs::read_dir(&dir).expect("dir").count(),
            0,
            "the old layout went"
        );
        for n in 0..10 {
            rendered
                .remember(key(&function(), &n.to_string()), "<p>x</p>")
                .expect("kept");
        }
        let sub = dir.join(function().as_str());
        trim(&sub, 3);
        assert_eq!(std::fs::read_dir(&sub).expect("dir").count(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_function_no_longer_current_has_its_folder_removed() {
        let dir = scratch("renamed");
        let old = memo::name(b"a renderer of an older build", []);
        Rendered::on_disk(&dir, 8, &[&old])
            .remember(key(&old, "x"), "<p>old</p>")
            .expect("kept");
        std::fs::create_dir_all(dir.join("not-a-function")).expect("dir");
        let _ = Rendered::on_disk(&dir, 8, &[&function()]);
        assert!(
            !dir.join(old.as_str()).exists(),
            "the old function's folder went"
        );
        assert!(
            dir.join("not-a-function").exists(),
            "what is not a function's folder stays"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_forget_takes_what_an_older_build_wrote_since_the_start() {
        let dir = scratch("older-since");
        let old = memo::name(b"a renderer of an older build", []);
        let here = Rendered::on_disk(&dir, 8, &[&function()]);
        Rendered::on_disk(&dir, 8, &[&old])
            .remember(key(&old, "erased words"), "<p>erased words</p>")
            .expect("kept");
        here.remember(key(&function(), "kept"), "<p>kept</p>")
            .expect("kept");
        here.forget(&key(&function(), "erased words"))
            .expect("forgot");
        assert!(
            !dir.join(old.as_str()).exists(),
            "the older build's folder went"
        );
        assert_eq!(
            here.recall(&key(&function(), "kept"))
                .expect("read")
                .as_deref(),
            Some("<p>kept</p>"),
            "and nothing of this build's but the key"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_functions_never_read_each_others_values() {
        let rendered = Rendered::in_memory(8);
        let other = memo::name(b"another renderer", []);
        rendered
            .remember(key(&function(), "x"), "<p>mine</p>")
            .expect("no finding");
        assert_eq!(rendered.recall(&key(&other, "x")).expect("read"), None);
    }

    #[test]
    fn a_bounded_table_forgets_and_answers_the_same_from_disk() {
        let dir = scratch("bounded");
        let rendered = Rendered::on_disk(&dir, 1, &[&function()]);
        let keys: Vec<Key> = ["a", "b", "c"]
            .iter()
            .map(|s| key(&function(), s))
            .collect();
        for (key, html) in keys.iter().zip(["<p>a</p>", "<p>b</p>", "<p>c</p>"]) {
            rendered.remember(key.clone(), html).expect("no finding");
        }
        for (key, html) in keys.iter().zip(["<p>a</p>", "<p>b</p>", "<p>c</p>"]) {
            assert_eq!(rendered.recall(key).expect("read").as_deref(), Some(html));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
