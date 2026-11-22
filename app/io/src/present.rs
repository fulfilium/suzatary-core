//! The one reading of a failed read: a path that is not there is absent, and nothing else is.

use std::io;

pub trait Present<T> {
    /// `Ok(None)` for a path that is not there; every other failure stays the error it was, so
    /// an unreadable file or directory is never read as an empty one.
    fn present(self) -> io::Result<Option<T>>;
}

impl<T> Present<T> for io::Result<T> {
    fn present(self) -> io::Result<Option<T>> {
        match self {
            Ok(found) => Ok(Some(found)),
            Err(absent) if absent.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(failed) => Err(failed),
        }
    }
}
