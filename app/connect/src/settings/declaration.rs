//! Which instance table declares a secret.

use crate::settings::remote::Pushed;
use suzatary_health::health::plugin::PluginName;

/// One table naming a secret; a name two tables declare is declared by both.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Declaration {
    Model,
    Channel(crate::connect::Channel),
    Coding,
    DataPush,
    /// `[[secretary.repos]]`.
    Repos,
    GoogleClient,
    /// By the account's address.
    GoogleGrant(String),
    Plugin(PluginName),
}

impl Declaration {
    /// No table declares the model's credential.
    #[must_use]
    pub(crate) const fn by_a_table(&self) -> bool {
        !matches!(self, Self::Model)
    }

    /// Whether a card of its own asks for it, because only a sign-in stores it.
    #[must_use]
    pub const fn own_card(&self) -> bool {
        matches!(
            self,
            Self::Model | Self::GoogleClient | Self::GoogleGrant(_)
        )
    }

    #[must_use]
    pub const fn by_a_secret_card(&self) -> bool {
        self.by_a_table() && !self.own_card()
    }

    #[must_use]
    pub const fn github(&self) -> bool {
        match self {
            Self::DataPush | Self::Repos => true,
            Self::Model
            | Self::Channel(_)
            | Self::Coding
            | Self::Plugin(_)
            | Self::GoogleClient
            | Self::GoogleGrant(_) => false,
        }
    }

    /// The data push is in use only while `pushed` needs its token.
    #[must_use]
    pub fn in_use(&self, pushed: &Pushed) -> bool {
        match self {
            Self::DataPush => pushed.needs_push_token(),
            Self::Model
            | Self::Channel(_)
            | Self::Coding
            | Self::Repos
            | Self::Plugin(_)
            | Self::GoogleClient
            | Self::GoogleGrant(_) => true,
        }
    }

    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Self::Model => "the model's credential".to_owned(),
            Self::Channel(name) => format!("the {name} notification channel"),
            Self::Coding => "the coding routine's fire token".to_owned(),
            Self::DataPush => "the push of the data repository".to_owned(),
            Self::Repos => "the secretary's repositories".to_owned(),
            Self::Plugin(plugin) => format!("the {plugin} plugin's link"),
            Self::GoogleClient => "every Google account's sign-in".to_owned(),
            Self::GoogleGrant(account) => format!("{account}'s mail and calendars"),
        }
    }
}
