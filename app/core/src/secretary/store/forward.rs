//! A stored message forwarded on a tap.

use prodrome_core::literal::{ProdromeError, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Draft;
use crate::held_value::Original;
use crate::mail::StoredMessage;
use crate::mail::send::Attachment;
use prodrome_core::literal::codec::{Constructor, Fields, Literal, record};

/// A forward as proposed: the message around the original, and the original by its record.
///
/// The record holds none of the original's words, so a held value in it leaves the box only in
/// the bytes the tap reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forward {
    /// The address it is sent as.
    pub account: String,
    /// The visible recipients.
    pub to: Vec<String>,
    /// The visible copies.
    pub cc: Vec<String>,
    /// The forward's own subject.
    pub subject: String,
    /// The note above the original, `""` for none.
    pub note: String,
    /// The original.
    pub message: Forwarded,
}

/// How the original travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Its headers and text below the note, its attachments attached.
    Inline,
    /// Its file whole, as an `.eml` attachment.
    Attached,
}

impl Style {
    /// Every style.
    pub const ALL: [Style; 2] = [Style::Inline, Style::Attached];

    /// The word its stored form and the verb's argument use.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Style::Inline => "inline",
            Style::Attached => "attached",
        }
    }
}

prodrome_core::word_literal!(Style: Style::ALL => |style| style.as_str());

/// The original of a forward, as its card shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forwarded {
    /// Its file, recorded when proposed.
    pub file: StoredMessage,
    /// How it travels.
    pub style: Style,
    /// Its sender, as the model read it.
    pub from: String,
    /// Its subject, as the model read it.
    pub subject: String,
    /// When it was sent, `None` when its date does not parse.
    pub date: Option<String>,
}

/// What an attached forward's file is called.
pub const EML: &str = "forwarded.eml";

/// The line an inline forward's original begins with.
pub const SEPARATOR: &str = "---------- Forwarded message ----------";

/// The headers an inline forward repeats, in this order.
const REPEATED: &[&str] = &["From", "Date", "Subject", "To", "Cc"];

impl Forward {
    /// The message the tap sends, its body the note; an inline forward's original follows it,
    /// read from the file at the tap ([`Forward::quoted`]).
    #[must_use]
    pub fn draft(&self) -> Draft {
        Draft {
            account: self.account.clone(),
            to: self.to.clone(),
            cc: self.cc.clone(),
            subject: self.subject.clone(),
            body: self.note.clone(),
            attachments: Vec::new(),
        }
    }

    /// The parts the tap sends: the original's attachments inline, its file attached.
    #[must_use]
    pub fn parts(&self, original: Original) -> Vec<Attachment> {
        match self.message.style {
            Style::Inline => original.parts,
            Style::Attached => vec![Attachment {
                filename: EML.to_owned(),
                content: original.raw,
                mime_type: "message/rfc822".to_owned(),
            }],
        }
    }

    /// What follows the note in an inline forward: `original`'s headers and text, every held
    /// value in place.
    #[must_use]
    pub fn quoted(&self, original: &Original) -> String {
        let headers = REPEATED.iter().filter_map(|name| {
            original
                .headers
                .iter()
                .find(|header| header.name.eq_ignore_ascii_case(name))
                .map(|header| format!("{name}: {}\n", header.value))
        });
        let mut parts: Vec<String> = Vec::new();
        if !self.note.trim().is_empty() {
            parts.push("\n\n".to_owned());
        }
        parts.push(format!("{SEPARATOR}\n"));
        parts.extend(headers);
        parts.push("\n".to_owned());
        parts.push(original.body.clone());
        parts.concat()
    }
}

/// `Forward(account, to, cc, subject, note, message)`.
pub const FORWARD: Constructor = (
    "Forward",
    &["account", "to", "cc", "subject", "note", "message"],
);
/// `Forwarded(file, style, from, subject, date)`.
pub const FORWARDED: Constructor = ("Forwarded", &["file", "style", "from", "subject", "date"]);

impl Literal for Forward {
    fn to_value(&self) -> Value {
        record(
            FORWARD,
            vec![
                self.account.to_value(),
                self.to.to_value(),
                self.cc.to_value(),
                self.subject.to_value(),
                self.note.to_value(),
                self.message.to_value(),
            ],
        )
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::of(value, FORWARD)?;
        Ok(Forward {
            account: fields.get("account")?,
            to: fields.get("to")?,
            cc: fields.get("cc")?,
            subject: fields.get("subject")?,
            note: fields.get("note")?,
            message: fields.get("message")?,
        })
    }
}

impl Literal for Forwarded {
    fn to_value(&self) -> Value {
        record(
            FORWARDED,
            vec![
                self.file.to_value(),
                self.style.to_value(),
                self.from.to_value(),
                self.subject.to_value(),
                self.date.to_value(),
            ],
        )
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::of(value, FORWARDED)?;
        Ok(Forwarded {
            file: fields.get("file")?,
            style: fields.get("style")?,
            from: fields.get("from")?,
            subject: fields.get("subject")?,
            date: fields.get("date")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rename_all` spells the generated `Style` and the verb's JSON schema, so serde's words are checked against `ALL`'s.
    #[test]
    fn a_styles_serde_word_is_its_literals() {
        crate::words::assert_serde_words(&Style::ALL, |style| style.as_str());
    }
}
