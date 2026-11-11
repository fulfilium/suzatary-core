//! The Drive as the model may reach it without a tap: any port, behind one fence.

use super::{DriveEntry, DrivePath, Port, allowed};
use suzatary_base::drive_path::DriveFailure;

/// `port` as the secretary reaches it unasked: it downloads only inside `allowed` folders and
/// never lists or shows the owner's private folder ([`crate::vault::FOLDER`]). It is a read port
/// only, so writing through it does not typecheck.
/// Every refusal is decided here, before `port` is called, so no implementation can weaken it;
/// where the fence admits, it is `port` itself (`conformance::drive`'s laws).
#[derive(Debug, Clone)]
pub struct Fenced<P> {
    port: P,
    allowed: Vec<DrivePath>,
}

impl<P: Port> Fenced<P> {
    /// `port` behind the fence, downloading inside `allowed` alone.
    #[must_use]
    pub fn new(port: P, allowed: Vec<DrivePath>) -> Self {
        Self { port, allowed }
    }
}

impl<P: Port> Port for Fenced<P> {
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure> {
        if crate::vault::holds(folder) {
            return Err(DriveFailure::Refused(format!(
                "{folder} is the owner's private folder: it is never listed, and a file in it \
                 is read only on his tap"
            )));
        }
        let mut entries = self.port.list(folder)?;
        entries.retain(|entry| !crate::vault::holds(&entry.path));
        Ok(entries)
    }

    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        if allowed(&self.allowed, file) {
            self.port.download(file)
        } else {
            Err(DriveFailure::Refused(format!(
                "{file} is not in a folder the owner allowed, so it is read only on his tap"
            )))
        }
    }
}
