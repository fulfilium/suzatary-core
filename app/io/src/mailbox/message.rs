//! One message of `notmuch show --format=json`, parsed once at the boundary.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;
use suzatary_core::mail_error::MailError;

/// One message as notmuch shows it: every field the readers use, typed, and a message without
/// an `id` or a file does not parse, so no reader meets an empty one.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NotmuchMessage {
    /// notmuch's own id, its `Message-ID` without the brackets.
    pub id: String,
    /// Whether the query matched it, rather than its thread bringing it along.
    #[serde(rename = "match")]
    pub matched: bool,
    /// The first file holding it.
    #[serde(rename = "filename", deserialize_with = "first_file")]
    pub file: PathBuf,
    pub tags: Vec<String>,
    pub headers: HashMap<String, String>,
    /// Its MIME tree, absent under `--body=false`.
    pub body: Option<Value>,
}

/// notmuch names a message's files as an array, and a message must have one.
fn first_file<'de, D: serde::Deserializer<'de>>(de: D) -> Result<PathBuf, D::Error> {
    Vec::<PathBuf>::deserialize(de)?
        .into_iter()
        .next()
        .ok_or_else(|| serde::de::Error::custom("a message names no file"))
}

/// One node of a thread: a message (`null` where notmuch elides one) and its replies.
#[derive(Debug, Deserialize)]
struct Node(Option<NotmuchMessage>, Vec<Node>);

/// The messages a `notmuch show` matched, in its order, parsed from its whole answer: a list of
/// threads, each a list of nodes.
///
/// One message that does not parse (no id, no file) refuses the whole answer, not that one row:
/// notmuch mints an id for every message it indexes, so such an answer is not its show format.
pub fn matched(raw: &[u8]) -> Result<Vec<NotmuchMessage>, MailError> {
    fn walk(nodes: Vec<Node>, out: &mut Vec<NotmuchMessage>) {
        for Node(message, replies) in nodes {
            out.extend(message.filter(|m| m.matched));
            walk(replies, out);
        }
    }
    let text = Text::of(raw);
    let threads: Vec<Vec<Node>> = serde_json::from_str(text.as_str())
        .map_err(|e| MailError::Unreachable(format!("notmuch's show output did not parse: {e}")))?;
    let mut out = Vec::new();
    for thread in threads {
        walk(thread, &mut out);
    }
    match out
        .iter()
        .find(|m| text.lost(&m.id) || text.lost(&m.file.to_string_lossy()))
    {
        Some(m) => Err(unnamed(&format!("{} in {}", m.id, m.file.display()))),
        None => Ok(out),
    }
}

/// notmuch's answer as text: exactly, when it is UTF-8, or with each byte that is not replaced
/// by U+FFFD, so a body or a header that is not UTF-8 still reads.
pub enum Text<'a> {
    /// It was UTF-8.
    Exact(&'a str),
    /// It was not, and lost bytes to the replacement.
    Replaced(String),
}

impl<'a> Text<'a> {
    #[must_use]
    pub fn of(raw: &'a [u8]) -> Self {
        match String::from_utf8_lossy(raw) {
            Cow::Borrowed(exact) => Text::Exact(exact),
            Cow::Owned(replaced) => Text::Replaced(replaced),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Text::Exact(text) => text,
            Text::Replaced(text) => text,
        }
    }

    /// Whether `field`, read out of this text, lost a byte: an id or a file name that did names
    /// nothing, so it is refused rather than looked up.
    #[must_use]
    pub fn lost(&self, field: &str) -> bool {
        match self {
            Text::Exact(_) => false,
            Text::Replaced(_) => field.contains(char::REPLACEMENT_CHARACTER),
        }
    }
}

/// The refusal of a message whose id or file name is not UTF-8.
#[must_use]
pub fn unnamed(what: &str) -> MailError {
    MailError::Unreachable(format!(
        "notmuch names a message by an id or a file name that is not UTF-8, so it cannot be read: {what}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_of_another_shape_is_refused_not_walked_past() {
        for raw in ["{}", "[[[1, []]]]", "[[[\"x\"]]]", "not json"] {
            assert!(matched(raw.as_bytes()).is_err(), "{raw}");
        }
        assert!(matched(b"[]").expect("no threads").is_empty());
    }

    /// A body or header that is not UTF-8 reads with the bad bytes replaced; an id or a file name
    /// that is not names nothing, and is refused by name, never dropped.
    #[test]
    fn bytes_that_are_not_utf8_read_in_text_and_refuse_in_a_name() {
        // `~` stands for the byte 0xff, which is never UTF-8.
        let shown = |id: &str, file: &str, subject: &str| {
            format!(
                r#"[[[{{"id": "{id}", "match": true, "filename": ["{file}"], "tags": [],
                       "headers": {{"Subject": "{subject}"}}}}, []]]]"#
            )
            .bytes()
            .map(|b| if b == b'~' { 0xff } else { b })
            .collect::<Vec<u8>>()
        };
        let read = matched(&shown("a", "/mail/a", "caf~")).expect("a bad subject reads");
        assert_eq!(read[0].headers["Subject"], "caf\u{fffd}");
        for (id, file) in [("a~", "/mail/a"), ("a", "/mail/a~")] {
            let refused = matched(&shown(id, file, "ok")).expect_err("a bad name");
            assert!(refused.to_string().contains("not UTF-8"), "{refused}");
        }
    }
}
