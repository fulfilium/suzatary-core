//! The messages and files a conversation read: the only holders a held value it proposes to send
//! may come from.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::capability::Turn;
use crate::held_value::{Handle, HeldFile, Holder};
use crate::secretary::attachment_source::AttachmentSource;
use crate::secretary::store::Stored;
use crate::secretary::surface::SecretaryVerb;
use crate::secretary::tool_name::ToolName;

/// The notmuch ids of every message a `show`, a `search` or a reference the person pointed at
/// answered in a conversation, by the answers' `id` fields, and the source of every file a
/// file reader answered, by the `source` field the reader writes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadHolders {
    messages: BTreeSet<String>,
    files: Vec<HeldFile>,
}

impl ReadHolders {
    /// The holders `stored`'s answers name.
    #[must_use]
    pub fn of(stored: &Stored) -> Self {
        let mut read = Self::default();
        for said in &stored.turns {
            let (verb, answer) = match &said.turn {
                Turn::Tool { verb, answer, .. } => (verb, answer),
                Turn::Person { .. }
                | Turn::Model { .. }
                | Turn::Act { .. }
                | Turn::Erased { .. } => continue,
            };
            let Ok(value) = serde_json::from_str::<Value>(answer) else {
                continue;
            };
            match verb {
                ToolName::Verb(SecretaryVerb::Show | SecretaryVerb::Search)
                | ToolName::Reference => named(&value, &mut read.messages),
                ToolName::Verb(
                    SecretaryVerb::Attachments
                    | SecretaryVerb::DriveRead
                    | SecretaryVerb::UploadRead,
                ) => {
                    if let Some(file) = value
                        .get(crate::secretary::verb_answer::SOURCE)
                        .and_then(|source| {
                            serde_json::from_value::<AttachmentSource>(source.clone()).ok()
                        })
                        .and_then(|source| HeldFile::try_from(source).ok())
                    {
                        read.files.push(file);
                    }
                }
                ToolName::Verb(_) | ToolName::Memory | ToolName::Unlisted(_) => {}
            }
        }
        read
    }

    /// Whether the holder of the value or file `handle` names was read.
    #[must_use]
    pub fn holds(&self, handle: &Handle) -> bool {
        match handle {
            Handle::Value(place) => match place.holder() {
                Holder::Message(message) => self.reads(message),
                Holder::File(file) => self.reads_file(file),
            },
            Handle::File(file) => self.reads_file(file),
        }
    }

    /// Whether a file reader answered `file` in this conversation.
    #[must_use]
    pub fn reads_file(&self, file: &HeldFile) -> bool {
        self.files.contains(file)
    }

    /// Whether the message `message`, a notmuch id, was read.
    #[must_use]
    pub fn reads(&self, message: &str) -> bool {
        self.messages.contains(message)
    }
}

/// Every `id` field of `value`: the notmuch id a mail answer names each message by. A
/// placeholder in an answer's text marks nothing read, since a Drive file or a web page may hold
/// one typed by anyone.
fn named(value: &Value, read: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields {
                if let (true, Value::String(id)) = (key == "id", field) {
                    read.insert(id.clone());
                }
                named(field, read);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| named(item, read)),
        Value::String(_) | Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}
