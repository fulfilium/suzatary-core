//! Values a model sees only by placeholder: held files, attachments, uploads, redaction and the
//! leak guard.

#![forbid(unsafe_code)]

pub mod attachment;
pub mod enclosed;
pub mod excerpt;
pub mod file_read;
pub mod for_model;
pub mod held_file;
pub mod held_value;
pub mod leak;
pub mod mail_error;
pub mod message_part;
pub mod pdf_text;
pub mod redact;
pub mod secretary {
    //! The secretary's held attachments, their sources and the uploads it keeps standing.
    pub mod attached;
    pub mod attachment_source;
    pub mod upload_standing;
}
pub mod upload;
pub mod upload_index;
pub mod vault;

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
