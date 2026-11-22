//! How a connection proves it works.

/// What a connection's check observes: an outcome the box computes, never words a service sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    Synced,
    MailSynced,
    Listed,
    Answered,
    Pushed,
    Delivered,
}

impl Check {
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
