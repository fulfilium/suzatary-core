//! A file attached to a person's turn, a photo or a PDF, named by its SHA-256.
//!
//! Types are recognised by their own magic bytes, never a browser's label, so no
//! stored file can render as a page.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::enclosed::Enclosure;

/// The largest image accepted: the model API's own per-image ceiling. A PDF's is
/// [`crate::file_read::MAX_PDF`].
pub const MAX_BYTES: usize = 5 * 1024 * 1024;

/// A file's name in the store: the SHA-256 of its bytes, as 64 lowercase hex digits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AttachmentId(String);

impl AttachmentId {
    /// The id of these bytes.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(hex::encode(Sha256::digest(bytes)))
    }

    /// The hex digits, which are also the file name in the store.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AttachmentId {
    type Err = AttachmentError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.len() == 64
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(text.to_owned()))
        } else {
            Err(AttachmentError::NotAnId)
        }
    }
}

impl TryFrom<String> for AttachmentId {
    type Error = AttachmentError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<AttachmentId> for String {
    fn from(id: AttachmentId) -> Self {
        id.0
    }
}

impl fmt::Display for AttachmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The image types the model accepts, and no others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    /// `image/jpeg`, what the browser re-encodes every photo to.
    #[serde(rename = "image/jpeg")]
    Jpeg,
    /// `image/png`.
    #[serde(rename = "image/png")]
    Png,
    /// `image/webp`.
    #[serde(rename = "image/webp")]
    Webp,
    /// `image/gif`.
    #[serde(rename = "image/gif")]
    Gif,
}

impl MediaType {
    /// What these bytes are, by their own magic numbers; `None` for anything
    /// else, whatever a sender called it.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0xFF, 0xD8, 0xFF, ..] => Some(Self::Jpeg),
            [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some(Self::Png),
            _ if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => {
                Some(Self::Webp)
            }
            [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some(Self::Gif),
            _ => None,
        }
    }

    /// The MIME spelling, as served and as the model's API names it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
        }
    }
}

/// A file on a turn: which one, what it is, and what the person called it. Storing one
/// answers it unnamed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Its name in the store.
    pub id: AttachmentId,
    /// Its type, as sniffed when it was stored.
    pub media_type: Enclosure,
    /// The file's name as it was picked; a photo has none, since the phone redraws it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where the file is kept when it is private: in the owner's Drive and not on the box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private: Option<crate::vault::PrivateCopy>,
}

/// The longest file name a turn keeps, in characters.
pub const MAX_NAME: usize = 200;

impl Attachment {
    /// Accept these bytes as a file: a PDF within [`crate::file_read::MAX_PDF`] or an image
    /// within [`MAX_BYTES`], named by their hash.
    ///
    /// # Errors
    /// Empty, too large, or neither a PDF nor one of the four image types.
    pub fn accept(bytes: &[u8]) -> Result<Self, AttachmentError> {
        if bytes.is_empty() {
            return Err(AttachmentError::Empty);
        }
        let media_type = Enclosure::sniff(bytes).ok_or(AttachmentError::Unreadable)?;
        let limit = match media_type {
            Enclosure::Pdf => crate::file_read::MAX_PDF,
            Enclosure::Image(_) => MAX_BYTES,
        };
        if bytes.len() > limit {
            return Err(AttachmentError::TooLarge {
                size: bytes.len(),
                limit,
            });
        }
        Ok(Self {
            id: AttachmentId::of(bytes),
            media_type,
            name: None,
            private: None,
        })
    }

    /// This file under the name it was picked with: trimmed, blank meaning none.
    ///
    /// # Errors
    /// [`AttachmentError::BadName`] over [`MAX_NAME`] characters or holding a control
    /// character.
    pub fn named(self, name: &str) -> Result<Self, AttachmentError> {
        let name = name.trim();
        if name.chars().count() > MAX_NAME || name.chars().any(char::is_control) {
            return Err(AttachmentError::BadName);
        }
        Ok(Self {
            name: (!name.is_empty()).then(|| name.to_owned()),
            ..self
        })
    }

    /// Its name, as [`called`] gives it.
    #[must_use]
    pub fn called(&self) -> String {
        called(&self.id, self.media_type, self.name.as_deref())
    }
}

/// An upload's name: the one it was picked with, or one its type and id give it,
/// `file-<id>.pdf` or `photo-<id>.png`, so two unnamed uploads are two names.
#[must_use]
pub fn called(id: &AttachmentId, media: Enclosure, name: Option<&str>) -> String {
    match name.filter(|name| !name.is_empty()) {
        Some(name) => name.to_owned(),
        None => match media {
            Enclosure::Pdf => format!("file-{}.pdf", &id.as_str()[..12]),
            Enclosure::Image(image) => format!(
                "photo-{}.{}",
                &id.as_str()[..12],
                image.as_str().trim_start_matches("image/")
            ),
        },
    }
}

/// Why bytes or a name are not an attachment.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AttachmentError {
    /// Nothing was sent.
    #[error("the file is empty")]
    Empty,
    /// Over its type's limit.
    #[error("the file is {size} bytes; the limit for its type is {limit}")]
    TooLarge {
        /// Its size.
        size: usize,
        /// Its type's limit.
        limit: usize,
    },
    /// Neither a PDF nor a JPEG, PNG, WebP or GIF by its own bytes.
    #[error("not a PDF, or a JPEG, PNG, WebP or GIF image")]
    Unreadable,
    /// Not 64 lowercase hex digits.
    #[error("not an attachment id")]
    NotAnId,
    /// Too long, or holding a control character.
    #[error("a file name is at most {MAX_NAME} characters with no control characters")]
    BadName,
}

// The literal of each is its text, as an inbox act stores it.
prodrome_core::text_literal!(
    AttachmentId => |id| String::from(id.clone()),
);

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];

    #[test]
    fn a_photo_is_named_by_its_hash_and_typed_by_its_own_bytes() {
        let accepted = Attachment::accept(PNG).expect("a png");
        assert_eq!(accepted.media_type, Enclosure::Image(MediaType::Png));
        assert_eq!(accepted.id, AttachmentId::of(PNG));
        assert_eq!(accepted.id.as_str().len(), 64);
        assert_eq!(
            MediaType::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(MediaType::Jpeg)
        );
        assert_eq!(
            MediaType::sniff(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(MediaType::Webp)
        );
        assert_eq!(MediaType::sniff(b"GIF89a"), Some(MediaType::Gif));
    }

    #[test]
    fn anything_else_is_refused_whatever_it_is_called() {
        assert_eq!(Attachment::accept(b""), Err(AttachmentError::Empty));
        assert_eq!(
            Attachment::accept(b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            Err(AttachmentError::Unreadable)
        );
        assert_eq!(
            Attachment::accept(b"<html>"),
            Err(AttachmentError::Unreadable)
        );
        let big = [PNG, &vec![0; MAX_BYTES][..]].concat();
        assert_eq!(
            Attachment::accept(&big),
            Err(AttachmentError::TooLarge {
                size: big.len(),
                limit: MAX_BYTES
            })
        );
    }

    #[test]
    fn a_turn_without_photos_reads_and_writes_as_it_always_did() {
        use crate::capability::Turn;
        let old = r#"{"who":"person","text":"hi","references":[]}"#;
        let turn: Turn = serde_json::from_str(old).expect("a transcript from before photos");
        assert!(matches!(&turn, Turn::Person { attachments, .. } if attachments.is_empty()));
        assert_eq!(serde_json::to_string(&turn).expect("serialises"), old);

        let photo = Attachment::accept(PNG).expect("a png");
        let with = Turn::Person {
            text: "look".to_owned(),
            references: Vec::new(),
            attachments: vec![photo.clone()],
        };
        let line = serde_json::to_string(&with).expect("serialises");
        assert!(line.contains(r#""media_type":"image/png""#), "{line}");
        let back: Turn = serde_json::from_str(&line).expect("round trip");
        assert_eq!(back, with);
    }

    #[test]
    fn a_stored_photo_turn_reads_and_writes_byte_for_byte() {
        use crate::capability::Turn;
        let id = AttachmentId::of(PNG);
        let stored = format!(
            r#"{{"who":"person","text":"look","references":[],"attachments":[{{"id":"{id}","media_type":"image/png"}}]}}"#
        );
        let turn: Turn = serde_json::from_str(&stored).expect("a transcript line from before PDFs");
        assert_eq!(serde_json::to_string(&turn).expect("serialises"), stored);
    }

    #[test]
    fn a_pdf_is_a_pdf_by_its_bytes_and_keeps_the_name_it_was_picked_with() {
        let pdf = b"%PDF-1.7\n%%EOF\n";
        let accepted = Attachment::accept(pdf).expect("a pdf");
        assert_eq!(accepted.media_type, Enclosure::Pdf);
        assert_eq!(accepted.name, None);
        let named = accepted.named("  I-797 notice.pdf ").expect("a plain name");
        assert_eq!(named.name.as_deref(), Some("I-797 notice.pdf"));
        assert_eq!(named.clone().named(" ").expect("blank").name, None);
        assert_eq!(
            named.clone().named("a\nb.pdf"),
            Err(AttachmentError::BadName)
        );
        assert_eq!(
            named.named(&"x".repeat(MAX_NAME + 1)),
            Err(AttachmentError::BadName)
        );
        let big = [&pdf[..], &vec![0; crate::file_read::MAX_PDF][..]].concat();
        assert_eq!(
            Attachment::accept(&big),
            Err(AttachmentError::TooLarge {
                size: big.len(),
                limit: crate::file_read::MAX_PDF
            })
        );
        let over_an_image = [&pdf[..], &vec![0; MAX_BYTES][..]].concat();
        assert!(
            Attachment::accept(&over_an_image).is_ok(),
            "a PDF's limit is its own"
        );
    }

    #[test]
    fn an_id_is_only_ever_64_hex_digits() {
        assert!("../../etc/passwd".parse::<AttachmentId>().is_err());
        assert!(
            "A".repeat(64).parse::<AttachmentId>().is_err(),
            "uppercase is not our spelling"
        );
        assert!("a".repeat(63).parse::<AttachmentId>().is_err());
        let id: AttachmentId = "a".repeat(64).parse().expect("64 hex digits");
        let json = serde_json::to_string(&id).expect("serialises");
        assert!(
            serde_json::from_str::<AttachmentId>("\"../x\"").is_err(),
            "serde goes through the check"
        );
        assert_eq!(
            serde_json::from_str::<AttachmentId>(&json).expect("round trip"),
            id
        );
    }
}
