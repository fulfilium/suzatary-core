//! What declares a secret this deployment reads: the table of the instance file that names it.

use crate::plugin::PluginName;
use crate::settings::remote::Pushed;

/// One table naming a secret; a name two tables declare is declared by both.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Declaration {
    /// The model's credential, which every deployment declares.
    Model,
    /// A `[notify]` channel's secret.
    Channel(crate::connect::Channel),
    /// `[coding]`'s routine, whose fire token it is.
    Coding,
    /// The data repository, which every deployment pushes its stores to.
    DataPush,
    /// `[[secretary.repos]]`, the repositories the secretary reads and proposes patches to.
    Repos,
    /// The box's one Google Cloud client, which every Google account signs in through.
    GoogleClient,
    /// A Google account's sign-in with Google, which the box keeps, by the account.
    GoogleGrant(String),
    /// A `[[plugins]]` manifest's credential, and the use it is put to (`url`, `bearer`,
    /// `header`).
    Plugin(PluginName),
}

impl Declaration {
    /// Whether a table of the instance file declares it; the model's credential is declared by
    /// no table, so its absence is no fault of the file's.
    #[must_use]
    pub const fn by_a_table(&self) -> bool {
        !matches!(self, Self::Model)
    }

    /// Whether its name is asked for by a card of its own rather than a Secret card: the model's
    /// credential, which only its sign-in stores; the Google client, by the card that takes
    /// Google's downloaded file whole and checks its redirect; and a Google account's grant,
    /// which only its sign-in with Google stores.
    #[must_use]
    pub const fn own_card(&self) -> bool {
        matches!(
            self,
            Self::Model | Self::GoogleClient | Self::GoogleGrant(_)
        )
    }

    /// Whether what it declares is in use, so its secret is asked for: the data repository's
    /// push while `pushed` needs its token, every other table always.
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

    /// What declares it, as a sentence names it: `the coding routine's fire token`.
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
