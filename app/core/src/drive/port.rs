//! The Drive as two ports, a read and a write: the four operations an implementation provides,
//! and what the app derives from them.

use super::{DriveEntry, DrivePath, EntryKind, MAX_FILE};
use suzatary_base::drive_path::DriveFailure;

/// A Drive, read: folders of named files under sections (`/my-files`, …), as any implementation
/// holds them. `list` and `download` are what one must provide; `entry` and `fetch` are derived,
/// so a second implementation (another Drive, a `WebDAV`) proves itself by `conformance::drive`
/// and the app's uses follow. A record that holds a `Port` can read and cannot write: writing
/// is [`WritePort`], which no record holds.
pub trait Port: Sync {
    /// A folder's entries, each one segment under it.
    ///
    /// # Errors
    /// [`DriveFailure::Missing`] for no such folder, else the implementation's own.
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure>;

    /// One file's bytes.
    ///
    /// # Errors
    /// [`DriveFailure::Missing`] for no such file, else the implementation's own.
    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure>;

    /// `file`'s entry as its folder lists it: a file, at most [`MAX_FILE`] bytes.
    ///
    /// # Errors
    /// [`DriveFailure::Missing`] when its folder does not list it, [`DriveFailure::Refused`] for
    /// `/`, a folder, anything not a file, or a file over [`MAX_FILE`].
    fn entry(&self, file: &DrivePath) -> Result<DriveEntry, DriveFailure> {
        let Some(folder) = file.parent() else {
            return Err(DriveFailure::Refused(
                "/ is the list of sections, not a file".to_owned(),
            ));
        };
        let entry = self
            .list(&folder)?
            .into_iter()
            .find(|entry| &entry.path == file)
            .ok_or_else(|| DriveFailure::Missing(file.to_string()))?;
        match &entry.kind {
            EntryKind::File => {}
            EntryKind::Folder => {
                return Err(DriveFailure::Refused(format!(
                    "{file} is a folder; drive_list lists it"
                )));
            }
            EntryKind::Other(word) => {
                return Err(DriveFailure::Refused(format!(
                    "{file} is a {word}, not a file"
                )));
            }
        }
        if entry.size.is_some_and(|size| size > MAX_FILE) {
            return Err(DriveFailure::Refused(format!(
                "{file} is larger than {} MB, and is not read",
                MAX_FILE / 1024 / 1024
            )));
        }
        Ok(entry)
    }

    /// One file's bytes, once its folder lists it as a file it may read.
    ///
    /// # Errors
    /// As [`Port::entry`], then [`Port::download`].
    fn fetch(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        self.entry(file)?;
        self.download(file)
    }
}

/// A Drive, written: what keeps a private file. Held by the vault and by no record, so no verb
/// can reach a write through the record it is handed.
pub trait WritePort: Port {
    /// Make the folder `name` in `parent`.
    ///
    /// # Errors
    /// The implementation's own.
    fn create_folder(&self, parent: &DrivePath, name: &str) -> Result<(), DriveFailure>;

    /// Put `bytes` in `parent` as `name`; a file already there under that name is left as it is,
    /// since a kept file's name carries its content's digest.
    ///
    /// # Errors
    /// The implementation's own.
    fn upload(&self, parent: &DrivePath, name: &str, bytes: &[u8]) -> Result<(), DriveFailure>;

    /// Keep `bytes` in `folder` as `name`, making the folder and its parents first where the
    /// Drive has none; answers where.
    ///
    /// # Errors
    /// [`DriveFailure::Refused`] for a name that is not one path segment, else as the operations.
    fn keep(
        &self,
        folder: &DrivePath,
        name: &str,
        bytes: &[u8],
    ) -> Result<DrivePath, DriveFailure> {
        let path = folder
            .join(name)
            .map_err(|failed| DriveFailure::Refused(failed.to_string()))?;
        ensure(self, folder)?;
        self.upload(folder, name, bytes)?;
        Ok(path)
    }
}

/// `folder`, made where missing, from its section down.
fn ensure<P: WritePort + ?Sized>(port: &P, folder: &DrivePath) -> Result<(), DriveFailure> {
    let (Some(parent), Some(name)) = (folder.parent(), folder.name()) else {
        return Ok(());
    };
    ensure(port, &parent)?;
    if port
        .list(&parent)?
        .iter()
        .any(|entry| &entry.path == folder)
    {
        return Ok(());
    }
    port.create_folder(&parent, name)
}

impl<P: Port + ?Sized> Port for &P {
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure> {
        (**self).list(folder)
    }
    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        (**self).download(file)
    }
    fn entry(&self, file: &DrivePath) -> Result<DriveEntry, DriveFailure> {
        (**self).entry(file)
    }
    fn fetch(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        (**self).fetch(file)
    }
}

impl<P: WritePort + ?Sized> WritePort for &P {
    fn create_folder(&self, parent: &DrivePath, name: &str) -> Result<(), DriveFailure> {
        (**self).create_folder(parent, name)
    }
    fn upload(&self, parent: &DrivePath, name: &str, bytes: &[u8]) -> Result<(), DriveFailure> {
        (**self).upload(parent, name, bytes)
    }
    fn keep(
        &self,
        folder: &DrivePath,
        name: &str,
        bytes: &[u8],
    ) -> Result<DrivePath, DriveFailure> {
        (**self).keep(folder, name, bytes)
    }
}
