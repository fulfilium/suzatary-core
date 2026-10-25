//! How a connection proves it works.

/// What a connection's check observes: a number the box computes, never words a service sent,
/// joined over time as each mail account's sync results are (`mail::health`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// The plugin fetched and admitted its items.
    Synced,
    /// The mail account synced.
    MailSynced,
    /// The Drive's folder lists.
    Listed,
    /// The model answered a call.
    Answered,
    /// The data repository pushed to its remote.
    Pushed,
    /// A notification was delivered.
    Delivered,
}

impl Check {
    /// What a success means, as a line says it.
    #[must_use]
    pub const fn sentence(self) -> &'static str {
        match self {
            Check::Synced => "its items were fetched and admitted",
            Check::MailSynced => "its mail synced",
            Check::Listed => "its folder lists",
            Check::Answered => "a model answered",
            Check::Pushed => "it pushed",
            Check::Delivered => "a notification was delivered",
        }
    }
}
