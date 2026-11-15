//! The record of a husked store's heads, `root/.heads`: the Prodrome's record (its SPEC §3),
//! kept by the husked store, which places its own files.
//!
//! The same file, in the same print ([`Heads::print`]), written by the same steps as the
//! Prodrome's store writes it: removed, with the root synced, before a holder of the lock places
//! any object, and written again, unlinked then created with no temp file and no sync, when the
//! holder is done. A record cut short at any byte does not parse, so no stop leaves a record
//! that is read and is not the heads. Before the first record, a `.gitignore` holding `/.heads`
//! is written durably where the root holds none, so the record stays on this machine.

use std::io::Write as _;
use std::path::Path;

use prodrome_core::dag::Heads;
use prodrome_core::literal::ProdromeError;

/// The record's file name in the store's root, the Prodrome's own.
pub(crate) const FILE: &str = ".heads";

/// What the store writes as its `.gitignore` where the root holds none.
const GITIGNORE: &str = "/.heads\n";

/// The record in `root`, where it is there and is the print of one value.
pub(crate) fn read(root: &Path) -> Option<Heads> {
    Heads::parse(&std::fs::read_to_string(root.join(FILE)).ok()?).ok()
}

/// The record removed, and the root synced also where it was absent (a host may delete it
/// without a sync), before a placement. A failure here places nothing.
///
/// # Errors
/// The record not removed, or the root not synced.
pub(crate) fn remove(root: &Path) -> Result<(), ProdromeError> {
    match std::fs::remove_file(root.join(FILE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(root, &e)),
        _ => {}
    }
    sync(root)
}

/// `heads` recorded at the end of a hold: the `.gitignore` first, durably, where there is none;
/// then any record unlinked and the file created anew and written, with no sync.
///
/// # Errors
/// The `.gitignore` or the record not written; the hold's write stands, and the record is
/// absent, so the next reader derives the heads.
pub(crate) fn write(root: &Path, heads: &Heads) -> Result<(), ProdromeError> {
    let ignore = root.join(".gitignore");
    if !ignore.exists() {
        crate::durable::replace(
            &ignore,
            GITIGNORE.as_bytes(),
            crate::durable::Mode::Readable,
        )
        .map_err(|e| failed(&ignore, &e))?;
    }
    let path = root.join(FILE);
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(&path, &e)),
        _ => {}
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut file| file.write_all(heads.print().as_bytes()))
        .map_err(|e| failed(&path, &e))
}

fn sync(dir: &Path) -> Result<(), ProdromeError> {
    std::fs::File::open(dir)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| failed(dir, &e))
}

fn failed(path: &Path, error: &std::io::Error) -> ProdromeError {
    ProdromeError::Store(format!("{}: {error}", path.display()))
}
