//! A message's attachments, read from the raw maildir file with `mail-parser`, because
//! notmuch's JSON never carries a binary part's content.

use mail_parser::{MessageParser, MimeHeaders as _};
use suzatary_core::mail::AttachmentMeta;

/// Every attachment one raw RFC 5322 message carries, in the order it lists them; `[]` for a
/// message that does not parse.
#[must_use]
pub fn list(bytes: &[u8]) -> Vec<AttachmentMeta> {
    let Some(message) = MessageParser::default().parse(bytes) else {
        return Vec::new();
    };
    message.attachments().map(meta_of).collect()
}

/// One named attachment's MIME type and decoded bytes; the first match when a name repeats.
#[must_use]
pub fn find(bytes: &[u8], name: &str) -> Option<(String, Vec<u8>)> {
    let message = MessageParser::default().parse(bytes)?;
    message
        .attachments()
        .find(|part| attachment_name(part) == name)
        .map(|part| (mime_of(part), part.contents().to_vec()))
}

fn meta_of(part: &mail_parser::MessagePart<'_>) -> AttachmentMeta {
    AttachmentMeta {
        name: attachment_name(part),
        mime: mime_of(part),
        size: part.contents().len(),
    }
}

/// `Content-Disposition`'s `filename`, else `Content-Type`'s `name`.
fn attachment_name(part: &mail_parser::MessagePart<'_>) -> String {
    part.attachment_name().unwrap_or_default().to_owned()
}

/// `type/subtype`, lower-cased; `""` when the part has no `Content-Type`.
pub(crate) fn mime_of(part: &mail_parser::MessagePart<'_>) -> String {
    part.content_type().map_or_else(String::new, |ct| {
        ct.c_subtype.as_deref().map_or_else(
            || ct.c_type.to_lowercase(),
            |sub| format!("{}/{}", ct.c_type.to_lowercase(), sub.to_lowercase()),
        )
    })
}
