//! Every effect the application performs is a trait here, one per file, or a store in one of
//! [`crate::store`]'s three shapes, and a role's effects are a record of them passed as a
//! value: [`Agent`], [`Secretary`], [`Server`].
//! `suzatary-io` implements the traits, the binary assembles the records, tests fill
//! them from fakes; a rule then is a fact about which fields a record has.
//!
//! - The triage agent cannot send, notify or name a peer: [`Agent`] has no such field.
//! - Reading mail never yields a send token: [`ImapToken`] and [`SmtpToken`] are two types.
//! - Rendered means confirmed: [`Render::html`] takes an [`Escaped`], which only
//!   [`Content::Confirmed`] yields.
//! - Loopback never writes: an actor is a [`Login`], and only [`Whois`] mints one.

use std::fmt;

use crate::escape::Escaped;

mod agent;
mod answering;
mod answers;
mod briefings;
mod calendar;
mod clock;
mod compact;
mod corpus;
mod drive;
mod entropy;
mod render_cache;
// The fake compactor, test support like `nothing` below.
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/joining.rs"]
mod joining;
mod junk;
mod mail_sync;
mod mailbox;
mod manuals;
mod model;
// Test support, never production: compiled into this crate (its own unit tests fill records from
// it, and a crate of its own would implement a second copy of these traits), kept out of `src/`.
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/nothing.rs"]
mod nothing;
mod notify;
mod older_mail;
mod render;
mod repositories;
mod repository;
mod secretary;
mod secrets;
mod server;
mod typeset;
mod upload_index;
mod usage;
mod weather;
mod web;
mod whois;

pub use agent::Agent;
pub use answering::Answering;
pub use answers::{AnswerError, Answers};
pub use briefings::{BriefingError, Briefings};
pub use calendar::Calendar;
pub use clock::Clock;
pub use compact::Compact;
pub use corpus::{Corpus, CorpusError};
pub use drive::{Drive, DriveFailure};
pub use entropy::Entropy;
#[cfg(any(test, feature = "nothing"))]
pub use joining::Joining;
pub use junk::{Junk, JunkFailure};
pub use mail_sync::{MailSync, SyncFailure, Synced};
pub use mailbox::{MAX_QUERY_CHARS, MailError, Mailbox, Query};
pub use manuals::{Manual, Manuals};
#[cfg(feature = "fixture")]
pub use model::pulled;
pub use model::{
    Conversation, Delta, Halt, Model, ModelError, NotAUuid, Reply, Session, SessionId,
    SessionModel, Sessioned, TokenUsage, ToolCall, Turn,
};
#[cfg(any(test, feature = "nothing"))]
pub use nothing::{Nothing, Parses};
pub use notify::{Delivered, Notify, NotifyError, Printed, Push};
pub use older_mail::OlderMail;
pub use render::{Html, Render, RenderError};
pub use render_cache::RenderCache;
pub use repositories::{MAX_FILE_BYTES, MAX_TEXT_BYTES, RepoFailure, Repositories, readable};
pub use repository::{Repository, RepositoryError};
pub use secretary::Secretary;
pub use secrets::{Answer, Locked, MailCredential, MailGrants, SecretState, Secrets, Snapshot};
pub use server::Server;
pub use typeset::{Diagnostic, Rejected, Typeset, parsed};
pub use upload_index::{IndexError, UploadIndex};
pub use usage::{Usage, UsageError};
pub use weather::{Weather, WeatherError};
pub use web::{AllowRule, Web, WebAnswer, WebFailure, WebRequest};
pub use whois::{Peer, Whois, WhoisError};

/// The instant a [`Clock`] hands back: the Prodrome's own literal, data and never an order.
pub type Datetime = prodrome_core::literal::Datetime;

/// A Tailscale login, as the local daemon named it for a peer address.
///
/// Only [`Login::from_whois`] mints one, so loopback, which resolves to no peer, cannot act.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Login(String);

impl Login {
    /// Mint a `Login` from what the whois interpreter read back, named for its one caller.
    #[must_use]
    pub fn from_whois(login: impl Into<String>) -> Self {
        Self(login.into())
    }

    /// The login as text, for a roster lookup and an event's `actor` field.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Login {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A secret whose `Debug` says only that it is one, so a record holding it may derive `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a secret; [`Secret::expose`] is the only way to read it back.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The secret itself, for the one transport that must send it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Read from a string, and the error says only that it was not one: a value typed
/// as the wrong JSON type is still a secret, and a parse error must not print it.
impl<'de> serde::Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        String::deserialize(input)
            .map(Self)
            .map_err(|_| serde::de::Error::custom("value is required, as a string"))
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// An access token that may read a mailbox over IMAP.
///
/// A separate type from [`SmtpToken`], so a read path that holds a token holds nothing that sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImapToken(Secret);

impl ImapToken {
    /// Mint one. Only the token store's refresh calls this.
    #[must_use]
    pub fn from_refresh(secret: Secret) -> Self {
        Self(secret)
    }

    /// The bearer token, for the IMAP transport.
    #[must_use]
    pub fn secret(&self) -> &Secret {
        &self.0
    }
}

/// An access token that may send over SMTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpToken(Secret);

impl SmtpToken {
    /// Mint one. Only the token store's refresh calls this.
    #[must_use]
    pub fn from_refresh(secret: Secret) -> Self {
        Self(secret)
    }

    /// The bearer token, for the SMTP transport.
    #[must_use]
    pub fn secret(&self) -> &Secret {
        &self.0
    }
}

/// A todo's prose, tagged by whether a trusted actor wrote it.
///
/// Only [`Content::Confirmed`] yields an [`Escaped`], which [`Render::html`] requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// A trusted actor's markup. Renders.
    Confirmed(Escaped),
    /// An untrusted actor's markup: served as source, marked `(unconfirmed)`, never rendered.
    Provisional(String),
}

impl Content {
    /// The markup a [`Render`] may take, `None` for the provisional arm.
    #[must_use]
    pub fn escaped(&self) -> Option<&Escaped> {
        match self {
            Content::Confirmed(markup) => Some(markup),
            Content::Provisional(_) => None,
        }
    }

    /// The stored source, for either arm.
    #[must_use]
    pub fn source(&self) -> &str {
        match self {
            Content::Confirmed(markup) => markup.as_str(),
            Content::Provisional(source) => source,
        }
    }
}

/// The model one round asks, picked once for that round and owned by it: nothing of one
/// round's pick reaches another round or another conversation.
pub enum Pick<'a> {
    /// The model that holds sessions.
    Held(&'a dyn SessionModel),
    /// A model with no session, picked for this round alone.
    Turn(Box<dyn Model + 'a>),
}

impl Pick<'_> {
    /// The model picked, asked statelessly.
    #[must_use]
    pub fn model(&self) -> &dyn Model {
        match self {
            Pick::Held(model) => *model,
            Pick::Turn(model) => model.as_ref(),
        }
    }
}

/// Which model a round asks, answered as a value rather than a `SessionModel` that always
/// fails beside a bool.
pub trait Sessions: Sync {
    /// The model this round asks, read now.
    fn pick(&self) -> Pick<'_>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each record is exactly these effects, so widening a role's authority is an edit here too.
    #[test]
    fn each_role_holds_exactly_its_effects() {
        let Agent {
            chain: _,
            corpus: _,
            mail: _,
            repository: _,
            manuals: _,
            render: _,
            clock: _,
            memory: _,
        } = Nothing::agent();
        let Secretary {
            chain: _,
            corpus: _,
            mail: _,
            repository: _,
            manuals: _,
            render: _,
            clock: _,
            model: _,
            conversations: _,
            inbox: _,
            memories: _,
            decisions: _,
            topics: _,
            briefings: _,
            weather: _,
            calendar: _,
            secrets: _,
            web: _,
            repositories: _,
            drive: _,
            junk: _,
            sync: _,
            health: _,
            older: _,
            uploads: _,
            upload_index: _,
            kept: _,
            configuration: _,
            watches: _,
        } = Nothing::secretary();
        let Server {
            secretary: _,
            whois: _,
            notify: _,
            answers: _,
            devices: _,
            usage: _,
            sessions: _,
        } = Nothing::server();
    }

    #[test]
    fn a_secret_does_not_print_itself() {
        let token = ImapToken::from_refresh(Secret::new("hunter2"));
        let shown = format!("{token:?}");
        assert!(
            !shown.contains("hunter2"),
            "a token printed itself: {shown}"
        );
        assert_eq!(shown, "ImapToken(Secret(<redacted>))");
    }

    #[test]
    fn only_confirmed_content_yields_markup_to_render() {
        let confirmed = Content::Confirmed(crate::escape::escape_markup("a [thing]"));
        let provisional = Content::Provisional("a \\[thing\\]".to_owned());
        assert!(confirmed.escaped().is_some());
        assert!(provisional.escaped().is_none());
        // The same bytes on both arms: the type is the trust decision.
        assert_eq!(confirmed.source(), provisional.source());
    }
}
