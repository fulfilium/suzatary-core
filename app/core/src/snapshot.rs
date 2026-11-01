//! A held replica's state as bytes, so a reader can be loaded again without being handed every
//! node and key it was handed before. A snapshot is a memo: the caller keys it by what it was
//! computed from and serves it only under that key; this module says nothing of where it is kept.
//!
//! The bytes are the replica's [`Held::FORM`] as four little-endian bytes, then what it was given
//! in postcard, the replica's own binary form. A snapshot of another form reads as
//! [`NotASnapshot::Form`], never as a misread replica, so a build that changes what a replica
//! holds moves its form and every older snapshot is passed over. Nothing here names what a
//! replica holds, so this module can join the Prodrome beside `memo`.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// A replica whose state can be printed and read back.
pub trait Held: Sized {
    /// What the replica was given, in the form it prints and reads back.
    type Given: Serialize + DeserializeOwned;
    /// Counts the forms of [`Held::Given`]: a change of it moves this.
    const FORM: u32;
    /// What the replica holds, as it was given.
    fn given(&self) -> Self::Given;
    /// The replica holding `given`, as if each part of it had been handed over.
    ///
    /// # Errors
    /// Why `given` is not a state the replica could hold.
    fn of_given(given: Self::Given) -> Result<Self, String>;
}

/// Why some bytes are not a snapshot of a replica.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotASnapshot {
    /// Shorter than the form's four bytes.
    #[error("a snapshot of {0} bytes holds no form")]
    Short(usize),
    /// A snapshot of another form of the replica.
    #[error("a snapshot of form {found}, and this build reads form {reads}")]
    Form { found: u32, reads: u32 },
    /// The form's bytes that do not read as what the replica was given.
    #[error("a snapshot that does not read: {0}")]
    Unread(String),
    /// What it holds read, and is not a state the replica could hold.
    #[error("a snapshot of a state the replica refuses: {0}")]
    Refused(String),
}

/// What a replica was given that postcard could not print: a `Serialize` that refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a snapshot that does not print: {0}")]
pub struct Unprinted(String);

/// `held` printed: its form, then what it was given.
///
/// # Errors
/// [`Unprinted`] where what it was given refuses to print.
pub fn snapshot<H: Held>(held: &H) -> Result<Vec<u8>, Unprinted> {
    postcard::to_extend(&held.given(), H::FORM.to_le_bytes().to_vec())
        .map_err(|e| Unprinted(e.to_string()))
}

/// The replica `bytes` print, when they print one of this build's form.
///
/// # Errors
/// [`NotASnapshot`]: too short, of another form, unread, or refused by the replica.
pub fn from_snapshot<H: Held>(bytes: &[u8]) -> Result<H, NotASnapshot> {
    let Some((form, rest)) = bytes.split_first_chunk::<4>() else {
        return Err(NotASnapshot::Short(bytes.len()));
    };
    let found = u32::from_le_bytes(*form);
    if found != H::FORM {
        return Err(NotASnapshot::Form {
            found,
            reads: H::FORM,
        });
    }
    let given = postcard::from_bytes(rest).map_err(|e| NotASnapshot::Unread(e.to_string()))?;
    H::of_given(given).map_err(NotASnapshot::Refused)
}
