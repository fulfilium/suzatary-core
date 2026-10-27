//! Parsing the query string, and every refusal it earns, as pure functions over `Option<&str>`.
//!
//! Capped work is the policy: asking for more than a cap gets the cap, not a slow answer that
//! ties up the one process a phone is waiting on. An empty value is absent, not malformed,
//! since a form sends `?t=` for a field nobody filled in.

use prodrome_core::fpl;

use crate::capability::Datetime;
use crate::reference::Reference;
use crate::refusal::{Refusal, shown};

/// A phone screenful of mail and a bit.
pub const MAIL_LIMIT: usize = 50;
/// The most messages one request will read and hash.
pub const MAX_MAIL: usize = 200;
/// A notmuch query is a query and not a document.
pub const MAX_QUERY_CHARS: usize = 500;
/// Every day the assistant has saved a briefing for, capped.
pub const MAX_BRIEFINGS: usize = 400;
/// The most rows a limited `/api/fold` will answer with.
pub const MAX_FOLD_ROWS: usize = 200;

/// An ISO instant, or the default when absent or empty.
pub fn moment(name: &str, raw: Option<&str>, default: Datetime) -> Result<Datetime, Refusal> {
    let Some(text) = raw.filter(|value| !value.is_empty()) else {
        return Ok(default);
    };
    let parsed = fpl::parse_iso(text).map_err(|_| {
        Refusal::bad_request(format!(
            "{name} must be an ISO datetime, got {}",
            shown(text)
        ))
    })?;
    fpl::datetime_of(parsed).map_err(|_| {
        Refusal::bad_request(format!(
            "{name} must be an ISO datetime, got {}",
            shown(text)
        ))
    })
}

/// A mail list's cap: capped, not obeyed, because the ceiling is what one request will read
/// and hash.
pub fn limit(raw: Option<&str>) -> Result<usize, Refusal> {
    let Some(text) = raw.filter(|value| !value.is_empty()) else {
        return Ok(MAIL_LIMIT);
    };
    match text.parse::<usize>() {
        Ok(asked) if asked >= 1 && text.chars().all(|c| c.is_ascii_digit()) => {
            Ok(asked.min(MAX_MAIL))
        }
        _ => Err(Refusal::bad_request(format!(
            "limit must be a positive integer, got {}",
            shown(text)
        ))),
    }
}

/// `/api/fold`'s `limit`: how many of the lowest-fulfillment todos to answer with, capped.
///
/// Unlike the mail's [`limit`], absent means the whole fold, so no existing caller is quietly
/// truncated.
pub fn fold_limit(raw: Option<&str>) -> Result<Option<usize>, Refusal> {
    let Some(text) = raw.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    match text.parse::<usize>() {
        Ok(asked) if asked >= 1 && text.chars().all(|c| c.is_ascii_digit()) => {
            Ok(Some(asked.min(MAX_FOLD_ROWS)))
        }
        _ => Err(Refusal::bad_request(format!(
            "limit must be a positive integer, got {}",
            shown(text)
        ))),
    }
}

/// A pull's `since`: absent or empty is a replica holding nothing; anything else is the frontier
/// an earlier answer named, 64 hex digits, so a request names what a replica holds in the same
/// few bytes however many tips it has.
pub fn since(raw: Option<&str>) -> Result<prodrome_core::store::frontier::Since, Refusal> {
    match raw.unwrap_or_default() {
        "" => Ok(prodrome_core::store::frontier::Since::Nothing),
        text => prodrome_core::store::frontier::Token::parse(text)
            .map(prodrome_core::store::frontier::Since::Frontier)
            .map_err(|refused| Refusal::bad_request(format!("{refused}, got {}", shown(text)))),
    }
}

/// The most hashes one `hashes=` may name: 64 names of 64 hex digits fit a URL of about 4 KiB.
pub const MOST_HASHES: usize = 64;

/// A content answer's `hashes`: one to [`MOST_HASHES`] object names, each 64 hex digits, in
/// ascending order with none twice, so one set of objects has one URL and a cache that kept its
/// answer answers every tab that asks for the same set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hashes(Vec<prodrome_core::event::Hash>);

impl Hashes {
    /// The names `raw` spells, comma-separated.
    ///
    /// # Errors
    /// [`Refusal::bad_request`] for none, more than [`MOST_HASHES`], a name that is not 64 hex
    /// digits, and names out of ascending order or named twice.
    pub fn parse(raw: Option<&str>) -> Result<Self, Refusal> {
        let names: Vec<&str> = raw.unwrap_or_default().split(',').collect();
        if names.len() > MOST_HASHES {
            return Err(Refusal::bad_request(format!(
                "hashes names at most {MOST_HASHES} objects, got {}",
                names.len()
            )));
        }
        let hashes = names
            .into_iter()
            .map(|name| {
                prodrome_core::event::Hash::new(name).map_err(|_| {
                    Refusal::bad_request(format!(
                        "hashes is object names of 64 hex digits, comma-separated, got {}",
                        shown(name)
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if hashes.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(Refusal::bad_request(
                "hashes names each object once, in ascending order",
            ));
        }
        Ok(Self(hashes))
    }

    /// The names, in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = &prodrome_core::event::Hash> {
        self.0.iter()
    }
}

/// `/api/tree`'s `by`: category when absent or empty, the list's default.
pub fn tree_by(raw: Option<&str>) -> Result<crate::wire::TreeBy, Refusal> {
    use crate::wire::TreeBy;
    match raw.unwrap_or_default() {
        "" | "category" => Ok(TreeBy::Category),
        "urgency" => Ok(TreeBy::Urgency),
        "flat" => Ok(TreeBy::Flat),
        other => Err(Refusal::bad_request(format!(
            "by must be category, urgency or flat, got {}",
            shown(other)
        ))),
    }
}

/// The widest window of turns one conversation request will draw.
pub const MAX_TURN_WINDOW: usize = 200;

/// A conversation page's paging cursor: an absolute turn index, or `None` for the newest
/// window.
pub fn turn_before(raw: Option<&str>) -> Result<Option<usize>, Refusal> {
    let Some(text) = raw.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    match text.parse::<usize>() {
        Ok(value) if text.chars().all(|c| c.is_ascii_digit()) => Ok(Some(value)),
        _ => Err(Refusal::bad_request(format!(
            "before must be a non-negative integer turn index, got {}",
            shown(text)
        ))),
    }
}

/// How many turns one window of a conversation carries, capped; `default` when absent.
pub fn turn_limit(raw: Option<&str>, default: usize) -> Result<usize, Refusal> {
    let Some(text) = raw.filter(|value| !value.is_empty()) else {
        return Ok(default);
    };
    match text.parse::<usize>() {
        Ok(asked) if asked >= 1 && text.chars().all(|c| c.is_ascii_digit()) => {
            Ok(asked.min(MAX_TURN_WINDOW))
        }
        _ => Err(Refusal::bad_request(format!(
            "limit must be a positive integer, got {}",
            shown(text)
        ))),
    }
}

/// `?about=<reference>` on a conversation window: only the rounds that touch one thing, or
/// `None` for the unfiltered window.
///
/// # Errors
/// [`Refusal::bad_request`] naming the text a [`Reference`] refused.
pub fn about(raw: Option<&str>) -> Result<Option<Reference>, Refusal> {
    let Some(text) = raw.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    text.parse::<Reference>()
        .map(Some)
        .map_err(|failed| Refusal::bad_request(format!("about: {failed}")))
}

/// A mail page's cursor: the instant the last page ended at, or `None` for the newest mail.
pub fn before(raw: Option<&str>) -> Result<Option<Datetime>, Refusal> {
    let Some(text) = raw.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let parsed = fpl::parse_iso(text).map_err(|_| {
        Refusal::bad_request(format!(
            "before must be an ISO datetime, got {}",
            shown(text)
        ))
    })?;
    fpl::datetime_of(parsed).map(Some).map_err(|_| {
        Refusal::bad_request(format!(
            "before must be an ISO datetime, got {}",
            shown(text)
        ))
    })
}

/// The notmuch query of `/api/mail/search`.
///
/// Passed through verbatim, so the only check is that it is a query and not a document.
pub fn search_query(raw: Option<&str>) -> Result<String, Refusal> {
    let text = raw.unwrap_or_default().trim();
    if text.is_empty() {
        return Err(Refusal::bad_request(
            "q is required: a notmuch query (plain words, from:, subject:, date: …)",
        ));
    }
    if text.chars().count() > MAX_QUERY_CHARS {
        return Err(Refusal::bad_request(format!(
            "q is capped at {MAX_QUERY_CHARS} characters, got {}",
            text.chars().count()
        )));
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One set of objects has one spelling: one to the most, ascending, none twice.
    #[test]
    fn hashes_are_one_to_the_most_ascending_and_none_twice() {
        let names: Vec<String> = (0..=MOST_HASHES).map(|n| format!("{n:064x}")).collect();
        let parsed = Hashes::parse(Some(&names[..MOST_HASHES].join(","))).expect("the most");
        assert_eq!(parsed.iter().count(), MOST_HASHES);
        for refused in [
            None,
            Some(String::new()),
            Some(names.join(",")),
            Some(format!("{},{}", names[1], names[0])),
            Some(format!("{},{}", names[0], names[0])),
            Some(format!("{},", names[0])),
            Some("ABC".to_owned()),
        ] {
            let refusal = Hashes::parse(refused.as_deref()).expect_err("refused");
            assert_eq!(refusal.status().code(), 400, "{refused:?}");
        }
    }

    /// The fold's limit defaults to the whole fold, not a screenful.
    #[test]
    fn a_fold_limit_is_absent_by_default_and_capped_when_it_is_not() {
        assert_eq!(fold_limit(None), Ok(None));
        assert_eq!(fold_limit(Some("")), Ok(None));
        assert_eq!(fold_limit(Some("5")), Ok(Some(5)));
        assert_eq!(fold_limit(Some("100000")), Ok(Some(MAX_FOLD_ROWS)));
        for bad in ["0", "-1", "five", "1.5"] {
            assert_eq!(
                fold_limit(Some(bad)).unwrap_err().status().code(),
                400,
                "{bad}"
            );
        }
        assert_eq!(
            fold_limit(Some("five")).unwrap_err().message(),
            "limit must be a positive integer, got 'five'"
        );
    }

    #[test]
    fn a_turn_limit_defaults_and_is_capped_not_obeyed() {
        assert_eq!(turn_limit(None, 60), Ok(60));
        assert_eq!(turn_limit(Some(""), 60), Ok(60));
        assert_eq!(turn_limit(Some("10"), 60), Ok(10));
        assert_eq!(turn_limit(Some("100000"), 60), Ok(MAX_TURN_WINDOW));
        for bad in ["0", "-1", "ten", "1.5"] {
            assert_eq!(
                turn_limit(Some(bad), 60).unwrap_err().status().code(),
                400,
                "{bad}"
            );
        }
    }
}
