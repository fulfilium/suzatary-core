//! Waking a phone, a capability the agent's bundle does not carry.

use std::num::NonZeroUsize;

use crate::reference::MailHash;
use crate::secretary::inbox::ProposalId;
use crate::secretary::store::ConversationId;

/// What a push says happened. A closed sum whose every field is a count, a day or an id the
/// box minted, so no stored text has a field to ride in: a relay (Google's, ntfy's) carries it
/// in the clear, and an OS this box does not control draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Push {
    /// Objects of the agent's reached a chain: how many todos it wrote and how many other changes.
    Agent {
        /// Todos authored or rewritten.
        authored: usize,
        /// Completions, cancellations, reopenings, tendings and spec revisions.
        changes: usize,
    },
    /// Things newly waiting for the owner.
    Waiting {
        /// How many.
        count: NonZeroUsize,
        /// The one proposal, when exactly one waits and it is a proposal (its id is a hash).
        open: Option<ProposalId>,
    },
    /// A day's briefing is saved.
    Brief {
        /// Its day.
        day: chrono::NaiveDate,
    },
    /// The morning brief was due and was not written.
    NoBrief,
    /// The secretary finished a round nobody was watching.
    Answered {
        /// Where, by the id the box minted.
        conversation: ConversationId,
        /// Whether the round was the morning's brief.
        brief: bool,
    },
    /// Conversations the store holds do not read, newly: their reviews are left out of the rest.
    Unreadable {
        /// How many.
        conversations: NonZeroUsize,
        /// Today's review, which names them.
        round: ConversationId,
    },
    /// A message a watch waited for synced.
    Expected {
        /// The message.
        message: MailHash,
    },
    /// The secretary's review has failed `passes` passes running.
    ReviewDown {
        /// Cards left open.
        standing: usize,
        /// Consecutive failed passes.
        passes: NonZeroUsize,
    },
}

/// A [`Push`] as the channels send it: a heading and a line composed here, and the link a tap
/// follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printed {
    /// The notification's heading, one of this module's constants.
    pub title: &'static str,
    /// Its line, composed from the push's numbers and day alone.
    pub body: String,
    /// Where tapping it lands, inside `app`.
    pub click: String,
    /// The relay's icon shorthand (ntfy's `Tags`), naming a shape, never a subject.
    pub tag: &'static str,
}

impl Push {
    /// The words and link for the app at `app`.
    #[must_use]
    pub fn printed(&self, app: &str) -> Printed {
        let plural =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        match self {
            Push::Agent { authored, changes } => Printed {
                title: "Suzatary",
                body: {
                    let parts: Vec<String> = [
                        (*authored, "new or updated todo", "new or updated todos"),
                        (*changes, "change", "changes"),
                    ]
                    .into_iter()
                    .filter(|(n, _, _)| *n > 0)
                    .map(|(n, one, many)| plural(n, one, many))
                    .collect();
                    if parts.is_empty() {
                        "Nothing new from the agent".to_owned()
                    } else {
                        format!("{} from the agent", parts.join(" and "))
                    }
                },
                click: format!("{app}/#/"),
                tag: "inbox_tray",
            },
            Push::Waiting { count, open } => Printed {
                title: "Suzatary needs you",
                body: format!(
                    "{} waiting for you",
                    plural(count.get(), "thing is", "things are")
                ),
                click: match open {
                    Some(id) => format!(
                        "{app}/#/inbox?open={}",
                        crate::url::percent_encode(id.as_str())
                    ),
                    None => format!("{app}/#/inbox"),
                },
                tag: "bell",
            },
            Push::Brief { day } => Printed {
                title: "Your morning brief",
                body: format!("The brief for {day} is ready."),
                click: format!("{app}/#/briefings/{day}"),
                tag: "newspaper",
            },
            Push::NoBrief => Printed {
                title: "No morning brief",
                body: "This morning's brief was not written; the box's log says why.".to_owned(),
                click: format!("{app}/#/secretary"),
                tag: "warning",
            },
            Push::Answered {
                conversation,
                brief,
            } => Printed {
                title: if *brief {
                    "Your brief is ready"
                } else {
                    "The secretary answered"
                },
                body: "Open the conversation to read it.".to_owned(),
                click: format!("{app}/#/secretary/{}", conversation.as_str()),
                tag: "speech_balloon",
            },
            Push::Unreadable {
                conversations,
                round,
            } => Printed {
                title: "Conversations do not read",
                body: format!(
                    "{} the box cannot read; the review says which",
                    plural(conversations.get(), "conversation", "conversations")
                ),
                click: format!("{app}/#/secretary/{}", round.as_str()),
                tag: "warning",
            },
            Push::Expected { message } => Printed {
                title: "The mail you were waiting for",
                body: "A message you asked to hear about has arrived.".to_owned(),
                click: format!("{app}/#/mail/{}", message.as_str()),
                tag: "envelope",
            },
            Push::ReviewDown { standing, passes } => Printed {
                title: "The secretary's review is down",
                body: format!(
                    "{} waiting; it has failed {} running",
                    plural(*standing, "card", "cards"),
                    plural(passes.get(), "pass", "passes")
                ),
                click: format!("{app}/#/inbox"),
                tag: "warning",
            },
        }
    }
}

/// How many devices the service accepted a [`Notify`] for; only the interpreter can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivered(usize);

impl Delivered {
    /// The interpreter's report; zero is a normal answer, logged and never thrown.
    #[must_use]
    pub fn to_devices(count: usize) -> Self {
        Self(count)
    }

    /// How many devices it reached.
    #[must_use]
    pub fn devices(self) -> usize {
        self.0
    }
}

/// The push service refused or could not be reached.
#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    /// The service was unreachable or answered something unintelligible.
    #[error("the push service could not be reached: {0}")]
    Unreachable(String),
    /// The service rejected the message itself; the token store is what to correct.
    #[error("the push service refused the message: {0}")]
    Refused(String),
}

/// Waking a phone.
pub trait Notify: Sync {
    /// Send one composed notification.
    fn push(&self, message: &Push) -> Result<Delivered, NotifyError>;
}
