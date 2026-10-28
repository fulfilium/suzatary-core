//! The Drive's check: each folder the secretary may read lists, observed into a register as a
//! mail account's syncs are.

use super::{DrivePath, Port};
use crate::capability::DriveFailure;
use crate::connect::Failure;
use crate::health::{Healths, Instant, Outcome};
use crate::source_key::SourceKey;

/// What listing each of `folders` through `port` at `at` says: a success when every one lists,
/// else a failure naming the first that did not, in the Drive's words. No folder to read is
/// nothing observed, since there is nothing to check.
#[must_use]
pub fn probe(port: &dyn Port, folders: &[DrivePath], at: Instant) -> Healths<SourceKey> {
    if folders.is_empty() {
        return Healths::default();
    }
    match folders
        .iter()
        .find_map(|folder| port.list(folder).err().map(|failed| (folder, failed)))
    {
        None => Healths::one(SourceKey::Drive, at, Outcome::Succeeded, ""),
        Some((folder, failed)) => Healths::one(
            SourceKey::Drive,
            at,
            Outcome::Failed(class(&failed)),
            &words(folder, &failed),
        ),
    }
}

/// A failed listing's class: a CLI that asks for its sign-in needs it again, which a terminal
/// gives; anything else nothing but the next probe mends.
fn class(failed: &DriveFailure) -> Failure {
    match failed {
        DriveFailure::SignedOut(_) => Failure::NeedsLogin,
        DriveFailure::Missing(_) | DriveFailure::Refused(_) | DriveFailure::Unreachable(_) => {
            Failure::Unclassified
        }
    }
}

/// A failed listing in the owner's words, by its kind alone: the CLI's own words name paths on
/// the box, which a line never shows.
fn words(folder: &DrivePath, failed: &DriveFailure) -> String {
    match failed {
        DriveFailure::Missing(_) => format!("{folder} is not in the Drive"),
        DriveFailure::Refused(_) => format!("{folder} could not be listed"),
        DriveFailure::Unreachable(_) => "the Drive did not answer".to_owned(),
        DriveFailure::SignedOut(_) => "the Drive's sign-in has expired".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> DrivePath {
        text.parse().expect("a path")
    }

    /// The CLI's own words, which name the box's paths, never reach the line.
    #[test]
    fn a_failure_is_worded_by_its_kind_alone() {
        let folder = path("/my-files/Docs");
        let said = "the Drive CLI at /opt/drive/bin/proton-drive could not be run";
        for failed in [
            DriveFailure::Missing(said.to_owned()),
            DriveFailure::Refused(said.to_owned()),
            DriveFailure::Unreachable(said.to_owned()),
            DriveFailure::SignedOut(said.to_owned()),
        ] {
            let worded = words(&folder, &failed);
            assert!(!worded.contains("/opt"), "{worded}");
        }
    }
}
