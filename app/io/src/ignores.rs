//! A store's `.gitignore` in its present form: each of the store's files local to one machine
//! on a line of its own, as the Prodrome's `verify` asks from 85e5083 on. The Prodrome writes one
//! only where there is none, so a store begun before the mark of the last hold (`.writes`) names
//! `/.heads` alone.

use std::io;
use std::path::Path;

use suzatary_os::durable;
use suzatary_os::present::Present as _;

/// In the order a store the Prodrome begins writes them.
const LOCAL: [&str; 2] = ["/.heads", "/.writes"];

/// The present `.gitignore` of a store the Prodrome begins.
pub const STORE: &str = "/.heads\n/.writes\n";
/// The present `.gitignore` of a store `suzatary prodrome init` charters private.
pub const PRIVATE_CHARTER: &str = "/.heads\n/.lock\n/.writes\n";
/// The present `.gitignore` of a store `suzatary prodrome init` charters public.
pub const PUBLIC_CHARTER: &str = "/.heads\n/.lock\n/_site\n/.writes\n";

const FILE: &str = ".gitignore";

/// What one store's `.gitignore` lacks of its present form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lacks {
    /// No store: no record of heads and no file in `objects/` named as an object; left alone.
    NoStore,
    /// A chartered store (`suzatary prodrome init`) with no `.gitignore`, left alone: its own
    /// form is its charter's, which `prodrome init` writes.
    Chartered,
    /// A symbolic link, left alone: [`bring`] would replace the link, not its target.
    Linked,
    /// It names every local file, in a form a push commits.
    Nothing,
    /// These lines, which [`bring`] adds at its end, giving a form a push commits.
    Lines(Vec<&'static str>),
    /// It is, or with these lines would be, in a form no push commits ([`STORE`],
    /// [`PRIVATE_CHARTER`] and [`PUBLIC_CHARTER`] are the ones safe-push and `roadmap-push.sh`
    /// commit), so [`bring`] leaves it alone: once staged, it would stop every push.
    Other(Vec<&'static str>),
}

impl Lacks {
    /// Whether the store needs nothing more: it names every local file in a form a push
    /// commits, or is no store.
    #[must_use]
    pub fn settled(&self) -> bool {
        match self {
            Lacks::NoStore | Lacks::Nothing => true,
            Lacks::Chartered | Lacks::Linked | Lacks::Lines(_) | Lacks::Other(_) => false,
        }
    }
}

/// `text` with each line of [`LOCAL`] it lacks added at its end, and the lines added.
fn brought(text: &str) -> (String, Vec<&'static str>) {
    let lacking: Vec<&'static str> = LOCAL
        .into_iter()
        .filter(|line| !text.lines().any(|held| held == *line))
        .collect();
    let mut out = text.to_owned();
    if !lacking.is_empty() && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for line in &lacking {
        out.push_str(line);
        out.push('\n');
    }
    (out, lacking)
}

/// Whether `root` reads as a Prodrome store of files: it holds a record of heads, or a file in
/// `objects/` named as the Prodrome names an object (`<64 lowercase hex digits>.py`).
fn reads_as_store(root: &Path) -> io::Result<bool> {
    if root.join(".heads").exists() {
        return Ok(true);
    }
    let Some(entries) = std::fs::read_dir(root.join("objects")).present()? else {
        return Ok(false);
    };
    for entry in entries {
        let name = entry?.file_name();
        let named = name
            .to_str()
            .and_then(|name| name.strip_suffix(".py"))
            .is_some_and(|hash| {
                hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            });
        if named {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The file `suzatary prodrome init` charters a store with.
pub const CHARTER: &str = "charter.toml";

/// What the `.gitignore` of the store at `root` lacks, and the bytes it holds once brought.
fn found(root: &Path) -> io::Result<(Lacks, Option<String>)> {
    if !reads_as_store(root)? {
        return Ok((Lacks::NoStore, None));
    }
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path).present()? {
        None if root.join(CHARTER).exists() => return Ok((Lacks::Chartered, None)),
        // The Prodrome writes a `.gitignore` only beside a record of heads it writes, and a hold
        // that finds the record true writes none, so a store with no `.gitignore` can hold a
        // `.writes` nothing ignores: it is given the store's form.
        None => return Ok((Lacks::Lines(LOCAL.to_vec()), Some(STORE.to_owned()))),
        Some(meta) if meta.file_type().is_symlink() => return Ok((Lacks::Linked, None)),
        Some(_) => {}
    }
    let (text, lacking) = brought(&std::fs::read_to_string(&path)?);
    Ok(
        match (
            [STORE, PRIVATE_CHARTER, PUBLIC_CHARTER].contains(&text.as_str()),
            lacking.is_empty(),
        ) {
            (true, true) => (Lacks::Nothing, None),
            (true, false) => (Lacks::Lines(lacking), Some(text)),
            (false, _) => (Lacks::Other(lacking), None),
        },
    )
}

/// What the `.gitignore` of the store at `root` lacks. Writes nothing.
///
/// # Errors
/// A `.gitignore` that is there and cannot be read.
pub fn lacks(root: &Path) -> io::Result<Lacks> {
    Ok(found(root)?.0)
}

/// Bring the `.gitignore` of the store at `root` to its present form, durably, and say what it
/// lacked; a file in a form no push commits is left alone ([`Lacks::Other`]). A second call
/// writes nothing.
///
/// # Errors
/// A `.gitignore` that cannot be read or replaced.
pub fn bring(root: &Path) -> io::Result<Lacks> {
    let (lacked, text) = found(root)?;
    if let Some(text) = text {
        durable::replace(&root.join(FILE), text.as_bytes(), durable::Mode::Kept)?;
    }
    Ok(lacked)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(text: Option<&str>) -> tempfile::TempDir {
        let dir = suzatary_testkit::scratch("ignores");
        std::fs::create_dir(dir.path().join("objects")).expect("a store");
        std::fs::write(
            dir.path()
                .join("objects")
                .join(format!("{}.py", "a".repeat(64))),
            "",
        )
        .expect("an object's name");
        if let Some(text) = text {
            std::fs::write(dir.path().join(FILE), text).expect("written");
        }
        dir
    }

    fn held(dir: &tempfile::TempDir) -> String {
        std::fs::read_to_string(dir.path().join(FILE)).expect("reads")
    }

    /// LAW: each form a store was begun in before the mark is brought to its present form, the
    /// one a push commits, by adding `/.writes` at its end; bringing it again writes nothing.
    #[test]
    fn each_old_form_is_brought_to_its_present_form_once() {
        for (old, present) in [
            ("/.heads\n", STORE),
            ("/.heads\n/.lock\n", PRIVATE_CHARTER),
            ("/.heads\n/.lock\n/_site\n", PUBLIC_CHARTER),
        ] {
            let dir = store(Some(old));
            assert_eq!(
                lacks(dir.path()).expect("read"),
                Lacks::Lines(vec!["/.writes"])
            );
            assert_eq!(held(&dir), old, "--check writes nothing");
            assert_eq!(
                bring(dir.path()).expect("brought"),
                Lacks::Lines(vec!["/.writes"])
            );
            assert_eq!(held(&dir), present);
            assert_eq!(bring(dir.path()).expect("brought"), Lacks::Nothing);
            assert_eq!(held(&dir), present);
        }
    }

    /// LAW: the Prodrome writes [`STORE`] for a store it begins, and a store of the form before
    /// the mark, once brought, is one its `verify` finds nothing in. A store with no `.gitignore`
    /// and a true record of heads gets none from a hold, which leaves a `.writes` that `verify`
    /// reports, and bringing it ends that.
    #[test]
    fn a_brought_store_holds_what_the_prodrome_writes_and_verify_finds_nothing() {
        use prodrome_core::store::EventStore;
        use suzatary_record::record::{GENESIS, TodoEvent};
        let dir = suzatary_testkit::scratch("ignores");
        let fresh = dir.path().join("fresh");
        EventStore::<TodoEvent, _>::new(&fresh, prodrome_core::policy::Untrusted::none())
            .init(GENESIS)
            .expect("begins");
        assert_eq!(
            std::fs::read_to_string(fresh.join(FILE)).expect("written"),
            STORE
        );

        let old = dir.path().join("old");
        let store = EventStore::<TodoEvent, _>::new(&old, prodrome_core::policy::Untrusted::none());
        store.init(GENESIS).expect("begins");
        std::fs::write(old.join(FILE), "/.heads\n").expect("the old form");
        assert!(
            store.verify().iter().any(|finding| matches!(
                finding,
                prodrome_core::dag::Finding::Unignored(".writes")
            )),
            "{:?}",
            store.verify()
        );
        assert_eq!(
            bring(&old).expect("brought"),
            Lacks::Lines(vec!["/.writes"])
        );
        assert_eq!(store.verify(), Vec::new());

        let bare = dir.path().join("bare");
        let store =
            EventStore::<TodoEvent, _>::new(&bare, prodrome_core::policy::Untrusted::none());
        store.init(GENESIS).expect("begins");
        std::fs::remove_file(bare.join(FILE)).expect("no .gitignore");
        store
            .record_heads()
            .expect("a hold that finds the record true");
        assert!(!bare.join(FILE).exists(), "the hold writes none");
        assert!(bare.join(".writes").exists());
        assert!(
            store.verify().iter().any(|finding| matches!(
                finding,
                prodrome_core::dag::Finding::Unignored(".writes")
            )),
            "{:?}",
            store.verify()
        );
        assert_eq!(bring(&bare).expect("brought"), Lacks::Lines(LOCAL.to_vec()));
        assert_eq!(store.verify(), Vec::new());
    }

    /// A `.gitignore` in a form no push commits, with the lines or without them, is left as it
    /// is: written, it would be staged and stop every push. A store with no `.gitignore` is given
    /// the store's form; a chartered one is left alone, as are a directory that does not read as
    /// a store (no `objects/`, or one holding nothing named as an object, as a git directory
    /// does) and a `.gitignore` that is a symbolic link.
    #[test]
    fn bringing_writes_only_a_form_a_push_commits() {
        for (held_before, lacking) in [
            ("/.heads\nobjects/", vec!["/.writes"]),
            ("*\n", vec!["/.heads", "/.writes"]),
            ("/.heads\n/.writes\n/notes.md\n", vec![]),
        ] {
            let dir = store(Some(held_before));
            assert_eq!(bring(dir.path()).expect("left"), Lacks::Other(lacking));
            assert_eq!(held(&dir), held_before);
        }

        let dir = store(None);
        assert_eq!(
            bring(dir.path()).expect("brought"),
            Lacks::Lines(LOCAL.to_vec())
        );
        assert_eq!(
            held(&dir),
            STORE,
            "a store with none is given the store's form"
        );

        let tier = suzatary_testkit::scratch("ignores");
        std::fs::write(tier.path().join(FILE), "*\n").expect("a tier's");
        assert_eq!(bring(tier.path()).expect("no store"), Lacks::NoStore);
        assert_eq!(held(&tier), "*\n");

        let git = suzatary_testkit::scratch("ignores");
        std::fs::create_dir_all(git.path().join("objects/ab")).expect("a git directory's");
        assert_eq!(bring(git.path()).expect("no store"), Lacks::NoStore);
        assert!(!git.path().join(FILE).exists());

        let chartered = store(None);
        std::fs::write(chartered.path().join(CHARTER), "public = true\n").expect("a charter");
        assert_eq!(bring(chartered.path()).expect("left"), Lacks::Chartered);
        assert!(!chartered.path().join(FILE).exists());

        let dir = store(None);
        let target = dir.path().join("elsewhere");
        std::fs::write(&target, "/.heads\n").expect("written");
        std::os::unix::fs::symlink(&target, dir.path().join(FILE)).expect("a link");
        assert_eq!(bring(dir.path()).expect("left"), Lacks::Linked);
        assert!(
            std::fs::symlink_metadata(dir.path().join(FILE))
                .expect("there")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target).expect("reads"),
            "/.heads\n"
        );
    }
}
