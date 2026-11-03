//! Where a page's links go and what it asks the box, printed once: the app's addresses as
//! `web/src/shell/route.ts` reads them, and the box's routes as `app/cli` answers them.

use crate::runtime::{component, search};

/// The rows the box answers per page of mail (its own cap, `MAX_MAIL`), so "Load more" asks for
/// a page.
pub const PAGE: usize = 50;

/// A thread longer than this comes back as its newest messages: the end, the part being read.
pub const THREAD: usize = 100;

/// A place in the app.
#[derive(Debug, Clone, Copy)]
pub enum Place<'a> {
    /// The mail, by a notmuch query (`""` for the inbox) and an account (`""` for every one).
    Mail {
        /// The query.
        query: &'a str,
        /// The account.
        account: &'a str,
    },
    /// One message, by its notmuch id or a prefix of its hash.
    Message(&'a str),
    /// One thread.
    Thread(&'a str),
    /// The secretary, asked about a reference (`mail:<hash>`, `thread:<id>`).
    Ask(&'a str),
    /// The secretary's stream.
    Secretary,
    /// A conversation: opened at a turn, or with a reference attached (`""` for none).
    Conversation {
        /// Its id.
        id: &'a str,
        /// A reference to attach.
        reference: &'a str,
        /// The turn its window ends at.
        at: Option<u32>,
    },
    /// The chain page, nothing focused.
    Chain,
    /// The saved briefings.
    Briefings,
    /// One day's briefing.
    Briefing(&'a str),
    /// A week of the calendars from a day.
    Calendar(&'a str),
}

impl Place<'_> {
    /// Its address.
    #[must_use]
    pub fn href(self) -> String {
        match self {
            Place::Mail { query, account } => {
                format!("#/mail{}", search(&[("q", query), ("account", account)]))
            }
            Place::Message(id) => format!("#/mail/{}", component(id)),
            Place::Thread(id) => format!("#/mail/thread/{}", component(id)),
            Place::Ask(reference) => format!("#/secretary{}", search(&[("ref", reference)])),
            Place::Secretary => "#/secretary".to_owned(),
            Place::Conversation { id, reference, at } => {
                let at = at.map(|t| t.to_string()).unwrap_or_default();
                format!(
                    "#/secretary/{}{}",
                    component(id),
                    search(&[("ref", reference), ("at", &at)])
                )
            }
            Place::Chain => "#/chain".to_owned(),
            Place::Briefings => "#/briefings".to_owned(),
            Place::Briefing(day) => format!("#/briefings/{}", component(day)),
            Place::Calendar(day) => format!("#/calendar/{}", component(day)),
        }
    }
}

/// A question the box answers.
#[derive(Debug, Clone, Copy)]
pub enum Question<'a> {
    /// A page of the inbox by date, from `before` (`""` for the newest).
    Inbox {
        /// The account, `""` for every one.
        account: &'a str,
        /// The oldest row's instant already shown; the box answers rows older than it.
        before: &'a str,
    },
    /// A page of a notmuch query, paged as the inbox is.
    Search {
        /// The query.
        q: &'a str,
        /// The account, `""` for every one.
        account: &'a str,
        /// As the inbox's.
        before: &'a str,
    },
    /// One message and the todos it produced.
    Message(&'a str),
    /// One thread, oldest first.
    Thread(&'a str),
    /// Every watch on expected mail that waits.
    Watches,
    /// A watch cancelled (a POST), answered by the watches that still wait.
    Cancel(&'a str),
    /// The secretary's memory.
    Memory,
    /// A fact forgotten (a POST).
    Forget(u64),
    /// The summaries of the owner's words a conversation is told.
    Record,
    /// A summary written again from the owner's lines (a POST).
    Retract(&'a str),
    /// The briefs and the saved briefings.
    Briefings,
    /// One day's briefing.
    Briefing(&'a str),
    /// A week of the calendars from a day.
    Calendar(&'a str),
    /// What the box runs.
    About,
    /// The releases the project chain reads.
    Releases,
}

impl Question<'_> {
    /// Its path.
    #[must_use]
    pub fn path(self) -> String {
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
            Question::Memory => "/api/secretary/memory".to_owned(),
            Question::Forget(id) => format!("/api/secretary/memory/{id}/forget"),
            Question::Record => "/api/secretary/record".to_owned(),
            Question::Retract(node) => format!("/api/secretary/record/{}/retract", component(node)),
            Question::Briefings => "/api/briefings".to_owned(),
            Question::Briefing(day) => format!("/api/briefings/{}", component(day)),
            Question::Calendar(day) => format!("/api/calendar/{}", component(day)),
            Question::About => "/api/about".to_owned(),
            Question::Releases => "/api/releases".to_owned(),
        }
    }
}
