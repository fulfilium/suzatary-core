//! A message file as a forward reads it: the proposal's record and the tap's original, both from
//! one read of one file, attachments by their place in it.

use mail_parser::{Address, HeaderValue, Message, MessageParser, MimeHeaders as _};
use suzatary_core::attachment::AttachmentId;
use suzatary_core::held_value::Original;
use suzatary_core::mail::send::Attachment;
use suzatary_core::mail::{MailHeader, StoredMessage, StoredPart};

/// `raw`, the file of the message `id`, as a forward records it: its digest, the held values in
/// its text and its attachments.
#[must_use]
pub fn stored(id: &str, raw: &[u8]) -> StoredMessage {
    StoredMessage {
        id: id.to_owned(),
        size: raw.len() as u64,
        sha256: AttachmentId::of(raw),
        held: super::held::values(raw, &[]).held(),
        parts: parts(raw)
            .into_iter()
            .map(|part| StoredPart {
                size: part.content.len() as u64,
                sha256: AttachmentId::of(&part.content),
                name: part.filename,
                media: part.mime_type,
            })
            .collect(),
    }
}

/// `raw` as the tap forwards it, every field read from these bytes.
#[must_use]
pub fn original(raw: Vec<u8>) -> Original {
    let (headers, body) = MessageParser::default().parse(&raw).map_or_else(
        || (Vec::new(), String::from_utf8_lossy(&raw).into_owned()),
        |message| (headers(&message), text(&message)),
    );
    Original {
        parts: parts(&raw),
        headers,
        body,
        raw,
    }
}

/// Every attachment of `raw`, in the file's order; the `n`th that names no file is
/// `attachment-<n>`, so two parts never share a place and none goes unnamed.
fn parts(raw: &[u8]) -> Vec<Attachment> {
    let Some(message) = MessageParser::default().parse(raw) else {
        return Vec::new();
    };
    message
        .attachments()
        .enumerate()
        .map(|(at, part)| {
            let mime = super::attachments::mime_of(part);
            Attachment {
                filename: part
                    .attachment_name()
                    .filter(|name| !name.trim().is_empty())
                    .map_or_else(|| format!("attachment-{}", at + 1), ToOwned::to_owned),
                mime_type: if mime.contains('/') {
                    mime
                } else {
                    "application/octet-stream".to_owned()
                },
                content: part.contents().to_vec(),
            }
        })
        .collect()
}

fn headers(message: &Message<'_>) -> Vec<MailHeader> {
    let header = |name: &str, value: Option<String>| {
        value
            .filter(|value| !value.is_empty())
            .map(|value| MailHeader {
                name: name.to_owned(),
                value,
            })
    };
    [
        header("From", message.from().map(addresses)),
        header("To", message.to().map(addresses)),
        header("Cc", message.cc().map(addresses)),
        header("Subject", message.subject().map(ToOwned::to_owned)),
        header("Date", message.date().map(mail_parser::DateTime::to_rfc822)),
        header(
            "Reply-To",
            message
                .header("Reply-To")
                .and_then(HeaderValue::as_address)
                .map(addresses),
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// An address list as a person writes it: `Name <address>`, comma-separated.
fn addresses(list: &Address<'_>) -> String {
    list.iter()
        .map(|addr| match (&addr.name, &addr.address) {
            (Some(name), Some(address)) => format!("{name} <{address}>"),
            (Some(name), None) => name.to_string(),
            (None, Some(address)) => address.to_string(),
            (None, None) => String::new(),
        })
        .filter(|one| !one.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Its text body, HTML read as text where it has no plain part.
fn text(message: &Message<'_>) -> String {
    message
        .body_text(0)
        .map(std::borrow::Cow::into_owned)
        .unwrap_or_default()
}
