//! The Drive's laws ([`crate::drive::Port`]), and the fence's over any implementation.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Broken, hold};
use crate::capability::DriveFailure;
use crate::drive::{DriveEntry, EntryKind, Fenced, Port, WritePort};
use crate::drive_path::DrivePath;

/// A law of a Drive, in the order the suite asks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Law {
    /// What is kept downloads as the bytes kept, and fetches the same.
    RoundTrip,
    /// A kept file is listed in its folder, as a file, at its size when a size is said.
    Listed,
    /// Keeping a name already there leaves the first bytes: a name is its content's.
    NeverReplaces,
    /// A folder made is listed in its parent as a folder, and lists nothing.
    Folder,
    /// A file nobody kept does not download.
    Missing,
    /// Outside the allowed folders, a download is refused before the implementation is called.
    FencedReads,
    /// The private folder is never listed, and nothing under it shows in a listing.
    FencedPrivate,
    /// Where the fence admits, it is the implementation: the same listing, the same bytes.
    Transparent,
}

impl fmt::Display for Law {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Law::RoundTrip => "what is kept downloads as kept",
            Law::Listed => "a kept file is listed at its size",
            Law::NeverReplaces => "keeping a name twice keeps the first",
            Law::Folder => "a folder made is listed",
            Law::Missing => "a file nobody kept does not download",
            Law::FencedReads => "a download outside the allowed folders never reaches the Drive",
            Law::FencedPrivate => "the private folder is never listed",
            Law::Transparent => "where the fence admits, it is the Drive",
        })
    }
}

/// One case: a folder that does not yet exist, which the suite fills, and a file to keep there.
#[derive(Debug, Clone)]
pub struct Case {
    /// A fresh folder under a section, e.g. `/my-files/case-1`.
    pub folder: DrivePath,
    /// A name, one segment.
    pub name: String,
    /// Its bytes.
    pub bytes: Vec<u8>,
}

/// Every law over every case, on one Drive: the Drive's own, then the fence's over it.
///
/// # Errors
/// The first [`Broken`] law.
pub fn check<P: WritePort>(port: &P, cases: &[Case]) -> Result<(), Broken<Law>> {
    for case in cases {
        laws(port, case)?;
        fence(port, case)?;
    }
    Ok(())
}

/// The Drive's own laws over one case.
///
/// # Errors
/// The first [`Broken`] law.
pub fn laws<P: WritePort>(port: &P, case: &Case) -> Result<(), Broken<Law>> {
    let Case {
        folder,
        name,
        bytes,
    } = case;
    let file = port
        .keep(folder, name, bytes)
        .map_err(|failed| Broken::of(Law::RoundTrip, format!("keeping {name}: {failed}")))?;
    let got = port.download(&file);
    hold(Law::RoundTrip, got.as_ref() == Ok(bytes), || {
        format!("{file} downloaded as {}", shown(&got))
    })?;
    let fetched = port.fetch(&file);
    hold(Law::RoundTrip, fetched.as_ref() == Ok(bytes), || {
        format!("{file} fetched as {}", shown(&fetched))
    })?;

    let listed = listing(port, folder, Law::Listed)?;
    let size = u64::try_from(bytes.len()).ok();
    hold(
        Law::Listed,
        listed.iter().any(|entry| {
            entry.path == file
                && entry.kind == EntryKind::File
                && entry.size.is_none_or(|said| Some(said) == size)
        }),
        || {
            format!(
                "{folder} lists {listed:?}, not {file} at {} bytes",
                bytes.len()
            )
        },
    )?;

    let mut other = bytes.clone();
    other.push(1);
    port.keep(folder, name, &other)
        .map_err(|failed| Broken::of(Law::NeverReplaces, failed.to_string()))?;
    let again = port.download(&file);
    hold(Law::NeverReplaces, again.as_ref() == Ok(bytes), || {
        format!("{file} downloads as {} after a second keep", shown(&again))
    })?;

    let sub = joined(folder, "sub", Law::Folder)?;
    port.create_folder(folder, "sub")
        .map_err(|failed| Broken::of(Law::Folder, failed.to_string()))?;
    let listed = listing(port, folder, Law::Folder)?;
    hold(
        Law::Folder,
        listed
            .iter()
            .any(|entry| entry.path == sub && entry.kind == EntryKind::Folder),
        || format!("{folder} lists {listed:?}, without the folder {sub}"),
    )?;
    let inside = listing(port, &sub, Law::Folder)?;
    hold(Law::Folder, inside.is_empty(), || {
        format!("a folder just made lists {inside:?}")
    })?;

    let absent = joined(folder, &format!("absent-{name}"), Law::Missing)?;
    hold(Law::Missing, port.download(&absent).is_err(), || {
        format!("{absent} downloaded, and nobody kept it")
    })?;
    hold(
        Law::Missing,
        matches!(port.entry(&absent), Err(DriveFailure::Missing(_))),
        || format!("{absent} has an entry, and nobody kept it"),
    )
}

/// The fence's laws over one case, on `port` watched by a `Spy`.
///
/// # Errors
/// The first [`Broken`] law.
pub fn fence<P: WritePort>(port: &P, case: &Case) -> Result<(), Broken<Law>> {
    let Case {
        folder,
        name,
        bytes,
    } = case;
    let file = port
        .keep(folder, name, bytes)
        .map_err(|failed| Broken::of(Law::Transparent, failed.to_string()))?;
    let private = crate::vault::folder();
    port.keep(&private, name, bytes)
        .map_err(|failed| Broken::of(Law::FencedPrivate, failed.to_string()))?;
    let spy = Spy::new(port);

    let elsewhere = joined(folder, "elsewhere", Law::FencedReads)?;
    for allowed in [vec![], vec![elsewhere], vec![private.clone()]] {
        let fenced = Fenced::new(&spy, allowed);
        hold(Law::FencedReads, fenced.download(&file).is_err(), || {
            format!("{file} downloaded outside the allowed folders")
        })?;
        hold(Law::FencedReads, spy.calls() == 0, || {
            format!("a refused download of {file} reached the Drive")
        })?;
    }

    // Allowing the private folder, or the section above it, opens nothing in it.
    let section = private
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or_else(DrivePath::root);
    let fenced = Fenced::new(&spy, vec![private.clone(), section]);
    let kept = joined(&private, name, Law::FencedPrivate)?;
    hold(
        Law::FencedPrivate,
        fenced.list(&private).is_err() && fenced.download(&kept).is_err(),
        || format!("{private} was listed or read behind the fence"),
    )?;
    hold(Law::FencedPrivate, spy.calls() == 0, || {
        format!("a refused read of {private} reached the Drive")
    })?;
    let mut ancestor = private.parent();
    while let Some(folder) = ancestor {
        let shown = fenced
            .list(&folder)
            .map_err(|failed| Broken::of(Law::FencedPrivate, failed.to_string()))?;
        hold(
            Law::FencedPrivate,
            !shown.iter().any(|entry| entry.path.is_under(&private)),
            || format!("{folder} shows the private folder: {shown:?}"),
        )?;
        ancestor = folder.parent();
    }

    // Nothing is written behind the fence because `Fenced` is no `WritePort`: a fact of its type.
    let fenced = Fenced::new(port, vec![folder.clone()]);
    let through = fenced.download(&file);
    hold(Law::Transparent, through == port.download(&file), || {
        format!("{file} downloads as {} behind the fence", shown(&through))
    })?;
    let listed = fenced.list(folder);
    hold(Law::Transparent, listed == port.list(folder), || {
        format!("{folder} lists as {listed:?} behind the fence")
    })
}

/// A Drive, read, that counts what reaches it, so a law can say a refusal never did.
#[derive(Debug)]
pub(crate) struct Spy<P> {
    port: P,
    calls: AtomicUsize,
}

impl<P: Port> Spy<P> {
    /// `port`, watched.
    pub fn new(port: P) -> Self {
        Self {
            port,
            calls: AtomicUsize::new(0),
        }
    }

    /// How many calls reached it.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl<P: Port> Port for Spy<P> {
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.port.list(folder)
    }
    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.port.download(file)
    }
}

fn listing<P: Port>(
    port: &P,
    folder: &DrivePath,
    law: Law,
) -> Result<Vec<DriveEntry>, Broken<Law>> {
    port.list(folder)
        .map_err(|failed| Broken::of(law, format!("listing {folder}: {failed}")))
}

fn joined(folder: &DrivePath, name: &str, law: Law) -> Result<DrivePath, Broken<Law>> {
    folder
        .join(name)
        .map_err(|failed| Broken::of(law, failed.to_string()))
}

fn shown(got: &Result<Vec<u8>, DriveFailure>) -> String {
    match got {
        Ok(bytes) => format!("{} bytes", bytes.len()),
        Err(failed) => failed.to_string(),
    }
}
