//! A bearer token held until shortly before it expires, and minted again by
//! its [`Grant`] when due.

use std::sync::Mutex;

use crate::http::Http;

/// A token is renewed this many seconds before it expires, not in flight.
pub const SLACK_SECONDS: u64 = 60;

/// A freshly minted token and how many seconds it lives.
#[derive(Debug, Clone)]
pub struct Fresh<T> {
    pub token: T,
    /// Seconds until it expires.
    pub lifetime: u64,
}

/// What mints a bearer token: a service account, a refresh token.
pub trait Grant {
    type Token: Clone;
    type Error;

    /// Mint one now.
    fn mint(&self, http: &dyn Http, now: u64) -> Result<Fresh<Self::Token>, Self::Error>;
}

/// One grant and the token it last minted.
pub struct Bearer<G: Grant> {
    grant: G,
    held: Mutex<Option<(G::Token, u64)>>,
}

impl<G: Grant> std::fmt::Debug for Bearer<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bearer").finish_non_exhaustive()
    }
}

impl<G: Grant> Bearer<G> {
    #[must_use]
    pub fn new(grant: G) -> Self {
        Self {
            grant,
            held: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn grant(&self) -> &G {
        &self.grant
    }

    /// The held token while it has more than [`SLACK_SECONDS`] left at `now` (unix seconds),
    /// else a freshly minted one. A failed mint holds nothing.
    pub fn token(&self, http: &dyn Http, now: u64) -> Result<G::Token, G::Error> {
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((token, expires)) = held.as_ref()
            && now < expires.saturating_sub(SLACK_SECONDS)
        {
            return Ok(token.clone());
        }
        let fresh = self.grant.mint(http, now)?;
        *held = Some((fresh.token.clone(), now.saturating_add(fresh.lifetime)));
        Ok(fresh.token)
    }
}

/// Seconds since the epoch.
#[must_use]
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Scripted;

    struct Counting {
        minted: Mutex<u32>,
        refuse: bool,
    }

    impl Grant for Counting {
        type Token = u32;
        type Error = &'static str;

        fn mint(&self, _http: &dyn Http, _now: u64) -> Result<Fresh<u32>, &'static str> {
            if self.refuse {
                return Err("refused");
            }
            let mut minted = self.minted.lock().expect("unpoisoned");
            *minted += 1;
            Ok(Fresh {
                token: *minted,
                lifetime: 3600,
            })
        }
    }

    fn bearer(refuse: bool) -> Bearer<Counting> {
        Bearer::new(Counting {
            minted: Mutex::new(0),
            refuse,
        })
    }

    #[test]
    fn a_token_is_held_until_the_slack_before_it_expires() {
        let http = Scripted::new(|req| panic!("no request: {req:?}"));
        let held = bearer(false);
        assert_eq!(held.token(&http, 1000), Ok(1));
        assert_eq!(held.token(&http, 1000 + 3600 - SLACK_SECONDS - 1), Ok(1));
        assert_eq!(held.token(&http, 1000 + 3600 - SLACK_SECONDS), Ok(2));
    }

    #[test]
    fn a_refusal_holds_nothing() {
        let http = Scripted::new(|req| panic!("no request: {req:?}"));
        let refused = bearer(true);
        assert_eq!(refused.token(&http, 0), Err("refused"));
        assert_eq!(refused.token(&http, 0), Err("refused"));
    }
}
