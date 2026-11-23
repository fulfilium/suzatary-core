//! The crate's one way to replace a file's bytes and one way to take a cross-process lock.
//!
//! A replacement is crash-consistent (temporary beside the target, sync, rename, sync the
//! directory): a reader or a rebooted box sees the old bytes or the new, never a partial file.

use crate::present::Present as _;
use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Write as _};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
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

/// Make `path` hold exactly `bytes`, durably, creating its directory: a failure before the rename
/// leaves the target as it was and no temporary, and a failed directory sync after it leaves the
/// new bytes in place, not yet known to survive a power cut.
pub fn replace(path: &Path, bytes: &[u8], mode: Mode) -> io::Result<()> {
    commit(&[(path, bytes)], Staging::Beside, mode).map_err(|failed| failed.error)
}

/// Make each path hold its bytes as one group commit: a failure before the renames leaves every
/// target as it was, and one after leaves the files before it renamed into place, not yet known to
/// survive a power cut.
pub fn replace_all(files: &[(&Path, &[u8])], mode: Mode) -> Result<(), Failed> {
    commit(files, Staging::Beside, mode)
}

/// [`replace`], staged in `staging` (a maildir's `tmp/`) rather than beside the target.
/// `staging` must be on the target's filesystem, so the rename is atomic.
pub fn deliver(path: &Path, staging: &Path, bytes: &[u8], mode: Mode) -> io::Result<()> {
    commit(&[(path, bytes)], Staging::In(staging), mode).map_err(|failed| failed.error)
}

#[derive(Debug, thiserror::Error)]
#[error("{}: {error}", path.display())]
pub struct Failed {
    pub path: std::path::PathBuf,
    pub error: io::Error,
}

#[derive(Clone, Copy)]
enum Staging<'a> {
    /// Beside each target.
    Beside,
    /// In one directory on the targets' filesystem.
    In(&'a Path),
}

/// Every directory is made and opened before any byte is staged, the bytes are synced (each file,
/// or each filesystem through a directory's handle), and only then is each file renamed and each
/// directory synced: a rename can reach the disk before the bytes it names.
///
/// It needs Linux 5.8 or later, where a sync of the filesystem through a handle reports every
/// write-back error since that handle was opened, so staging after the opens makes it as strict as
/// an fsync of each file.
fn commit<'p>(files: &[(&'p Path, &[u8])], staging: Staging<'p>, mode: Mode) -> Result<(), Failed> {
    let at = |path: &Path| {
        let path = path.to_path_buf();
        move |error| Failed { path, error }
    };
    let into = |path: &'p Path| match staging {
        Staging::Beside => parent(path),
        Staging::In(dir) => Ok(dir),
    };
    let mut dirs = std::collections::BTreeMap::new();
    for (path, _) in files {
        for dir in [
            parent(path).map_err(at(path))?,
            into(path).map_err(at(path))?,
        ] {
            if !dirs.contains_key(dir) {
                made(dir, mode).map_err(at(dir))?;
                dirs.insert(dir, File::open(dir).map_err(at(dir))?);
            }
        }
    }
    let mut staged = Vec::with_capacity(files.len());
    for (path, bytes) in files {
        let dir = into(path).map_err(at(path))?;
        staged.push((stage(path, dir, bytes, mode).map_err(at(path))?, *path));
    }
    if let [(one, path)] = staged.as_slice() {
        sync_file(one.as_file()).map_err(at(path))?;
    } else {
        let mut synced_devices = std::collections::BTreeSet::new();
        for (dir, handle) in &dirs {
            let device = handle.metadata().map_err(at(dir))?.dev();
            if synced_devices.insert(device) {
                sync_filesystem(handle).map_err(at(dir))?;
            }
        }
    }
    for (temp, path) in staged {
        temp.persist(path)
            .map_err(|failed| at(path)(failed.error))?;
    }
    for (dir, handle) in &dirs {
        synced(handle, &DIR_SYNCS).map_err(at(dir))?;
    }
    Ok(())
}

/// `dir` and any missing ancestor made; a private file's new directories are `0o700`, whatever
/// the umask said.
fn made(dir: &Path, mode: Mode) -> io::Result<()> {
    let new_dirs = create_dirs(dir)?;
    if mode == Mode::Private {
        for new_dir in new_dirs {
            std::fs::set_permissions(new_dir, Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

/// `bytes` written to a temporary in `staging`, not synced.
fn stage(
    path: &Path,
    staging: &Path,
    bytes: &[u8],
    mode: Mode,
) -> io::Result<tempfile::NamedTempFile> {
    let mut temp = tempfile::Builder::new()
        .prefix(".")
        .suffix(STAGED)
        .tempfile_in(staging)?;
    // Set after creating, since a mode given at creation is filtered by the umask.
    temp.as_file()
        .set_permissions(Permissions::from_mode(mode.bits(path)?))?;
    temp.write_all(bytes)?;
    Ok(temp)
}

/// The ending of a staged file's name: what a commit stopped before its renames leaves.
const STAGED: &str = ".writing";

/// Remove every staged file a stopped commit left in `dir`; answers how many. Only a writer that
/// excludes every other writer of `dir` may call it, since it cannot tell a stopped commit's file
/// from a running one's.
pub fn remove_staged(dir: &Path) -> io::Result<usize> {
    let Some(entries) = std::fs::read_dir(dir).present()? else {
        return Ok(0);
    };
    let mut staged = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name.ends_with(STAGED) {
            staged.push(entry.path());
        }
    }
    remove_all(
        &staged
            .iter()
            .map(std::path::PathBuf::as_path)
            .collect::<Vec<_>>(),
    )
}

/// Make `path` absent, durably; answers whether a file was there. Removing what is not there
/// still syncs the directory, since an earlier removal may not have reached the disk.
pub fn remove(path: &Path) -> io::Result<bool> {
    remove_all(&[path]).map(|removed| removed == 1)
}

/// Make every path absent, each directory synced once after every removal from it; answers how
/// many files were there, and a failure leaves the removals before it not yet durable.
pub fn remove_all(paths: &[&Path]) -> io::Result<usize> {
    let mut dirs = std::collections::BTreeSet::new();
    let mut removed = 0;
    for path in paths {
        removed += usize::from(std::fs::remove_file(path).present()?.is_some());
        dirs.insert(parent(path)?);
    }
    for dir in dirs {
        File::open(dir)
            .present()?
            .map_or(Ok(()), |dir| synced(&dir, &DIR_SYNCS))?;
    }
    Ok(removed)
}

/// An exclusive `flock` on a file, released when dropped.
#[derive(Debug)]
pub struct Held {
    _file: File,
}

impl suzatary_store::store::WriteLock for Held {}

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
        sync_dir(parent(made)?)?;
    }
    Ok(missing)
}

thread_local! {
    static FILE_SYNCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static DIR_SYNCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static FILESYSTEM_SYNCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many syncs this thread has asked of the disk through this module: what a measurement
/// counts, since a sync's cost is the disk's and not the clock's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Syncs {
    /// Of a file's bytes.
    pub files: usize,
    /// Of a directory's entries: what makes a rename or a removal survive a power cut.
    pub dirs: usize,
    /// Of a whole filesystem: what makes a batch's bytes survive a power cut at once.
    pub filesystems: usize,
}

#[must_use]
pub fn syncs() -> Syncs {
    Syncs {
        files: FILE_SYNCS.with(std::cell::Cell::get),
        dirs: DIR_SYNCS.with(std::cell::Cell::get),
        filesystems: FILESYSTEM_SYNCS.with(std::cell::Cell::get),
    }
}

fn synced(
    file: &File,
    count: &'static std::thread::LocalKey<std::cell::Cell<usize>>,
) -> io::Result<()> {
    count.with(|count| count.set(count.get() + 1));
    file.sync_all()
}

fn sync_file(file: &File) -> io::Result<()> {
    synced(file, &FILE_SYNCS)
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    synced(&File::open(dir)?, &DIR_SYNCS)
}

fn sync_filesystem(handle: &File) -> io::Result<()> {
    FILESYSTEM_SYNCS.with(|count| count.set(count.get() + 1));
    rustix::fs::syncfs(handle).map_err(io::Error::from)
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
    use std::path::PathBuf;

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

    #[test]
    fn a_removal_is_idempotent_and_absence_is_success() {
        let dir = suzatary_testkit::scratch("durable");
        let path = dir.path().join("keys/k");
        replace(&path, b"key", Mode::Private).expect("written");
        remove(&path).expect("removed");
        assert!(entries(&dir.path().join("keys")).is_empty());
        remove(&path).expect("removing what is gone succeeds");
        remove(&dir.path().join("never/made")).expect("nothing to remove");
    }

    /// A group commit over two directories of one filesystem: one sync of the filesystem for the
    /// batch's bytes and one sync per directory for the writes, and one per directory for the
    /// removals.
    #[test]
    fn a_batch_syncs_each_filesystem_once_and_each_directory_once() {
        let dir = suzatary_testkit::scratch("durable");
        let paths: Vec<_> = (0..6)
            .map(|n| {
                let shard = if n % 2 == 0 { "keys" } else { "more" };
                dir.path().join(shard).join(n.to_string())
            })
            .collect();
        replace(&paths[0], b"made", Mode::Private).expect("one directory made");
        replace(&paths[1], b"made", Mode::Private).expect("the other made");
        let counted = |run: &dyn Fn()| {
            let before = syncs();
            run();
            let after = syncs();
            (
                after.files - before.files,
                after.filesystems - before.filesystems,
                after.dirs - before.dirs,
            )
        };
        let files: Vec<(&Path, &[u8])> = paths
            .iter()
            .map(|path| (path.as_path(), &b"key"[..]))
            .collect();
        assert_eq!(
            counted(&|| replace_all(&files, Mode::Private).expect("written")),
            (0, 1, 2)
        );
        assert!(
            paths
                .iter()
                .all(|path| std::fs::read(path).expect("there") == b"key")
        );
        let named: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        assert_eq!(
            counted(&|| assert_eq!(remove_all(&named).expect("removed"), 6)),
            (0, 0, 2)
        );
        assert!(entries(&dir.path().join("keys")).is_empty());
        assert!(entries(&dir.path().join("more")).is_empty());
    }

    #[test]
    fn a_replacement_holds_the_new_bytes_and_leaves_nothing_beside_them() {
        let dir = suzatary_testkit::scratch("durable");
        let path = dir.path().join("deep/er/state.json");
        replace(&path, b"first", Mode::Readable).expect("written");
        replace(&path, b"second", Mode::Readable).expect("rewritten");
        assert_eq!(std::fs::read(&path).expect("read"), b"second");
        assert_eq!(entries(&dir.path().join("deep/er")), ["state.json"]);
    }

    #[test]
    fn each_mode_writes_its_own_bits() {
        let dir = suzatary_testkit::scratch("durable");
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
        let dir = suzatary_testkit::scratch("durable");
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
        let dir = suzatary_testkit::scratch("durable");
        let (tmp, new) = (dir.path().join("tmp"), dir.path().join("new"));
        deliver(&new.join("1.eml"), &tmp, b"mail", Mode::Private).expect("delivered");
        assert_eq!(std::fs::read(new.join("1.eml")).expect("read"), b"mail");
        assert!(entries(&tmp).is_empty());
    }

    #[test]
    fn a_hold_excludes_a_second_until_dropped_and_never_truncates() {
        let dir = suzatary_testkit::scratch("durable");
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
