//! An act the box performs itself when the owner taps its proposal.
//!
//! `Auth` has no `scope` field on purpose: its tap mints the IMAP grant alone. Sending's consent
//! is `AuthSend`, a card only the box files, kept in the SMTP slot apart from the IMAP one, so a
//! leaked read token still cannot send.

use prodrome_core::literal::{ProdromeError, Value};
use serde::{Deserialize, Serialize};

use crate::mail::address::EmailAddress;
use prodrome_core::literal::codec::{Constructor, Fields, Literal, arm, no_arm, record};
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::ProposalRef;

/// One act, from a closed set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Act {
    /// Authorise a Microsoft account for mail sync through the device-code flow, IMAP only.
    Auth {
        /// The account to authorise.
        account: EmailAddress,
    },
    /// Authorise a Microsoft account to send mail through the device-code flow, SMTP only.
    #[serde(rename = "auth_send")]
    AuthSend {
        /// The account to authorise.
        account: EmailAddress,
    },
    /// Store a deployment-declared secret, asked for in the Inbox.
    ///
    /// No `value` field on purpose: the value travels only in the tap's body, never stored.
    Secret {
        /// The secret's name as declared when asked; the tap mints it against the names
        /// declared then, so a card cannot outlive its declaration.
        name: String,
        /// Why the secretary is asking.
        why: String,
    },
    /// Read the web once: a request no allow rule covers, sent only on a tap.
    Web {
        request: crate::web::WebRequest,
        /// The rule the secretary drafted for "Always allow", if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<crate::web::AllowRule>,
    },
    /// Propose this diff against this commit as a pull request; a person merges it.
    Patch {
        repo: suzatary_base::repo_ids::RepoName,
        /// Who can read the branch once pushed, as the listing said when proposed.
        visibility: crate::repos::Visibility,
        /// The commit the diff was written against.
        base: suzatary_base::repo_ids::CommitId,
        patch: crate::repos::Patch,
        message: String,
    },
    /// Read one of the owner's documents in a folder no `drive_allow` entry covers, redacted.
    #[serde(rename = "drive_read")]
    DriveRead {
        path: suzatary_base::drive_path::DrivePath,
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
    /// Trust a new device's key (SPEC law 42), on a tap a trusted device endorses.
    Device(suzatary_signatures::signatures::Enrolment),
    /// Sign the model in: the tap runs the sign-in on the box, which files the code's card.
    #[serde(rename = "sign_in")]
    SignIn {
        /// The episode in the box's words, when a model last answered; `None` before any has.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<String>,
    },
    /// The code an open sign-in waits for, typed on the page its link opens.
    ///
    /// No `code` field on purpose: the code travels only in the tap's body, and its token only
    /// into the secret store.
    #[serde(rename = "sign_in_code")]
    SignInCode {
        /// The sign-in's link, as the box read it off the CLI.
        url: crate::secretary::sign_in::SignInUrl,
    },
    /// Ask the secret store again, which could not answer for a declared name; the passphrase is
    /// typed at the box.
    #[serde(rename = "unlock_store")]
    UnlockStore {
        /// Where the last unlock card not answered no was asked, so a store that answers and
        /// locks again is a new card.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<ProposalRef>,
    },
    /// Write a connection's table into the instance file.
    Connect {
        connection: crate::connect::Connection,
    },
    /// Take a connection's table out; a mail account's mail goes only on the card's wider yes.
    Disconnect {
        connection: crate::connect::Connection,
    },
    /// Let a plugin reach one more origin, naming the origin and never the link.
    Grant {
        plugin: crate::health::plugin::PluginName,
        origin: crate::health::plugin::Origin,
    },
    /// Keep the box's Google Cloud client: the tap's value is its downloaded file, which travels
    /// only in the tap's body and is kept only when it lists `redirect`.
    #[serde(rename = "google_client")]
    GoogleClient {
        /// The box's callback, as the client must list it.
        redirect: crate::google::RedirectUri,
    },
    /// Sign a Google account in with Google, bound to the tapping owner.
    #[serde(rename = "google_sign_in")]
    GoogleSignIn {
        account: EmailAddress,
        scopes: crate::google::Scopes,
    },
    /// Write one event into a Google account's own calendar, as the card shows it.
    #[serde(rename = "calendar_event")]
    CalendarEvent {
        account: EmailAddress,
        event: crate::google::NewEvent,
    },
    /// Erase these words, which may restate a forgotten fact its erasure left in other words.
    Erase {
        /// The fact whose forgetting offers it.
        fact: crate::secretary::memory::FactId,
        keys: std::collections::BTreeSet<crate::secretary::locked_text::KeyName>,
    },
    /// Retract the summary `node` shows, as the memory page's Wrong does.
    Retract {
        /// The node, by its content name.
        #[serde(with = "node_text")]
        node: prodrome_core::event::Hash,
    },
    /// Ask the owner's request again in a fresh topic, copying turn `turn` only while it is his
    /// own clean words ([`crate::secretary::afresh::Request::at_tap`]).
    Afresh {
        /// Where the request was said and the card is drawn.
        conversation: ConversationId,
        /// The request's index among that conversation's turns.
        turn: usize,
    },
    /// Leave every later message from `sender` out of the triage queue, as asked on `message`.
    Noise {
        /// The address, as the queue lists the message's sender.
        sender: String,
        /// The queued message the sender was read off, by its canonical hash.
        message: String,
    },
    /// Apply the changes of the triage card `card` the secretary withdrew.
    Restore {
        card: suzatary_base::reference::ProposalRef,
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
            Act::Afresh { .. } => "afresh",
            Act::Retract { .. } => "retract",
            Act::Noise { .. } => "noise",
            Act::Restore { .. } => "restore",
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
                 it needs{}",
                connection.called(),
                connection.writes(),
                if connection.id() == crate::connect::TemplateId::Google {
                    "; an address lieer reads already keeps its entry, read by lieer, until its \
                     sign-in with Google is kept and has read its mail"
                } else {
                    ""
                }
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
            Act::Retract { node } => retracting(node),
            Act::Afresh { .. } => "start it in a fresh conversation: a new topic that holds only \
                                   your request, word for word, and nothing this one read"
                .to_owned(),
            Act::Noise { sender, message } => silencing(sender, message),
            Act::Restore { card } => format!("Apply the changes of {card} after all"),
        }
    }
}

/// A noise card's imperative, naming the message it was asked on by the start of its hash.
fn silencing(sender: &str, message: &str) -> String {
    format!(
        "leave every later message from {sender} out of triage, as the triage agent asked on \
         mail:{}",
        message.chars().take(12).collect::<String>()
    )
}

/// A retraction's imperative, naming the summary by the start of its node's name.
fn retracting(node: &prodrome_core::event::Hash) -> String {
    format!(
        "mark wrong the summary {}… on the Memory page: it is deleted, never shown to a model \
         again, and written again from your lines, with every summary above it",
        node.as_str().chars().take(12).collect::<String>()
    )
}

/// The folder a document is in: the one "Always allow this folder" names.
#[must_use]
pub fn folder_of(
    path: &suzatary_base::drive_path::DrivePath,
) -> suzatary_base::drive_path::DrivePath {
    path.parent()
        .unwrap_or_else(suzatary_base::drive_path::DrivePath::root)
}

/// `Auth(account)`.
pub const AUTH: Constructor = ("Auth", &["account"]);
/// `AuthSend(account)`.
pub const AUTH_SEND: Constructor = ("AuthSend", &["account"]);
/// `Secret(name, why)`.
pub const SECRET: Constructor = ("Secret", &["name", "why"]);
/// `Web(request, rule)`.
pub const WEB: Constructor = ("Web", &["request", "rule"]);
/// `Patch(repo, visibility, base, patch, message)`.
pub const PATCH: Constructor = ("Patch", &["repo", "visibility", "base", "patch", "message"]);
/// `DriveRead(path, size)`.
pub const DRIVE_READ: Constructor = ("DriveRead", &["path", "size"]);
/// `CodingSession(task)`.
pub const CODING_SESSION: Constructor = ("CodingSession", &["task"]);
/// `Device(actor, key)`.
pub const DEVICE: Constructor = ("Device", &["actor", "key"]);
/// `SignIn(since)`.
pub const SIGN_IN: Constructor = ("SignIn", &["since"]);
/// `SignInCode(url)`.
pub const SIGN_IN_CODE: Constructor = ("SignInCode", &["url"]);
/// `UnlockStore(after)`.
pub const UNLOCK_STORE: Constructor = ("UnlockStore", &["after"]);
/// `Connect(connection)`.
pub const CONNECT: Constructor = ("Connect", &["connection"]);
/// `Disconnect(connection)`.
pub const DISCONNECT: Constructor = ("Disconnect", &["connection"]);
/// `Grant(plugin, origin)`.
pub const GRANT: Constructor = ("Grant", &["plugin", "origin"]);
/// `Erase(fact, keys)`.
pub const ERASE: Constructor = ("Erase", &["fact", "keys"]);
/// `Afresh(turn)`.
pub const AFRESH: Constructor = ("Afresh", &["conversation", "turn"]);
/// `Retract(node)`.
pub const RETRACT: Constructor = ("Retract", &["node"]);
/// `Noise(sender, message)`.
pub const NOISE: Constructor = ("Noise", &["sender", "message"]);
/// `Restore(card)`.
pub const RESTORE: Constructor = ("Restore", &["card"]);
/// `GoogleClient(redirect)`.
pub const GOOGLE_CLIENT: Constructor = ("GoogleClient", &["redirect"]);
/// `GoogleSignIn(account, scopes)`.
pub const GOOGLE_SIGN_IN: Constructor = ("GoogleSignIn", &["account", "scopes"]);
/// `CalendarEvent(account, title, when, place)`.
pub const CALENDAR_EVENT: Constructor = ("CalendarEvent", &["account", "title", "when", "place"]);

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
            Act::Afresh { conversation, turn } => {
                record(AFRESH, vec![conversation.to_value(), turn.to_value()])
            }
            Act::Retract { node } => record(RETRACT, vec![node.to_value()]),
            Act::Noise { sender, message } => {
                record(NOISE, vec![sender.to_value(), message.to_value()])
            }
            Act::Restore { card } => record(RESTORE, vec![card.to_string().to_value()]),
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
                Act::Device(suzatary_signatures::signatures::Enrolment {
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
            "Afresh" => {
                let fields = Fields::of(value, AFRESH)?;
                Act::Afresh {
                    conversation: fields.get("conversation")?,
                    turn: fields.get("turn")?,
                }
            }
            "Noise" => noise(value)?,
            "Restore" => restore(value)?,
            "Retract" => Act::Retract {
                node: Fields::of(value, RETRACT)?.get("node")?,
            },
            other => return Err(no_arm("an act the box performs", other)),
        })
    }
}

/// `Restore(card)`, read.
fn restore(value: &Value) -> Result<Act, ProdromeError> {
    Ok(Act::Restore {
        card: Fields::of(value, RESTORE)?
            .get::<String>("card")?
            .parse()
            .map_err(|failed| ProdromeError::invalid(format!("card: {failed}")))?,
    })
}

/// `Noise(sender, message)`, read.
fn noise(value: &Value) -> Result<Act, ProdromeError> {
    let fields = Fields::of(value, NOISE)?;
    Ok(Act::Noise {
        sender: fields.get("sender")?,
        message: fields.get("message")?,
    })
}

/// A node's content name as its text, as serde spells it.
mod node_text {
    use prodrome_core::event::Hash;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(node: &Hash, to: S) -> Result<S::Ok, S::Error> {
        to.serialize_str(node.as_str())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<Hash, D::Error> {
        Hash::new(&String::deserialize(from)?).map_err(serde::de::Error::custom)
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
    use crate::mail::address::EmailAddress;

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
                conversation: suzatary_base::conversation_id::ConversationId::new("setup")
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
}
