//! What reading one file for the model answers: the file itself, or its text.
//!
//! No held number the box can find reaches a model: a PDF whose text layer names one
//! ([`crate::redact`]) is withheld, and answered by its placeholder (`crate::held_value`), which
//! reveals or sends the file on the owner's tap, and by its text with each number a placeholder
//! naming the file and the number's place in it. [`FileRead::of`] is the only way to a
//! [`Readable`], so a file that skipped the check does not typecheck.
//!
//! A scan's pixels cannot be redacted: a PDF with no text layer, and an image, are handed over
//! as they are, by the owner's choice.

use crate::enclosed::Enclosure;
use crate::held_file::HeldFile;
use crate::held_value::{Masked, Placeholder, Values};
use crate::redact::{Redacted, identity_numbers, redact};

/// The largest PDF handed to a model, in bytes, under the request size its API takes.
pub const MAX_PDF: usize = 20 * 1024 * 1024;

/// The most pages of a PDF handed to a model, as its API takes them.
pub const MAX_PAGES: usize = 100;

/// The most characters of text a read answers, with the cut declared.
pub const MAX_CHARS: usize = crate::mail::MAX_ATTACHMENT_CHARS;

/// What a read answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRead {
    /// The file itself, for the model to read.
    File(Readable),
    /// A text file's words.
    Text {
        /// The words, masked.
        text: Masked,
        /// Whether they were cut at [`MAX_CHARS`].
        truncated: bool,
    },
    /// A PDF the box holds back, answered by its placeholder and its text.
    Withheld {
        /// The file's placeholder, which reveals it on the owner's device or sends it on a tap.
        held: Placeholder,
        /// Its text layer, masked.
        text: Masked,
        /// Whether the text was cut at [`MAX_CHARS`].
        truncated: bool,
        /// Why the file itself is not handed over.
        why: Withheld,
        /// Its size in bytes, which the box keeps and never tells a model.
        size: u64,
        /// Its SHA-256, which the box keeps and never tells a model: from a file's text and the
        /// hints, a model could test guesses of a number against it.
        digest: crate::attachment::AttachmentId,
    },
    /// Nothing a model reads, named so the refusal is a sentence.
    Unsupported {
        /// What it is: its type, or its extension.
        what: String,
    },
}

/// Why a PDF itself is not handed to a model, said to neither the model nor the owner in
/// particular: each reader frames it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Withheld {
    /// Its text names a number [`crate::redact`] holds.
    #[error("its text names an identity number, a code, or a card or bank number")]
    HeldNumber,
    /// Over [`MAX_PDF`], in bytes.
    #[error("it is {}, over the 20 MB a model reads", megabytes(*.0))]
    TooLarge(usize),
    /// Over [`MAX_PAGES`].
    #[error("it has {0} pages, over the {MAX_PAGES} a model reads")]
    TooManyPages(usize),
}

/// A file a model may read, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readable {
    media: Enclosure,
    bytes: Vec<u8>,
    text: Redacted,
}

impl Readable {
    /// What it is.
    #[must_use]
    pub fn media(&self) -> Enclosure {
        self.media
    }

    /// Its bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Its text layer, redacted and capped, for a model that cannot read the file.
    #[must_use]
    pub fn text(&self) -> &Redacted {
        &self.text
    }
}

impl FileRead {
    /// Read `bytes`, the file `file` names and calls `name`, which its source calls `mime`.
    /// `layer` is a PDF's text layer as `pdftotext` read it, a form feed ending each page; `None`
    /// for anything else.
    #[must_use]
    pub fn of(
        file: &HeldFile,
        name: &str,
        mime: &str,
        bytes: Vec<u8>,
        layer: Option<&str>,
    ) -> Self {
        match Enclosure::sniff(&bytes) {
            Some(Enclosure::Pdf) => pdf(file, name, bytes, layer.unwrap_or_default()),
            Some(Enclosure::Image(media)) if bytes.len() > crate::attachment::MAX_BYTES => {
                FileRead::Unsupported {
                    what: format!(
                        "a {} image of {}, over the 5 MB a model reads",
                        media.as_str(),
                        megabytes(bytes.len())
                    ),
                }
            }
            Some(media @ Enclosure::Image(_)) => FileRead::File(Readable {
                media,
                bytes,
                text: redact(""),
            }),
            None if mime.starts_with("text/") => {
                let (text, truncated) = Masked::of(file, &words(&bytes, None)).clipped(MAX_CHARS);
                FileRead::Text { text, truncated }
            }
            None => FileRead::Unsupported {
                what: if mime.is_empty() {
                    "a file of no stated type".to_owned()
                } else {
                    mime.to_owned()
                },
            },
        }
    }
}

/// A file's whole words, which a read masks before it cuts them, so a number across the cut is
/// found: a PDF's text layer, nothing of an image, and anything else's bytes as text.
fn words(bytes: &[u8], layer: Option<&str>) -> String {
    match Enclosure::sniff(bytes) {
        Some(Enclosure::Pdf) => layer.unwrap_or_default().to_owned(),
        Some(Enclosure::Image(_)) => String::new(),
        None => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// The held numbers of a file's words as [`FileRead::of`] masked them, in the order a
/// placeholder's handle counts them.
#[must_use]
pub fn values(bytes: &[u8], layer: Option<&str>) -> Values {
    Values::of([words(bytes, layer).as_str()])
}

fn pdf(file: &HeldFile, name: &str, bytes: Vec<u8>, layer: &str) -> FileRead {
    let pages = layer.matches('\u{c}').count();
    let why = if !identity_numbers(layer).is_empty() {
        Some(Withheld::HeldNumber)
    } else if bytes.len() > MAX_PDF {
        Some(Withheld::TooLarge(bytes.len()))
    } else if pages > MAX_PAGES {
        Some(Withheld::TooManyPages(pages))
    } else {
        None
    };
    match why {
        Some(why) => {
            let (text, truncated) = Masked::of(file, layer).clipped(MAX_CHARS);
            FileRead::Withheld {
                held: Placeholder::file(file, name, bytes.len() as u64),
                text,
                truncated,
                why,
                size: bytes.len() as u64,
                digest: crate::attachment::AttachmentId::of(&bytes),
            }
        }
        None => FileRead::File(Readable {
            media: Enclosure::Pdf,
            text: redact(&crate::mail::clip(layer, MAX_CHARS).0),
            bytes,
        }),
    }
}

/// A size as a sentence names it.
fn megabytes(size: usize) -> String {
    crate::secretary::card::bytes(size as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::held_value::{Handle, Holder, Place};

    const PDF: &[u8] = b"%PDF-1.7\n%%EOF\n";

    fn notice() -> HeldFile {
        HeldFile::Mail {
            message: "m1@example.test".to_owned(),
            name: "w2.pdf".to_owned(),
        }
    }

    #[test]
    fn a_pdf_whose_text_names_an_identity_number_is_withheld_and_named_by_the_file() {
        let layer = "Employee SSN: 123-45-6789. Wages: 70,000.\u{c}";
        let FileRead::Withheld {
            held, text, why, ..
        } = FileRead::of(
            &notice(),
            "w2.pdf",
            "application/pdf",
            PDF.to_vec(),
            Some(layer),
        )
        else {
            panic!("withheld");
        };
        assert_eq!(
            held,
            Placeholder::file(&notice(), "w2.pdf", PDF.len() as u64)
        );
        let place = Place::new(Holder::File(notice()), 0).expect("a place");
        assert_eq!(
            text.as_str(),
            format!(
                "Employee SSN: [SSN ending 6789 · tap to show · {}]. Wages: 70,000.\u{c}",
                Handle::Value(place).print()
            )
        );
        assert_eq!(why, Withheld::HeldNumber);
        assert_eq!(
            values(PDF, Some(layer)).get(0),
            Some((crate::redact::Kind::Ssn, "123-45-6789"))
        );
    }

    /// The decision reads the whole layer: a number past the cut a read shows still withholds
    /// the file, and the text shown stops before it.
    #[test]
    fn a_number_past_the_cut_withholds_the_pdf() {
        let layer = format!(
            "{} Employee SSN: 123-45-6789.\u{c}",
            "wages ".repeat(MAX_CHARS)
        );
        let FileRead::Withheld {
            text,
            truncated,
            why,
            ..
        } = FileRead::of(
            &notice(),
            "w2.pdf",
            "application/pdf",
            PDF.to_vec(),
            Some(&layer),
        )
        else {
            panic!("withheld");
        };
        assert_eq!(why, Withheld::HeldNumber);
        assert!(truncated);
        assert!(!text.as_str().contains("6789"));
    }

    /// A number the cut falls inside is masked whole before the cut, so none of its digits
    /// reach a model, in a PDF's text or a text file's.
    #[test]
    fn a_number_across_the_cut_is_masked_whole() {
        let words = format!("{}SSN 123-45-6789 ok", "x".repeat(MAX_CHARS - 9));
        for read in [
            FileRead::of(
                &notice(),
                "w2.pdf",
                "application/pdf",
                PDF.to_vec(),
                Some(&words),
            ),
            FileRead::of(
                &notice(),
                "w2.txt",
                "text/plain",
                words.clone().into_bytes(),
                None,
            ),
        ] {
            let (FileRead::Withheld { text, .. } | FileRead::Text { text, .. }) = read else {
                panic!("text: {read:?}");
            };
            let tail: String = text.as_str().chars().skip(MAX_CHARS - 20).collect();
            assert!(!tail.contains("123") && !tail.contains("45-"), "{tail}");
        }
        assert_eq!(
            values(PDF, Some(&words)).get(0),
            Some((crate::redact::Kind::Ssn, "123-45-6789"))
        );
    }

    #[test]
    fn text_is_text_and_anything_else_is_named() {
        assert!(matches!(
            FileRead::of(&notice(), "a.txt", "text/plain", b"SSN: 123-45-6789".to_vec(), None),
            FileRead::Text { text, .. } if text.as_str().starts_with("SSN: [SSN ending 6789 · tap to show · held:file:")
        ));
        assert_eq!(
            FileRead::of(
                &notice(),
                "a.zip",
                "application/zip",
                b"PK\x03\x04".to_vec(),
                None
            ),
            FileRead::Unsupported {
                what: "application/zip".to_owned()
            }
        );
        assert!(
            matches!(
                FileRead::of(
                    &notice(),
                    "a.pdf",
                    "application/pdf",
                    b"<html>".to_vec(),
                    None
                ),
                FileRead::Unsupported { .. }
            ),
            "a PDF by name only is not one"
        );
    }
}
