use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::capability::{Secretary, Turn};
use crate::held_value::HeldFile;
use crate::refusal::Refusal;
use crate::secretary;
use crate::secretary::attached::{self, Attached};
use crate::secretary::attachment_source::AttachmentSource;
use crate::secretary::opening;
use crate::secretary::read_holders::ReadHolders;
use crate::secretary::store::{Draft, Proposal, Stored};
use crate::secretary::verbs::{FilingArgs, kept, mail_refusal, refused_by};
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

/// The body's held values as the card names them, and the held files they name, which the draft
/// attaches. Refused: a placeholder in the subject, since the tap writes values into the body
/// alone; a placeholder whose handle does not decode; a code; a withheld value or file; and a
/// value or file from a message or file this conversation did not read, so mail or a document
/// the model read cannot have another holder's value or another file sent.
fn held_in(
    read: &ReadHolders,
    subject: &str,
    body: &str,
) -> Result<(Vec<String>, Vec<HeldFile>), Refusal> {
    use crate::held_value::{Handle, Placeholder, Reach, Written};
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
        return Ok((Vec::new(), Vec::new()));
    }
    let mut named = Vec::new();
    let mut files: Vec<HeldFile> = Vec::new();
    for placeholder in body.held() {
        let handle = match (placeholder, placeholder.reach()) {
            (
                Placeholder::Number {
                    kind: Kind::Code, ..
                },
                _,
            ) => {
                return Err(Refusal::bad_request(
                    "a verification code is never sent: the owner types it where it is asked \
                     for",
                ));
            }
            (_, Reach::Withheld) => {
                return Err(Refusal::bad_request(format!(
                    "the body names the {} as withheld, which the box cannot read again; only a \
                     value written `· tap to show · held:…` can be sent",
                    placeholder.described()
                )));
            }
            (_, Reach::Revealable(handle)) => handle,
        };
        if !read.holds(&handle) {
            return Err(Refusal::bad_request(format!(
                "the {} comes from a message or file this conversation has not read; `show` the \
                 message or read the file first",
                placeholder.described()
            )));
        }
        if let Handle::File(file) = handle
            && !files.contains(&file)
        {
            files.push(file);
        }
        named.push(placeholder.described());
    }
    Ok((named, files))
}

/// Refuse a file's placeholder in `draft`'s body that does not print as the file's record does: a
/// name or size the model changed would go on the card and into the sent body.
fn as_recorded(draft: &Draft) -> Result<(), Refusal> {
    use crate::held_value::{Placeholder, Reach, Written};
    let written = Written::parse(&draft.body)
        .map_err(|malformed| Refusal::bad_request(malformed.to_string()))?;
    for placeholder in written.held() {
        let Placeholder::File {
            reach: Reach::Revealable(file),
            ..
        } = placeholder
        else {
            continue;
        };
        let source = file.source();
        let recorded = draft
            .attachments
            .iter()
            .find(|one| one.source == source)
            .map(|one| Placeholder::file(file, &one.name, one.size));
        if recorded.as_ref() != Some(placeholder) {
            return Err(Refusal::bad_request(format!(
                "the body names the {}, which is not how the file reads now ({}); copy its `held` \
                 placeholder whole",
                placeholder.described(),
                recorded.map_or_else(|| "no record".to_owned(), |file| file.described())
            )));
        }
    }
    Ok(())
}

/// `attach`, then each held file the body names that `attach` does not list, each resolved now
/// and recorded with its digest; absent is none.
///
/// # Errors
/// An entry that names no source, a source that answers with nothing, a Drive file outside
/// the allowed folders that this conversation never read, or a total over
/// [`attached::MAX_TOTAL`].
fn attach_of(
    caps: &Secretary<'_>,
    (stored, read): (&Stored, &ReadHolders),
    attach: Option<&Value>,
    held: Vec<HeldFile>,
) -> Result<Vec<Attached>, Refusal> {
    let listed: &[Value] = match attach {
        None => &[],
        Some(Value::Array(listed)) => listed,
        Some(_) => {
            return Err(Refusal::bad_request(
                "attach must be an array of sources, each {\"mail\": <message id>, \"name\": \
                 <filename>}, {\"drive\": <path>}, {\"upload\": <file id>}, {\"kept\": <file id>} \
                 or {\"repo\": <owner/name>, \"path\": <file>, \"at\": <branch, tag or commit>}",
            ));
        }
    };
    let listed = listed
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            AttachmentSource::parse(entry)
                .map(|source| (format!("attach[{index}]"), source))
                .map_err(|failed| Refusal::bad_request(format!("attach[{index}]: {failed}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut attached = Vec::new();
    for (named, source) in &listed {
        attached.push(
            read_now(caps, (stored, read), source.clone()).map_err(|failed| {
                Refusal::new(failed.status(), format!("{named}: {}", failed.message()))
            })?,
        );
    }
    for file in held {
        let source = file.source();
        if listed.iter().all(|(_, listed)| *listed != source) {
            // As the handle names it, unresolved, so the tap's fill finds this record by it.
            attached.push(recorded(caps, read, source).map_err(|failed| {
                Refusal::new(
                    failed.status(),
                    format!(
                        "the held {}: {}",
                        file.source().describe(),
                        failed.message()
                    ),
                )
            })?);
        }
    }
    attached::within_cap(&attached).map_err(|failed| Refusal::bad_request(failed.to_string()))?;
    Ok(attached)
}

/// Read one `attach` entry now and record what it holds.
fn read_now(
    caps: &Secretary<'_>,
    (stored, read): (&Stored, &ReadHolders),
    source: AttachmentSource,
) -> Result<Attached, Refusal> {
    match source {
        AttachmentSource::Mail { message, name } => {
            // The notmuch id, which names the message whether or not triage has marked it.
            let message = caps
                .mail
                .message(&message)
                .map_err(|failed| mail_refusal(&failed))?
                .row
                .id;
            recorded(caps, read, AttachmentSource::Mail { message, name })
        }
        AttachmentSource::Kept { id } => kept_copy(caps, stored, &id),
        AttachmentSource::Repo { repo, path, at } => repo_file(caps, repo, path, &at),
        source @ (AttachmentSource::Drive { .. } | AttachmentSource::Upload { .. }) => {
            recorded(caps, read, source)
        }
    }
}

/// `source` recorded with the name, type, size and digest it reads as now. Two are recorded without
/// a read, so the tap that sends them is their one read: a private upload, from what the upload
/// kept, and a Drive file outside the allowed folders that this conversation read on the owner's
/// tap, from what that tap read. A model's call reads nothing past the fence.
fn recorded(
    caps: &Secretary<'_>,
    read: &ReadHolders,
    source: AttachmentSource,
) -> Result<Attached, Refusal> {
    let unrecorded = |failed: attached::Unattachable| Refusal::bad_request(failed.to_string());
    if let AttachmentSource::Drive { path } = &source
        && !crate::drive::allowed(&caps.drive.allowed_folders(), path)
    {
        let file = HeldFile::Drive { path: path.clone() };
        return match caps.tapped.get(&file)? {
            Some(record) if read.reads_file(&file) => Ok(record),
            Some(_) | None => Err(Refusal::bad_request(format!(
                "{path} is not in a folder the owner allowed and this conversation never read it \
                 on their tap, so it cannot be attached from Drive; drive_read asks them"
            ))),
        };
    }
    if let AttachmentSource::Upload { id } = &source
        && let Some(Upload::Private { media, name, copy }) = caps.uploads.get(id)?
    {
        let name = crate::attachment::called(id, media, Some(&name));
        let id = id.clone();
        return Attached::known(source, &name, media.as_str(), copy.size, id).map_err(unrecorded);
    }
    let opened = opening::open(caps, &source, &ReadHolders::default())?;
    Attached::of(source, &opened.name, &opened.media, &opened.bytes).map_err(unrecorded)
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
    let source = AttachmentSource::Repo {
        repo,
        path,
        at: commit.into(),
    };
    let opened = opening::open(caps, &source, &ReadHolders::default())?;
    let recorded = Attached::of(source, &opened.name, &opened.media, &opened.bytes)
        .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
    // Kept only once recorded, so a refused file leaves nothing behind.
    kept(caps, &opened.bytes).map_err(Refusal::internal)?;
    Ok(recorded)
}

/// A file read into this conversation, recorded from the copy kept then; only a file this
/// conversation holds, since the owner's Allow once is what put it here.
fn kept_copy(
    caps: &Secretary<'_>,
    stored: &Stored,
    id: &crate::attachment::AttachmentId,
) -> Result<Attached, Refusal> {
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
    let source = AttachmentSource::Kept { id: id.clone() };
    let opened = opening::open(caps, &source, &ReadHolders::default())?;
    Attached::of(source, &file.name, file.media.as_str(), &opened.bytes)
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
        the message; you never see it. A HELD FILE (`[file w2.pdf, 48.2 KB · tap \
        to show · held:…]`, the `held` of a file the box withheld from you) goes \
        in the body the same way: the box attaches the file, and the tap writes \
        its name where the placeholder stood. Only a value or file from a message \
        or file this conversation read can be sent. A verification code, a `withheld` \
        placeholder and a placeholder in the subject are refused.";

    fn run(call: &Call<'_>, args: ProposeEmailArgs) -> Result<Value, Refusal> {
        let reasons = args.filing.reasons()?;
        let stored = secretary::read(call.caps.conversations, call.conversation)?;
        let read = ReadHolders::of(&stored);
        let (held, files) = held_in(&read, &args.subject, &args.body)?;
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
            attachments: attach_of(call.caps, (&stored, &read), args.attach.as_ref(), files)?,
        };
        as_recorded(&draft)?;
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
            })).collect::<Vec<_>>(),
            "why": reasons.why(),
            "sources": reasons.sources().iter().map(ToString::to_string).collect::<Vec<_>>(),
            "next": "The owner sees this with its exact text, its reason and its sources as links, \
                     and taps it, dismisses it with a reason, or leaves it. You cannot send it \
                     and there is no verb that can; do not offer to.",
        }))
    }
}
