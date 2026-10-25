//! An act the box performs itself when the owner taps its proposal.
//!
//! `Auth` has no `scope` field on purpose: its tap mints the IMAP grant alone. Sending's
//! consent is `AuthSend`, a card of its own that only the box files, for a Microsoft account
//! that reads mail and holds no grant to send; no verb proposes it, and its grant is kept in
//! the SMTP slot, apart from the IMAP one, so a leaked read token still cannot send.

use prodrome_core::literal::{ProdromeError, Value};
use serde::{Deserialize, Serialize};

use crate::literal::{Fields, Literal, Signature, arm, no_arm, record};
use crate::mail::address::EmailAddress;
use crate::reference::{ProposalRef, Reference, TodoRef};
use crate::refusal::Refusal;
use crate::secretary::responded::{Performed, Tried};
use crate::secretary::stamp::Stamp;
use crate::secretary::store::{ConversationId, Entry};

/// One act, from a closed set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Act {
    /// Authorise a Microsoft account for mail sync through the device-code flow, IMAP only.
    Auth {
        /// The account to authorise.
        account: EmailAddress,
    },
    /// Authorise a Microsoft account to send mail through the device-code flow, SMTP only:
    /// filed by the box once the account reads mail and holds no grant to send.
    #[serde(rename = "auth_send")]
    AuthSend {
        /// The account to authorise.
        account: EmailAddress,
    },
    /// Store a deployment-declared secret, asked for in the Inbox.
    ///
    /// No `value` field on purpose: the value travels only in the tap's body, never stored.
    Secret {
        /// The name of the secret it asks for, as it was declared when asked; the tap mints it
        /// against the names declared then, so a card cannot outlive its declaration.
        name: String,
        /// Why the secretary is asking, carried here so `sentence()` needs no second lookup.
        why: String,
    },
    /// Read the web once: a request no allow rule covers, sent only on a tap.
    Web {
        /// What would go out.
        request: crate::web::WebRequest,
        /// The rule the secretary drafted for "Always allow", if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<crate::web::AllowRule>,
    },
    /// Propose exactly this diff against this commit with this message as a pull request; the
    /// tap pushes it to a branch of its own and opens the request, and a person merges it.
    Patch {
        /// The repository.
        repo: crate::repos::RepoName,
        /// Who can read the branch once pushed, as the listing said when proposed.
        visibility: crate::repos::Visibility,
        /// The commit the diff was written against.
        base: crate::repos::CommitId,
        /// The change.
        patch: crate::repos::Patch,
        /// The commit message.
        message: String,
    },
    /// Read one of the owner's documents in a folder no `drive_allow` entry covers, redacted.
    #[serde(rename = "drive_read")]
    DriveRead {
        /// The file.
        path: crate::drive::DrivePath,
        /// Its size in bytes as listed when asked, for the card.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<u64>,
    },
    /// Start a cloud coding session with this task; it spends the owner's usage, so only he acts.
    #[serde(rename = "coding_session")]
    CodingSession {
        /// What the session is asked to do, as the fire sends it.
        task: crate::coding::Task,
    },
    /// Trust a new device's key to write as its actor (SPEC law 42): filed by the box when the
    /// device asks, performed only by a tap that a device already trusted endorses.
    Device(crate::signatures::Enrolment),
    /// Sign the model in: the tap runs the deployment's sign-in on the box and the box files the
    /// card its code goes in. Filed by the box when no model answers.
    #[serde(rename = "sign_in")]
    SignIn {
        /// The episode in the box's words, when a model last answered; `None` before any has.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<String>,
    },
    /// The code an open sign-in waits for, typed on the page its link opens.
    ///
    /// No `code` field on purpose: the code travels only in the tap's body, and the token it is
    /// exchanged for only into the secret store.
    #[serde(rename = "sign_in_code")]
    SignInCode {
        /// The sign-in's link, as the box read it off the CLI.
        url: crate::secretary::sign_in::SignInUrl,
    },
    /// Unlock the secret store, which could not answer for a declared name: filed by the box,
    /// once for every such name, while it holds their cards as they stand. The tap asks the store
    /// again; it unlocks nothing itself, since the passphrase is typed at the box.
    #[serde(rename = "unlock_store")]
    UnlockStore {
        /// Where the box's unlock card that last ended without the owner's no was asked, once one
        /// has, so a store that answers and locks again is a new card.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<ProposalRef>,
    },
    /// Write a connection's table into the instance file; the box then files a card for each
    /// step its standing needs.
    Connect {
        /// What is connected: a template and the parameters chosen for it.
        connection: crate::connect::Connection,
    },
    /// Take a connection's table out of the instance file. Its todos, cards and conversations
    /// stay; a mail account's cached mail goes only on the card's wider yes.
    Disconnect {
        /// What is removed.
        connection: crate::connect::Connection,
    },
    /// Let a plugin reach one more origin: filed by the box when a link is served from an
    /// origin its manifest does not allow, naming the origin and never the link.
    Grant {
        /// The plugin.
        plugin: crate::plugin::PluginName,
        /// The origin it would reach.
        origin: crate::plugin::Origin,
    },
    /// Keep the box's Google Cloud client: filed by the box while a Google account is declared
    /// and the client is not stored. The card says how to make one and shows `redirect`, which
    /// the client must list; the tap's value is the client's downloaded file, which travels only
    /// in the tap's body and is kept only when it lists `redirect`.
    #[serde(rename = "google_client")]
    GoogleClient {
        /// The box's callback, as the client must list it.
        redirect: crate::google::RedirectUri,
    },
    /// Sign a Google account in with Google: the tap opens a consent bound to the tapping
    /// owner, and Google sends the browser back to the box, which keeps the grant.
    #[serde(rename = "google_sign_in")]
    GoogleSignIn {
        /// The account.
        account: EmailAddress,
        /// What the consent asks for.
        scopes: crate::google::Scopes,
    },
    /// Write one event into a Google account's own calendar, as the card shows it.
    #[serde(rename = "calendar_event")]
    CalendarEvent {
        /// The account whose calendar it goes in.
        account: EmailAddress,
        /// The event.
        event: crate::google::NewEvent,
    },
    /// Erase these words of the conversation the card is drawn in: filed by the box when
    /// forgetting a fact erased the copies its remembering provably made and left the turns
    /// around them, which may restate it in other words.
    Erase {
        /// The fact whose forgetting offers it.
        fact: crate::secretary::memory::FactId,
        /// The words' keys.
        keys: std::collections::BTreeSet<crate::secretary::locked_text::KeyName>,
    },
}

impl Act {
    /// Its kind, one stable token a verb's answer names it by.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Act::Auth { .. } => "auth",
            Act::AuthSend { .. } => "auth_send",
            Act::Secret { .. } => "secret",
            Act::Web { .. } => "web",
            Act::Patch { .. } => "patch",
            Act::DriveRead { .. } => "drive_read",
            Act::CodingSession { .. } => "coding_session",
            Act::Device(_) => "device",
            Act::SignIn { .. } => "sign_in",
            Act::SignInCode { .. } => "sign_in_code",
            Act::UnlockStore { .. } => "unlock_store",
            Act::Connect { .. } => "connect",
            Act::Disconnect { .. } => "disconnect",
            Act::Grant { .. } => "grant",
            Act::GoogleClient { .. } => "google_client",
            Act::GoogleSignIn { .. } => "google_sign_in",
            Act::CalendarEvent { .. } => "calendar_event",
            Act::Erase { .. } => "erase",
        }
    }

    /// The imperative a card and a turn both print, written once so the two cannot drift.
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Act::Auth { account } => format!("authorise {account} to sync mail (IMAP)"),
            Act::AuthSend { account } => format!("authorise {account} to send mail (SMTP)"),
            Act::Secret { name, why } => {
                format!("store {} ({why})", crate::settings::Help::label(name))
            }
            Act::Web { request, .. } => request.sentence(),
            Act::Patch {
                repo,
                patch,
                message,
                ..
            } => {
                let files = patch.files().len();
                format!(
                    "open a pull request on {repo} ({files} file{}): {}",
                    if files == 1 { "" } else { "s" },
                    message.lines().next().unwrap_or_default()
                )
            }
            Act::DriveRead { path, .. } => format!(
                "read the text of {} in {}",
                path.name().unwrap_or("/"),
                folder_of(path)
            ),
            Act::CodingSession { task } => {
                format!("start a coding session: {}", task.headline())
            }
            Act::Device(enrolment) => format!(
                "trust the device key {}… to write as {}",
                enrolment.key.as_str().chars().take(12).collect::<String>(),
                enrolment.actor
            ),
            Act::SignIn { .. } => {
                "connect a model: sign it in, which runs its sign-in on the box and shows you \
                 its link"
                    .to_owned()
            }
            Act::SignInCode { .. } => {
                "finish the sign-in with the code its page shows; the token it is exchanged for \
                 is kept as the model's sign-in"
                    .to_owned()
            }
            Act::UnlockStore { after } => {
                let unlock = "unlock the secret store on the box, then tap to have it asked again; \
                              until it answers, no card for a secret it holds is filed or withdrawn";
                match after {
                    None => unlock.to_owned(),
                    Some(card) => format!("{unlock} (it is locked again after {card})"),
                }
            }
            Act::Connect { connection } => format!(
                "connect {}: the box writes {} into the instance file, then asks for each step \
                 it needs",
                connection.called(),
                connection.writes()
            ),
            Act::Disconnect { connection } => format!(
                "remove {}: {} goes from the instance file, and its todos, cards and \
                 conversations stay",
                connection.called(),
                connection.writes()
            ),
            Act::Grant { plugin, origin } => format!(
                "let {} reach {origin}",
                crate::connect::plugin_called(plugin.as_str())
            ),
            Act::GoogleClient { redirect } => format!(
                "keep the box's Google Cloud client, a web application client listing {redirect} \
                 as its redirect URI; every Google account signs in through it"
            ),
            Act::GoogleSignIn { account, scopes } => format!(
                "sign {account} in with Google, granting {}: the box opens Google's consent page, \
                 and Google sends you back here",
                scopes.sentence()
            ),
            Act::CalendarEvent { account, event } => {
                format!("add {} to {account}'s calendar", event.sentence())
            }
            Act::Erase { fact, keys } => format!(
                "erase {} more passage{} of this conversation around fact {fact}, now forgotten: \
                 the turn that said it and the answers of its round, which may say it in other \
                 words",
                keys.len(),
                if keys.len() == 1 { "" } else { "s" }
            ),
        }
    }
}

/// The folder a document is in: the one "Always allow this folder" names.
#[must_use]
pub fn folder_of(path: &crate::drive::DrivePath) -> crate::drive::DrivePath {
    path.parent().unwrap_or_else(crate::drive::DrivePath::root)
}

/// The words a restart leaves behind, since the device code is never persisted.
const RESTARTED: &str = "the box restarted while this act was waiting; the device code is gone \
                          and was never written down — Try again asks for a fresh one";

/// At serve startup, close every running act nobody owns, idempotently.
///
/// Each is ended with [`Tried::NotSent`] and its conversation gets a matching [`Turn::Act`](crate::capability::Turn::Act).
///
/// # Errors
/// The stores' own failures.
pub fn reconcile(
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox: &dyn crate::store::Prodrome<super::inbox_event::InboxEvent>,
    now: crate::capability::Datetime,
) -> Result<usize, Refusal> {
    let restarted = Performed::Tried(Tried::NotSent {
        reason: RESTARTED.to_owned(),
    });
    let fold = super::inbox::fold(inbox)?;
    let mut reconciled = 0usize;
    for object in fold.objects() {
        if !object.reading.running() {
            continue;
        }
        super::inbox::end(
            inbox,
            &object.id,
            &super::writes_as(),
            now,
            restarted.clone(),
        )?;
        super::append(
            conversations,
            &object.asked.asked.conversation,
            &Entry::noted(now, RESTARTED.to_owned(), Vec::new()),
        )?;
        reconciled += 1;
    }
    Ok(reconciled)
}

/// The sentence an act turn prints, one grammar for chain acts and dismissals alike. What it acted
/// on travels only in the turn's `references`, never in this text: a todo's id can be chosen by
/// mail the agent triaged, and an act line stamped by its actor reads clean.
#[must_use]
pub fn describe_act(actor: &str, verb: &str, note: &str) -> String {
    format!("{actor} {verb} it: {note}")
}

/// An actor as an act's sentence names them: their roster name, capitalised.
#[must_use]
pub fn named(actor: &str) -> String {
    let mut chars = actor.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// One act turn, built: what [`note_act`] appends and [`heard_from_objects`] computes.
#[derive(Debug, Clone)]
pub struct ActLine {
    /// The event's own instant, never the clock's, so an adopted act reads as when it happened.
    pub at: crate::capability::Datetime,
    /// Who did it: the actor the gate admitted, which stamps the line and [`named`] prints.
    pub actor: String,
    /// Past tense, such as "dismissed" or "completed".
    pub verb: &'static str,
    /// What the turn's `references` array carries.
    pub reference: Reference,
    /// The write's own why, untrimmed; [`note_act`] trims and gates on it.
    pub note: String,
}

/// Append an act's note to its conversation as a [`Turn::Act`](crate::capability::Turn::Act)
/// stamped `by`, what its writer decided of the actor and what the act points at
/// ([`super::stamp::pointing`]), or nothing when the note is empty.
///
/// # Errors
/// The store's own failures.
pub fn note_act(
    conversations: crate::secretary::conversations::Conversations<'_>,
    id: &ConversationId,
    line: &ActLine,
    by: Stamp,
) -> Result<(), Refusal> {
    let note = line.note.trim();
    if note.is_empty() {
        return Ok(());
    }
    super::append(
        conversations,
        id,
        &Entry::Said {
            at: line.at,
            turn: crate::capability::Turn::Act {
                text: describe_act(&named(&line.actor), line.verb, note),
                references: vec![line.reference.clone()],
            },
            by: Some(by),
        },
    )
}

/// The act lines sealed lifecycle events with a note carry, heard when the box adopts them.
///
/// Events not by `actor` are skipped even though adoption already refused them.
#[must_use]
pub fn heard_from_objects(
    events: &[crate::record::TodoEvent],
    actor: &prodrome_core::event::Actor,
    chain: &crate::chains::ChainName,
) -> Vec<ActLine> {
    events
        .iter()
        .filter_map(|event| {
            let (lifecycle, verb) = match event {
                crate::record::TodoEvent::Completed(lifecycle) => (lifecycle, "completed"),
                crate::record::TodoEvent::Cancelled(lifecycle) => (lifecycle, "cancelled"),
                crate::record::TodoEvent::Reopened(lifecycle) => (lifecycle, "reopened"),
                crate::record::TodoEvent::Tended(lifecycle) => (lifecycle, "tended"),
                _ => return None,
            };
            if lifecycle.actor != *actor || lifecycle.note.trim().is_empty() {
                return None;
            }
            let reference = Reference::Todo(TodoRef {
                chain: chain.clone(),
                id: lifecycle.todo.clone(),
            });
            Some(ActLine {
                at: lifecycle.at,
                actor: lifecycle.actor.as_str().to_owned(),
                verb,
                reference,
                note: lifecycle.note.clone(),
            })
        })
        .collect()
}

/// `Auth(account)`.
pub const AUTH: Signature = ("Auth", &["account"]);
/// `AuthSend(account)`.
pub const AUTH_SEND: Signature = ("AuthSend", &["account"]);
/// `Secret(name, why)`.
pub const SECRET: Signature = ("Secret", &["name", "why"]);
/// `Web(request, rule)`.
pub const WEB: Signature = ("Web", &["request", "rule"]);
/// `Patch(repo, visibility, base, patch, message)`.
pub const PATCH: Signature = ("Patch", &["repo", "visibility", "base", "patch", "message"]);
/// `DriveRead(path, size)`.
pub const DRIVE_READ: Signature = ("DriveRead", &["path", "size"]);
/// `CodingSession(task)`.
pub const CODING_SESSION: Signature = ("CodingSession", &["task"]);
/// `Device(actor, key)`.
pub const DEVICE: Signature = ("Device", &["actor", "key"]);
/// `SignIn(since)`.
pub const SIGN_IN: Signature = ("SignIn", &["since"]);
/// `SignInCode(url)`.
pub const SIGN_IN_CODE: Signature = ("SignInCode", &["url"]);
/// `UnlockStore(after)`.
pub const UNLOCK_STORE: Signature = ("UnlockStore", &["after"]);
/// `Connect(connection)`.
pub const CONNECT: Signature = ("Connect", &["connection"]);
/// `Disconnect(connection)`.
pub const DISCONNECT: Signature = ("Disconnect", &["connection"]);
/// `Grant(plugin, origin)`.
pub const GRANT: Signature = ("Grant", &["plugin", "origin"]);
/// `Erase(fact, keys)`.
pub const ERASE: Signature = ("Erase", &["fact", "keys"]);
/// `GoogleClient(redirect)`.
pub const GOOGLE_CLIENT: Signature = ("GoogleClient", &["redirect"]);
/// `GoogleSignIn(account, scopes)`.
pub const GOOGLE_SIGN_IN: Signature = ("GoogleSignIn", &["account", "scopes"]);
/// `CalendarEvent(account, title, when, place)`.
pub const CALENDAR_EVENT: Signature = ("CalendarEvent", &["account", "title", "when", "place"]);

impl Literal for Act {
    fn to_value(&self) -> Value {
        match self {
            Act::Auth { account } => record(AUTH, vec![account.to_value()]),
            Act::AuthSend { account } => record(AUTH_SEND, vec![account.to_value()]),
            Act::Secret { name, why } => record(SECRET, vec![name.to_value(), why.to_value()]),
            Act::Web { request, rule } => record(WEB, vec![request.to_value(), rule.to_value()]),
            Act::Patch {
                repo,
                visibility,
                base,
                patch,
                message,
            } => record(
                PATCH,
                vec![
                    repo.to_value(),
                    visibility.to_value(),
                    base.to_value(),
                    patch.to_value(),
                    message.to_value(),
                ],
            ),
            Act::DriveRead { path, size } => {
                record(DRIVE_READ, vec![path.to_value(), size.to_value()])
            }
            Act::CodingSession { task } => record(CODING_SESSION, vec![task.to_value()]),
            Act::Device(enrolment) => record(
                DEVICE,
                vec![
                    Value::str(enrolment.actor.as_str()),
                    Value::str(enrolment.key.as_str()),
                ],
            ),
            Act::SignIn { since } => record(SIGN_IN, vec![since.to_value()]),
            Act::SignInCode { url } => record(SIGN_IN_CODE, vec![url.to_value()]),
            Act::UnlockStore { after } => record(UNLOCK_STORE, vec![after.to_value()]),
            Act::Connect { connection } => record(CONNECT, vec![connection.to_value()]),
            Act::Disconnect { connection } => record(DISCONNECT, vec![connection.to_value()]),
            Act::Grant { plugin, origin } => {
                record(GRANT, vec![plugin.to_value(), origin.to_value()])
            }
            Act::GoogleClient { redirect } => record(GOOGLE_CLIENT, vec![redirect.to_value()]),
            Act::GoogleSignIn { account, scopes } => {
                record(GOOGLE_SIGN_IN, vec![account.to_value(), scopes.to_value()])
            }
            Act::CalendarEvent { account, event } => record(
                CALENDAR_EVENT,
                vec![
                    account.to_value(),
                    Value::str(event.title.as_str()),
                    Value::str(event.when.print()),
                    Value::str(&event.place),
                ],
            ),
            Act::Erase { fact, keys } => record(
                ERASE,
                vec![
                    fact.0.to_value(),
                    keys.iter()
                        .map(|key| key.as_str().to_owned())
                        .collect::<Vec<_>>()
                        .to_value(),
                ],
            ),
        }
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        Ok(match arm(value)? {
            "Auth" => Act::Auth {
                account: Fields::of(value, AUTH)?.get("account")?,
            },
            "AuthSend" => Act::AuthSend {
                account: Fields::of(value, AUTH_SEND)?.get("account")?,
            },
            "Secret" => {
                let fields = Fields::of(value, SECRET)?;
                Act::Secret {
                    name: fields.get("name")?,
                    why: fields.get("why")?,
                }
            }
            "Web" => {
                let fields = Fields::of(value, WEB)?;
                Act::Web {
                    request: fields.get("request")?,
                    rule: fields.get("rule")?,
                }
            }
            "Patch" => {
                let fields = Fields::of(value, PATCH)?;
                Act::Patch {
                    repo: fields.get("repo")?,
                    visibility: fields.get("visibility")?,
                    base: fields.get("base")?,
                    patch: fields.get("patch")?,
                    message: fields.get("message")?,
                }
            }
            "DriveRead" => {
                let fields = Fields::of(value, DRIVE_READ)?;
                Act::DriveRead {
                    path: fields.get("path")?,
                    size: fields.get("size")?,
                }
            }
            "CodingSession" => Act::CodingSession {
                task: Fields::of(value, CODING_SESSION)?.get("task")?,
            },
            "Device" => {
                let fields = Fields::of(value, DEVICE)?;
                Act::Device(crate::signatures::Enrolment {
                    actor: prodrome_core::event::Actor::new(fields.get::<String>("actor")?)?,
                    key: prodrome_core::sign::PublicKey::new(fields.get::<String>("key")?)?,
                })
            }
            "SignIn" => Act::SignIn {
                since: Fields::of(value, SIGN_IN)?.get("since")?,
            },
            "SignInCode" => Act::SignInCode {
                url: Fields::of(value, SIGN_IN_CODE)?.get("url")?,
            },
            "UnlockStore" => Act::UnlockStore {
                after: Fields::of(value, UNLOCK_STORE)?.get("after")?,
            },
            "Connect" => Act::Connect {
                connection: Fields::of(value, CONNECT)?.get("connection")?,
            },
            "Disconnect" => Act::Disconnect {
                connection: Fields::of(value, DISCONNECT)?.get("connection")?,
            },
            "Grant" => {
                let fields = Fields::of(value, GRANT)?;
                Act::Grant {
                    plugin: fields.get("plugin")?,
                    origin: fields.get("origin")?,
                }
            }
            arm @ ("GoogleClient" | "GoogleSignIn" | "CalendarEvent") => google(value, arm)?,
            "Erase" => {
                let fields = Fields::of(value, ERASE)?;
                Act::Erase {
                    fact: crate::secretary::memory::FactId(fields.get("fact")?),
                    keys: fields
                        .get::<Vec<String>>("keys")?
                        .into_iter()
                        .map(|key| {
                            crate::secretary::locked_text::KeyName::new(key)
                                .map_err(|failed| ProdromeError::invalid(failed.to_string()))
                        })
                        .collect::<Result<_, _>>()?,
                }
            }
            other => return Err(no_arm("an act the box performs", other)),
        })
    }
}

/// A Google act read back from its literal, each field through its constructor.
fn google(value: &Value, arm: &str) -> Result<Act, ProdromeError> {
    Ok(match arm {
        "GoogleClient" => Act::GoogleClient {
            redirect: Fields::of(value, GOOGLE_CLIENT)?.get("redirect")?,
        },
        "GoogleSignIn" => {
            let fields = Fields::of(value, GOOGLE_SIGN_IN)?;
            Act::GoogleSignIn {
                account: fields.get("account")?,
                scopes: fields.get("scopes")?,
            }
        }
        "CalendarEvent" => {
            let fields = Fields::of(value, CALENDAR_EVENT)?;
            let when = crate::calendar::When::parse(&fields.get::<String>("when")?)
                .map_err(|refused| ProdromeError::invalid(refused.to_string()))?;
            Act::CalendarEvent {
                account: fields.get("account")?,
                event: crate::google::NewEvent::new(
                    &fields.get::<String>("title")?,
                    when,
                    &fields.get::<String>("place")?,
                )
                .map_err(ProdromeError::invalid)?,
            }
        }
        other => return Err(no_arm("a Google act", other)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{Datetime, Turn};
    use crate::secretary::responded::{Done, Responded};
    use crate::secretary::store::{ConversationId, Entry, Proposal, Reasons};
    use crate::store::Prodrome as _;

    /// An in-memory conversation store, since this crate reaches no filesystem.
    #[derive(Default)]
    struct InMemory {
        lines: crate::secretary::conversations::InMemory,
        inbox: crate::store::InMemory<crate::secretary::inbox_event::InboxEvent>,
    }

    impl crate::capability::Clock for InMemory {
        fn now(&self) -> Datetime {
            now()
        }
    }

    fn now() -> Datetime {
        Datetime::new(2026, 9, 14, 9, 0, 0, 0).expect("a real instant")
    }

    fn reasons() -> Reasons {
        Reasons::new(
            "the sync has failed for want of a grant",
            vec!["briefing:2026-09-13".parse().expect("a reference")],
        )
        .expect("a reason with a source")
    }

    fn opened_at(store: &InMemory, id: &ConversationId) {
        crate::secretary::transcript::start(
            store.lines.conversations(),
            id,
            &Entry::Opened {
                at: crate::capability::Datetime::new(2026, 9, 14, 8, 0, 0, 0).expect("an instant"),
                title: crate::secretary::words::Words::kept("A grant".to_owned()),
                model: "a fake model".to_owned(),
                kind: crate::secretary::store::Kind::Topic,
            },
        )
        .expect("opens");
    }

    /// A proposal of `act` in `id`, tapped once to `performed`.
    fn tapped(store: &InMemory, id: &ConversationId, act: Act, performed: Performed) {
        opened_at(store, id);
        let asked = crate::secretary::inbox::Asked {
            proposer: crate::secretary::proposer::Proposer::Secretary,
            proposal: Proposal::Act(act),
            reasons: reasons(),
            asked: ProposalRef {
                conversation: id.clone(),
                index: 0,
            },
            price: crate::secretary::inbox::Price::unpriced(),
        };
        let object = crate::secretary::inbox::ask(
            &store.inbox,
            asked,
            &crate::secretary::writes_as(),
            now(),
        )
        .expect("filed");
        crate::secretary::transcript::push(
            store.lines.conversations(),
            id,
            &Entry::Proposed {
                at: crate::capability::Datetime::new(2026, 9, 14, 8, 59, 0, 0).expect("an instant"),
                object: object.clone(),
                revises: None,
            },
        )
        .expect("proposes");
        let owner = prodrome_core::event::Actor::new("ada").expect("an actor");
        crate::secretary::inbox::submit(&store.inbox, &object, &owner, now(), |_| Ok(performed))
            .expect("taps");
    }

    fn with_one_started(store: &InMemory) -> ConversationId {
        let id = ConversationId::new("2026-09-14-1").expect("a slug");
        tapped(
            store,
            &id,
            auth(),
            Performed::Tried(Tried::Running {
                began: "2026-09-14T09:00:00".to_owned(),
                expires_at: "2026-09-14T09:15:00".to_owned(),
            }),
        );
        id
    }

    /// What proposal 0 of `id` reads as answered.
    fn shown(
        store: &InMemory,
        id: &ConversationId,
    ) -> Option<Responded<crate::secretary::inbox::Check>> {
        let fold = crate::secretary::inbox::fold(&store.inbox).expect("the inbox reads");
        super::super::read(store.lines.conversations(), id)
            .expect("reads")
            .standing(0, &fold)
            .expect("proposal 0")
            .object
            .reading
            .shown()
            .map(|answer| answer.response.clone())
    }

    /// A running act ends as sent-nothing, with a matching `Turn::Act`, and counts once.
    #[test]
    fn a_standing_started_is_reconciled_once() {
        let store = InMemory::default();
        let id = with_one_started(&store);

        assert_eq!(
            reconcile(store.lines.conversations(), &store.inbox, now()).expect("reconciles"),
            1
        );

        match shown(&store, &id) {
            Some(Responded::Tried {
                tried: Tried::NotSent { reason },
                ..
            }) => {
                assert!(reason.contains("restarted"), "{reason}");
            }
            other => panic!("expected a try that sent nothing, got {other:?}"),
        }
        let stored = super::super::read(store.lines.conversations(), &id).expect("reads");
        let last = stored.turns.last().expect("a turn");
        match &last.turn {
            Turn::Act { text, .. } => assert!(text.contains("restarted"), "{text}"),
            other => panic!("expected an Act turn, got {other:?}"),
        }
    }

    #[test]
    fn running_reconcile_twice_appends_nothing_the_second_time() {
        let store = InMemory::default();
        let id = with_one_started(&store);

        assert_eq!(
            reconcile(store.lines.conversations(), &store.inbox, now()).expect("reconciles"),
            1
        );
        let after_first = super::super::read(store.lines.conversations(), &id)
            .expect("reads")
            .turns
            .len();

        assert_eq!(
            reconcile(store.lines.conversations(), &store.inbox, now()).expect("reconciles again"),
            0
        );
        let after_second = super::super::read(store.lines.conversations(), &id)
            .expect("reads again")
            .turns
            .len();
        assert_eq!(after_first, after_second, "nothing new was appended");
    }

    #[test]
    fn a_store_with_no_standing_started_is_untouched() {
        let store = InMemory::default();
        let acted_id = ConversationId::new("2026-09-14-2").expect("a slug");
        tapped(
            &store,
            &acted_id,
            auth(),
            Performed::Done(Done::Acted {
                what: "imap grant stored for ada@example.com".to_owned(),
            }),
        );
        let before = store.inbox.objects().expect("reads").len();

        assert_eq!(
            reconcile(store.lines.conversations(), &store.inbox, now()).expect("reconciles"),
            0
        );

        assert_eq!(store.inbox.objects().expect("reads").len(), before);
        assert!(matches!(
            shown(&store, &acted_id),
            Some(Responded::Done(Done::Acted { .. }))
        ));
    }

    fn auth() -> Act {
        Act::Auth {
            account: EmailAddress::new("ada@example.com").expect("a shaped address"),
        }
    }

    /// Sending's consent is an act of its own, naming SMTP, and prints and reads back as one;
    /// reading's still names IMAP alone.
    #[test]
    fn sending_is_its_own_act_and_names_smtp() {
        let send = Act::AuthSend {
            account: EmailAddress::new("ada@example.com").expect("a shaped address"),
        };
        assert_eq!(
            send.sentence(),
            "authorise ada@example.com to send mail (SMTP)"
        );
        assert_eq!(
            prodrome_core::literal::print_literal(&send.to_value()),
            "AuthSend(account='ada@example.com')"
        );
        assert_eq!(Act::from_value(&send.to_value()).as_ref(), Ok(&send));
        assert_ne!(Act::from_value(&auth().to_value()).as_ref(), Ok(&send));
    }

    /// The sign-in's two acts print and read back as literals, the code card naming its link.
    #[test]
    fn the_sign_in_acts_round_trip_as_literals() {
        for act in [
            Act::UnlockStore { after: None },
            Act::UnlockStore {
                after: Some("proposal:setup/3".parse().expect("a reference")),
            },
            Act::SignIn { since: None },
            Act::SignIn {
                since: Some("no model has answered since 2026-10-07 09:00 UTC".to_owned()),
            },
            Act::SignInCode {
                url: crate::secretary::sign_in::SignInUrl::parse(
                    "https://sign-in.example.test/oauth/authorize?code=true&state=s",
                )
                .expect("a link"),
            },
        ] {
            assert_eq!(Act::from_value(&act.to_value()).as_ref(), Ok(&act));
        }
    }

    /// An unlock card names the card it follows by its reference, printed as the text it was
    /// stored as before it was typed, and an `after` that is no proposal's reference is refused
    /// on reading.
    #[test]
    fn an_unlock_card_names_the_card_it_follows_by_reference() {
        let act = Act::UnlockStore {
            after: Some(ProposalRef {
                conversation: crate::secretary::store::ConversationId::new("setup")
                    .expect("a slug"),
                index: 3,
            }),
        };
        assert_eq!(
            prodrome_core::literal::print_literal(&act.to_value()),
            "UnlockStore(after='proposal:setup/3')"
        );
        let unparsed = record(
            UNLOCK_STORE,
            vec![prodrome_core::literal::Value::str("the third card")],
        );
        assert!(Act::from_value(&unparsed).is_err());
        let a_todo = record(
            UNLOCK_STORE,
            vec![prodrome_core::literal::Value::str("todo:life/reply")],
        );
        assert!(Act::from_value(&a_todo).is_err(), "a todo is no card");
    }

    /// A line round trips, and its tag names the act.
    /// The Google acts read back as they were stored, as literals and as JSON, and a stored
    /// event no constructor admits is refused.
    #[test]
    fn the_google_acts_round_trip() {
        let account = EmailAddress::new("ada@example.test").expect("an address");
        for act in [
            Act::GoogleClient {
                redirect: crate::google::RedirectUri::of_app("https://box.example.test")
                    .expect("a callback"),
            },
            Act::GoogleSignIn {
                account: account.clone(),
                scopes: crate::google::Scopes::all(),
            },
            Act::CalendarEvent {
                account: account.clone(),
                event: crate::google::NewEvent::new(
                    "Dentist",
                    crate::calendar::When::parse("2026-10-20T14:00:00Z/2026-10-20T14:45:00Z")
                        .expect("a span"),
                    "Main St 4",
                )
                .expect("an event"),
            },
        ] {
            assert_eq!(Act::from_value(&act.to_value()).as_ref(), Ok(&act));
            let printed = serde_json::to_string(&act).expect("serialises");
            assert_eq!(
                serde_json::from_str::<Act>(&printed).expect("reads back"),
                act
            );
        }
        let untitled = record(
            CALENDAR_EVENT,
            vec![
                account.to_value(),
                Value::str(""),
                Value::str("2026-10-20/2026-10-21"),
                Value::str(""),
            ],
        );
        assert!(Act::from_value(&untitled).is_err());
    }

    #[test]
    fn an_act_round_trips_and_names_itself() {
        let act = auth();
        let printed = serde_json::to_string(&act).expect("an act serialises");
        assert!(printed.contains(r#""kind":"auth""#), "{printed}");
        assert!(
            printed.contains(r#""account":"ada@example.com""#),
            "{printed}"
        );
        let read: Act = serde_json::from_str(&printed).expect("and reads back");
        assert_eq!(read, act);
    }

    fn a_todo_reference() -> Reference {
        Reference::Todo(TodoRef {
            chain: crate::chains::ChainName::new("life").expect("a slug"),
            id: prodrome_core::event::TodoId::new("x").expect("a todo id"),
        })
    }

    fn opened(store: &InMemory, id: &ConversationId) {
        crate::secretary::transcript::start(
            store.lines.conversations(),
            id,
            &Entry::Opened {
                at: crate::capability::Datetime::new(2026, 9, 15, 8, 0, 0, 0).expect("an instant"),
                title: crate::secretary::words::Words::kept("A chain act".to_owned()),
                model: "a fake model".to_owned(),
                kind: crate::secretary::store::Kind::Topic,
            },
        )
        .expect("opens");
    }

    /// Through the real writer, in the default conversation: a noted act is stamped by its actor,
    /// so a clean round the owner opens after it still writes the memory, and a mail read in
    /// the window still makes it a card.
    #[test]
    fn a_noted_act_in_the_default_conversation_leaves_his_round_clean() {
        use crate::capability::ToolCall;
        use crate::secretary::store::DEFAULT;
        let said = |turn: Turn, by: Option<&str>| Entry::Said {
            at: now(),
            turn,
            by: by.map(Into::into),
        };
        let person = |text: &str| Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        };
        let calling = |verb: &str| Turn::Model {
            text: String::new(),
            calls: vec![ToolCall {
                verb: verb.into(),
                args: std::collections::BTreeMap::new(),
            }],
        };
        let note = |store: &InMemory| {
            note_act(
                store.lines.conversations(),
                &DEFAULT,
                &ActLine {
                    at: now(),
                    actor: "ada".to_owned(),
                    verb: "dismissed",
                    reference: a_todo_reference(),
                    note: "not now".to_owned(),
                },
                Stamp::Actor("ada".to_owned()),
            )
            .expect("appends");
        };
        let ask = |store: &InMemory| {
            for line in [
                said(person("remember I moved"), Some("ada")),
                said(calling("remember"), None),
            ] {
                super::super::append(store.lines.conversations(), &DEFAULT, &line).expect("said");
            }
        };
        let clean = InMemory::default();
        opened(&clean, &DEFAULT);
        note(&clean);
        ask(&clean);
        let stored = super::super::read(clean.lines.conversations(), &DEFAULT).expect("reads");
        assert!(matches!(
            &stored.turns[0].by,
            Some(Stamp::Actor(actor)) if actor == "ada"
        ));
        assert!(!crate::secretary::taint::tainted(&stored));
        assert!(crate::secretary::taint::opener(&stored).is_some());

        let read = InMemory::default();
        opened(&read, &DEFAULT);
        super::super::append(
            read.lines.conversations(),
            &DEFAULT,
            &said(calling("search"), None),
        )
        .expect("said");
        super::super::append(
            read.lines.conversations(),
            &DEFAULT,
            &said(
                Turn::Tool {
                    verb: "search".into(),
                    answer: "an email".to_owned(),
                    outcome: crate::secretary::outcome::Outcome::Answered,
                    files: Vec::new(),
                },
                None,
            ),
        )
        .expect("said");
        note(&read);
        ask(&read);
        let stored = super::super::read(read.lines.conversations(), &DEFAULT).expect("reads");
        assert!(
            crate::secretary::taint::tainted(&stored),
            "a read in the window"
        );
    }

    /// Only the poster's own lifecycle acts with a note are heard: an empty note, an event naming a
    /// different actor and a non-lifecycle event are each skipped.
    #[test]
    fn heard_from_objects_skips_what_carries_no_note_of_the_posters() {
        let actor = prodrome_core::event::Actor::new("ada").expect("an actor");
        let chain = crate::chains::ChainName::new("life").expect("a slug");
        for event in [
            prodrome_core::event::mk_cancelled::<crate::record::Record>("x", now(), "ada", "")
                .expect("a cancelled event"),
            prodrome_core::event::mk_reopened::<crate::record::Record>(
                "x",
                now(),
                "triage",
                "wrong call",
            )
            .expect("a reopened event"),
            prodrome_core::event::mk_created::<crate::record::Record>(
                "x",
                now(),
                "ada",
                "text",
                "a note",
            )
            .expect("a created event"),
        ] {
            assert!(
                heard_from_objects(std::slice::from_ref(&event), &actor, &chain).is_empty(),
                "{event:?}"
            );
        }
    }
}
