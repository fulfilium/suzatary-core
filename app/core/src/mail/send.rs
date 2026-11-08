//! The outgoing message, pure: values, MIME building, threading and pre-send guards.
//!
//! No transport lives here, and there is no Bcc, so the envelope names exactly the
//! people the headers do.

use crate::message_part::Attachment;
use mail_builder::MessageBuilder;

use super::address::EmailAddress;

/// One message to send, plain text only. [`Outgoing::new`] is its one constructor, so every
/// value has at least one recipient, a subject and something to say, and each address on it
/// is an [`EmailAddress`].
#[derive(Clone, PartialEq, Eq)]
pub struct Outgoing {
    sender: EmailAddress,
    to: Vec<EmailAddress>,
    cc: Vec<EmailAddress>,
    subject: String,
    body: String,
    attachments: Vec<Attachment>,
    in_reply_to: Option<String>,
    references: Vec<String>,
}

/// A message as it was written, before [`Outgoing::new`] checks it.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Composed {
    /// An address this deployment may send as.
    pub sender: String,
    /// The visible recipients.
    pub to: Vec<String>,
    /// The visible copies.
    pub cc: Vec<String>,
    /// The subject.
    pub subject: String,
    /// The body, as text.
    pub body: String,
    /// Files to attach.
    pub attachments: Vec<Attachment>,
    /// The bare `Message-Id` this replies to, with no angle brackets.
    pub in_reply_to: Option<String>,
    /// The chain, bare; defaults to just the parent so the reply stays threaded.
    pub references: Vec<String>,
}

/// A body's length in place of the body: a tapped body holds the owner's held values.
struct Withheld(usize);

impl std::fmt::Debug for Withheld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{} bytes>", self.0)
    }
}

impl std::fmt::Debug for Outgoing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outgoing")
            .field("sender", &self.sender)
            .field("to", &self.to)
            .field("cc", &self.cc)
            .field("subject", &self.subject)
            .field("body", &Withheld(self.body.len()))
            .field("attachments", &self.attachments.len())
            .field("in_reply_to", &self.in_reply_to)
            .field("references", &self.references)
            .finish()
    }
}

impl std::fmt::Debug for Composed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Composed")
            .field("sender", &self.sender)
            .field("to", &self.to)
            .field("cc", &self.cc)
            .field("subject", &self.subject)
            .field("body", &Withheld(self.body.len()))
            .field("attachments", &self.attachments.len())
            .field("in_reply_to", &self.in_reply_to)
            .field("references", &self.references)
            .finish()
    }
}

impl Outgoing {
    /// Check a written message.
    ///
    /// # Errors
    /// [`SendError::Refused`] with the one-line reason: no recipients, an address that is not
    /// one (every such address named), an empty subject, or an empty body with no attachment.
    pub fn new(composed: Composed) -> Result<Self, SendError> {
        let Composed {
            sender,
            to,
            cc,
            subject,
            body,
            attachments,
            in_reply_to,
            references,
        } = composed;
        if to.is_empty() {
            return Err(SendError::Refused("no recipients".to_owned()));
        }
        let mut bad = Vec::new();
        let to = addresses(&to, &mut bad);
        let cc = addresses(&cc, &mut bad);
        let sender = match (EmailAddress::new(&sender), bad.is_empty()) {
            (Ok(sender), true) => sender,
            (Ok(_), false) => return Err(not_addresses(&bad)),
            (Err(_), _) => {
                bad.push(sender);
                return Err(not_addresses(&bad));
            }
        };
        if subject.trim().is_empty() {
            return Err(SendError::Refused("empty subject".to_owned()));
        }
        if body.trim().is_empty() && attachments.is_empty() {
            return Err(SendError::Refused(
                "empty body and no attachments".to_owned(),
            ));
        }
        Ok(Self {
            sender,
            to,
            cc,
            subject,
            body,
            attachments,
            in_reply_to,
            references,
        })
    }

    /// Who it is from.
    #[must_use]
    pub fn sender(&self) -> &EmailAddress {
        &self.sender
    }

    /// The subject.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// The envelope recipients: `to + cc`, and nothing else.
    #[must_use]
    pub fn recipients(&self) -> Vec<String> {
        self.to
            .iter()
            .chain(self.cc.iter())
            .map(|address| address.as_str().to_owned())
            .collect()
    }

    /// The message as it was written, for a caller that writes it again.
    #[must_use]
    pub fn composed(&self) -> Composed {
        let texts =
            |addresses: &[EmailAddress]| addresses.iter().map(ToString::to_string).collect();
        Composed {
            sender: self.sender.to_string(),
            to: texts(&self.to),
            cc: texts(&self.cc),
            subject: self.subject.clone(),
            body: self.body.clone(),
            attachments: self.attachments.clone(),
            in_reply_to: self.in_reply_to.clone(),
            references: self.references.clone(),
        }
    }
}

/// Each of `written` that is an address, every one that is not pushed onto `bad`.
fn addresses(written: &[String], bad: &mut Vec<String>) -> Vec<EmailAddress> {
    written
        .iter()
        .filter_map(|address| {
            EmailAddress::new(address)
                .inspect_err(|_| bad.push(address.clone()))
                .ok()
        })
        .collect()
}

/// The refusal naming every text that is not an address.
fn not_addresses(bad: &[String]) -> SendError {
    SendError::Refused(format!("not an address: {}", bad.join(", ")))
}

/// A message could not be built or sent.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SendError {
    /// A pre-send guard refused.
    #[error("{0}")]
    Refused(String),
    /// The `From` is not the account that authenticated.
    #[error("From ({from}) is not the authenticated account ({account})")]
    WrongIdentity {
        /// What the message said.
        from: String,
        /// Who the transport is.
        account: String,
    },
    /// The transport refused or failed at `step` before the far end held the message.
    #[error("{step}: {why}")]
    Transport {
        /// Where.
        step: Step,
        /// What it said.
        why: String,
    },
    /// The message was handed over and nothing answered for it; the far end may hold it.
    #[error("the message was handed over and no answer came: {0}")]
    Unanswered(String),
    /// The server took the message for some recipients and not others.
    #[error("some recipients refused: {0:?}")]
    RecipientsRefused(Vec<String>),
    /// The account holds no grant to send, so nothing left the box.
    #[error("{}", .0.sentence())]
    Ungranted(crate::connect::Lacking),
    /// The MIME could not be written, which is a bug rather than a refusal.
    #[error("the message could not be built: {0}")]
    Unbuildable(String),
}

/// Where in a send a transport failed, in protocol order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// Reading the transport's own grant or certificate off disk.
    Credentials,
    /// The TCP connection, the greeting and the first EHLO.
    Connect,
    /// STARTTLS, the handshake and the EHLO over it.
    Tls,
    /// AUTH, or a token exchange.
    Auth,
    /// MAIL FROM.
    Mail,
    /// RCPT TO.
    Rcpt,
    /// DATA, up to its go-ahead.
    Data,
    /// The message itself, the final dot and the server's answer for it.
    Message,
}

impl std::fmt::Display for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Step::Credentials => "credentials",
            Step::Connect => "connect",
            Step::Tls => "tls",
            Step::Auth => "auth",
            Step::Mail => "mail from",
            Step::Rcpt => "rcpt to",
            Step::Data => "data",
            Step::Message => "message",
        })
    }
}

/// What a failed send says about the far end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Nothing was accepted, so sending again sends it once.
    NotSent,
    /// It may have been accepted, so sending again may send it twice.
    Unknown,
}

impl SendError {
    /// Whether the far end may hold the message, decided by the variant, never its words.
    #[must_use]
    pub fn delivery(&self) -> Delivery {
        match self {
            SendError::Refused(_)
            | SendError::WrongIdentity { .. }
            | SendError::Transport { .. }
            | SendError::Ungranted(_)
            | SendError::Unbuildable(_) => Delivery::NotSent,
            SendError::Unanswered(_) | SendError::RecipientsRefused(_) => Delivery::Unknown,
        }
    }
}

/// A message that went out.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Sent {
    /// The `Message-Id`, in angle brackets as it went on the wire.
    pub message_id: String,
    /// Who it went to.
    pub recipients: Vec<String>,
    /// How big it was.
    pub bytes_sent: usize,
}

/// A `Message-Id` travels bare in these values and bracketed on the wire; this is
/// where the two spellings meet.
#[must_use]
pub fn bare(message_id: &str) -> &str {
    message_id
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
}

/// The bracketed form, for a header this module writes itself.
///
/// Not for ids handed to the builder, which brackets them itself.
#[must_use]
pub fn angle(message_id: &str) -> String {
    format!("<{}>", bare(message_id))
}

/// `In-Reply-To` / `References` for a reply, empty for a message that replies to nothing.
#[must_use]
pub(crate) fn thread_headers(out: &Outgoing) -> Vec<(String, String)> {
    let Some(parent) = &out.in_reply_to else {
        return Vec::new();
    };
    let refs: Vec<String> = if out.references.is_empty() {
        vec![parent.clone()]
    } else {
        out.references.clone()
    };
    vec![
        ("In-Reply-To".to_owned(), angle(parent)),
        (
            "References".to_owned(),
            refs.iter()
                .map(|one| angle(one))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    ]
}

/// Render an [`Outgoing`] to MIME, with the id and `date` (epoch seconds) handed in.
///
/// # Errors
/// [`SendError::Unbuildable`], which is a bug rather than a refusal.
///
/// A message with attachments carries a pseudo-random multipart boundary.
pub fn build_mime(out: &Outgoing, message_id: &str, date: i64) -> Result<Vec<u8>, SendError> {
    let to: Vec<&str> = out.to.iter().map(EmailAddress::as_str).collect();
    let cc: Vec<&str> = out.cc.iter().map(EmailAddress::as_str).collect();
    let mut builder = MessageBuilder::new()
        .from(out.sender.as_str())
        .to(to)
        .subject(out.subject.as_str())
        .date(date)
        // Bare: the builder brackets it.
        .message_id(bare(message_id));
    if !cc.is_empty() {
        builder = builder.cc(cc);
    }
    // Written as raw header text because the builder would bracket the joined
    // `References` list as one id.
    for (name, value) in thread_headers(out) {
        builder = builder.header(name, mail_builder::headers::raw::Raw::new(value));
    }
    // `text_body`, not an explicit body, which would drop the attachments.
    builder = builder.text_body(out.body.as_str());
    for attachment in &out.attachments {
        builder = match whole_message(attachment) {
            Some(part) => {
                builder.attachments.get_or_insert_with(Vec::new).push(part);
                builder
            }
            None => builder.attachment(
                attachment.mime_type.as_str(),
                attachment.filename.as_str(),
                attachment.content.as_slice(),
            ),
        };
    }
    builder
        .write_to_vec()
        .map_err(|failed| SendError::Unbuildable(failed.to_string()))
}

/// A `message/rfc822` attachment as RFC 2046 §5.2.1 wants it, in 7bit or 8bit rather than
/// base64, its lines ended by CRLF; `None` for any other attachment, and for a message with a
/// NUL or a line over 998 octets, which neither encoding carries and base64 still does.
fn whole_message(attachment: &Attachment) -> Option<mail_builder::mime::MimePart<'_>> {
    if !attachment.mime_type.eq_ignore_ascii_case("message/rfc822") {
        return None;
    }
    let mut lines = Vec::with_capacity(attachment.content.len());
    for line in attachment.content.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() > 998 || line.contains(&0) {
            return None;
        }
        lines.extend_from_slice(line);
        lines.extend_from_slice(b"\r\n");
    }
    if attachment.content.ends_with(b"\n") {
        lines.truncate(lines.len() - 2);
    }
    let encoding = if lines.is_ascii() { "7bit" } else { "8bit" };
    Some(
        mail_builder::mime::MimePart::new(
            attachment.mime_type.as_str(),
            mail_builder::mime::BodyPart::Binary(lines.into()),
        )
        .attachment(attachment.filename.as_str())
        .transfer_encoding(encoding),
    )
}

/// The MIME type to guess for an attachment, from a short table then a default.
#[must_use]
pub fn guess_mime_type(filename: &str) -> String {
    let extension = filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "pdf" => "application/pdf",
        "txt" | "text" | "typ" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composed() -> Composed {
        Composed {
            sender: "her@example.test".to_owned(),
            to: vec!["him@example.test".to_owned()],
            subject: "Re: the thing".to_owned(),
            body: "One line.\n".to_owned(),
            ..Composed::default()
        }
    }

    fn outgoing() -> Outgoing {
        Outgoing::new(composed()).expect("a sendable message")
    }

    fn refused(composed: Composed) -> Option<String> {
        match Outgoing::new(composed) {
            Ok(_) => None,
            Err(SendError::Refused(reason)) => Some(reason),
            Err(other) => Some(format!("not a refusal: {other}")),
        }
    }

    fn text(out: &Outgoing, message_id: &str) -> String {
        // 2026-09-08T00:00:00Z.
        String::from_utf8(build_mime(out, message_id, 1_788_912_000).expect("builds"))
            .expect("utf-8")
    }

    #[test]
    fn a_plain_message_carries_the_headers_it_was_given() {
        let built = text(&outgoing(), "abc@example.test");
        assert!(built.contains("From: <her@example.test>"), "{built}");
        assert!(built.contains("To: <him@example.test>"), "{built}");
        assert!(built.contains("Subject: Re: the thing"), "{built}");
        // Exactly one pair of brackets.
        assert!(built.contains("Message-ID: <abc@example.test>"), "{built}");
        assert!(!built.contains("<<"), "{built}");
        assert!(built.contains("Date: "), "{built}");
        assert!(built.contains("One line."), "{built}");
        assert!(built.contains("text/plain"), "{built}");
        assert!(!built.contains("text/html"), "{built}");
    }

    #[test]
    fn a_cc_appears_and_an_absent_one_does_not() {
        assert!(!text(&outgoing(), "a@x.test").contains("Cc:"));
        let out = Outgoing::new(Composed {
            cc: vec!["third@example.test".to_owned()],
            ..composed()
        })
        .expect("a sendable message");
        let built = text(&out, "a@x.test");
        assert!(built.contains("Cc: <third@example.test>"), "{built}");
        assert_eq!(
            out.recipients(),
            vec![
                "him@example.test".to_owned(),
                "third@example.test".to_owned()
            ]
        );
    }

    // --- The constructor's guards -------------------------------------------

    #[test]
    fn every_guard_refuses_the_mistake_it_is_about() {
        assert_eq!(refused(composed()), None);
        assert_eq!(
            refused(Composed {
                to: Vec::new(),
                cc: vec!["third@example.test".to_owned()],
                ..composed()
            })
            .as_deref(),
            Some("no recipients")
        );
        assert_eq!(
            refused(Composed {
                to: vec!["not-an-address".to_owned()],
                ..composed()
            })
            .as_deref(),
            Some("not an address: not-an-address")
        );
        assert_eq!(
            refused(Composed {
                sender: "nope".to_owned(),
                ..composed()
            })
            .as_deref(),
            Some("not an address: nope")
        );
        assert_eq!(
            refused(Composed {
                subject: "   ".to_owned(),
                ..composed()
            })
            .as_deref(),
            Some("empty subject")
        );
        assert_eq!(
            refused(Composed {
                body: "\n\n".to_owned(),
                ..composed()
            })
            .as_deref(),
            Some("empty body and no attachments")
        );
    }

    /// A message read back as it was written builds the same message.
    #[test]
    fn a_message_is_its_composition() {
        assert_eq!(Outgoing::new(outgoing().composed()), Ok(outgoing()));
    }

    /// Only a message handed over and not answered for may have gone.
    #[test]
    fn only_an_unanswered_message_may_have_gone() {
        let steps = [
            Step::Credentials,
            Step::Connect,
            Step::Tls,
            Step::Auth,
            Step::Mail,
            Step::Rcpt,
            Step::Data,
            Step::Message,
        ];
        for step in steps {
            let refused = SendError::Transport {
                step,
                why: "550 no".to_owned(),
            };
            assert_eq!(refused.delivery(), Delivery::NotSent, "{refused}");
        }
        for not_sent in [
            SendError::Refused("no recipients".to_owned()),
            SendError::WrongIdentity {
                from: "a@x.test".to_owned(),
                account: "b@x.test".to_owned(),
            },
            SendError::Unbuildable("a header".to_owned()),
        ] {
            assert_eq!(not_sent.delivery(), Delivery::NotSent, "{not_sent}");
        }
        for unknown in [
            SendError::Unanswered("the connection closed".to_owned()),
            SendError::RecipientsRefused(vec!["b@x.test".to_owned()]),
        ] {
            assert_eq!(unknown.delivery(), Delivery::Unknown, "{unknown}");
        }
    }

    /// A reply is threaded, and `References` defaults to the parent.
    #[test]
    fn a_reply_carries_in_reply_to_and_references() {
        let reply = |references: &[&str]| {
            Outgoing::new(Composed {
                in_reply_to: Some("parent@example.test".to_owned()),
                references: references.iter().map(|one| (*one).to_owned()).collect(),
                ..composed()
            })
            .expect("a sendable message")
        };
        let out = reply(&[]);
        assert_eq!(
            thread_headers(&out),
            vec![
                ("In-Reply-To".to_owned(), "<parent@example.test>".to_owned()),
                ("References".to_owned(), "<parent@example.test>".to_owned()),
            ]
        );

        let out = reply(&["grand@example.test", "parent@example.test"]);
        assert_eq!(
            thread_headers(&out)[1].1,
            "<grand@example.test> <parent@example.test>"
        );

        let built = text(&out, "a@x.test");
        assert!(
            built.contains("In-Reply-To: <parent@example.test>"),
            "{built}"
        );
    }

    #[test]
    fn a_message_that_replies_to_nothing_carries_no_thread_headers() {
        assert!(thread_headers(&outgoing()).is_empty());
        assert!(!text(&outgoing(), "a@x.test").contains("In-Reply-To"));
    }
}
