use crate::drive::Port as _;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::capability::{Secretary, Turn};
use crate::enclosed::Enclosure;
use crate::refusal::Refusal;
use crate::secretary;
use crate::secretary::attached::{self, Attached};
use crate::secretary::attachment_source::AttachmentSource;
use crate::secretary::store::{ConversationId, Draft, Proposal};
use crate::secretary::verbs::{FilingArgs, drive_refusal, kept, mail_refusal, refused_by};
use crate::secretary::{Call, SecretarySurface};
use crate::upload::Upload;
use crate::verb::Verb;

/// Propose a message, which only a tap sends.
pub struct ProposeEmail;

#[derive(Deserialize, JsonSchema)]
pub struct ProposeEmailArgs {
    account: String,
    to: String,
    subject: String,
    body: String,
    cc: Option<String>,
    #[schemars(with = "Option<Vec<Value>>")]
    attach: Option<Value>,
    #[serde(flatten)]
    filing: FilingArgs,
}

/// One address, shaped by [`crate::mail::address::EmailAddress`], refused with its field name.
pub(super) fn address(text: &str, field: &str) -> Result<String, Refusal> {
    crate::mail::address::EmailAddress::new(text)
        .map(|address| address.as_str().to_owned())
        .map_err(|_| Refusal::bad_request(format!("{field} must be an email address")))
}

/// A comma-separated list of addresses.
pub(super) fn addresses(list: &str, field: &str) -> Result<Vec<String>, Refusal> {
    list.split(',')
        .map(str::trim)
        .filter(|one| !one.is_empty())
        .map(|one| address(one, field))
        .collect()
}

/// What the body's held values are, as the card names them. Refused: a placeholder in the
/// subject, since the tap writes values into the body alone; a placeholder whose handle does not
/// decode; a code; a withheld value; and a value from a message this conversation did not read,
/// so mail the model read cannot have another message's value sent.
fn held_in(
    caps: &Secretary<'_>,
    conversation: &ConversationId,
    subject: &str,
    body: &str,
) -> Result<Vec<String>, Refusal> {
    use crate::held_value::{Reach, Written};
    use crate::redact::Kind;
    let written = |text: &str| {
        Written::parse(text).map_err(|malformed| Refusal::bad_request(malformed.to_string()))
    };
    if let Some(placeholder) = written(subject)?.held().next() {
        return Err(Refusal::bad_request(format!(
            "the subject names the {}: a held value goes in the body, where the owner's tap \
             writes it in; the subject is sent as written",
            placeholder.described()
        )));
    }
    let body = written(body)?;
    if body.held().next().is_none() {
        return Ok(Vec::new());
    }
    let read = crate::secretary::read_mail::ReadMail::of(&secretary::read(
        caps.conversations,
        conversation,
    )?);
    body.held()
        .map(|placeholder| match (placeholder.kind(), placeholder.reach()) {
            (Kind::Code, _) => Err(Refusal::bad_request(
                "a verification code is never sent: the owner types it where it is asked for",
            )),
            (_, Reach::Withheld) => Err(Refusal::bad_request(format!(
                "the body names the {} as withheld, which the box cannot read again; only a \
                 value written `· tap to show · held:…` can be sent",
                placeholder.described()
            ))),
            (_, Reach::Revealable(handle)) if !read.holds(handle) => {
                Err(Refusal::bad_request(format!(
                    "the {} comes from a message this conversation has not read; `show` it first",
                    placeholder.described()
                )))
            }
            (_, Reach::Revealable(_)) => Ok(placeholder.described()),
        })
        .collect()
}

/// `attach`, each entry resolved now and recorded with its digest; absent is none.
///
/// # Errors
/// An entry that names no source, a source that answers with nothing, a Drive file outside
/// the allowed folders, or a total over [`attached::MAX_TOTAL`].
fn attach_of(
    caps: &Secretary<'_>,
    conversation: &ConversationId,
    attach: Option<&Value>,
) -> Result<Vec<Attached>, Refusal> {
    let listed = match attach {
        None => return Ok(Vec::new()),
        Some(Value::Array(listed)) => listed,
        Some(_) => {
            return Err(Refusal::bad_request(
                "attach must be an array of sources, each {\"mail\": <message id>, \"name\": \
                 <filename>}, {\"drive\": <path>}, {\"upload\": <file id>}, {\"kept\": <file id>} \
                 or {\"repo\": <owner/name>, \"path\": <file>, \"at\": <branch, tag or commit>}",
            ));
        }
    };
    let attached = listed
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let source = AttachmentSource::parse(entry)
                .map_err(|failed| Refusal::bad_request(format!("attach[{index}]: {failed}")))?;
            read_now(caps, conversation, source).map_err(|failed| {
                Refusal::new(
                    failed.status(),
                    format!("attach[{index}]: {}", failed.message()),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    attached::within_cap(&attached).map_err(|failed| Refusal::bad_request(failed.to_string()))?;
    Ok(attached)
}

/// Read one source now and record what it holds.
fn read_now(
    caps: &Secretary<'_>,
    conversation: &ConversationId,
    source: AttachmentSource,
) -> Result<Attached, Refusal> {
    let recorded = |source: AttachmentSource, name: &str, media: &str, bytes: &[u8]| {
        Attached::of(source, name, media, bytes)
            .map_err(|failed| Refusal::bad_request(failed.to_string()))
    };
    match source {
        AttachmentSource::Mail { message, name } => {
            // The notmuch id, which names the message whether or not triage has marked it.
            let message = caps
                .mail
                .message(&message)
                .map_err(|failed| mail_refusal(&failed))?
                .row
                .id;
            let file = caps
                .mail
                .attachment(&message, &name)
                .map_err(|failed| mail_refusal(&failed))?;
            let media = if file.mime.is_empty() {
                crate::mail::send::guess_mime_type(&name)
            } else {
                file.mime
            };
            recorded(
                AttachmentSource::Mail {
                    message,
                    name: name.clone(),
                },
                &name,
                &media,
                &file.content,
            )
        }
        AttachmentSource::Drive { path } => {
            if !crate::drive::allowed(&caps.drive.allowed_folders(), &path) {
                return Err(Refusal::bad_request(format!(
                    "{path} is not in a folder the owner allowed, so it cannot be attached from \
                     Drive; drive_read asks them, and once they allow it the copy read into this \
                     conversation attaches as {{\"kept\": <its file id>}}"
                )));
            }
            let name = path.name().unwrap_or_default().to_owned();
            let bytes = caps
                .drive
                .fenced()
                .fetch(&path)
                .map_err(|failed| drive_refusal(&failed))?;
            let media = crate::mail::send::guess_mime_type(&name);
            recorded(AttachmentSource::Drive { path }, &name, &media, &bytes)
        }
        AttachmentSource::Kept { id } => kept_copy(caps, conversation, &id),
        AttachmentSource::Repo { repo, path, at } => repo_file(caps, repo, path, &at),
        AttachmentSource::Upload { id } => {
            let upload = caps
                .uploads
                .get(&id)?
                .ok_or_else(|| Refusal::not_found(format!("no file {id} was uploaded")))?;
            let (media, bytes) = match upload {
                Upload::Here { media, bytes } => (media, bytes),
                // Recorded from what the upload kept, never read: the tap that sends it is
                // the one read.
                Upload::Private { media, name, copy } => {
                    return Attached::known(
                        AttachmentSource::Upload { id: id.clone() },
                        &name,
                        media.as_str(),
                        copy.size,
                        id,
                    )
                    .map_err(|failed| Refusal::bad_request(failed.to_string()));
                }
            };
            let name = match media {
                Enclosure::Pdf => format!("file-{}.pdf", &id.as_str()[..12]),
                Enclosure::Image(image) => format!(
                    "photo-{}.{}",
                    &id.as_str()[..12],
                    image.as_str().trim_start_matches("image/")
                ),
            };
            recorded(
                AttachmentSource::Upload { id },
                &name,
                media.as_str(),
                &bytes,
            )
        }
    }
}

/// A file of a declared repository at the commit `at` resolves to, recorded and then kept by its
/// digest, so the tap sends these bytes whatever the branch does next.
fn repo_file(
    caps: &Secretary<'_>,
    repo: crate::repos::RepoName,
    path: crate::repos::RepoPath,
    at: &crate::repos::Rev,
) -> Result<Attached, Refusal> {
    let commit = caps
        .repositories
        .resolve(&repo, Some(at))
        .map_err(|failed| refused_by(&failed))?;
    let bytes = caps
        .repositories
        .blob(&repo, &commit, &path)
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
    let media = crate::mail::send::guess_mime_type(&name);
    let recorded = Attached::of(
        AttachmentSource::Repo {
            repo,
            path,
            at: commit.into(),
        },
        &name,
        &media,
        &bytes,
    )
    .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
    // Kept only once recorded, so a refused file leaves nothing behind.
    kept(caps, &bytes).map_err(Refusal::internal)?;
    Ok(recorded)
}

/// A file read into this conversation, recorded from the copy kept then; only a file this
/// conversation holds, since the owner's Allow once is what put it here.
fn kept_copy(
    caps: &Secretary<'_>,
    conversation: &ConversationId,
    id: &crate::attachment::AttachmentId,
) -> Result<Attached, Refusal> {
    let stored = secretary::read(caps.conversations, conversation)?;
    let file = stored
        .turns
        .iter()
        .find_map(|said| match &said.turn {
            Turn::Tool { files, .. } => files.iter().find(|file| &file.id == id),
            _ => None,
        })
        .ok_or_else(|| {
            Refusal::not_found(format!(
                "no file {} was read into this conversation; drive_read asks the owner, and their \
                 Allow once keeps the copy it reads",
                &id.as_str()[..12]
            ))
        })?;
    let bytes = caps.kept.get(id)?.ok_or_else(|| {
        Refusal::not_found(format!("the copy of {} is gone from the box", file.name))
    })?;
    Attached::of(
        AttachmentSource::Kept { id: id.clone() },
        &file.name,
        file.media.as_str(),
        &bytes,
    )
    .map_err(|failed| Refusal::bad_request(failed.to_string()))
}

impl Verb for ProposeEmail {
    type Surface = SecretarySurface;
    type Args = ProposeEmailArgs;
    type Answer = Value;
    const NAME: &'static str = "propose_email";
    const DESCRIPTION: &'static str = "PROPOSE a message. It is STORED in this conversation and shown to \
        The owner with its exact recipients, subject and body, and it is sent only \
        if they tap it. You cannot send and there is no verb that does — that is \
        the design, not a limitation to work around, so write the message you \
        would send rather than describing one.\n\n\
        `account` is the address to send AS and must be one this box can send \
        from. Plain text, comma-chained and brief, never em-dashes, one ask per \
        message — SECRETARY.md has the voice.\n\n\
        ⚠️ `why` AND `sources` ARE REQUIRED, and they are the half of the card \
        The owner reads first (2026-09-10). `why` is one or two sentences: what \
        this message is FOR and what in the sources led to it. `sources` is a \
        comma-separated list of references — `mail:<hash>`, `thread:<id>`, \
        `todo:<chain>/<id>`, `briefing:<YYYY-MM-DD>`, \
        `proposal:<conversation>/<n>` — naming at least one thing this rests \
        on. If you cannot name what you read, you are guessing, and a guess \
        addressed to somebody's counsel is the one thing this surface exists to \
        prevent.\n\n\
        ⚠️ IT WAITS IN THEIR INBOX until they tap or dismiss it. `serves` is the \
        todo it is for, as `todo:<chain>/<id>`, when there is one. `revises` is an \
        earlier proposal's index ON THIS SAME CONVERSATION: use it when you are \
        replacing a standing proposal rather than adding a second one for the \
        same question — it cancels the old one and marks it superseded. The \
        same message to the same people is the same proposal: proposing it \
        again files nothing new and it is never sent twice.\n\n\
        `attach` is an ARRAY of files to send with it, each ONE of: \
        {\"mail\": <the message id `attachments` took>, \"name\": <the \
        attachment's name>}, {\"drive\": <a path in a folder the owner \
        allowed>}, {\"upload\": <the id of a photo they sent in a \
        conversation>}, or {\"repo\": <owner/name>, \"path\": <a file in it>, \
        \"at\": <a branch, tag or commit; omit for the default branch>} for a file \
        of one of their repositories, a PDF such as their CV included. Only their own things can be attached: nothing from the \
        web and nothing you wrote. Each is read now and recorded with its name, \
        type, size and SHA-256, the card shows every one, and the tap sends \
        exactly those bytes or nothing. At most 20 MB in all.\n\n\
        A HELD VALUE (an SSN, a card or bank number you read as \
        `[SSN ending 3456 · tap to show · held:…]`) goes in the BODY as that \
        placeholder, copied whole, where the number belongs. The card tells the \
        owner it is included, and only their tap writes the real number into \
        the message; you never see it. Only a value from a message this \
        conversation read can be sent. A verification code, a `withheld` \
        placeholder and a placeholder in the subject are refused.";

    fn run(call: &Call<'_>, args: ProposeEmailArgs) -> Result<Value, Refusal> {
        let reasons = args.filing.reasons()?;
        let held = held_in(call.caps, call.conversation, &args.subject, &args.body)?;
        let draft = Draft {
            account: address(&args.account, "account")?,
            to: addresses(&args.to, "to")?,
            cc: match args.cc.as_deref() {
                None => Vec::new(),
                Some(list) if list.trim().is_empty() => Vec::new(),
                Some(list) => addresses(list, "cc")?,
            },
            subject: args.subject,
            body: args.body,
            attachments: attach_of(call.caps, call.conversation, args.attach.as_ref())?,
        };
        if draft.to.is_empty() {
            return Err(Refusal::bad_request("to must name at least one recipient"));
        }
        let index = args
            .filing
            .file(call, Proposal::Email(draft.clone()), &reasons)?;
        Ok(json!({
            "proposed": index,
            "to": draft.to,
            "cc": draft.cc,
            "subject": draft.subject,
            "held": held,
            "attachments": draft.attachments.iter().map(|one| json!({
                "name": one.name,
                "media": one.media,
                "size": one.size,
                "from": one.source.describe(),
                "sha256": one.sha256.as_str(),
            })).collect::<Vec<_>>(),
            "why": reasons.why(),
            "sources": reasons.sources().iter().map(ToString::to_string).collect::<Vec<_>>(),
            "next": "The owner sees this with its exact text, its reason and its sources as links, \
                     and taps it, dismisses it with a reason, or leaves it. You cannot send it \
                     and there is no verb that can; do not offer to.",
        }))
    }
}
