//! A store's `.gitignore` in its present form: each of the store's files local to one machine
//! on a line of its own, as the Prodrome's `verify` asks from 85e5083 on. The Prodrome writes one
//! only where there is none, so a store begun before the mark of the last hold (`.writes`) names
//! `/.heads` alone.

use std::io;
use std::path::Path;

use crate::durable;
use crate::present::Present as _;

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
    /// No `objects/`: no store, left alone.
    NoStore,
    /// The store writes its own at its next hold.
    NoFile,
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
    /// commits, writes its own `.gitignore`, or is no store.
    #[must_use]
    pub fn settled(&self) -> bool {
        match self {
            Lacks::NoStore | Lacks::NoFile | Lacks::Nothing => true,
            Lacks::Linked | Lacks::Lines(_) | Lacks::Other(_) => false,
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

/// What the `.gitignore` of the store at `root` lacks, and the bytes it holds once brought.
fn found(root: &Path) -> io::Result<(Lacks, Option<String>)> {
    if !root.join("objects").is_dir() {
        return Ok((Lacks::NoStore, None));
    }
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path).present()? {
        None => return Ok((Lacks::NoFile, None)),
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

    /// A `.gitignore` in a form no push commits, with the lines or without them, is left as it
    /// is: written, it would be staged and stop every push. A store with no `.gitignore`, a
    /// directory with no `objects/` and a `.gitignore` that is a symbolic link are left alone.
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
        assert_eq!(bring(dir.path()).expect("nothing to bring"), Lacks::NoFile);
        assert!(!dir.path().join(FILE).exists());

        let tier = suzatary_testkit::scratch("ignores");
        std::fs::write(tier.path().join(FILE), "*\n").expect("a tier's");
        assert_eq!(bring(tier.path()).expect("no store"), Lacks::NoStore);
        assert_eq!(held(&tier), "*\n");

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
