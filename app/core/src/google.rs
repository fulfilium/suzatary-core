//! "Sign in with Google": one consent that connects a Google account's mail and calendars,
//! as a sans-I/O state machine over the authorization-code grant with PKCE (RFC 6749 §4.1,
//! RFC 7636, the practices of RFC 9700); `suzatary_io::oauth` is the transport, as it is for
//! Microsoft's device-code flow in [`crate::mail::oauth`], whose request and answer shapes
//! ([`FormPost`](crate::mail::oauth::FormPost), [`Answered`](crate::mail::oauth::Answered),
//! [`OauthError`](crate::mail::oauth::OauthError), [`Silent`](crate::mail::oauth::Silent)) this
//! reuses.
//!
//! Google grants Gmail and Calendar scopes through no device-code flow, so the consent page
//! sends the owner's browser back to the box: to [`RedirectUri`], the app's own origin, which the
//! tailnet serves and nothing else. The rules, each a type:
//!
//! 1. A request is bound to its [`State`] and its PKCE [`Verifier`], both drawn from entropy per
//!    consent, and the verifier is sent only to the token endpoint, as its S256 [`Challenge`]
//!    beforehand. A callback is read into a [`Callback`] once, and its state is compared with the
//!    one the box drew; a code is exchanged at most once, since the box drops the state as it
//!    reads it.
//! 2. The redirect URI is derived from the app's URL, never typed, and the client the owner
//!    pastes is refused unless Google lists exactly that URI for it ([`Client::of_download`]).
//! 3. A grant is read into a [`RefreshToken`], the [`Scopes`] Google says it granted and the
//!    account its ID token names, which must be the account asked for.

mod calendar;
mod client;
mod flow;
pub mod ical;
mod pkce;
mod scope;

pub use calendar::{
    ACTOR, CALENDARS, Calendars, EVENTS, EventTitle, Listed, MAX_ANSWER, NewEvent, SCHEMA, Unread,
    calendar_name, events_url, insert_body, insert_url, list_url, read_calendars, read_events,
    window,
};
pub use client::{CLIENT_ID, CLIENT_SECRET, Client, ClientId, ClientRefused, ClientSecret};
pub use flow::{
    AccessToken, Callback, Code, Endpoints, Exchanged, Held, KeptGrant, RefreshToken, authorize,
    exchange, grant_name, read_exchange, read_refresh, refresh,
};
pub use pkce::{Challenge, State, Verifier};
pub use scope::{RedirectUri, Scope, Scopes};

/// The path on the app's origin Google sends the owner back to.
pub const CALLBACK: &str = "/api/oauth/google/callback";

/// The path on the app's origin a sign-in card's button opens, followed by the card's proposal:
/// the box sends the browser on to the consent the card's tap opened, which it holds in memory
/// alone, so no consent's state is ever stored.
pub const CONSENT: &str = "/api/oauth/google/consent/";

/// What the owner is told, on the card that asks for the client and in the guide, about making
/// the one Google Cloud client this box signs in with. Each step names the button it means.
pub const CLIENT_STEPS: [&str; 6] = [
    "Open the Google Cloud console with this card's button. If it asks, create a project, then \
     press Get started: give the app a name and your address, choose External, and press Create.",
    "Under Audience, press Publish app, so the app is In production. Gmail's access is \
     restricted, so when you sign in Google shows one warning (\"Google hasn't verified this \
     app\"): choose Advanced, then Go to the app. If the app stays in Testing, Google ends each \
     sign-in after 7 days, and the box asks you to sign in again every week.",
    "Under APIs & Services, Library, enable the Gmail API and the Google Calendar API.",
    "Under Clients, press Create client and choose the type Web application.",
    "Under Authorized redirect URIs, press Add URI, paste exactly the address this card shows, \
     and press Create.",
    "In the dialog that opens, press Download JSON, open the file, and paste all of it into \
     this card's field.",
];

/// The Google Cloud console's credentials page, where the client is made.
pub const CONSOLE: &str = "https://console.cloud.google.com/auth/overview";
