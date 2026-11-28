//! Every effect is a trait, and a role's effects are a record of them, so a rule is a fact
//! about which fields a record has.
//!
//! - The triage agent cannot send, notify or name a peer: its `Agent` has no such field.
//! - Reading mail never yields a send token: [`ImapToken`] and [`SmtpToken`] are two types.
//! - Rendered means confirmed: `Render::html` takes an [`Escaped`], which only
//!   [`Content::Confirmed`] yields.
//! - Loopback never writes: an actor is a [`Login`], and only [`Whois`] mints one.

use std::fmt;

use crate::escape::Escaped;

mod clock;
mod entropy;
mod manuals;
mod render_error;
// Test support, never production.
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/nothing.rs"]
mod nothing;
mod typeset;
mod whois;

pub use clock::Clock;
pub use entropy::{Entropy, SALT_BYTES};
pub use manuals::{Manual, Manuals};
#[cfg(any(test, feature = "nothing"))]
pub use nothing::{Nothing, Parses, Recorded, Recording};
pub use render_error::RenderError;
pub use typeset::{Diagnostic, Rejected, Typeset, parsed};
pub use whois::{Peer, Whois, WhoisError};

/// The instant a [`Clock`] hands back: the Prodrome's own literal, data and never an order.
pub type Datetime = prodrome_data::literal::Datetime;

/// A Tailscale login; only [`Login::from_whois`] mints one, so loopback cannot act.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Login(String);

impl Login {
    #[must_use]
    pub fn from_whois(login: impl Into<String>) -> Self {
        Self(login.into())
    }

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

/// A parse error says only that the value was not a string: a mistyped value is still a
/// secret.
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

/// A token that may read a mailbox over IMAP, a type apart from [`SmtpToken`] so it cannot send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImapToken(Secret);

impl ImapToken {
    /// Only the token store's refresh calls this.
    #[must_use]
    pub fn from_refresh(secret: Secret) -> Self {
        Self(secret)
    }

    #[must_use]
    pub fn secret(&self) -> &Secret {
        &self.0
    }
}

/// An access token that may send over SMTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpToken(Secret);

impl SmtpToken {
    /// Only the token store's refresh calls this.
    #[must_use]
    pub fn from_refresh(secret: Secret) -> Self {
        Self(secret)
    }

    #[must_use]
    pub fn secret(&self) -> &Secret {
        &self.0
    }
}

/// An access token that may read and edit a Microsoft account's calendars through Graph, a type
/// apart from [`ImapToken`] and [`SmtpToken`] so neither mail consent can touch a calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarToken(Secret);

impl CalendarToken {
    /// Only the token store's refresh calls this.
    #[must_use]
    pub fn from_refresh(secret: Secret) -> Self {
        Self(secret)
    }

    #[must_use]
    pub fn secret(&self) -> &Secret {
        &self.0
    }
}

/// A todo's prose, tagged by whether a trusted actor wrote it.
/// Only [`Content::Confirmed`] yields an [`Escaped`], which `Render::html` requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Confirmed(Escaped),
    /// An untrusted actor's markup: served as source, marked `(unconfirmed)`, never rendered.
    Provisional(String),
}

impl Content {
    #[must_use]
    pub fn escaped(&self) -> Option<&Escaped> {
        match self {
            Content::Confirmed(markup) => Some(markup),
            Content::Provisional(_) => None,
        }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        match self {
            Content::Confirmed(markup) => markup.as_str(),
            Content::Provisional(source) => source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
