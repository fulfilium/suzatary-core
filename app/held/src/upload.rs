//! A file the owner uploaded, as the upload store answers it.

use crate::enclosed::Enclosure;
use crate::vault::PrivateCopy;

/// A file the owner uploaded, as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upload {
    /// On the box: its bytes, re-typed from themselves.
    Here {
        /// What it is.
        media: Enclosure,
        /// Its bytes.
        bytes: Vec<u8>,
    },
    /// In the owner's Drive and not on the box; reading it is a tap.
    Private {
        /// What it is.
        media: Enclosure,
        /// The name it was picked with.
        name: String,
        /// Where it is kept.
        copy: PrivateCopy,
    },
}
