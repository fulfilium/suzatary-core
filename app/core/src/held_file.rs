//! A file the box holds back (`crate::held_value`): one of the three sources a reader hands a
//! model a withheld file from, and never a kept copy or a repository's file, which no reader
//! withholds.

use prodrome_core::literal::codec::Literal as _;
use prodrome_core::literal::{Table, parse_literal, print_literal};

use crate::attachment::AttachmentId;
use crate::drive::DrivePath;
use crate::secretary::attachment_source::{AttachmentSource, FROM_DRIVE, FROM_MAIL, FROM_UPLOAD};

/// A held file, by where the box reads it again.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HeldFile {
    /// An attachment of a message, by the id the `attachments` verb took and its name.
    Mail {
        /// The message.
        message: String,
        /// The attachment's filename.
        name: String,
    },
    /// A file in the owner's Drive.
    Drive {
        /// Its full path.
        path: DrivePath,
    },
    /// A file the owner uploaded, kept on the box or privately in their Drive.
    Upload {
        /// Its name in the upload store.
        id: AttachmentId,
    },
}

/// A source no reader withholds a file from.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not a file the box holds back")]
pub struct NotHeld(pub String);

impl HeldFile {
    /// The source an attachment reads it from.
    #[must_use]
    pub fn source(&self) -> AttachmentSource {
        match self {
            Self::Mail { message, name } => AttachmentSource::Mail {
                message: message.clone(),
                name: name.clone(),
            },
            Self::Drive { path } => AttachmentSource::Drive { path: path.clone() },
            Self::Upload { id } => AttachmentSource::Upload { id: id.clone() },
        }
    }

    /// Its text form: its source's literal print, as a stored attachment's source prints.
    #[must_use]
    pub fn print(&self) -> String {
        print_literal(&self.source().to_value())
    }

    /// Read [`HeldFile::print`]'s text form.
    ///
    /// # Errors
    /// [`NotHeld`] for anything but the print of a mail, Drive or upload source.
    pub fn parse(text: &str) -> Result<Self, NotHeld> {
        let refused = || NotHeld(text.chars().take(64).collect());
        let value = parse_literal(text, &Table(&[FROM_MAIL, FROM_DRIVE, FROM_UPLOAD]))
            .map_err(|_| refused())?;
        AttachmentSource::from_value(&value)
            .map_err(|_| refused())
            .and_then(|source| Self::try_from(source).map_err(|_| refused()))
    }
}

impl TryFrom<AttachmentSource> for HeldFile {
    type Error = NotHeld;

    fn try_from(source: AttachmentSource) -> Result<Self, NotHeld> {
        match source {
            AttachmentSource::Mail { message, name } => Ok(Self::Mail { message, name }),
            AttachmentSource::Drive { path } => Ok(Self::Drive { path }),
            AttachmentSource::Upload { id } => Ok(Self::Upload { id }),
            source @ (AttachmentSource::Kept { .. } | AttachmentSource::Repo { .. }) => {
                Err(NotHeld(source.describe()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every spelling a stored handle carries, character for character: notes, index entries,
    /// turns and drafts hold them, so a change to any of them is a migration.
    #[test]
    fn each_held_file_prints_as_its_source_does() {
        let id = AttachmentId::of(b"w2");
        for (file, printed) in [
            (
                HeldFile::Mail {
                    message: "form-1@example.test".to_owned(),
                    name: "w2.pdf".to_owned(),
                },
                "FromMail(message='form-1@example.test', name='w2.pdf')".to_owned(),
            ),
            (
                HeldFile::Drive {
                    path: "/my-files/Docs/w2.pdf".parse().expect("a path"),
                },
                "FromDrive(path='/my-files/Docs/w2.pdf')".to_owned(),
            ),
            (
                HeldFile::Upload { id: id.clone() },
                format!("FromUpload(id='{id}')"),
            ),
        ] {
            assert_eq!(file.print(), printed);
            assert_eq!(HeldFile::parse(&printed), Ok(file));
        }
        for never in [
            format!("FromKept(id='{id}')"),
            format!(
                "FromRepo(repo='a/b', path='w2.pdf', at='{}')",
                "0".repeat(40)
            ),
            "FromMail(message='m')".to_owned(),
        ] {
            assert!(HeldFile::parse(&never).is_err(), "{never}");
        }
    }
}
