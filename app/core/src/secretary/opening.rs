//! The one reading of a file the box names by its source: what a proposal records, what the tap
//! sends and what a reveal answers all come from [`open`].

use crate::capability::Secretary;
use crate::drive::Port as _;
use crate::enclosed::Enclosure;
use crate::held_value::{Files, HeldFile, Opened};
use crate::refusal::Refusal;
use crate::secretary::attachment_source::AttachmentSource;
use crate::secretary::read_holders::ReadHolders;
use crate::secretary::verbs::{drive_refusal, mail_refusal, refused_by};
use crate::upload::Upload;

/// The file `file` names, as its source holds it now. A Drive file is read through the fence
/// while its folder is allowed, and otherwise only when a file reader of this conversation
/// answered it (`read`): the owner's tap on Read put it there. A private upload is read from the
/// owner's Drive, which only a tap does.
///
/// # Errors
/// The source's refusal: a missing file, a Drive file outside the allowed folders that this
/// conversation never read, a Git LFS pointer, or the source's own failure.
pub fn open(
    caps: &Secretary<'_>,
    file: &AttachmentSource,
    read: &ReadHolders,
) -> Result<Opened, Refusal> {
    match file {
        AttachmentSource::Mail { message, name } => {
            let file = caps
                .mail
                .attachment(message, name)
                .map_err(|failed| mail_refusal(&failed))?;
            Ok(Opened {
                media: if file.mime.is_empty() {
                    crate::mail::send::guess_mime_type(name)
                } else {
                    file.mime
                },
                name: name.clone(),
                bytes: file.content,
            })
        }
        AttachmentSource::Drive { path } => {
            let bytes = if crate::drive::allowed(&caps.drive.allowed_folders(), path) {
                caps.drive.fenced().fetch(path)
            } else if read.reads_file(&HeldFile::Drive { path: path.clone() }) {
                caps.drive.port().fetch(path)
            } else {
                return Err(Refusal::bad_request(format!(
                    "{path} is not in a folder the owner allowed and this conversation never read \
                     it, so it cannot be attached from Drive; drive_read asks them, and once they \
                     allow it the copy read into this conversation attaches"
                )));
            }
            .map_err(|failed| drive_refusal(&failed))?;
            let name = path.name().unwrap_or_default().to_owned();
            Ok(Opened {
                media: crate::mail::send::guess_mime_type(&name),
                name,
                bytes,
            })
        }
        AttachmentSource::Upload { id } => upload(caps, id),
        AttachmentSource::Kept { id } => {
            let bytes = caps.kept.get(id)?.ok_or_else(|| {
                Refusal::not_found(format!(
                    "the copy of {} is gone from the box",
                    &id.as_str()[..12]
                ))
            })?;
            Ok(Opened {
                name: format!("file-{}", &id.as_str()[..12]),
                media: Enclosure::sniff(&bytes)
                    .map_or("application/octet-stream", Enclosure::as_str)
                    .to_owned(),
                bytes,
            })
        }
        AttachmentSource::Repo { repo, path, at } => repository(caps, repo, path, at),
    }
}

/// An upload, on the box or in the owner's Drive.
fn upload(caps: &Secretary<'_>, id: &crate::attachment::AttachmentId) -> Result<Opened, Refusal> {
    match caps
        .uploads
        .get(id)?
        .ok_or_else(|| Refusal::not_found(format!("no file {id} was uploaded")))?
    {
        Upload::Here { media, bytes } => Ok(Opened {
            name: crate::attachment::called(id, media, None),
            media: media.as_str().to_owned(),
            bytes,
        }),
        Upload::Private { media, name, copy } => Ok(Opened {
            name: crate::attachment::called(id, media, Some(&name)),
            media: media.as_str().to_owned(),
            bytes: caps
                .drive
                .port()
                .fetch(&copy.path)
                .map_err(|failed| drive_refusal(&failed))?,
        }),
    }
}

/// A file of a declared repository at the commit `at` resolves to.
fn repository(
    caps: &Secretary<'_>,
    repo: &crate::repos::RepoName,
    path: &crate::repos::RepoPath,
    at: &crate::repos::Rev,
) -> Result<Opened, Refusal> {
    let commit = caps
        .repositories
        .resolve(repo, Some(at))
        .map_err(|failed| refused_by(&failed))?;
    let bytes = caps
        .repositories
        .blob(repo, &commit, path)
        .map_err(|failed| refused_by(&failed))?;
    // A Git LFS file is stored as a pointer; its bytes are elsewhere, and the pointer is not the
    // file its name and type would claim.
    if bytes.starts_with(b"version https://git-lfs.github.com/spec/") {
        return Err(Refusal::bad_request(format!(
            "{} in {repo} is a Git LFS pointer, not the file; the box does not fetch LFS objects, \
             so it cannot be attached",
            path.as_str()
        )));
    }
    let name = path
        .as_str()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    Ok(Opened {
        media: crate::mail::send::guess_mime_type(&name),
        name,
        bytes,
    })
}

/// The held files as one conversation's tap reads them: through [`open`], and the record's PDF
/// reader.
pub struct Opening<'a, 'b> {
    /// The record whose sources are read.
    pub caps: &'b Secretary<'a>,
    /// What the conversation read, which opens a Drive file outside the allowed folders.
    pub read: &'b ReadHolders,
}

impl Files for Opening<'_, '_> {
    fn open(&self, file: &HeldFile) -> Result<Option<Opened>, String> {
        match open(self.caps, &file.source(), self.read) {
            Ok(opened) => Ok(Some(opened)),
            Err(refused) if refused.status() == crate::refusal::Status::NotFound => Ok(None),
            Err(refused) => Err(refused.message().to_owned()),
        }
    }

    fn layer(&self, pdf: &[u8]) -> Result<String, String> {
        self.caps.pdf.text(pdf)
    }
}
