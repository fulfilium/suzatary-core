//! The owner's clock as the host hands it over: the zone's offsets from UTC, each from the instant
//! it starts, read from the browser by `time.offsets` (`web/src/app/time.ts`), so a page links no
//! zone database. An instant older than the oldest offset is read at that offset.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime};
use serde::Deserialize;
use ts_rs::TS;

/// An offset of the owner's zone from UTC, from an instant on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
pub struct Offset {
    /// From when, in epoch milliseconds.
    #[ts(type = "number")]
    pub from: i64,
    /// The offset, in minutes east of UTC.
    pub minutes: i32,
}

/// An instant as the box prints it (RFC 3339, or a naive UTC time with `T` or a space); `None`
/// for one that does not parse.
fn parsed(at: &str) -> Option<NaiveDateTime> {
    DateTime::parse_from_rfc3339(at)
        .map(|t| t.naive_utc())
        .or_else(|_| NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S%.f"))
        .or_else(|_| NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()
}

/// `t` on the owner's wall clock.
fn wall(offsets: &[Offset], t: NaiveDateTime) -> NaiveDateTime {
    let ms = t.and_utc().timestamp_millis();
    let offset = offsets
        .iter()
        .take_while(|o| o.from <= ms)
        .last()
        .or(offsets.first())
        .map_or(0, |o| o.minutes);
    t + Duration::minutes(offset.into())
}

/// An instant as a row shows it, `2026-09-04 11:32` on the owner's clock; `""` for `""`, and one
/// that does not parse as written (`web/src/view/format.ts`'s `whenOf`).
#[must_use]
pub fn minute(offsets: &[Offset], at: &str) -> String {
    match parsed(at) {
        _ if at.is_empty() => String::new(),
        Some(t) => wall(offsets, t).format("%Y-%m-%d %H:%M").to_string(),
        None => at.to_owned(),
    }
}

/// An instant for a sentence, `11:32 on 2026-09-04` on the owner's clock; one that does not parse
/// is said as written, and the epoch is an unknown time (`format.ts`'s `when`).
#[must_use]
pub fn when(offsets: &[Offset], at: &str) -> String {
    match parsed(at) {
        Some(t) if t.and_utc().timestamp_millis() == 0 => "an unknown time".to_owned(),
        Some(t) => wall(offsets, t).format("%H:%M on %Y-%m-%d").to_string(),
        None => at.to_owned(),
    }
}

/// [`when`] of an instant in epoch milliseconds.
#[must_use]
pub fn when_ms(offsets: &[Offset], ms: i64) -> String {
    match DateTime::from_timestamp_millis(ms) {
        Some(_) if ms == 0 => "an unknown time".to_owned(),
        Some(t) => wall(offsets, t.naive_utc())
            .format("%H:%M on %Y-%m-%d")
            .to_string(),
        None => "an unknown time".to_owned(),
    }
}

/// `n` days on from a day; a day is a date, so no change of offset moves it. A string that is no
/// day is handed back.
#[must_use]
pub fn shift(day: &str, n: i64) -> String {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").map_or_else(
        |_| day.to_owned(),
        |d| (d + Duration::days(n)).format("%Y-%m-%d").to_string(),
    )
}
