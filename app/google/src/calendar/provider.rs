//! Which service keeps an account's calendars.

use suzatary_mail::mail::account::Sync;

use crate::google::Held;

/// A service the box reads calendars from and writes events to. Each writes its occurrences into
/// the one calendar store, as its own actor and under its own calendar names.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Google,
    Microsoft,
}

impl Provider {
    /// The actor its occurrences are written as, which names their calendars
    /// (`PLUGIN_<ACTOR>_…`).
    #[must_use]
    pub const fn actor(self) -> &'static str {
        match self {
            Provider::Google => "google",
            Provider::Microsoft => "microsoft",
        }
    }

    /// The provider of an act stored before acts named one.
    #[must_use]
    pub const fn google() -> Self {
        Provider::Google
    }

    #[must_use]
    pub const fn is_google(&self) -> bool {
        matches!(self, Provider::Google)
    }

    /// What the owner calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Provider::Google => "Google",
            Provider::Microsoft => "Microsoft",
        }
    }

    /// The one word it is spelt by in a stored act and in `propose_act`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        self.actor()
    }

    /// The provider spelt `word`.
    #[must_use]
    pub fn of_word(word: &str) -> Option<Self> {
        [Provider::Google, Provider::Microsoft]
            .into_iter()
            .find(|provider| provider.word() == word)
    }

    /// The provider an event for an account goes to when its proposal names none: where the
    /// account's mail is.
    #[must_use]
    pub const fn home(sync: &Sync) -> Self {
        match sync {
            Sync::MsImap(_) => Provider::Microsoft,
            Sync::Google | Sync::Gmail | Sync::Proton(_) => Provider::Google,
        }
    }

    /// Every provider whose calendar grant the box holds for an account, each read under its own
    /// names and none replacing another: a Google account's sign-in always reads them (its
    /// failures its health); any other account's Google grant reads them while it holds the
    /// calendars; a Microsoft account's calendar consent reads them when held.
    #[must_use]
    pub fn held(sync: &Sync, google: &Held, microsoft_calendar: bool) -> Vec<Self> {
        let google_calendar = matches!(
            google,
            Held::Signed(parts) if parts.contains(crate::google::Scope::Calendar)
        );
        let mut held = Vec::new();
        if matches!(sync, Sync::Google) || google_calendar {
            held.push(Provider::Google);
        }
        if microsoft_calendar && matches!(sync, Sync::MsImap(_)) {
            held.push(Provider::Microsoft);
        }
        held
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::{Held, Scope, Scopes};
    use suzatary_mail::mail::account::{Authority, MsImap};

    /// An address keeps calendars at every provider whose grant the box holds, none replacing
    /// another.
    #[test]
    fn every_provider_whose_grant_is_held_is_read() {
        let ms = Sync::MsImap(MsImap {
            authority: Authority::Organizations,
            inbox_folders: suzatary_mail::mail::folders::Folders::inbox(),
            sent_folder: "Sent Items".to_owned(),
        });
        let google = Held::Signed(Scopes::of([Scope::Calendar]).expect("a part"));
        assert_eq!(
            Provider::held(&ms, &google, true),
            [Provider::Google, Provider::Microsoft]
        );
        assert_eq!(
            Provider::held(&ms, &Held::NotSignedIn, true),
            [Provider::Microsoft]
        );
        assert_eq!(Provider::held(&ms, &google, false), [Provider::Google]);
        assert!(Provider::held(&ms, &Held::NotSignedIn, false).is_empty());
        assert_eq!(
            Provider::held(&Sync::Google, &Held::NotSignedIn, false),
            [Provider::Google]
        );
    }
}
