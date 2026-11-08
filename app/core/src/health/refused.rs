//! Why a connection was not made, read or removed.

use crate::health::plugin::ManifestError;

/// Each refusal names what was wrong; none holds a secret, since no connection does.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    /// A template this box has no data for.
    #[error("{0:?} is not a template this box knows")]
    Template(String),
    /// A parameter its own constructor refused.
    #[error("{param}: {said}")]
    Param {
        /// Which parameter.
        param: &'static str,
        /// The constructor's words.
        said: String,
    },
    /// Parameters of another shape than the template reads.
    #[error("{template} takes {wanted}")]
    Params {
        /// The template.
        template: &'static str,
        /// What it reads, in words.
        wanted: &'static str,
    },
    /// A connection under this key is already declared, named as the owner calls it.
    #[error("{0} is already connected")]
    Duplicate(String),
    /// No connection under this key is declared, named as the owner calls it.
    #[error("{0} is not connected")]
    Absent(String),
    /// A template whose connection no table of the instance holds, so there is nothing to
    /// write or remove.
    #[error("{template} is not written to the instance: {why}")]
    Unwritten {
        /// The template.
        template: &'static str,
        /// What makes it connected instead.
        why: &'static str,
    },
    /// A grant for a plugin the instance does not declare.
    #[error("no plugin named {0} is declared")]
    NoPlugin(String),
    /// The manifest the edit would leave, refused as the file's table would be.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// The account roster the edit would leave, refused as the file's would be.
    #[error("{0}")]
    Roster(String),
}
