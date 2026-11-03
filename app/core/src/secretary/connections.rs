//! The rule over the connections: what each one's standing owes, filed by the box with no model
//! asked, as the same cards the setup agent will file.
//!
//! A connection's first unmet step is owed its card where the box performs it on a tap: an
//! account's device-code sign-in to read, and once it reads, its consent to send; the box's
//! Google client, and a Google account's sign-in with Google once that client is stored. A
//! secret step is the secrets rule's (`fixes`), which reads every declared name, the
//! connections' among them; the subscription's sign-in is the models rule's (`bootstrap`); a
//! terminal step has no card, since only the person can take it. A card a tap asked the box for
//! is owed while what it asks is unmet, so it is withdrawn once that is met however it was, and
//! stands while it is not: a grant while its plugin may not reach the origin, and a Google
//! account's connection, asked for when a link to one of its calendars was pasted, while the
//! deployment does not hold the account.
//!
//! A link a Google account's calendar was pasted as is that account's: once the account reads its
//! calendars through its sign-in with Google, the link is not needed, so its removal is owed
//! while it is declared, and the box asks for no link it asks to remove ([`Owed::retiring`]).
//! The Connect card the paste filed is what says whose the link is: its sources name the link's
//! card.

use crate::capability::Datetime;
use crate::capability::{SecretState, Secrets, Server};
use crate::connect::{Checked, Connection, Declared, Group, Params, Seen, Step, Table};
use crate::google::{Held, Scope, Scopes};
use crate::mail::account::Sync;
use crate::mail::address::EmailAddress;
use crate::plugin::{Origin, PluginName};
use crate::reference::{ProposalRef, Reference};
use crate::refusal::Refusal;
use crate::secretary::acts::Act;
use crate::secretary::inbox;
use crate::secretary::noticed::Owed;
use crate::secretary::proposer::Proposer;
use crate::secretary::responded::{Done, Responded};
use crate::secretary::store::{Proposal, Reasons};

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
                // A lieer account asks for no sign-in of its own: only its move onto the sign-in
                // with Google asks, and that one is met by the grant.
                Sync::Google | Sync::Gmail => {
                    matches!(secrets.google_grant(email), Held::Signed(_))
                }
                Sync::MsImap(_) | Sync::Proton(_) => secrets.mail_grants(account).imap,
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
        .chain(moving(declared, inbox))
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
    let tapped = inbox
        .objects()
        .filter(|object| object.asked.proposer == Proposer::Box && object.reading.performs())
        .filter_map(|object| {
            unmet(
                declared,
                inbox,
                &object.asked.proposal,
                &object.asked.reasons,
            )
        });
    signins
        .chain(lacking(declared, secrets, inbox))
        .chain(tapped)
        .chain(read_through(declared, secrets, inbox))
        .fold(Owed::default(), Owed::join)
}

/// A card of the box's that a tap asked for, owed while what it asks is unmet: a grant while its
/// plugin may not reach the origin, a connection while the deployment does not hold it, and a
/// pasted link's removal while the link is declared.
fn unmet(
    declared: &Declared,
    inbox: &inbox::Fold,
    proposal: &Proposal,
    reasons: &Reasons,
) -> Option<Owed> {
    match proposal {
        Proposal::Act(Act::Disconnect { connection }) => {
            let (link, card) = pasted(declared, inbox, reasons)?;
            (link == *connection).then(|| retiring(link, card))
        }
        Proposal::Act(Act::Grant { plugin, origin }) => outside(declared, plugin, origin)
            .then(|| Owed::one(proposal.clone(), asking(plugin, origin))),
        Proposal::Act(Act::Connect { connection }) => {
            let owed = match declared.connections().get(&connection.key()) {
                None => true,
                Some(held) => connection.moves(held),
            };
            owed.then(|| Owed::one(proposal.clone(), connecting(connection)))
        }
        Proposal::Act(
            Act::Secret { .. }
            | Act::Web { .. }
            | Act::Patch { .. }
            | Act::DriveRead { .. }
            | Act::CodingSession { .. }
            | Act::Device(_)
            | Act::Auth { .. }
            | Act::AuthSend { .. }
            | Act::SignIn { .. }
            | Act::SignInCode { .. }
            | Act::UnlockStore { .. }
            | Act::Erase { .. }
            | Act::Afresh { .. }
            | Act::GoogleClient { .. }
            | Act::GoogleSignIn { .. }
            | Act::CalendarEvent { .. },
        )
        | Proposal::Email(_)
        | Proposal::Forward(_)
        | Proposal::Changes(_)
        | Proposal::Remember { .. }
        | Proposal::Decide(_) => None,
    }
}

/// Each account lieer reads whose Connect card for a sign-in with Google was tapped: that
/// connection's steps are owed (the box's client, then the sign-in), while the instance still
/// declares the account as lieer's. The tap wrote nothing, so lieer reads its mail until the
/// sign-in's callback has read the mail through the grant and moved the account.
fn moving<'a>(
    declared: &'a Declared,
    inbox: &'a inbox::Fold,
) -> impl Iterator<Item = Connection> + 'a {
    let held = declared.connections();
    let mut asked: Vec<Connection> = inbox
        .objects()
        .filter(|object| {
            object.reading.shown().is_some_and(|answer| {
                matches!(answer.response, Responded::Done(Done::Acted { .. }))
            })
        })
        .filter_map(move |object| {
            let Proposal::Act(Act::Connect { connection }) = &object.asked.proposal else {
                return None;
            };
            held.get(&connection.key())
                .is_some_and(|account| connection.moves(account))
                .then(|| connection.clone())
        })
        .collect();
    asked.sort();
    asked.dedup();
    asked.into_iter()
}

/// The removal of every link a Google account's calendar was pasted as, once that account reads
/// its calendars through its sign-in with Google.
fn read_through<'a>(
    declared: &'a Declared,
    secrets: &'a dyn Secrets,
    inbox: &'a inbox::Fold,
) -> impl Iterator<Item = Owed> + 'a {
    inbox
        .objects()
        .filter(|object| object.asked.proposer == Proposer::Box)
        .filter_map(move |object| {
            let Proposal::Act(Act::Connect { connection }) = &object.asked.proposal else {
                return None;
            };
            let Params::Address(account) = connection.params() else {
                return None;
            };
            let (link, card) = pasted(declared, inbox, &object.asked.reasons)?;
            reads_calendars(declared, secrets, account).then(|| retiring(link, card))
        })
}

/// Whether `account` is declared signed in with Google and its grant holds its calendars.
fn reads_calendars(declared: &Declared, secrets: &dyn Secrets, account: &EmailAddress) -> bool {
    declared
        .accounts
        .iter()
        .any(|held| held.email.as_str() == account.as_str() && matches!(held.sync, Sync::Google))
        && matches!(
            secrets.google_grant(account),
            Held::Signed(parts) if parts.contains(Scope::Calendar)
        )
}

/// The declared link a card of the box's rests on, and that link's card: a card its sources
/// name that asks for the link's secret.
fn pasted(
    declared: &Declared,
    inbox: &inbox::Fold,
    reasons: &Reasons,
) -> Option<(Connection, ProposalRef)> {
    reasons.sources().iter().find_map(|source| {
        let Reference::Proposal(card) = source else {
            return None;
        };
        let asked = inbox.objects().find(|object| object.asked.asked == *card)?;
        let Proposal::Act(Act::Secret { name, .. }) = &asked.asked.proposal else {
            return None;
        };
        declared
            .connections()
            .into_values()
            .find(|link| {
                matches!(link.template().table, Table::Link(_))
                    && link.secrets().first() == Some(name)
            })
            .map(|link| (link, card.clone()))
    })
}

/// The removal of `link`, which the card `pasted` was answered with a Google calendar's link.
fn retiring(link: Connection, pasted: ProposalRef) -> Owed {
    let why = format!(
        "{} is a Google account's calendar, which the box reads through that account's sign-in \
         with Google, so its link is not needed.",
        capitalised(&link.called())
    );
    Owed::one(Proposal::Act(Act::Disconnect { connection: link }), why).about(&[pasted.into()])
}

/// `words` begun with a capital, as a sentence begins.
fn capitalised(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
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

/// Why the box asks to connect `connection`, a Google account whose calendar's link was pasted.
fn connecting(connection: &crate::connect::Connection) -> String {
    format!(
        "A link to a calendar of {} was pasted, and the box reads a Google account's calendars \
         through its sign-in with Google, never by a link.",
        connection.called()
    )
}

/// File the box's card asking to let `plugin` reach `origin`, when a secret card was tapped with
/// a link served from it.
///
/// # Errors
/// The stores' own failures.
pub fn grant(caps: &Server<'_>, plugin: &PluginName, origin: &Origin) -> Result<(), Refusal> {
    tapped(
        caps,
        Owed::one(
            Proposal::Act(Act::Grant {
                plugin: plugin.clone(),
                origin: origin.clone(),
            }),
            asking(plugin, origin),
        ),
    )
}

/// File the box's card asking to connect `connection`, a Google account, when the secret card
/// `pasted` was tapped with a link to one of its calendars; its sources name that card, which
/// makes the link the account's.
///
/// # Errors
/// The stores' own failures.
pub fn connect(
    caps: &Server<'_>,
    connection: Connection,
    pasted: &ProposalRef,
) -> Result<(), Refusal> {
    let why = connecting(&connection);
    tapped(
        caps,
        Owed::one(Proposal::Act(Act::Connect { connection }), why).about(&[pasted.clone().into()]),
    )
}

/// File the box's card asking to remove `link`, when its secret card `pasted` was tapped with a
/// link to a calendar of a Google account the deployment holds signed in with Google.
///
/// # Errors
/// The stores' own failures.
pub fn retire(caps: &Server<'_>, link: Connection, pasted: &ProposalRef) -> Result<(), Refusal> {
    tapped(caps, retiring(link, pasted.clone()))
}

/// File `owed`, a card a tap asked for. It withdraws nothing: the next filing holds it while
/// [`unmet`] owes it.
fn tapped(caps: &Server<'_>, owed: Owed) -> Result<(), Refusal> {
    let now = caps.secretary.clock.now();
    // Each paste is the owner asking again, so a grant once dismissed is asked for anew; a
    // connection is its content alone, so one dismissed stays dismissed.
    owed.since(now)
        .holding()
        .file(caps.secretary.conversations, caps.secretary.inbox, now)
        .map(|_| ())
}
