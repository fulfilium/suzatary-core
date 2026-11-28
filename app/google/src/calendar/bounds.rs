//! What every provider's calendar read keeps within bounds, so a provider's answer, however long,
//! is read in a fixed span and a fixed number of items.

use chrono::{Duration, NaiveDateTime};

/// The most calendars of one account a sync reads.
pub const CALENDARS: usize = 20;

/// The most events of one calendar a sync reads.
pub const EVENTS: usize = 250;

/// The most bytes of one answer the box reads.
pub const MAX_ANSWER: usize = 1 << 20;

/// The span a sync reads: a week back, for what just happened, and ninety days ahead.
#[must_use]
pub fn window(now: NaiveDateTime) -> (NaiveDateTime, NaiveDateTime) {
    (now - Duration::days(7), now + Duration::days(90))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unread {
    #[error("an answer of {0} bytes is longer than the box reads")]
    TooLarge(usize),
    #[error("the answer is not JSON")]
    NotJson,
}

/// `body` as JSON, refused unread past [`MAX_ANSWER`].
pub(crate) fn json(body: &[u8]) -> Result<serde_json::Value, Unread> {
    if body.len() > MAX_ANSWER {
        return Err(Unread::TooLarge(body.len()));
    }
    serde_json::from_slice(body).map_err(|_| Unread::NotJson)
}

/// The first `most` of the array an answer holds under `key`.
pub(crate) fn items<'a>(
    answer: &'a serde_json::Value,
    key: &str,
    most: usize,
) -> Vec<&'a serde_json::Value> {
    answer
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().take(most).collect())
        .unwrap_or_default()
}

pub(crate) fn cut(text: &str, most: usize) -> String {
    text.chars().take(most).collect()
}
