//! The app's addresses as `web/src/shell/route.ts` reads them, and the box's routes as `app/cli`
//! answers them, each printed in one place.

use crate::runtime::{component, search};

/// The box's own cap on mail rows per page (`MAX_MAIL`).
pub(crate) const PAGE: usize = 50;

/// A longer thread comes back as its newest messages.
pub(crate) const THREAD: usize = 100;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Place<'a> {
    /// `""` for the inbox and for every account.
    Mail {
        query: &'a str,
        account: &'a str,
    },
    /// By its notmuch id or a prefix of its hash.
    Message(&'a str),
    Thread(&'a str),
    /// The secretary, asked about a reference (`mail:<hash>`, `thread:<id>`).
    Ask(&'a str),
    /// The stream, filtered to the rounds carrying every topic named.
    Secretary(&'a [String]),
    Memory,
    NewTopic,
    Inbox(&'a str),
    Settings,
    Todo(&'a str),
    Todos,
    /// A todo open, `""` for none.
    Typst(&'a str),
    /// An uploaded file, served as itself by the box.
    Attachment(&'a str),
    /// A todo's route drops the chain; a proposal, round or note goes to its conversation, a
    /// batch to the Inbox.
    Reference(&'a suzatary_core::reference::Reference),
    Conversation {
        id: &'a str,
        /// `""` for none.
        reference: &'a str,
        focus: bool,
        at: Option<u32>,
    },
    Briefings,
    Briefing(&'a str),
    Calendar(&'a str),
}

impl<'a> Place<'a> {
    /// Opened at its newest turn with nothing attached.
    #[must_use]
    pub(crate) const fn conversation(id: &'a str) -> Place<'a> {
        Place::Conversation {
            id,
            reference: "",
            focus: false,
            at: None,
        }
    }

    #[must_use]
    pub(crate) fn href(self) -> String {
        match self {
            Place::Mail { query, account } => {
                format!("#/mail{}", search(&[("q", query), ("account", account)]))
            }
            Place::Message(id) => format!("#/mail/{}", component(id)),
            Place::Thread(id) => format!("#/mail/thread/{}", component(id)),
            Place::Ask(reference) => format!("#/secretary{}", search(&[("ref", reference)])),
            Place::Secretary(topics) => {
                format!("#/secretary{}", search(&[("topics", &topics.join(","))]))
            }
            Place::Memory => "#/secretary/memory".to_owned(),
            Place::NewTopic => "#/secretary/new".to_owned(),
            Place::Inbox(open) => format!("#/inbox{}", search(&[("open", open)])),
            Place::Settings => "#/settings".to_owned(),
            Place::Todo(id) => format!("#/todo/{}", component(id)),
            Place::Todos => "#/".to_owned(),
            Place::Typst("") => "#/typst".to_owned(),
            Place::Typst(open) => format!("#/typst/{}", component(open)),
            Place::Attachment(id) => format!("/api/secretary/attachments/{}", component(id)),
            Place::Reference(r) => {
                use suzatary_core::reference::Reference;
                match r {
                    Reference::Todo(t) => Place::Todo(t.id.as_str()).href(),
                    Reference::Mail { hash } => Place::Message(hash.as_str()).href(),
                    Reference::Thread { id } => Place::Thread(id.as_str()).href(),
                    Reference::Briefing { date } => {
                        Place::Briefing(&date.format("%Y-%m-%d").to_string()).href()
                    }
                    Reference::Proposal(p) => Place::conversation(p.conversation.as_str()).href(),
                    Reference::Batch { .. } => Place::Inbox("").href(),
                    Reference::Target(
                        suzatary_core::topic_target::Target::Round { conversation, .. }
                        | suzatary_core::topic_target::Target::Note { conversation, .. },
                    ) => Place::conversation(conversation.as_str()).href(),
                }
            }
            Place::Conversation {
                id,
                reference,
                focus,
                at,
            } => {
                let at = at.map(|t| t.to_string()).unwrap_or_default();
                let focus = if focus { "1" } else { "" };
                format!(
                    "#/secretary/{}{}",
                    component(id),
                    search(&[("ref", reference), ("focus", focus), ("at", &at)])
                )
            }
            Place::Briefings => "#/briefings".to_owned(),
            Place::Briefing(day) => format!("#/briefings/{}", component(day)),
            Place::Calendar(day) => format!("#/calendar/{}", component(day)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Question<'a> {
    Inbox {
        /// `""` for every one.
        account: &'a str,
        /// The oldest row's instant already shown; the box answers older rows. `""` for the
        /// newest.
        before: &'a str,
    },
    /// A notmuch query, paged as the inbox is.
    Search {
        q: &'a str,
        account: &'a str,
        before: &'a str,
    },
    Message(&'a str),
    Thread(&'a str),
    Watches,
    /// A POST, answered by the watches that still wait.
    Cancel(&'a str),
    Overview,
    /// A POST beginning a setup conversation on a template, answered by the conversation.
    Setup(&'a str),
    /// A POST filing the card that asks for a new value of a stored secret, by its handle.
    Replace(&'a str),
    /// A POST filing this device's card for a trusted device to tap.
    Key,
    /// A POST trusting this device with the code from the box.
    Pair,
    Memory,
    Forget(u64),
    /// The summaries of the owner's words a conversation is told.
    Record,
    /// A POST rewriting a summary from the owner's lines.
    Retract(&'a str),
    Briefings,
    Briefing(&'a str),
    Calendar(&'a str),
    About,
    Releases,
    /// One todo in full.
    Todo {
        /// The chain.
        chain: &'a str,
        /// The todo.
        id: &'a str,
    },
    /// The coding session's card that builds a todo filed (a POST).
    Build {
        /// The chain.
        chain: &'a str,
        /// The todo.
        id: &'a str,
    },
    /// An event the box prints for a chain and appends nothing of (a POST).
    Author(&'a str),
}

impl Question<'_> {
    #[must_use]
    pub(crate) fn path(self) -> String {
        let limit = PAGE.to_string();
        match self {
            Question::Inbox { account, before } => format!(
                "/api/mail/inbox{}",
                search(&[("limit", &limit), ("account", account), ("before", before)])
            ),
            Question::Search { q, account, before } => format!(
                "/api/mail/search{}",
                search(&[
                    ("q", q),
                    ("limit", &limit),
                    ("account", account),
                    ("before", before),
                ])
            ),
            Question::Message(id) => format!("/api/mail/message/{}", component(id)),
            Question::Thread(id) => format!("/api/mail/thread/{}?limit={THREAD}", component(id)),
            Question::Watches => "/api/mail/watches".to_owned(),
            Question::Cancel(id) => format!("/api/mail/watches/{}/cancel", component(id)),
            Question::Overview => "/api/overview".to_owned(),
            Question::Setup(template) => format!("/api/secretary/setup/{}", component(template)),
            Question::Replace(handle) => format!("/api/secretary/replace/{}", component(handle)),
            Question::Key => "/api/act/key".to_owned(),
            Question::Pair => "/api/act/pair".to_owned(),
            Question::Memory => "/api/secretary/memory".to_owned(),
            Question::Forget(id) => format!("/api/secretary/memory/{id}/forget"),
            Question::Record => "/api/secretary/record".to_owned(),
            Question::Retract(node) => format!("/api/secretary/record/{}/retract", component(node)),
            Question::Briefings => "/api/briefings".to_owned(),
            Question::Briefing(day) => format!("/api/briefings/{}", component(day)),
            Question::Calendar(day) => format!("/api/calendar/{}", component(day)),
            Question::About => "/api/about".to_owned(),
            Question::Releases => "/api/releases".to_owned(),
            Question::Todo { chain, id } => {
                format!("/api/{}/todo/{}", component(chain), component(id))
            }
            Question::Build { chain, id } => {
                format!("/api/{}/todo/{}/build", component(chain), component(id))
            }
            Question::Author(chain) => format!("/api/{}/author", component(chain)),
        }
    }
}
