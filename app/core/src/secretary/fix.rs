//! A lack's fix: the box's card that gives what a tapped card lacked, performed by its tap.

use crate::google::{Held, RedirectUri, Scope, Scopes};
use crate::mail::address::EmailAddress;
use crate::mail::lacking::{Grant, Lacking};
use crate::secretary::acts::Act;
use crate::secretary::inbox::ProposalId;

/// Why a lack has no card to fix it now.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unfixable {
    #[error("{0} holds that grant already: tap Try again")]
    Held(EmailAddress),
    #[error("the app's URL is not https, so Google cannot send you back to it")]
    NoRedirect,
}

/// The step that gives what a card lacked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fix {
    GoogleSignIn {
        account: EmailAddress,
        scopes: Scopes,
    },
    /// Its card shows a device code.
    MicrosoftSend { account: EmailAddress },
    /// The owner pastes the client's file into its card.
    GoogleClient { redirect: RedirectUri },
}

impl Fix {
    /// The act of the box's card that performs it.
    #[must_use]
    pub fn act(&self) -> Act {
        match self {
            Fix::GoogleSignIn { account, scopes } => Act::GoogleSignIn {
                account: account.clone(),
                scopes: scopes.clone(),
            },
            Fix::MicrosoftSend { account } => Act::AuthSend {
                account: account.clone(),
            },
            Fix::GoogleClient { redirect } => Act::GoogleClient {
                redirect: redirect.clone(),
            },
        }
    }
}

/// The step that gives what `lacking` names, asking only for the parts `held` lacks.
///
/// # Errors
/// [`Unfixable`] when the step is done already or cannot begin from this box.
pub fn of(
    lacking: &Lacking,
    held: &Held,
    redirect: Option<&RedirectUri>,
) -> Result<Fix, Unfixable> {
    match lacking {
        Lacking::Grant {
            account,
            grant: Grant::GoogleCalendar,
        } => google(account, &[Scope::Calendar], held),
        Lacking::Grant {
            account,
            grant: Grant::GoogleSend,
        } => google(account, &[Scope::Mail], held),
        Lacking::Grant {
            account,
            grant: Grant::MicrosoftSend,
        } => Ok(Fix::MicrosoftSend {
            account: account.clone(),
        }),
        Lacking::GoogleClient => redirect
            .map(|redirect| Fix::GoogleClient {
                redirect: redirect.clone(),
            })
            .ok_or(Unfixable::NoRedirect),
    }
}

fn google(account: &EmailAddress, wanted: &[Scope], held: &Held) -> Result<Fix, Unfixable> {
    let wanted = Scopes::of(wanted.iter().copied());
    let scopes = match held {
        Held::NotSignedIn => wanted,
        Held::Signed(parts) => wanted.and_then(|wanted| wanted.missing(parts)),
    };
    scopes
        .map(|scopes| Fix::GoogleSignIn {
            account: account.clone(),
            scopes,
        })
        .ok_or_else(|| Unfixable::Held(account.clone()))
}

/// Where the page goes once the fix tap of `card` is taken: Google's consent, a path the box
/// serves outside the app, or Settings, where the fix's card stands.
#[must_use]
pub fn then(lacking: &Lacking, card: &ProposalId) -> crate::wire::inbox::FixThen {
    use crate::wire::inbox::FixThen;
    match lacking {
        Lacking::Grant {
            grant: Grant::GoogleCalendar | Grant::GoogleSend,
            ..
        } => FixThen::Leave {
            href: format!("{}{}", crate::google::CONSENT, card.as_str()),
        },
        Lacking::Grant {
            grant: Grant::MicrosoftSend,
            ..
        }
        | Lacking::GoogleClient => FixThen::Place {
            href: "#/settings".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ada() -> EmailAddress {
        EmailAddress::new("ada@example.test").expect("an address")
    }

    fn lacks(grant: Grant) -> Lacking {
        Lacking::Grant {
            account: ada(),
            grant,
        }
    }

    fn parts(parts: &[Scope]) -> Scopes {
        Scopes::of(parts.iter().copied()).expect("parts")
    }

    /// A fix asks for the grant's parts the account lacks and nothing else; none, when it lacks none.
    #[test]
    fn a_fix_asks_for_what_the_grant_needs_and_the_account_does_not_hold() {
        let signs_in = |lacking: &Lacking, held: &Held| match of(lacking, held, None) {
            Ok(Fix::GoogleSignIn { account, scopes }) => Ok((account, scopes)),
            Ok(other) => panic!("not a sign-in with Google: {other:?}"),
            Err(unfixable) => Err(unfixable),
        };
        let calendar = lacks(Grant::GoogleCalendar);
        let send = lacks(Grant::GoogleSend);
        for held in [Held::NotSignedIn, Held::Signed(parts(&[Scope::Mail]))] {
            assert_eq!(
                signs_in(&calendar, &held),
                Ok((ada(), parts(&[Scope::Calendar]))),
                "{held:?}"
            );
        }
        assert_eq!(
            signs_in(&send, &Held::NotSignedIn),
            Ok((ada(), parts(&[Scope::Mail])))
        );
        for (lacking, held) in [
            (&calendar, parts(&[Scope::Calendar])),
            (&calendar, parts(&[Scope::Mail, Scope::Calendar])),
            (&send, parts(&[Scope::Mail])),
        ] {
            assert_eq!(
                signs_in(lacking, &Held::Signed(held)),
                Err(Unfixable::Held(ada()))
            );
        }
    }

    /// Each other lack names its own card.
    #[test]
    fn every_lack_names_the_card_that_gives_it() {
        assert_eq!(
            of(&lacks(Grant::MicrosoftSend), &Held::NotSignedIn, None),
            Ok(Fix::MicrosoftSend { account: ada() })
        );
        assert_eq!(
            of(&Lacking::GoogleClient, &Held::NotSignedIn, None),
            Err(Unfixable::NoRedirect)
        );
        let redirect = RedirectUri::of_app("https://box.example.test").expect("a redirect");
        assert_eq!(
            of(&Lacking::GoogleClient, &Held::NotSignedIn, Some(&redirect)),
            Ok(Fix::GoogleClient { redirect })
        );
    }
}
