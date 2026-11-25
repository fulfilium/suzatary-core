//! What a handle of a store opened as its program's alone watches for a change to `objects/` made
//! without the store's lock (the Prodrome's SPEC §3, law 85): the time `objects/` last gained or
//! lost a file, the mark of the store's last hold (`.writes`), and, where the store lies in a git
//! checkout, the checkout's [`Stamp`]. It names no schema.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use prodrome_store::store::Doors;

use suzatary_io_access::repository::{Checkout, Stamp};
use suzatary_os::present::Present as _;

/// A handle's watch, by its store's doors.
pub(crate) enum Outside {
    /// A store opened as shared lists `objects/` at every decision, so it watches nothing.
    Listed,
    Watched(Box<Watch>),
}

pub(crate) struct Watch {
    checkout: Option<Checkout>,
    /// The look after this handle's last hold; `None` until it holds, or after a look failed.
    seen: Mutex<Option<Look>>,
    /// Whether a look that failed was said.
    said: AtomicBool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Look {
    git: Option<Stamp>,
    objects: Option<SystemTime>,
    mark: Option<Vec<u8>>,
}

/// A hold under way: `seen` is held from the look before it to the look after it, so another
/// task's look cannot fall between this hold's write and its look.
pub(crate) enum Watching<'a> {
    Listed,
    Held {
        seen: MutexGuard<'a, Option<Look>>,
        root: &'a Path,
    },
}

impl Outside {
    pub(crate) fn of(doors: Doors) -> Self {
        match doors {
            Doors::Shared => Self::Listed,
            Doors::Alone => Self::Watched(Box::new(Watch {
                checkout: None,
                seen: Mutex::new(None),
                said: AtomicBool::new(false),
            })),
        }
    }

    /// Watching `checkout`'s stamp too, where it is a git checkout.
    pub(crate) fn in_checkout(self, checkout: Checkout) -> Self {
        match self {
            Self::Watched(mut watch) if checkout.is_repository() => {
                watch.checkout = Some(checkout);
                Self::Watched(watch)
            }
            unchanged @ (Self::Watched(_) | Self::Listed) => unchanged,
        }
    }

    /// The same watch with nothing seen, for a second handle of the store.
    pub(crate) fn again(&self, doors: Doors) -> Self {
        match self {
            Self::Listed => Self::of(doors),
            Self::Watched(watch) => match &watch.checkout {
                Some(checkout) => Self::of(doors).in_checkout(checkout.clone()),
                None => Self::of(doors),
            },
        }
    }

    /// Before a hold of the store at `root`: `record` (a hold that lists) runs where the directory's
    /// time or the stamp moved since this handle's last hold, or where the look fails. A mark that
    /// moved alone is another hold that changed no file, and records nothing. A door that lands
    /// after this look and before the look after the hold's write is not seen by this handle.
    ///
    /// # Errors
    /// What `record` answers.
    pub(crate) fn before<'a, E>(
        &'a self,
        root: &'a Path,
        record: impl FnOnce() -> Result<(), E>,
    ) -> Result<Watching<'a>, E> {
        let watch = match self {
            Self::Listed => return Ok(Watching::Listed),
            Self::Watched(watch) => watch,
        };
        let mut seen = watch.seen.lock().unwrap_or_else(PoisonError::into_inner);
        match (watch.look(root), seen.as_ref()) {
            (Ok(now), Some(was)) if now == *was => {}
            (Ok(now), Some(was)) if now.objects == was.objects && now.git == was.git => {
                *seen = Some(now);
            }
            (Ok(now), _) => {
                record()?;
                // A file come or gone during the listing (a door, or a temp the record swept)
                // leaves nothing seen, so the next hold records again.
                *seen = watch
                    .look(root)
                    .ok()
                    .filter(|after| after.objects == now.objects)
                    .map(|after| Look {
                        mark: after.mark,
                        ..now
                    });
            }
            (Err(failed), _) => {
                if !watch.said.swap(true, Ordering::Relaxed) {
                    tracing::warn!(
                        "{}: {failed}; until it reads, every hold of this store lists",
                        root.display()
                    );
                }
                record()?;
                *seen = None;
            }
        }
        Ok(Watching::Held { seen, root })
    }
}

impl Watching<'_> {
    /// After the handle's own write: the directory's time and the mark move, the stamp does not.
    pub(crate) fn wrote(self) {
        match self {
            Self::Listed => {}
            Self::Held { mut seen, root } => {
                let after = seen.take().and_then(|was| {
                    Some(Look {
                        objects: objects(root).ok()?,
                        mark: mark(root).ok()?,
                        ..was
                    })
                });
                *seen = after;
            }
        }
    }
}

impl Watch {
    fn look(&self, root: &Path) -> std::io::Result<Look> {
        Ok(Look {
            git: self.checkout.as_ref().map(Checkout::stamp).transpose()?,
            objects: objects(root)?,
            mark: mark(root)?,
        })
    }
}

/// When `objects/` last gained or lost a file; `None` for a store never begun.
fn objects(root: &Path) -> std::io::Result<Option<SystemTime>> {
    std::fs::metadata(root.join("objects"))
        .present()?
        .map(|meta| meta.modified())
        .transpose()
}

/// The mark of the store's last hold (`.writes`, SPEC §3), compared as bytes.
fn mark(root: &Path) -> std::io::Result<Option<Vec<u8>>> {
    std::fs::read(root.join(".writes")).present()
}

#[cfg(test)]
mod tests {
    /// The file system type of the mount holding `path`, from `/proc/self/mountinfo`.
    fn file_system(path: &std::path::Path) -> Option<String> {
        let path = std::fs::canonicalize(path).ok()?;
        let mounts = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
        mounts
            .lines()
            .filter_map(|line| {
                let (left, right) = line.split_once(" - ")?;
                let point = left.split_whitespace().nth(4)?;
                let kind = right.split_whitespace().next()?;
                path.starts_with(point)
                    .then(|| (point.len(), kind.to_owned()))
            })
            .max()
            .map(|(_, kind)| kind)
    }

    /// The running kernel's release as (major, minor).
    fn kernel() -> Option<(u32, u32)> {
        let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").ok()?;
        let mut parts = release.trim().split(['.', '-']);
        Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
    }

    /// FACT the watch rests on: a file that enters `objects/` after a `stat` of it moves its time,
    /// however soon. Linux gives a fine-grained time to a change after a `stat` from 6.13 on, for
    /// ext4, xfs, btrfs and tmpfs; on an older kernel the time is the clock's tick, and a door in
    /// the tick after the box's own write is found by `verify` at the push instead.
    #[test]
    fn a_change_after_a_stat_moves_the_directory_s_time() {
        let dir = suzatary_testkit::scratch("outside");
        let fine = matches!(
            file_system(dir.path()).as_deref(),
            Some("ext4" | "xfs" | "btrfs" | "tmpfs")
        ) && kernel().is_some_and(|release| release >= (6, 13));
        if !fine {
            eprintln!(
                "not asserted: {:?} on Linux {:?} keeps a coarse time",
                file_system(dir.path()),
                kernel()
            );
            return;
        }
        let objects = dir.path().join("objects");
        std::fs::create_dir(&objects).expect("a directory");
        for k in 0..1000 {
            let before = super::objects(dir.path()).expect("a time");
            std::fs::write(objects.join(format!("{k}.py")), b"x").expect("a file");
            let after = super::objects(dir.path()).expect("a time");
            assert_ne!(
                before, after,
                "file {k} left the directory's time as it was"
            );
        }
    }
}
