//! The rule over the connections: what each one's standing owes, filed by the box with no model
//! asked, as the same cards the setup agent will file.
//!
//! A connection's first unmet step is owed its card where the box performs it on a tap: an
//! account's device-code sign-in to read, and once it reads, its consent to send; the box's
//! Google client, and a Google account's sign-in with Google once that client is stored. A
//! secret step is the secrets rule's (`fixes`), which reads every declared name, the
//! connections' among them; the subscription's sign-in is the models rule's (`bootstrap`); a
//! terminal step has no card, since only the person can take it. A grant the box asked for is
//! owed while its plugin still may not reach the origin, so it is withdrawn once the origin is
//! allowed however it was, and stands while it is not.

use crate::capability::Datetime;
use crate::capability::{SecretState, Secrets, Server};
use crate::connect::{Checked, Declared, Group, Seen, Step};
use crate::google::{Held, Scopes};
use crate::mail::account::Sync;
use crate::mail::address::EmailAddress;
use crate::plugin::{Origin, PluginName};
use crate::refusal::Refusal;
use crate::secretary::acts::Act;
use crate::secretary::inbox;
use crate::secretary::noticed::Owed;
use crate::secretary::proposer::Proposer;
use crate::secretary::responded::{Done, Responded};
use crate::secretary::store::Proposal;

/// What the rule reads of a deployment's Google accounts.
#[derive(Debug, Clone, Copy)]
pub struct Google<'a> {
    /// Whether the box's Google Cloud client is stored, its id and its secret both.
    pub client: SecretState,
    /// The box's callback, `None` when the app's URL names no origin Google can send the owner
    /// back to, which asks for no client card, since no client could list it.
    pub redirect: Option<&'a crate::google::RedirectUri>,
}

/// The cards the connections owe: each account's sign-in while it holds none to read its mail
/// (a Google account's once the box's Google client is stored, and that client's card before
/// it), its consent to send once it reads and while it holds none to send, and each grant the
/// box asked for while its plugin still may not reach the origin.
#[must_use]
pub fn owed(
    declared: &Declared,
    secrets: &dyn Secrets,
    google: Google<'_>,
    inbox: &inbox::Fold,
) -> Owed {
    let secret = |name: &str| match name {
        crate::google::CLIENT_ID | crate::google::CLIENT_SECRET => google.client,
        _ => SecretState::Set,
    };
    let signed_in = |email: &EmailAddress| {
        declared
            .accounts
            .iter()
            .find(|account| account.email.as_str() == email.as_str())
            .is_some_and(|account| match account.sync {
                Sync::Google => matches!(secrets.google_grant(email), Held::Signed(_)),
                Sync::MsImap(_) | Sync::Gmail | Sync::Proton(_) => {
                    secrets.mail_grants(account).imap
                }
            })
    };
    let may_send = |email: &EmailAddress| {
        declared
            .accounts
            .iter()
            .find(|account| account.email.as_str() == email.as_str())
            .is_some_and(|account| secrets.mail_grants(account).smtp)
    };
    let signins = declared
        .connections()
        .into_values()
        .flat_map(|connection| {
            let unmet = connection.unmet(&Seen {
                secret: &secret,
                signed_in: &signed_in,
                may_send: &may_send,
                checked: Checked::Never,
                reach: &crate::connect::PLAIN,
            });
            // Sending is asked for once the account reads, one sign-in card at a time; a sign-in
            // with Google needs the client first, so it is asked for once that is stored.
            let reads = !unmet
                .iter()
                .any(|step| matches!(step, Step::DeviceCode { .. }));
            let client_first = unmet.contains(&Step::GoogleClient);
            unmet.into_iter().filter(move |step| match step {
                Step::SendingConsent { .. } => reads,
                Step::AuthCode { .. } => !client_first,
                Step::Secret { .. }
                | Step::DeviceCode { .. }
                | Step::GoogleClient
                | Step::SignIn
                | Step::Terminal { .. } => true,
            })
        })
        .filter_map(|step| card(step, google, inbox));
    let grants = inbox
        .objects()
        .filter_map(|object| match &object.asked.proposal {
            Proposal::Act(Act::Grant { plugin, origin })
                if object.asked.proposer == Proposer::Box
                    && object.reading.performs()
                    && outside(declared, plugin, origin) =>
            {
                Some(Owed::one(
                    object.asked.proposal.clone(),
                    asking(plugin, origin),
                ))
            }
            _ => None,
        });
    signins
        .chain(lacking(declared, secrets, inbox))
        .chain(grants)
        .fold(Owed::default(), Owed::join)
}

/// The box's card for a connection's unmet `step`, where the box performs it on a tap; `None`
/// for a step another rule asks for, or one only the person can take.
fn card(step: Step, google: Google<'_>, inbox: &inbox::Fold) -> Option<Owed> {
    match step {
        Step::DeviceCode { account } => Some(signing_in(
            inbox,
            Act::Auth {
                account: account.clone(),
            },
            &account,
            format!("Sign in to {account} so the box can read its mail."),
        )),
        Step::SendingConsent { account } => Some(signing_in(
            inbox,
            Act::AuthSend {
                account: account.clone(),
            },
            &account,
            format!("{account} reads its mail and holds no sign-in to send it"),
        )),
        Step::AuthCode { account } => Some(signing_in(
            inbox,
            Act::GoogleSignIn {
                account: account.clone(),
                scopes: Scopes::all(),
            },
            &account,
            format!(
                "{account} is connected and holds no sign-in with Google to read its mail \
                 and calendars"
            ),
        )),
        Step::GoogleClient => google.redirect.map(|redirect| {
            Owed::one(
                Proposal::Act(Act::GoogleClient {
                    redirect: redirect.clone(),
                }),
                "A Google account is connected, and every Google account signs in through a \
                 Google Cloud client of your own, which the box does not hold yet.",
            )
        }),
        Step::Secret { .. } | Step::SignIn | Step::Terminal { .. } => None,
    }
}

/// Each Google account signed in with a grant that lacks a part its connection reads (one the
/// owner left unticked on Google's page): a sign-in card asking for those parts alone, which
/// Google adds to the grant the account holds (`include_granted_scopes`).
fn lacking<'a>(
    declared: &'a Declared,
    secrets: &'a dyn Secrets,
    inbox: &'a inbox::Fold,
) -> impl Iterator<Item = Owed> + 'a {
    declared
        .accounts
        .iter()
        .filter(|account| matches!(account.sync, Sync::Google))
        .map(|account| account.email.address())
        .filter_map(move |account| match secrets.google_grant(account) {
            Held::Signed(parts) => Scopes::all().missing(&parts).map(|missing| {
                let why = format!(
                    "{account} is signed in with Google without {}, which it was not granted: \
                     this asks Google for {} alone",
                    missing.sentence(),
                    if missing.iter().count() == 1 {
                        "it"
                    } else {
                        "them"
                    }
                );
                signing_in(
                    inbox,
                    Act::GoogleSignIn {
                        account: account.clone(),
                        scopes: missing,
                    },
                    account,
                    why,
                )
            }),
            Held::NotSignedIn => None,
        })
}

/// An account's sign-in card, its episode beginning when the account was last connected from a
/// card, so one dismissed before is asked again.
fn signing_in(inbox: &inbox::Fold, act: Act, account: &EmailAddress, why: String) -> Owed {
    let owed = Owed::one(Proposal::Act(act), why);
    match connected(inbox, account) {
        Some(at) => owed.since(at),
        None => owed,
    }
}

/// When `account` was last connected from a card: its sign-in's episode begins there, so a
/// sign-in card dismissed before is asked again; `None` for an account written by hand.
fn connected(inbox: &inbox::Fold, account: &EmailAddress) -> Option<Datetime> {
    inbox
        .objects()
        .filter(|object| match &object.asked.proposal {
            Proposal::Act(Act::Connect { connection }) => {
                connection.key().group == Group::Mail
                    && connection.key().name.eq_ignore_ascii_case(account.as_str())
            }
            _ => false,
        })
        .filter_map(|object| match object.reading.shown() {
            Some(answer) if matches!(answer.response, Responded::Done(Done::Acted { .. })) => {
                Some(answer.at)
            }
            _ => None,
        })
        .max()
}

/// Whether `plugin` is declared and may not reach `origin`.
fn outside(declared: &Declared, plugin: &PluginName, origin: &Origin) -> bool {
    declared
        .plugins
        .iter()
        .any(|manifest| manifest.name() == plugin && !manifest.allows(origin))
}

/// Why the box asks to let `plugin` reach `origin`, in the owner's words.
fn asking(plugin: &PluginName, origin: &Origin) -> String {
    format!(
        "The link is served from {origin}, which {} may not reach yet.",
        crate::connect::plugin_called(plugin.as_str())
    )
}

/// File the box's card asking to let `plugin` reach `origin`, when a secret card was tapped with
/// a link served from it. It withdraws nothing: the next
/// filing holds it while the rule above owes it.
///
/// # Errors
/// The stores' own failures.
pub fn grant(caps: &Server<'_>, plugin: &PluginName, origin: &Origin) -> Result<(), Refusal> {
    Owed::one(
        Proposal::Act(Act::Grant {
            plugin: plugin.clone(),
            origin: origin.clone(),
        }),
        asking(plugin, origin),
    )
    // Each paste is the owner asking again, so a grant once dismissed is asked for anew.
    .since(caps.secretary.clock.now())
    .holding()
    .file(
        caps.secretary.conversations,
        caps.secretary.inbox,
        caps.secretary.clock.now(),
    )
    .map(|_| ())
}
