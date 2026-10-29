//! What the upload index makes of one upload a turn names: its words, held back, or gone.

use crate::enclosed::Enclosure;
use crate::file_read::{FileRead, Withheld};
use crate::upload::Upload;

/// One upload as the secretary's search may read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadStanding {
    /// Searchable by these words: a PDF's text layer or a private file's text, identity numbers
    /// masked, or `""` for a photo, since the box reads no text out of pixels.
    Indexed(String),
    /// Held back until the owner keeps it private: its text names a held number, so the
    /// index reads nothing of it.
    Held,
    /// No live turn names it, or the store no longer holds it: the index keeps nothing of it.
    Erased,
}

impl UploadStanding {
    /// The standing of what the store answers for an upload a live turn names; `layer` reads a
    /// PDF's text layer and is asked only for a PDF on the box.
    ///
    /// # Errors
    /// `layer`'s sentence: without the text the box cannot tell whether a PDF names an identity
    /// number, so it indexes nothing rather than guess.
    pub fn of(
        id: &crate::attachment::AttachmentId,
        upload: Option<Upload>,
        layer: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<Self, String> {
        match upload {
            None => Ok(Self::Erased),
            // Kept private by the owner's say, with its text masked when it was kept.
            Some(Upload::Private { copy, .. }) => Ok(Self::Indexed(copy.text)),
            Some(Upload::Here {
                media: Enclosure::Image(_),
                ..
            }) => Ok(Self::Indexed(String::new())),
            Some(Upload::Here {
                media: Enclosure::Pdf,
                bytes,
            }) => {
                let layer = layer(&bytes)?;
                Ok(
                    match FileRead::of(
                        &crate::held_value::HeldFile::Upload { id: id.clone() },
                        "file.pdf",
                        Enclosure::Pdf.as_str(),
                        bytes,
                        Some(&layer),
                    ) {
                        FileRead::File(readable) => {
                            Self::Indexed(readable.text().as_str().to_owned())
                        }
                        FileRead::Withheld {
                            why: Withheld::HeldNumber,
                            ..
                        } => Self::Held,
                        // Too large or too long for a model, and naming no held number.
                        FileRead::Withheld {
                            text,
                            why: Withheld::TooLarge(_) | Withheld::TooManyPages(_),
                            ..
                        }
                        | FileRead::Text { text, .. } => Self::Indexed(text.into_string()),
                        FileRead::Unsupported { .. } => Self::Indexed(String::new()),
                    },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::PrivateCopy;

    const PDF: &[u8] = b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n%%EOF\n";

    fn here_pdf() -> Upload {
        Upload::Here {
            media: Enclosure::Pdf,
            bytes: PDF.to_vec(),
        }
    }

    /// A PDF the box would hold back today is held, whenever it was stored.
    #[test]
    fn a_pdf_naming_an_identity_number_is_held() {
        assert_eq!(
            UploadStanding::of(
                &crate::attachment::AttachmentId::of(PDF),
                Some(here_pdf()),
                |_| Ok("SSN 123-45-6789 wages\u{c}".to_owned())
            ),
            Ok(UploadStanding::Held)
        );
    }

    /// A private file is indexed by the redacted text kept with it, and its layer is not read.
    #[test]
    fn a_private_file_is_indexed_by_its_redacted_text() {
        let private = Upload::Private {
            media: Enclosure::Pdf,
            name: "w2.pdf".to_owned(),
            copy: PrivateCopy {
                path: crate::vault::folder().join("w2.pdf").expect("a name"),
                size: 10,
                text: "Wages 70000 SSN [redacted]".to_owned(),
            },
        };
        assert_eq!(
            UploadStanding::of(
                &crate::attachment::AttachmentId::of(PDF),
                Some(private),
                |_| Err("not asked".to_owned())
            ),
            Ok(UploadStanding::Indexed(
                "Wages 70000 SSN [redacted]".to_owned()
            ))
        );
    }
}
