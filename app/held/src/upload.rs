//! A file the owner uploaded, as the upload store answers it.

use crate::enclosed::Enclosure;
use crate::vault::PrivateCopy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upload {
    /// On the box: its bytes, re-typed from themselves.
    Here {
        /// What it is.
        media: Enclosure,
        bytes: Vec<u8>,
    },
    /// In the owner's Drive and not on the box; reading it is a tap.
    Private {
        media: Enclosure,
        name: String,
        copy: PrivateCopy,
    },
}
