//! The Google consents the box holds open, each begun by a tapped card and bound to the actor who
//! tapped it. A consent lives in memory alone, so a restart closes every one and no verifier is
//! ever written down. [`Open`] is pure; the server holds it behind a lock and does the exchange.

use std::collections::BTreeMap;

use chrono::NaiveDateTime;
use prodrome_core::event::Actor;

use crate::google::{Client, Exchanged, RedirectUri, Scope, Scopes, State, Verifier};
use crate::mail::address::EmailAddress;
use crate::mail::oauth::OauthError;
use crate::secretary::inbox::ProposalId;
use crate::secretary::responded::{Done, Performed, Tried};

/// Long enough to read Google's page and its one warning.
#[must_use]
pub fn lasts() -> chrono::Duration {
    chrono::Duration::minutes(LASTS_MINUTES)
}

/// [`lasts`], in minutes.
pub const LASTS_MINUTES: i64 = 10;

/// One open consent. Its `Debug` redacts the link, which carries the state in the clear.
#[derive(Clone, PartialEq, Eq)]
pub struct Begun {
    /// The card whose tap began it.
    pub card: ProposalId,
    /// The account it is for.
    pub account: EmailAddress,
    /// What it asks for.
    pub scopes: Scopes,
    /// Only a callback from this actor is read.
    pub by: Actor,
    /// The verifier its challenge was made from.
    pub verifier: Verifier,
    /// The client it was asked through.
    pub client: Client,
    /// Where Google sends the owner back.
    pub redirect: RedirectUri,
    /// The consent page's URL.
    pub link: String,
    /// When it lapses, UTC.
    pub until: NaiveDateTime,
}

impl std::fmt::Debug for Begun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Begun")
            .field("card", &self.card)
            .field("account", &self.account)
            .field("scopes", &self.scopes)
            .field("by", &self.by)
            .field("verifier", &self.verifier)
            .field("client", &self.client)
            .field("redirect", &self.redirect)
            .field("link", &"<redacted>")
            .field("until", &self.until)
            .finish()
    }
}

/// What a callback's state found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Taken {
    /// Never drawn here, already used, or closed by a restart.
    Unknown,
    /// A consent past its time.
    Lapsed(Begun),
    /// A consent another actor began.
    NotYours(Begun),
    /// The consent the callback finishes.
    Yours(Box<Begun>),
}

/// Every consent the box holds open, by its state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Open(BTreeMap<State, Begun>);

impl Open {
    /// Closes any consent the same card held, so the earlier link opens nothing.
    pub fn open(&mut self, state: State, begun: Begun) {
        self.0.retain(|_, held| held.card != begun.card);
        self.0.insert(state, begun);
    }

    /// Held past its lapse until read or replaced, so a late link reaches the callback, which
    /// says it lapsed.
    #[must_use]
    pub fn begun(&self, card: &ProposalId) -> Option<&Begun> {
        self.0.values().find(|begun| begun.card == *card)
    }

    /// The state is closed whatever it finds, so a replayed code finds nothing.
    pub fn take(&mut self, state: &State, by: &Actor, now: NaiveDateTime) -> Taken {
        match self.0.remove(state) {
            None => Taken::Unknown,
            Some(begun) if now >= begun.until => Taken::Lapsed(begun),
            Some(begun) if begun.by != *by => Taken::NotYours(begun),
            Some(begun) => Taken::Yours(Box::new(begun)),
        }
    }

    /// Closes every lapsed consent, returning its card.
    pub fn lapse(&mut self, now: NaiveDateTime) -> Vec<ProposalId> {
        let (lapsed, open): (BTreeMap<_, _>, BTreeMap<_, _>) = std::mem::take(&mut self.0)
            .into_iter()
            .partition(|(_, begun)| now >= begun.until);
        self.0 = open;
        lapsed.into_values().map(|begun| begun.card).collect()
    }
}

/// In no capability record, so only a tap begins a consent and only the callback finishes one.
pub trait Consents: Sync {
    /// Running until it lapses, or why it could not begin.
    fn begin(
        &self,
        card: &ProposalId,
        account: &EmailAddress,
        scopes: &Scopes,
        by: &Actor,
        now: crate::capability::Datetime,
    ) -> Performed;
}

const AGAIN: &str = "tap Sign in with Google to ask Google again";

/// How the card ends, and the grant to keep; a grant without mail access is not kept.
#[must_use]
pub fn exchanged(
    begun: &Begun,
    read: Result<Exchanged, OauthError>,
) -> (Performed, Option<crate::google::KeptGrant>) {
    let read = read.map(|exchanged| {
        let parts = Scopes::of(exchanged.granted.iter().copied())
            .filter(|parts| parts.contains(Scope::Mail));
        (exchanged.refresh, parts)
    });
    match read {
        Ok((refresh, Some(parts))) => {
            let what = match begun.scopes.missing(&parts) {
                None => format!(
                    "signed {} in with Google: {}",
                    begun.account,
                    parts.sentence()
                ),
                Some(left) => format!(
                    "signed {} in with Google: {}; not {}, which you left unticked on Google's \
                     page, and which a card of its own asks for alone",
                    begun.account,
                    parts.sentence(),
                    left.sentence()
                ),
            };
            (
                Performed::Done(Done::Acted { what }),
                Some(crate::google::KeptGrant { parts, refresh }),
            )
        }
        Ok((_, None)) => (
            Performed::Tried(Tried::NotSent {
                reason: format!(
                    "Google granted no access to {}'s mail, which you left unticked on its \
                     page, so nothing was kept; {AGAIN} and tick it",
                    begun.account
                ),
            }),
            None,
        ),
        Err(failed) => (
            Performed::Tried(Tried::NotSent {
                reason: format!("no grant came of the sign-in: {failed}; {AGAIN}"),
            }),
            None,
        ),
    }
}

/// How a callback that finished no consent ends the card it found.
#[must_use]
pub fn refused(taken: &Taken, error: Option<&str>) -> Option<(ProposalId, Performed)> {
    let ended = |begun: &Begun, why: String| {
        Some((
            begun.card.clone(),
            Performed::Tried(Tried::NotSent {
                reason: format!("{why}; {AGAIN}"),
            }),
        ))
    };
    match (taken, error) {
        (Taken::Unknown, _) | (Taken::Yours(_), None) => None,
        (Taken::Lapsed(begun), _) => ended(
            begun,
            format!("Google sent you back after the sign-in's {LASTS_MINUTES} minutes had passed"),
        ),
        (Taken::NotYours(begun), _) => ended(
            begun,
            "Google sent back a browser signed in to the tailnet as someone other than who \
             tapped the card, so nothing was kept"
                .to_owned(),
        ),
        (Taken::Yours(begun), Some(error)) => ended(
            begun,
            format!("Google did not grant it ({error}), so nothing was kept"),
        ),
    }
}

/// Ends `card` as the box and says so in its conversation, which it returns.
///
/// # Errors
/// The stores' own failures.
pub fn end(
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox: &dyn crate::store::Prodrome<crate::secretary::inbox_event::InboxEvent>,
    card: &ProposalId,
    at: crate::capability::Datetime,
    performed: Performed,
) -> Result<Option<crate::secretary::store::ConversationId>, crate::refusal::Refusal> {
    let said = match &performed {
        Performed::Done(Done::Acted { what }) => format!("Signed in: {what}."),
        Performed::Tried(Tried::NotSent { reason } | Tried::Refused { reason }) => {
            format!("Not signed in: {reason}.")
        }
        Performed::Tried(Tried::Lacks(lacking)) => format!("Not signed in. {}", lacking.sentence()),
        Performed::Done(_) | Performed::Tried(Tried::Running { .. } | Tried::Unfinished { .. }) => {
            String::new()
        }
    };
    crate::secretary::inbox::end(inbox, card, &super::writes_as(), at, performed)?;
    let fold = crate::secretary::inbox::fold(inbox)?;
    let Some(asked) = fold
        .objects()
        .find(|object| object.id == *card)
        .map(|object| object.asked.asked.clone())
    else {
        return Ok(None);
    };
    if !said.is_empty() {
        super::append(
            conversations,
            &asked.conversation,
            &crate::secretary::store::Entry::noted(at, said, vec![asked.clone().into()]),
        )?;
    }
    Ok(Some(asked.conversation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::{ClientId, ClientSecret};

    fn at(minute: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 17)
            .and_then(|day| day.and_hms_opt(9, minute, 0))
            .expect("an instant")
    }

    struct Fixed(u8);

    impl crate::capability::Entropy for Fixed {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
            bytes.fill(self.0);
            Ok(())
        }
    }

    fn card(n: u8) -> ProposalId {
        ProposalId::new(format!("p{n:032x}")).expect("a proposal id")
    }

    fn begun(card_n: u8, by: &str) -> (State, Begun) {
        let state = State::draw(&Fixed(card_n)).expect("a state");
        (
            state,
            Begun {
                card: card(card_n),
                account: EmailAddress::new("ada@example.test").expect("an address"),
                scopes: Scopes::all(),
                by: Actor::new(by).expect("an actor"),
                verifier: Verifier::draw(&Fixed(200)).expect("a verifier"),
                client: Client {
                    id: ClientId::parse("1-a.apps.googleusercontent.com").expect("an id"),
                    secret: ClientSecret::parse("GOCSPX-x").expect("a secret"),
                },
                redirect: RedirectUri::of_app("https://box.example.test").expect("a callback"),
                link: format!("https://accounts.google.com/o/oauth2/v2/auth?card={card_n}"),
                until: at(10),
            },
        )
    }

    /// A state is taken once: the first callback finds its consent, the second finds nothing,
    /// so a replayed code is never exchanged.
    #[test]
    fn a_state_is_taken_once() {
        let mut open = Open::default();
        let (state, begun) = begun(1, "ada");
        open.open(state.clone(), begun);
        let ada = Actor::new("ada").expect("an actor");
        assert!(matches!(open.take(&state, &ada, at(1)), Taken::Yours(_)));
        assert_eq!(open.take(&state, &ada, at(2)), Taken::Unknown);
    }

    /// A consent is the tapping actor's: another actor's callback closes it unread, and one
    /// past its time is lapsed.
    #[test]
    fn a_consent_is_the_tappers_and_lapses() {
        let mut open = Open::default();
        let (state, begun_one) = begun(1, "ada");
        open.open(state.clone(), begun_one);
        let eve = Actor::new("eve").expect("an actor");
        assert!(matches!(open.take(&state, &eve, at(1)), Taken::NotYours(_)));
        let (state, begun_two) = begun(2, "ada");
        open.open(state.clone(), begun_two);
        let ada = Actor::new("ada").expect("an actor");
        assert!(matches!(open.take(&state, &ada, at(10)), Taken::Lapsed(_)));
        let (_, begun_three) = begun(3, "ada");
        let (state, _) = begun(4, "ada");
        open.open(state, begun_three);
        assert!(open.begun(&card(3)).is_some());
        assert_eq!(open.lapse(at(11)), vec![card(3)]);
        assert_eq!(open.begun(&card(3)), None);
    }

    /// Nothing a consent's `Debug` prints holds its state or its verifier.
    #[test]
    fn a_consent_prints_neither_its_state_nor_its_verifier() {
        let (state, begun) = begun(1, "ada");
        let link = format!("{}&state={}", begun.link, state.as_str());
        let begun = Begun { link, ..begun };
        let mut open = Open::default();
        open.open(state.clone(), begun.clone());
        for printed in [format!("{begun:?}"), format!("{open:?}")] {
            assert!(!printed.contains(state.as_str()), "{printed}");
            assert!(!printed.contains(begun.verifier.expose()), "{printed}");
            assert!(!printed.contains("GOCSPX-x"), "{printed}");
        }
    }

    /// A card tapped again holds one consent: the earlier state finds nothing.
    #[test]
    fn a_card_tapped_again_closes_its_earlier_consent() {
        let mut open = Open::default();
        let (first, begun_one) = begun(1, "ada");
        open.open(first.clone(), begun_one.clone());
        let second = State::draw(&Fixed(99)).expect("a state");
        open.open(second.clone(), begun_one);
        let ada = Actor::new("ada").expect("an actor");
        assert_eq!(open.take(&first, &ada, at(1)), Taken::Unknown);
        assert!(matches!(open.take(&second, &ada, at(1)), Taken::Yours(_)));
    }

    /// A grant without its mail keeps nothing, and one without a part says which to tick.
    #[test]
    fn a_grant_is_kept_only_with_its_mail_and_says_what_was_left_out() {
        let (_, begun) = begun(1, "ada");
        let grant = |parts: &[Scope]| Exchanged {
            refresh: crate::google::RefreshToken::parse("1//r").expect("a token"),
            granted: parts.iter().copied().collect(),
            account: begun.account.clone(),
        };
        let (all, kept) = exchanged(&begun, Ok(grant(&Scope::ALL)));
        assert!(kept.is_some());
        assert!(
            matches!(all, Performed::Done(Done::Acted { ref what }) if !what.contains("unticked"))
        );
        let (some, kept) = exchanged(&begun, Ok(grant(&[Scope::Mail])));
        assert!(kept.is_some());
        assert!(
            matches!(some, Performed::Done(Done::Acted { ref what }) if what.contains("not its calendars")),
            "{some:?}"
        );
        let (none, kept) = exchanged(&begun, Ok(grant(&[Scope::Calendar])));
        assert!(kept.is_none());
        assert!(matches!(none, Performed::Tried(Tried::NotSent { .. })));
    }
}
