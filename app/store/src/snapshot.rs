//! A held replica's state as bytes; the caller keys a snapshot by what it was computed from.
//!
//! The bytes are the replica's [`Held::FORM`] as four little-endian bytes, then what it was given
//! in postcard. A snapshot of another form reads as [`NotASnapshot::Form`], never as a misread
//! replica.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// A replica whose state can be printed and read back.
pub trait Held: Sized {
    type Given: Serialize + DeserializeOwned;
    /// Move this on any change to [`Held::Given`].
    const FORM: u32;
    fn given(&self) -> Self::Given;
    /// The replica holding `given`, as if each part of it had been handed over.
    fn of_given(given: Self::Given) -> Result<Self, String>;
}

/// Why some bytes are not a snapshot of a replica.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotASnapshot {
    /// Shorter than the form's four bytes.
    #[error("a snapshot of {0} bytes holds no form")]
    Short(usize),
    #[error("a snapshot of form {found}, and this build reads form {reads}")]
    Form { found: u32, reads: u32 },
    #[error("a snapshot that does not read: {0}")]
    Unread(String),
    #[error("a snapshot of a state the replica refuses: {0}")]
    Refused(String),
}

/// A `Serialize` that refused to print in postcard.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a snapshot that does not print: {0}")]
pub struct Unprinted(String);

pub fn snapshot<H: Held>(held: &H) -> Result<Vec<u8>, Unprinted> {
    print(H::FORM, &held.given())
}

pub fn from_snapshot<H: Held>(bytes: &[u8]) -> Result<H, NotASnapshot> {
    H::of_given(read(H::FORM, bytes)?).map_err(NotASnapshot::Refused)
}

/// `value` printed under `form`: the form as four little-endian bytes, then `value` in postcard.
pub fn print<T: Serialize>(form: u32, value: &T) -> Result<Vec<u8>, Unprinted> {
    postcard::to_extend(value, form.to_le_bytes().to_vec()).map_err(|e| Unprinted(e.to_string()))
}

pub fn read<T: DeserializeOwned>(form: u32, bytes: &[u8]) -> Result<T, NotASnapshot> {
    let Some((found, rest)) = bytes.split_first_chunk::<4>() else {
        return Err(NotASnapshot::Short(bytes.len()));
    };
    let found = u32::from_le_bytes(*found);
    if found != form {
        return Err(NotASnapshot::Form { found, reads: form });
    }
    postcard::from_bytes(rest).map_err(|e| NotASnapshot::Unread(e.to_string()))
}

/// The frames `bytes` hold one after another, each after its length as four little-endian bytes.
pub fn frames(mut bytes: &[u8]) -> Result<Vec<&[u8]>, NotASnapshot> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let Some((length, rest)) = bytes.split_first_chunk::<4>() else {
            return Err(NotASnapshot::Short(bytes.len()));
        };
        let length = usize::try_from(u32::from_le_bytes(*length))
            .map_err(|e| NotASnapshot::Unread(e.to_string()))?;
        let Some((frame, rest)) = rest.split_at_checked(length) else {
            return Err(NotASnapshot::Short(rest.len()));
        };
        out.push(frame);
        bytes = rest;
    }
    Ok(out)
}
