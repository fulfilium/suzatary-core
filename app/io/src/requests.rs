//! The requests devices made and the box performed, in two stores. The RECORDS, one JSON
//! provenance record per request named by it, are pushed with the data, so who asked travels
//! with the history; the ANSWERS a replay is given stay on the box, outside git, and are pruned
//! once the request is past its window, since a replay is refused from then on anyway.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use prodrome_core::event::Hash;
use suzatary_core::signatures::{Answer, Requested, WINDOW_SECONDS};
use suzatary_core::store::{Doc, Gets as _, StoreError};

use crate::docs::{DocCodec, Docs};
use crate::present::Present as _;

/// The requests' records under one directory and their answers under another, each at
/// `<name>.json`.
#[derive(Clone)]
pub struct Requests {
    records: Docs<Hash, Requested>,
    dir: PathBuf,
    answers: Docs<Hash, Answer>,
    answered: PathBuf,
}

impl std::fmt::Debug for Requests {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Requests")
            .field("dir", &self.dir)
            .field("answered", &self.answered)
            .finish_non_exhaustive()
    }
}

/// What the records' store keeps out of git beside them: a record's write in progress.
const IGNORED: &str = "*.writing\n";

/// How long an answer is kept: twice the window, so a request inside it always finds its own.
const KEPT: Duration = Duration::from_secs(2 * WINDOW_SECONDS.unsigned_abs());

impl Requests {
    /// The records under `dir`, pushed with the data, and the answers under `answered`, on the
    /// box alone; each made on the first request.
    #[must_use]
    pub fn open(dir: PathBuf, answered: PathBuf) -> Self {
        let (at, answered_at) = (dir.clone(), answered.clone());
        Self {
            records: Docs::new(
                "request store",
                move |name: &Hash| at.join(format!("{}.json", name.as_str())),
                &RECORD,
            ),
            dir,
            answers: Docs::new(
                "answer store",
                move |name: &Hash| answered_at.join(format!("{}.json", name.as_str())),
                &DocCodec::JSON,
            ),
            answered,
        }
    }

    /// The record of `name`, if the box performed it.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    pub fn get(&self, name: &Hash) -> Result<Option<Requested>, StoreError> {
        self.records.get(name)
    }

    /// The answer the box gave `name`, while it is kept.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    pub fn answer(&self, name: &Hash) -> Result<Option<Answer>, StoreError> {
        self.answers.get(name)
    }

    /// Keep `answer` on the box and `record` under its name, the answer first so a record never
    /// stands without one; then drop every answer past its window.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`], and a record whose name is not one.
    pub fn keep(&self, record: &Requested, answer: &Answer) -> Result<(), StoreError> {
        let name = Hash::new(record.name.clone())
            .map_err(|e| StoreError::unwritable("request store", e.to_string()))?;
        self.answers.set(&name, answer)?;
        ignoring(&self.dir)?;
        self.records.set(&name, record)?;
        prune(&self.answered, SystemTime::now());
        Ok(())
    }

    /// The record store's fsck: every file but its `.gitignore` and a write in progress is a
    /// `<name>.json` holding the record of that name, signed by the key it names; one line per
    /// file that is not. A store never written is clean; one that cannot be listed is a finding,
    /// never a clean store.
    #[must_use]
    pub fn verify(&self) -> Vec<String> {
        let unlisted = |e: std::io::Error| vec![format!("{}: {e}", self.dir.display())];
        let entries = match std::fs::read_dir(&self.dir).present() {
            Ok(None) => return Vec::new(),
            Ok(Some(entries)) => entries,
            Err(e) => return unlisted(e),
        };
        let names: Result<Vec<String>, std::io::Error> = entries
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect();
        let mut names = match names {
            Ok(names) => names,
            Err(e) => return unlisted(e),
        };
        names.retain(|file| file != ".gitignore" && !file.ends_with(".writing"));
        names.sort();
        names
            .into_iter()
            .filter_map(|file| self.finding(&file))
            .collect()
    }

    /// What is wrong with the file `file` of the record store, if anything.
    fn finding(&self, file: &str) -> Option<String> {
        let Some(stem) = file.strip_suffix(".json") else {
            return Some(format!("{file}: not a request record"));
        };
        let name = match Hash::new(stem) {
            Ok(name) => name,
            Err(e) => return Some(format!("{file}: {e}")),
        };
        match self.records.get(&name) {
            Ok(Some(record)) if record.verifies() => None,
            Ok(Some(_)) => Some(format!("{file}: its signature does not verify")),
            Ok(None) => Some(format!("{file}: holds the record of another request")),
            Err(e) => Some(format!("{file}: {e}")),
        }
    }
}

/// The records' `.gitignore`, written once, so a write caught in progress never rides a push.
fn ignoring(dir: &Path) -> Result<(), StoreError> {
    let path = dir.join(".gitignore");
    if path.exists() {
        return Ok(());
    }
    let unwritable = |e: std::io::Error| {
        StoreError::unwritable("request store", format!("{}: {e}", path.display()))
    };
    std::fs::create_dir_all(dir).map_err(unwritable)?;
    std::fs::write(&path, IGNORED).map_err(unwritable)
}

/// Remove each answer written more than [`KEPT`] before `now`. Best effort: an answer left
/// behind is pruned by the next keep, and one that cannot be read for its age stays.
fn prune(answered: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(answered) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|written| now.duration_since(written).is_ok_and(|age| age > KEPT));
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// One compact JSON record; bytes whose name is not the key's read as absent.
const RECORD: DocCodec<Hash, Requested> = DocCodec {
    print: |_, record| serde_json::to_vec(record).map_err(|e| e.to_string()),
    parse: |name, bytes| {
        let record: Requested = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok((record.name == name.as_str()).then_some(record))
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The record of the request named by `seed`'s bytes, signed by a fixed device key.
    fn record(seed: &[u8]) -> Requested {
        let signed = prodrome_core::sign::Secret::new(&"1".repeat(64))
            .expect("a seed")
            .sign(&prodrome_core::memo::name(seed, []));
        Requested {
            name: signed.object.as_str().to_owned(),
            method: "POST".to_owned(),
            target: "/api/act/add".to_owned(),
            at: "2026-10-09T12:00:00".to_owned(),
            key: signed.key.as_str().to_owned(),
            signature: signed.signature.as_str().to_owned(),
            actor: "ada".to_owned(),
            status: 200,
            wrote: Vec::new(),
        }
    }

    /// A record store holding only signed records under their own names verifies; a stray, a
    /// record under another name, unreadable bytes and a forged signature are each named.
    #[test]
    fn the_record_store_names_every_file_that_is_not_its_record() {
        let dir = tempfile::tempdir().expect("a directory");
        let requests = Requests::open(dir.path().join("requests"), dir.path().join("answers"));
        assert_eq!(requests.verify(), Vec::<String>::new(), "an absent store");
        let answer = Answer {
            status: 200,
            body: "{}".to_owned(),
        };
        let kept = record(b"a request");
        let a = kept.name.clone();
        requests.keep(&kept, &answer).expect("kept");
        let mut forged = record(b"another request");
        forged.signature = kept.signature.clone();
        let f = forged.name.clone();
        requests.keep(&forged, &answer).expect("kept");
        let b = "b".repeat(64);
        std::fs::write(dir.path().join("requests/x.json.writing"), "{").expect("in progress");
        assert_eq!(
            requests.verify(),
            vec![format!("{f}.json: its signature does not verify")]
        );
        std::fs::remove_file(dir.path().join(format!("requests/{f}.json"))).expect("removed");
        assert_eq!(requests.verify(), Vec::<String>::new());

        let records = dir.path().join("requests");
        std::fs::copy(
            records.join(format!("{a}.json")),
            records.join(format!("{b}.json")),
        )
        .expect("copied under another name");
        std::fs::write(records.join("stray.txt"), "x").expect("a stray");
        std::fs::write(records.join(format!("{}.json", "c".repeat(64))), "{").expect("junk");
        let found = requests.verify();
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found[0].starts_with(&format!("{b}.json: holds the record of another")));
        assert!(found[1].starts_with(&format!("{}.json: ", "c".repeat(64))));
        assert_eq!(found[2], "stray.txt: not a request record");
    }

    /// A record store never written is clean; one that is there and cannot be listed (a file
    /// where the directory belongs) is a finding, never a clean store.
    #[test]
    fn an_unlistable_record_store_is_a_finding() {
        let dir = tempfile::tempdir().expect("a directory");
        let records = dir.path().join("requests");
        let requests = Requests::open(records.clone(), dir.path().join("answers"));
        assert_eq!(requests.verify(), Vec::<String>::new());
        std::fs::write(&records, "not a directory").expect("written");
        let found = requests.verify();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with(&records.display().to_string()),
            "{found:?}"
        );
    }

    /// An answer older than twice the window goes, a fresh one stays.
    #[test]
    fn an_answer_past_its_window_is_pruned() {
        let dir = std::env::temp_dir().join(format!("suzatary-answers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        std::fs::write(dir.join("a.json"), "{}").expect("written");
        prune(&dir, SystemTime::now());
        assert!(dir.join("a.json").exists(), "a fresh answer stays");
        prune(&dir, SystemTime::now() + KEPT + Duration::from_secs(1));
        assert!(!dir.join("a.json").exists(), "an old one goes");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
