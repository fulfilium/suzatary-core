//! A message as the case file freezes it: the bytes of every file that held it, so a replay
//! indexes them in a scratch maildir and never reads the box's corpus.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

/// One message: notmuch's id for it and each file that held its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenMessage {
    /// The id notmuch gave it, which an index of the same bytes gives it again.
    pub id: String,
    /// Each file, at its place under the mail root.
    pub files: Vec<FrozenFile>,
}

/// One file of a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenFile {
    /// Where it lay under the mail root, so its account and folder read the same in a replay.
    pub path: MailPath,
    /// Its bytes, as base64 in the file, since a message need not be UTF-8.
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
}

/// A file's place under a mail root.
///
/// Every component is a plain name, so a replay that joins it to its scratch maildir writes
/// inside that maildir and nowhere else, whatever the case file says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MailPath(String);

/// A path that is not a place under a mail root.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a mail path is relative, with only plain names in it: {0:?}")]
pub struct NotAMailPath(pub String);

impl MailPath {
    /// Parse one; the only constructor.
    ///
    /// # Errors
    /// [`NotAMailPath`] when the path is empty, absolute, or holds `.` or `..`.
    pub fn new(text: impl Into<String>) -> Result<Self, NotAMailPath> {
        let text = text.into();
        let mut components = Path::new(&text).components().peekable();
        let plain = components.peek().is_some()
            && components.all(|component| matches!(component, Component::Normal(_)));
        if plain {
            Ok(Self(text))
        } else {
            Err(NotAMailPath(text))
        }
    }

    /// The path, to join to a mail root.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl TryFrom<String> for MailPath {
    type Error = NotAMailPath;
    fn try_from(text: String) -> Result<Self, NotAMailPath> {
        Self::new(text)
    }
}

impl From<MailPath> for String {
    fn from(path: MailPath) -> Self {
        path.0
    }
}

mod base64_bytes {
    use base64::Engine as _;
    use serde::{Deserialize as _, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A case file is read back from disk, so a path in it that climbs out of the scratch
    /// maildir must not parse.
    #[test]
    fn a_mail_path_is_only_a_place_under_the_mail_root() {
        for refused in [
            "",
            "/etc/passwd",
            "../outside",
            "her/../../outside",
            "./her/cur/1",
        ] {
            assert_eq!(
                MailPath::new(refused),
                Err(NotAMailPath(refused.to_owned())),
                "{refused:?}"
            );
            assert!(
                serde_json::from_value::<MailPath>(serde_json::json!(refused)).is_err(),
                "{refused:?}"
            );
        }
        let kept = MailPath::new("her/Inbox/cur/1:2,S").expect("a place under the root");
        assert_eq!(kept.as_path(), Path::new("her/Inbox/cur/1:2,S"));
    }

    #[test]
    fn a_frozen_file_keeps_bytes_that_are_not_utf8() {
        let file = FrozenFile {
            path: MailPath::new("her/cur/1").expect("a place"),
            bytes: vec![0xff, 0x00, b'a'],
        };
        let text = serde_json::to_string(&file).expect("serialises");
        assert_eq!(
            serde_json::from_str::<FrozenFile>(&text).expect("parses"),
            file
        );
    }
}
