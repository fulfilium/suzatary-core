//! The crate's one way to replace a file's bytes and one way to take a cross-process lock.
//!
//! A replacement is crash-consistent (temporary beside the target, sync, rename, sync the
//! directory): a reader or a rebooted box sees the old bytes or the new, never a partial file.

use crate::present::Present as _;
use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

/// Who may read a replaced file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `0o600`: a grant, a ledger, anything only this user reads.
    Private,
    /// `0o644`: a document other users of the box may read.
    Readable,
    /// The target's own bits, and `0o600` for a new one, so no rewrite widens them.
    Kept,
}

impl Mode {
    fn bits(self, path: &Path) -> io::Result<u32> {
        match self {
            Self::Private => Ok(0o600),
            Self::Readable => Ok(0o644),
            Self::Kept => Ok(std::fs::metadata(path)
                .present()?
                .map_or(0o600, |held| held.permissions().mode() & 0o7777)),
        }
    }
}

/// Make `path` hold exactly `bytes`, durably, creating its directory.
///
/// # Errors
/// A failure before the rename leaves the target as it was and no temporary; a failed directory
/// sync after it leaves the new bytes in place, not yet known to survive a power cut.
pub fn replace(path: &Path, bytes: &[u8], mode: Mode) -> io::Result<()> {
    deliver(path, parent(path)?, bytes, mode)
}

/// [`replace`], staged in `staging` (a maildir's `tmp/`) rather than beside the target.
/// `staging` must be on the target's filesystem, so the rename is atomic.
pub fn deliver(path: &Path, staging: &Path, bytes: &[u8], mode: Mode) -> io::Result<()> {
    let dir = parent(path)?;
    let new_dirs = create_dirs(dir)?;
    // A private file's new directories are 0o700 too, whatever the umask said.
    if mode == Mode::Private {
        for new_dir in new_dirs {
            std::fs::set_permissions(new_dir, Permissions::from_mode(0o700))?;
        }
    }
    create_dirs(staging)?;
    let mut temp = tempfile::Builder::new()
        .prefix(".")
        .suffix(".writing")
        .tempfile_in(staging)?;
    // Set after creating, since a mode given at creation is filtered by the umask.
    temp.as_file()
        .set_permissions(Permissions::from_mode(mode.bits(path)?))?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|failed| failed.error)?;
    File::open(dir)?.sync_all()?;
    // The rename took the name out of the staging directory too.
    if staging != dir {
        File::open(staging)?.sync_all()?;
    }
    Ok(())
}

/// Make `path` absent, durably; answers whether a file was there. Removing what is not there
/// still syncs the directory, since an earlier removal may not have reached the disk.
pub fn remove(path: &Path) -> io::Result<bool> {
    let removed = std::fs::remove_file(path).present()?.is_some();
    File::open(parent(path)?)
        .present()?
        .map_or(Ok(()), |dir| dir.sync_all())?;
    Ok(removed)
}

/// An exclusive `flock` on a file, released when dropped.
#[derive(Debug)]
pub struct Held {
    _file: File,
}

/// Wait for, then take, the lock on `path`, creating it empty when absent and never truncating
/// it.
pub fn hold(path: &Path) -> io::Result<Held> {
    create_dirs(parent(path)?)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    file.lock()?;
    Ok(Held { _file: file })
}

/// `dir` and any missing ancestor, each synced into its parent; answers the directories made.
fn create_dirs(dir: &Path) -> io::Result<Vec<&Path>> {
    let mut missing = Vec::new();
    let mut at = dir;
    while !at.exists() {
        missing.push(at);
        match at.parent() {
            Some(up) if !up.as_os_str().is_empty() => at = up,
            _ => break,
        }
    }
    std::fs::create_dir_all(dir)?;
    for made in &missing {
        File::open(parent(made)?)?.sync_all()?;
    }
    Ok(missing)
}

fn parent(path: &Path) -> io::Result<&Path> {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => Ok(dir),
        Some(_) => Ok(Path::new(".")),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} names no file", path.display()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt as _;

    use super::*;

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("a directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    /// Removal leaves nothing, removing again succeeds, and a directory that was never made
    /// is already absent.
    #[test]
    fn a_removal_is_idempotent_and_absence_is_success() {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let path = dir.path().join("keys/k");
        replace(&path, b"key", Mode::Private).expect("written");
        remove(&path).expect("removed");
        assert!(entries(&dir.path().join("keys")).is_empty());
        remove(&path).expect("removing what is gone succeeds");
        remove(&dir.path().join("never/made")).expect("nothing to remove");
    }

    #[test]
    fn a_replacement_holds_the_new_bytes_and_leaves_nothing_beside_them() {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let path = dir.path().join("deep/er/state.json");
        replace(&path, b"first", Mode::Readable).expect("written");
        replace(&path, b"second", Mode::Readable).expect("rewritten");
        assert_eq!(std::fs::read(&path).expect("read"), b"second");
        assert_eq!(entries(&dir.path().join("deep/er")), ["state.json"]);
    }

    #[test]
    fn each_mode_writes_its_own_bits() {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let private = dir.path().join("grant.json");
        let readable = dir.path().join("instance.toml");
        replace(&private, b"{}", Mode::Private).expect("written");
        replace(&readable, b"", Mode::Readable).expect("written");
        let mode = |path: &Path| std::fs::metadata(path).expect("there").mode() & 0o777;
        assert_eq!(mode(&private), 0o600);
        assert_eq!(mode(&readable), 0o644);
        let kept = dir.path().join("log");
        replace(&kept, b"1", Mode::Kept).expect("written");
        assert_eq!(mode(&kept), 0o600, "a new one is private");
        std::fs::set_permissions(&kept, Permissions::from_mode(0o640)).expect("chmod");
        replace(&kept, b"2", Mode::Kept).expect("rewritten");
        assert_eq!(mode(&kept), 0o640, "a rewrite keeps the bits it found");
    }

    #[test]
    fn a_failed_replacement_leaves_the_target_and_no_temporary() {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let path = dir.path().join("kept");
        replace(&path, b"kept", Mode::Readable).expect("written");
        // A directory where the file goes: the rename is refused.
        let blocked = dir.path().join("blocked");
        std::fs::create_dir(&blocked).expect("made");
        std::fs::write(blocked.join("inside"), b"x").expect("made");
        assert!(replace(&blocked, b"new", Mode::Readable).is_err());
        assert_eq!(std::fs::read(&path).expect("read"), b"kept");
        assert_eq!(entries(dir.path()), ["blocked", "kept"]);
    }

    #[test]
    fn a_delivery_is_staged_apart_and_leaves_nothing_there() {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let (tmp, new) = (dir.path().join("tmp"), dir.path().join("new"));
        deliver(&new.join("1.eml"), &tmp, b"mail", Mode::Private).expect("delivered");
        assert_eq!(std::fs::read(new.join("1.eml")).expect("read"), b"mail");
        assert!(entries(&tmp).is_empty());
    }

    #[test]
    fn a_hold_excludes_a_second_until_dropped_and_never_truncates() {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let path = dir.path().join("sub/.lock");
        let first = hold(&path).expect("held");
        std::fs::write(&path, b"note").expect("written");
        let probe = File::open(&path).expect("opened");
        assert!(probe.try_lock().is_err(), "the lock is held");
        drop(first);
        let second = hold(&path).expect("held again");
        assert_eq!(std::fs::read(&path).expect("read"), b"note");
        drop(second);
        assert!(probe.try_lock().is_ok(), "released on drop");
    }
}
