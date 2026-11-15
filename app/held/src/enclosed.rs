//! A file handed to the model with a verb's answer: a PDF as a document, an image as an image.
//!
//! `text` is the file's text layer, already redacted, for an interpreter whose API cannot take
//! the file itself.

use serde::{Deserialize, Serialize};

use crate::attachment::{AttachmentId, MediaType};

/// What an enclosed file is, which is what block it becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Enclosure {
    /// A PDF, read page by page, a scanned one included.
    Pdf,
    /// An image of a type the model accepts.
    Image(MediaType),
}

/// One file on a verb's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enclosed {
    /// Its name in the file store: the SHA-256 of its bytes.
    pub id: AttachmentId,
    /// What it was called where it was read.
    pub name: String,
    /// What it is.
    pub media: Enclosure,
    /// Its size in bytes.
    pub size: u64,
    /// Its text layer, redacted and capped, `""` for an image or a scan.
    pub text: String,
}

/// A media type no block is made of.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not a PDF or an image type a model reads")]
pub struct NotEnclosable(pub String);

impl Enclosure {
    /// The MIME spelling, as the model's API names it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Image(media) => media.as_str(),
        }
    }

    /// What these bytes are, by their own magic numbers.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"%PDF-") {
            Some(Self::Pdf)
        } else {
            MediaType::sniff(bytes).map(Self::Image)
        }
    }
}

impl TryFrom<String> for Enclosure {
    type Error = NotEnclosable;
    fn try_from(text: String) -> Result<Self, NotEnclosable> {
        Ok(match text.as_str() {
            "application/pdf" => Self::Pdf,
            "image/jpeg" => Self::Image(MediaType::Jpeg),
            "image/png" => Self::Image(MediaType::Png),
            "image/webp" => Self::Image(MediaType::Webp),
            "image/gif" => Self::Image(MediaType::Gif),
            _ => return Err(NotEnclosable(text)),
        })
    }
}

impl From<Enclosure> for String {
    fn from(media: Enclosure) -> String {
        media.as_str().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_typed_by_its_own_bytes() {
        assert_eq!(Enclosure::sniff(b"%PDF-1.7\n"), Some(Enclosure::Pdf));
        assert_eq!(
            Enclosure::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(Enclosure::Image(MediaType::Jpeg))
        );
        assert_eq!(Enclosure::sniff(b"<html>"), None);
    }

    #[test]
    fn an_enclosure_round_trips_as_its_mime_type() {
        for media in [Enclosure::Pdf, Enclosure::Image(MediaType::Png)] {
            let json = serde_json::to_string(&media).expect("serialises");
            assert_eq!(json, format!("\"{}\"", media.as_str()));
            assert_eq!(
                serde_json::from_str::<Enclosure>(&json).expect("reads back"),
                media
            );
        }
        assert!(serde_json::from_str::<Enclosure>("\"text/html\"").is_err());
    }
}
