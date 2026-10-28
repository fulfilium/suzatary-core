use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::held_value::Written;
use crate::mail::Stored;
use crate::refusal::Refusal;
use crate::secretary::attached::MAX_TOTAL;
use crate::secretary::read_mail::ReadMail;
use crate::secretary::store::{Forward, Forwarded, Proposal, Style};
use crate::secretary::verbs::propose_email::{address, addresses};
use crate::secretary::verbs::{FilingArgs, mail_refusal};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Propose forwarding a stored message, which only a tap sends.
pub struct ProposeForward;

#[derive(Deserialize, JsonSchema)]
pub struct ProposeForwardArgs {
    account: String,
    to: String,
    subject: String,
    message: String,
    #[serde(rename = "as")]
    style: Style,
    note: Option<String>,
    cc: Option<String>,
    #[serde(flatten)]
    filing: FilingArgs,
}

impl Verb for ProposeForward {
    type Surface = SecretarySurface;
    type Args = ProposeForwardArgs;
    type Answer = Value;
    const NAME: &'static str = "propose_forward";
    const DESCRIPTION: &'static str = "PROPOSE forwarding one stored message, whole, with its \
        attachments. Like propose_email it is STORED and shown to the owner with its exact \
        recipients, and it is sent only if they tap it; you cannot send.\n\n\
        `message` is the `id` `show` takes (or the hash a todo's source carries). `as` is \
        \"inline\" (its headers and text below your `note`, its attachments attached) or \
        \"attached\" (the message file whole, as forwarded.eml, after your `note`). `subject` \
        is the forward's own, usually \"Fwd: \" and the original's. `account`, `to`, `cc`, \
        `why`, `sources`, `serves` and `revises` are as for propose_email.\n\n\
        A held value in the message (a `[SSN ending 3456 · tap to show · held:…]` placeholder \
        where you read it) is sent as the message holds it: the tap reads the original again, \
        and the card names each one in its text so the owner sees what leaves. You never see \
        the value, and you must not copy a placeholder into `note` or `subject`. The tap \
        sends the message as it was when proposed, or nothing. Only a message this \
        conversation read can be forwarded, so `show` it first; `note` and `subject` take no \
        placeholder. A message over 20 MB cannot be forwarded.";

    fn run(call: &Call<'_>, args: ProposeForwardArgs) -> Result<Value, Refusal> {
        let reasons = args.filing.reasons()?;
        let note = args.note.unwrap_or_default();
        let to = addresses(&args.to, "to")?;
        if to.is_empty() {
            return Err(Refusal::bad_request("to must name at least one recipient"));
        }
        for (field, text) in [("subject", &args.subject), ("note", &note)] {
            let written = Written::parse(text)
                .map_err(|malformed| Refusal::bad_request(malformed.to_string()))?;
            if let Some(placeholder) = written.held().next() {
                return Err(Refusal::bad_request(format!(
                    "the {field} names the {}: a forward's own words take no held value, since \
                     the message it forwards carries its values",
                    placeholder.described()
                )));
            }
        }
        let mail = call.caps.mail;
        let shown = mail
            .message(&args.message)
            .map_err(|failed| mail_refusal(&failed))?;
        if !ReadMail::of(&crate::secretary::read(
            call.caps.conversations,
            call.conversation,
        )?)
        .reads(&shown.row.id)
        {
            return Err(Refusal::bad_request(
                "only a message this conversation read can be forwarded; `show` it first",
            ));
        }
        let file = match mail
            .stored(&shown.row.id, MAX_TOTAL)
            .map_err(|failed| mail_refusal(&failed))?
        {
            Stored::TooLarge { size } => {
                return Err(Refusal::bad_request(format!(
                    "the message's file is {size} bytes; a forward carries at most {MAX_TOTAL} \
                     (20 MB)"
                )));
            }
            Stored::Recorded(file) => file,
        };
        let forward = Forward {
            account: address(&args.account, "account")?,
            to,
            cc: args
                .cc
                .as_deref()
                .map_or(Ok(Vec::new()), |list| addresses(list, "cc"))?,
            subject: args.subject,
            note,
            message: Forwarded {
                file,
                style: args.style,
                from: shown.row.sender,
                subject: shown.row.subject,
                date: shown.row.date.map(crate::capability::Datetime::isoformat),
            },
        };
        let index = args
            .filing
            .file(call, Proposal::Forward(forward.clone()), &reasons)?;
        let original = &forward.message;
        Ok(json!({
            "proposed": index,
            "to": forward.to,
            "cc": forward.cc,
            "subject": forward.subject,
            "forwards": {
                "from": original.from,
                "subject": original.subject,
                "as": original.style.as_str(),
                "held_in_its_text": original.file.held.iter().map(crate::held_value::Held::said).collect::<Vec<_>>(),
                "attachments": original.file.parts.iter().map(|part| json!({
                    "name": part.name,
                    "media": part.media,
                    "size": part.size,
                })).collect::<Vec<_>>(),
            },
            "why": reasons.why(),
            "sources": reasons.sources().iter().map(ToString::to_string).collect::<Vec<_>>(),
            "next": "The owner sees the forward with its recipients, the original it carries and \
                     every held value in its text, and taps it, dismisses it with a reason, or \
                     leaves it. You cannot send it and there is no verb that can; do not offer to.",
        }))
    }
}
