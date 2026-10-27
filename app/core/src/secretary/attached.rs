//! One file on a proposed email, as the card showed it.
//!
//! Only the source, name, type, size and SHA-256 are stored (a proposal is hashed into its id);
//! the tap fetches again and [`checked`] passes only the recorded digest, so what leaves is what
//! the card showed or nothing.

use prodrome_core::literal::{ProdromeError, Value};
use serde::{Deserialize, Serialize};

use crate::attachment::AttachmentId;
use crate::mail::send::Attachment;
use crate::secretary::attachment_source::AttachmentSource;
use prodrome_core::literal::codec::{Constructor, Fields, Literal, record};

/// The most one message may carry in attachments, in bytes: under the 25 MB
/// the common transports accept once base64 has grown it by a third.
pub const MAX_TOTAL: u64 = 20 * 1024 * 1024;

/// One attachment, recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attached {
    /// Where the bytes are read from, at proposal time and at the tap.
    pub source: AttachmentSource,
    /// What the recipient sees it called.
    pub name: String,
    /// Its `type/subtype`.
    pub media: String,
    /// Its size in bytes.
    pub size: u64,
    /// The SHA-256 of its bytes, the same digest a photo is named by.
    pub sha256: AttachmentId,
}

/// Why an attachment was not recorded, or not sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unattachable {
    /// The source did not answer with bytes.
    #[error("{from}: {why}")]
    Unreadable {
        /// The source, as a sentence names it.
        from: String,
        /// Its own words.
        why: String,
    },
    /// Nothing is there.
    #[error("{0} is empty")]
    Empty(String),
    /// The name or type cannot go on a MIME part.
    #[error("{0}")]
    Unnamed(String),
    /// The whole message would carry too much.
    #[error(
        "the attachments come to {total} bytes; one message carries at most {MAX_TOTAL} \
         (20 MB)"
    )]
    TooLarge {
        /// Their total.
        total: u64,
    },
    /// The source's bytes are not the ones the card showed.
    #[error(
        "{name} changed since it was proposed (its SHA-256 began {was}, it now begins {now}); \
         nothing was sent"
    )]
    Changed {
        /// The file.
        name: String,
        /// The recorded digest's first twelve hex digits, as a card prints a hash.
        was: String,
        /// The same of what the source holds now.
        now: String,
    },
    /// A forwarded message's file is not the one the card showed, which no later tap changes.
    #[error(
        "the message changed since the forward was proposed, so nothing was sent; ask the \
         secretary for a new forward of it"
    )]
    OriginalChanged,
}

impl Attached {
    /// Record `bytes`, read from `source` now, as `name` of type `media`.
    ///
    /// # Errors
    /// [`Unattachable::Empty`] for no bytes, and [`Unattachable::Unnamed`] for
    /// a name or type a MIME part cannot carry.
    pub fn of(
        source: AttachmentSource,
        name: &str,
        media: &str,
        bytes: &[u8],
    ) -> Result<Self, Unattachable> {
        if bytes.is_empty() {
            return Err(Unattachable::Empty(source.describe()));
        }
        Self::known(
            source,
            name,
            media,
            bytes.len() as u64,
            AttachmentId::of(bytes),
        )
    }

    /// Record a file whose size and digest are already known, without reading it: a private
    /// file, which only the tap that sends it reads.
    ///
    /// # Errors
    /// [`Unattachable::Unnamed`] as for [`Attached::of`].
    pub fn known(
        source: AttachmentSource,
        name: &str,
        media: &str,
        size: u64,
        sha256: AttachmentId,
    ) -> Result<Self, Unattachable> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 255 || name.chars().any(char::is_control) {
            return Err(Unattachable::Unnamed(format!(
                "{name:?} is no filename: 1 to 255 characters, none of them control characters"
            )));
        }
        if !is_media(media) {
            return Err(Unattachable::Unnamed(format!("{media:?} is no media type")));
        }
        Ok(Self {
            source,
            name: name.to_owned(),
            media: media.to_owned(),
            size,
            sha256,
        })
    }

    /// These bytes as the part to send, when they are the ones recorded.
    ///
    /// # Errors
    /// [`Unattachable::Changed`] naming the file.
    pub fn check(&self, bytes: Vec<u8>) -> Result<Attachment, Unattachable> {
        let now = AttachmentId::of(&bytes);
        if now == self.sha256 {
            Ok(Attachment {
                filename: self.name.clone(),
                content: bytes,
                mime_type: self.media.clone(),
            })
        } else {
            Err(Unattachable::Changed {
                name: self.name.clone(),
                was: self.sha256.as_str()[..12].to_owned(),
                now: now.as_str()[..12].to_owned(),
            })
        }
    }
}

/// Refuse a set whose total is over [`MAX_TOTAL`].
///
/// # Errors
/// [`Unattachable::TooLarge`].
pub fn within_cap(attached: &[Attached]) -> Result<(), Unattachable> {
    let total = attached.iter().map(|one| one.size).sum();
    if total > MAX_TOTAL {
        Err(Unattachable::TooLarge { total })
    } else {
        Ok(())
    }
}

/// The parts a tap may send, each fetched again and matching its digest; [`checked`] and
/// [`read_from`] are the only constructors, so a transport handed one is handed the bytes the card
/// showed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Checked(Vec<Attachment>);

impl Checked {
    /// The parts, in the order the card listed them.
    #[must_use]
    pub fn into_parts(self) -> Vec<Attachment> {
        self.0
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// `parts`, read out of a forwarded message's file, when that file's digest as `read` is the
/// one the card `recorded`.
///
/// # Errors
/// [`Unattachable::OriginalChanged`].
pub fn read_from(
    recorded: &AttachmentId,
    read: &AttachmentId,
    parts: Vec<Attachment>,
) -> Result<Checked, Unattachable> {
    if recorded == read {
        Ok(Checked(parts))
    } else {
        Err(Unattachable::OriginalChanged)
    }
}

/// Fetch every attachment through `fetch`, which is handed the whole record, and check each
/// against it.
///
/// # Errors
/// The first attachment that cannot be read, or whose bytes changed.
pub fn checked(
    attached: &[Attached],
    fetch: impl Fn(&Attached) -> Result<Vec<u8>, String>,
) -> Result<Checked, Unattachable> {
    within_cap(attached)?;
    attached
        .iter()
        .map(|one| {
            let bytes = fetch(one).map_err(|why| Unattachable::Unreadable {
                from: one.source.describe(),
                why,
            })?;
            one.check(bytes)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Checked)
}

/// `type/subtype`, each a run of the characters a MIME token allows.
fn is_media(media: &str) -> bool {
    let token = |part: &str| {
        !part.is_empty()
            && part.len() <= 127
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&-^_.+".contains(&b))
    };
    media
        .split_once('/')
        .is_some_and(|(kind, sub)| token(kind) && token(sub))
}

/// `Attached(source, name, media, size, sha256)`.
pub const ATTACHED: Constructor = ("Attached", &["source", "name", "media", "size", "sha256"]);

impl Literal for Attached {
    fn to_value(&self) -> Value {
        record(
            ATTACHED,
            vec![
                self.source.to_value(),
                self.name.to_value(),
                self.media.to_value(),
                self.size.to_value(),
                self.sha256.to_value(),
            ],
        )
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::of(value, ATTACHED)?;
        Ok(Attached {
            source: fields.get("source")?,
            name: fields.get("name")?,
            media: fields.get("media")?,
            size: fields.get("size")?,
            sha256: fields.get("sha256")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice() -> AttachmentSource {
        AttachmentSource::Mail {
            message: "m1@example.test".to_owned(),
            name: "notice.pdf".to_owned(),
        }
    }

    #[test]
    fn a_record_holds_the_digest_and_size_of_what_was_read() {
        let one = Attached::of(notice(), "notice.pdf", "application/pdf", b"%PDF-1.7 x")
            .expect("records");
        assert_eq!(one.size, 10);
        assert_eq!(one.sha256, AttachmentId::of(b"%PDF-1.7 x"));
        let part = one.check(b"%PDF-1.7 x".to_vec()).expect("the same bytes");
        assert_eq!(part.filename, "notice.pdf");
        assert_eq!(part.mime_type, "application/pdf");
    }

    #[test]
    fn other_bytes_are_refused_naming_the_file() {
        let one = Attached::of(notice(), "notice.pdf", "application/pdf", b"%PDF-1.7 x")
            .expect("records");
        let refused = one.check(b"%PDF-1.7 y".to_vec()).expect_err("changed");
        assert!(matches!(&refused, Unattachable::Changed { name, .. } if name == "notice.pdf"));
        assert!(
            refused.to_string().contains("nothing was sent"),
            "{refused}"
        );
    }

    #[test]
    fn a_missing_source_refuses_the_whole_set() {
        let one = Attached::of(notice(), "notice.pdf", "application/pdf", b"%PDF").expect("ok");
        let refused = checked(&[one], |_| Err("no such message".to_owned())).expect_err("absent");
        assert!(
            refused.to_string().contains("notice.pdf on message"),
            "{refused}"
        );
        assert!(refused.to_string().contains("no such message"), "{refused}");
    }

    #[test]
    fn the_total_is_capped_at_twenty_megabytes() {
        let at = |size: u64| Attached {
            size,
            ..Attached::of(notice(), "notice.pdf", "application/pdf", b"x").expect("ok")
        };
        assert_eq!(within_cap(&[at(MAX_TOTAL)]), Ok(()));
        assert_eq!(
            within_cap(&[at(MAX_TOTAL / 2), at(MAX_TOTAL / 2 + 1)]),
            Err(Unattachable::TooLarge {
                total: MAX_TOTAL + 1
            })
        );
        assert!(checked(&[at(MAX_TOTAL + 1)], |_| Ok(b"x".to_vec())).is_err());
    }

    #[test]
    fn a_name_or_type_a_part_cannot_carry_is_refused() {
        for (name, media) in [
            ("", "application/pdf"),
            ("a\r\nBcc: x@example.test", "application/pdf"),
            ("notice.pdf", "application/pdf\r\nX: y"),
            ("notice.pdf", "pdf"),
        ] {
            assert!(
                Attached::of(notice(), name, media, b"x").is_err(),
                "{name:?} {media:?}"
            );
        }
        assert!(matches!(
            Attached::of(notice(), "notice.pdf", "application/pdf", b""),
            Err(Unattachable::Empty(_))
        ));
    }
}
