//! Where a proposed email's attachment comes from: only the owner's own things.
//!
//! There is no arm for a URL or for bytes, so nothing from the web and nothing a model wrote can
//! be attached; a source is a name the box resolves itself, at proposal time and at the tap.

use prodrome_core::literal::{ProdromeError, Value as LiteralValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::attachment::AttachmentId;
use crate::drive::DrivePath;
use crate::repos::{RepoName, RepoPath, Rev};
use prodrome_core::literal::codec::{Constructor, Fields, Literal, arm, no_arm, record};

/// One source, by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "lowercase")]
pub enum AttachmentSource {
    /// An attachment of a message in the corpus.
    Mail {
        /// The message's notmuch id, which names it whether or not triage has seen it.
        message: String,
        /// The attachment's filename, as the message lists it.
        name: String,
    },
    /// A file in the owner's Drive.
    Drive {
        /// Its full path.
        path: DrivePath,
    },
    /// A photo or PDF the owner uploaded into a conversation, kept on the box or privately in
    /// his Drive.
    Upload {
        /// Its name in the upload store.
        id: AttachmentId,
    },
    /// A file the owner let the secretary read in this conversation (Allow once), as the box
    /// kept it then.
    Kept {
        /// Its SHA-256, the id its turn names.
        id: AttachmentId,
    },
    /// A file in one of the owner's declared repositories, kept by its digest when proposed.
    Repo {
        /// The repository.
        repo: RepoName,
        /// The file.
        path: RepoPath,
        /// The revision as asked (`HEAD`, the default branch, when none was); once recorded, the
        /// commit it resolved to, which resolves to itself, so the source names one file forever.
        at: Rev,
    },
}

/// An `attach` entry that names no source.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "each attach entry is one of {{\"mail\": <message id>, \"name\": <filename>}}, \
     {{\"drive\": <path>}}, {{\"upload\": <file id>}}, {{\"kept\": <file id>}} or \
     {{\"repo\": <owner/name>, \"path\": <file>, \"at\": <branch, tag or commit>}}: {0}"
)]
pub struct NotASource(pub String);

/// The longest message id or filename accepted, in characters.
const MAX_NAME: usize = 512;

impl AttachmentSource {
    /// Parse one entry of a model's `attach` array.
    ///
    /// # Errors
    /// [`NotASource`] for anything but exactly one of the four shapes.
    pub fn parse(entry: &Value) -> Result<Self, NotASource> {
        let Some(object) = entry.as_object() else {
            return Err(NotASource(format!("{entry} is not an object")));
        };
        let field = |key: &str| -> Result<Option<String>, NotASource> {
            match object.get(key) {
                None => Ok(None),
                Some(Value::String(text)) => named(text, key).map(Some),
                Some(other) => Err(NotASource(format!("{key} is {other}, not text"))),
            }
        };
        let keys: Vec<&str> = object.keys().map(String::as_str).collect();
        let id_of = |id: &str| {
            id.parse::<AttachmentId>()
                .map_err(|_| NotASource(format!("{id:?} is not a file's id")))
        };
        if let Some(repo) = field("repo")? {
            let at = field("at")?;
            let path = field("path")?
                .ok_or_else(|| NotASource(format!("{entry} names a repository and no path")))?;
            if keys.len() != 2 + usize::from(at.is_some()) {
                return Err(NotASource(format!("{entry} names no single source")));
            }
            let bad = |failed: crate::repos::RepoNameError| NotASource(failed.to_string());
            return Ok(Self::Repo {
                repo: repo.parse().map_err(bad)?,
                path: RepoPath::try_from(path).map_err(bad)?,
                at: Rev::try_from(at.unwrap_or_else(|| "HEAD".to_owned())).map_err(bad)?,
            });
        }
        if let Some(kept) = field("kept")? {
            return if keys.len() == 1 {
                Ok(Self::Kept { id: id_of(&kept)? })
            } else {
                Err(NotASource(format!("{entry} names no single source")))
            };
        }
        match (
            field("mail")?,
            field("drive")?,
            field("upload")?,
            field("name")?,
        ) {
            (Some(message), None, None, Some(name)) if keys.len() == 2 => {
                Ok(Self::Mail { message, name })
            }
            (None, Some(path), None, None) if keys.len() == 1 => Ok(Self::Drive {
                path: path
                    .parse()
                    .map_err(|failed: crate::drive::DrivePathError| {
                        NotASource(failed.to_string())
                    })?,
            }),
            (None, None, Some(id), None) if keys.len() == 1 => Ok(Self::Upload { id: id_of(&id)? }),
            _ => Err(NotASource(format!("{entry} names no single source"))),
        }
    }

    /// The source as a sentence names it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Mail { message, name } => format!("{name} on message {message}"),
            Self::Drive { path } => format!("{path} in Drive"),
            Self::Upload { id } => format!("uploaded file {}", &id.as_str()[..12]),
            Self::Kept { id } => format!("the file read here as {}", &id.as_str()[..12]),
            Self::Repo { repo, path, at } => format!(
                "{} in {repo} at {}",
                path.as_str(),
                at.as_str().get(..12).unwrap_or(at.as_str())
            ),
        }
    }
}

fn named(text: &str, key: &str) -> Result<String, NotASource> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_NAME || text.chars().any(char::is_control) {
        Err(NotASource(format!(
            "{key} is empty, longer than {MAX_NAME} characters, or holds a control character"
        )))
    } else {
        Ok(text.to_owned())
    }
}

/// `FromMail(message, name)`.
pub const FROM_MAIL: Constructor = ("FromMail", &["message", "name"]);
/// `FromDrive(path)`.
pub const FROM_DRIVE: Constructor = ("FromDrive", &["path"]);
/// `FromUpload(id)`.
pub const FROM_UPLOAD: Constructor = ("FromUpload", &["id"]);
/// `FromKept(id)`.
pub const FROM_KEPT: Constructor = ("FromKept", &["id"]);
/// `FromRepo(repo, path, at)`.
pub const FROM_REPO: Constructor = ("FromRepo", &["repo", "path", "at"]);

impl Literal for AttachmentSource {
    fn to_value(&self) -> LiteralValue {
        match self {
            AttachmentSource::Mail { message, name } => {
                record(FROM_MAIL, vec![message.to_value(), name.to_value()])
            }
            AttachmentSource::Drive { path } => record(FROM_DRIVE, vec![path.to_value()]),
            AttachmentSource::Upload { id } => record(FROM_UPLOAD, vec![id.to_value()]),
            AttachmentSource::Kept { id } => record(FROM_KEPT, vec![id.to_value()]),
            AttachmentSource::Repo { repo, path, at } => record(
                FROM_REPO,
                vec![repo.to_value(), path.to_value(), at.to_value()],
            ),
        }
    }

    fn from_value(value: &LiteralValue) -> Result<Self, ProdromeError> {
        Ok(match arm(value)? {
            "FromMail" => {
                let fields = Fields::of(value, FROM_MAIL)?;
                AttachmentSource::Mail {
                    message: fields.get("message")?,
                    name: fields.get("name")?,
                }
            }
            "FromDrive" => AttachmentSource::Drive {
                path: Fields::of(value, FROM_DRIVE)?.get("path")?,
            },
            "FromUpload" => AttachmentSource::Upload {
                id: Fields::of(value, FROM_UPLOAD)?.get("id")?,
            },
            "FromKept" => AttachmentSource::Kept {
                id: Fields::of(value, FROM_KEPT)?.get("id")?,
            },
            "FromRepo" => {
                let fields = Fields::of(value, FROM_REPO)?;
                AttachmentSource::Repo {
                    repo: fields.get("repo")?,
                    path: fields.get("path")?,
                    at: fields.get("at")?,
                }
            }
            other => return Err(no_arm("an attachment's source", other)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nothing_else_is_a_source() {
        for wrong in [
            json!("https://example.test/file.pdf"),
            json!({"url": "https://example.test/file.pdf"}),
            json!({"bytes": "JVBERi0="}),
            json!({"mail": "m1"}),
            json!({"mail": "m1", "name": "a.pdf", "drive": "/my-files/a.pdf"}),
            json!({"drive": "my-files/relative.pdf"}),
            json!({"upload": "../../etc/passwd"}),
            json!({"kept": "../../etc/passwd"}),
            json!({"kept": "aaaa", "drive": "/my-files/a.pdf"}),
            json!({"mail": "m1", "name": "a\r\nBcc: x@example.test"}),
            json!({"mail": "", "name": "a.pdf"}),
            json!({"repo": "bmabsout/cv"}),
            json!({"repo": "bmabsout/cv", "path": "../etc/passwd"}),
            json!({"repo": "bmabsout/cv", "path": "cv.pdf", "at": "main~1"}),
            json!({"repo": "bmabsout/cv", "path": "cv.pdf", "drive": "/my-files/a.pdf"}),
            json!({"repo": "not a repo", "path": "cv.pdf"}),
        ] {
            assert!(
                AttachmentSource::parse(&wrong).is_err(),
                "{wrong} must not parse"
            );
        }
    }

    #[test]
    fn a_source_round_trips_through_its_stored_form() {
        let source = AttachmentSource::Mail {
            message: "m1@example.test".to_owned(),
            name: "notice.pdf".to_owned(),
        };
        let line = serde_json::to_string(&source).expect("serialises");
        assert_eq!(
            line,
            r#"{"from":"mail","message":"m1@example.test","name":"notice.pdf"}"#
        );
        assert_eq!(
            serde_json::from_str::<AttachmentSource>(&line).expect("reads back"),
            source
        );
    }
}
