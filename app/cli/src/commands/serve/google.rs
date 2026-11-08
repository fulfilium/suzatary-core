//! Sign-ins with Google: consents opened by a tapped card and finished by Google's callback.
//! Held in no capability record (only a tap begins one) and in memory alone (a restart closes
//! every one).
//!
//! The callback is a GET with no CSRF triple; in its place stand the one-use state (RFC 6749
//! §10.12) and the gate's actor, which must be the actor who tapped.

use std::sync::{Arc, Mutex};

use axum::response::Redirect;
use suzatary_core::capability::Entropy;
use suzatary_core::google::{Callback, Endpoints, RedirectUri, Scopes, State as Drawn, Verifier};
use suzatary_core::mail::account::Sync as Strategy;
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::refusal::Refusal;
use suzatary_core::secretary::google_sign_in::{self, Begun, Open, Taken};
use suzatary_core::secretary::inbox::ProposalId;
use suzatary_core::secretary::responded::{Done, Performed, Tried};
use suzatary_core::secretary::store::stamp;
use suzatary_io::google::Google;
use suzatary_io::secrets::Resolves;

use super::call::Call;
use super::guard::Reader;
use super::params::{Asked, Params};
use super::{Chained, State};

const BACK: &str = "/#/inbox";

/// The consents this server holds open, and what it opens and finishes them with.
pub struct Consenting {
    open: Mutex<Open>,
    google: Google,
    store: Arc<dyn Resolves + Send + Sync>,
    edits: Arc<dyn suzatary_core::settings::Edits + Send + Sync>,
    /// `None` when the app's URL names no origin Google can send the owner back to.
    redirect: Option<RedirectUri>,
    entropy: Arc<dyn Entropy + Send + Sync>,
}

impl Consenting {
    #[must_use]
    pub fn new(
        google: Google,
        store: Arc<dyn Resolves + Send + Sync>,
        edits: Arc<dyn suzatary_core::settings::Edits + Send + Sync>,
        app_url: &str,
    ) -> Self {
        Self {
            open: Mutex::default(),
            google,
            store,
            edits,
            redirect: RedirectUri::of_app(app_url).ok(),
            entropy: Arc::new(suzatary_io::texts::SystemEntropy),
        }
    }

    #[must_use]
    pub fn endpoints(&self) -> &Endpoints {
        self.google.endpoints()
    }

    #[must_use]
    pub fn redirect(&self) -> Option<&RedirectUri> {
        self.redirect.as_ref()
    }

    fn table(&self) -> std::sync::MutexGuard<'_, Open> {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// `Google`, `Gmail` (read by lieer) or `None` for any other sync.
    fn declared(&self, account: &EmailAddress) -> Result<Option<Strategy>, String> {
        let declared = self.edits.declared().map_err(|failed| failed.to_string())?;
        Ok(declared
            .accounts
            .iter()
            .find(|held| held.email.is(account.as_str()))
            .map(|held| held.sync.clone())
            .filter(|sync| matches!(sync, Strategy::Google | Strategy::Gmail)))
    }
}

impl google_sign_in::Consents for Consenting {
    fn begin(
        &self,
        card: &ProposalId,
        account: &EmailAddress,
        scopes: &Scopes,
        by: &prodrome_core::event::Actor,
        now: suzatary_core::capability::Datetime,
    ) -> Performed {
        let not_sent = |reason: String| Performed::Tried(Tried::NotSent { reason });
        match self.declared(account) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return not_sent(format!(
                    "{account} is not a Google account the instance file declares"
                ));
            }
            Err(failed) => return not_sent(format!("the instance file does not read: {failed}")),
        }
        let Some(redirect) = self.redirect.clone() else {
            return not_sent(
                "the app's URL is not https, so Google cannot send you back to it".to_owned(),
            );
        };
        let client = match suzatary_io::google::Grant::client(self.store.as_ref()) {
            Ok(Some(client)) => client,
            Ok(None) => {
                return not_sent(
                    "the box's Google client is not kept yet: paste it into its card first"
                        .to_owned(),
                );
            }
            Err(failed) => return not_sent(format!("the secret store did not answer: {failed}")),
        };
        let (state, verifier) = match (
            Drawn::draw(self.entropy.as_ref()),
            Verifier::draw(self.entropy.as_ref()),
        ) {
            (Ok(state), Ok(verifier)) => (state, verifier),
            (Err(failed), _) | (_, Err(failed)) => {
                return not_sent(format!("no state could be drawn: {failed}"));
            }
        };
        let link = suzatary_core::google::authorize(
            self.google.endpoints(),
            &client.id,
            &redirect,
            (&state, &verifier.challenge()),
            scopes,
            account,
        );
        let began = prodrome_core::fpl::instant_of(now);
        let until = began + google_sign_in::lasts();
        let mut table = self.table();
        table.lapse(began);
        table.open(
            state,
            Begun {
                card: card.clone(),
                account: account.clone(),
                scopes: scopes.clone(),
                by: by.clone(),
                verifier,
                client,
                redirect,
                link,
                until,
            },
        );
        Performed::Tried(Tried::Running {
            began: stamp(now),
            expires_at: prodrome_core::fpl::iso(until),
        })
    }
}

impl Params for Callback {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        Callback::of_query(
            asked.query("state"),
            asked.query("code"),
            asked.query("error"),
        )
        .map_err(Refusal::bad_request)
    }
}

pub struct Card(ProposalId);

impl Params for Card {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        ProposalId::new(asked.segment("card")?)
            .map(Card)
            .map_err(|refused| Refusal::bad_request(refused.to_string()))
    }
}

/// `GET /api/oauth/google/consent/{card}`: redirects the tapping actor to Google's consent page.
/// The state and challenge are held in memory, never stored.
///
/// # Errors
/// [`Refusal::forbidden`] for a browser whose login maps to no actor or another actor than the
/// one who tapped, and [`Refusal::not_found`] when the card holds no consent open.
pub fn consent(call: &Call<Reader, Card, ()>) -> Result<Redirect, Refusal> {
    let by = suzatary_core::gate::actor_of(&call.state.deployment, &call.admitted)?;
    let table = call.state.consenting.table();
    let Some(begun) = table.begun(&call.params.0) else {
        return Err(Refusal::not_found(
            "this card holds no sign-in with Google open: tap Sign in with Google to open one",
        ));
    };
    if begun.by != by {
        return Err(Refusal::forbidden(
            "this sign-in with Google was opened by someone else",
        ));
    }
    Ok(Redirect::to(&begun.link))
}

/// `GET /api/oauth/google/callback`: trades Google's code for the grant and ends the card.
/// The state is taken once, and only for the actor who tapped.
///
/// # Errors
/// [`Refusal::forbidden`] for a browser whose login maps to no actor, or to another actor than
/// the one who tapped (whose consent is closed, its card saying so), and
/// [`Refusal::bad_request`] for a state no consent is open under: never drawn here, used once
/// already, or closed by a restart. None of them reads a code.
pub fn callback(call: &Call<Reader, Callback, ()>) -> Result<Redirect, Refusal> {
    let state = &call.state;
    let by = suzatary_core::gate::actor_of(&state.deployment, &call.admitted)?;
    let consenting = &state.consenting;
    let now = state.first.caps.secretary.clock.now();
    let taken = consenting.table().take(
        call.params.state(),
        &by,
        prodrome_core::fpl::instant_of(now),
    );
    let (card, performed) = match (&taken, &call.params) {
        (Taken::Yours(begun), Callback::Code { code, .. }) => (
            begun.card.clone(),
            exchanged(state, &consenting.google, begun, code),
        ),
        (_, Callback::Refused { error, .. }) => {
            match google_sign_in::refused(&taken, Some(error)) {
                Some(ended) => ended,
                None => return Err(Refusal::bad_request(NOT_OPEN)),
            }
        }
        (_, Callback::Code { .. }) => match google_sign_in::refused(&taken, None) {
            Some(ended) => ended,
            None => return Err(Refusal::bad_request(NOT_OPEN)),
        },
    };
    let kept = if let (Taken::Yours(begun), Performed::Done(_)) = (&taken, &performed) {
        Some(suzatary_core::health::SourceKey::GoogleCalendar(
            suzatary_core::mail::account_email::AccountEmail::folded(&begun.account),
        ))
    } else {
        None
    };
    ended(state, &card, now, performed)?;
    if let Some(calendars) = kept {
        state.checks.check(&calendars);
        state.notice();
    }
    match taken {
        Taken::NotYours(_) => Err(Refusal::forbidden(
            "this sign-in with Google was begun by someone else, so nothing was kept",
        )),
        Taken::Unknown | Taken::Lapsed(_) | Taken::Yours(_) => Ok(Redirect::to(BACK)),
    }
}

const NOT_OPEN: &str = "this sign-in with Google is not open: it was finished already, it was \
                        never begun on this box, or the box restarted since. Tap Sign in with \
                        Google on its card to begin again.";

/// Trade `code` for the grant and keep its refresh token under the account's name.
fn exchanged(
    state: &State,
    google: &Google,
    begun: &Begun,
    code: &suzatary_core::google::Code,
) -> Performed {
    let read = google.exchange(
        &begun.client,
        code,
        &begun.verifier,
        &begun.redirect,
        &begun.account,
    );
    let (performed, kept) = google_sign_in::exchanged(begun, read);
    let Some(kept) = kept else {
        return performed;
    };
    let not_sent = |reason: String| Performed::Tried(Tried::NotSent { reason });
    // A lieer-read Gmail account moves onto the grant only once the grant has read its mail.
    let moving = match state.consenting.declared(&begun.account) {
        Ok(sync) => matches!(sync, Some(Strategy::Gmail)),
        Err(failed) => {
            return not_sent(format!(
                "Google granted it, and the instance file does not read, so nothing was kept: \
                 {failed}"
            ));
        }
    };
    if moving {
        let grant = suzatary_io::google::Grant {
            account: begun.account.clone(),
            client: begun.client.clone(),
            refresh: kept.refresh.clone(),
            parts: kept.parts.clone(),
        };
        if let Err(failed) = google.mailbox(&grant) {
            return not_sent(format!(
                "Google granted it, and reading {}'s mail through it failed ({failed}), so \
                 nothing was kept and lieer reads its mail as before; tap Sign in with Google to \
                 try again",
                begun.account
            ));
        }
    }
    let name = suzatary_core::google::grant_name(&begun.account);
    let stored = suzatary_core::settings::Settable::mint(&state.settables(), &name)
        .map_err(|failed| failed.to_string())
        .and_then(|name| state.secrets.write_secret(&name, &kept.print()));
    if let Err(failed) = stored {
        return not_sent(format!(
            "Google granted it, and the secret store did not keep it: {failed}; tap Sign in with \
             Google to ask again"
        ));
    }
    if !moving {
        return performed;
    }
    let moved =
        suzatary_core::connect::Connection::parse("google", &[begun.account.as_str().to_owned()])
            .map_err(|failed| failed.to_string())
            .and_then(|connection| {
                state
                    .consenting
                    .edits
                    .add_connection(&connection)
                    .map_err(|failed| failed.to_string())
            });
    match (moved, performed) {
        (Ok(()), Performed::Done(Done::Acted { what })) => Performed::Done(Done::Acted {
            what: format!(
                "{what}; its mail is read through this sign-in now, in place of lieer's own"
            ),
        }),
        (Ok(()), performed) => performed,
        (Err(failed), _) => not_sent(format!(
            "the grant is kept, and the instance file was not changed ({failed}), so lieer \
             reads {}'s mail as before; tap Sign in with Google to try again",
            begun.account
        )),
    }
}

fn ended(
    state: &State,
    card: &ProposalId,
    now: suzatary_core::capability::Datetime,
    performed: Performed,
) -> Result<(), Refusal> {
    let caps = state.first.caps;
    if let Some(conversation) = google_sign_in::end(
        caps.secretary.conversations,
        caps.secretary.inbox,
        card,
        now,
        performed,
    )? {
        state.heard(&conversation);
    }
    state.notice();
    Ok(())
}
