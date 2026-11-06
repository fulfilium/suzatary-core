//! The owner's clock as offsets from UTC read by the browser (`web/src/app/time.ts`), so a page
//! links no zone database. An instant older than the oldest offset reads at that offset.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime};
use serde::Deserialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
pub struct Offset {
    /// Epoch milliseconds.
    #[ts(type = "number")]
    pub from: i64,
    /// Minutes east of UTC.
    pub minutes: i32,
}

/// RFC 3339, or a naive UTC time with `T` or a space.
fn parsed(at: &str) -> Option<NaiveDateTime> {
    DateTime::parse_from_rfc3339(at)
        .map(|t| t.naive_utc())
        .or_else(|_| NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S%.f"))
        .or_else(|_| NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()
}

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

/// `2026-09-04 11:32`; `""` for `""`, and one that does not parse as written
/// (`web/src/view/format.ts`'s `whenOf`).
#[must_use]
pub(crate) fn minute(offsets: &[Offset], at: &str) -> String {
    match parsed(at) {
        _ if at.is_empty() => String::new(),
        Some(t) => wall(offsets, t).format("%Y-%m-%d %H:%M").to_string(),
        None => at.to_owned(),
    }
}

/// `11:32 on 2026-09-04`; one that does not parse as written, and the epoch is an unknown time
/// (`format.ts`'s `when`).
#[must_use]
pub(crate) fn when(offsets: &[Offset], at: &str) -> String {
    match parsed(at) {
        Some(t) if t.and_utc().timestamp_millis() == 0 => "an unknown time".to_owned(),
        Some(t) => wall(offsets, t).format("%H:%M on %Y-%m-%d").to_string(),
        None => at.to_owned(),
    }
}

#[must_use]
pub(crate) fn when_ms(offsets: &[Offset], ms: i64) -> String {
    match DateTime::from_timestamp_millis(ms) {
        Some(_) if ms == 0 => "an unknown time".to_owned(),
        Some(t) => wall(offsets, t.naive_utc())
            .format("%H:%M on %Y-%m-%d")
            .to_string(),
        None => "an unknown time".to_owned(),
    }
}

/// A day is a date, so no change of offset moves it. A string that is no day is handed back.
#[must_use]
pub(crate) fn shift(day: &str, n: i64) -> String {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").map_or_else(
        |_| day.to_owned(),
        |d| (d + Duration::days(n)).format("%Y-%m-%d").to_string(),
    )
}

#[must_use]
pub(crate) fn ms(at: &str) -> Option<i64> {
    parsed(at).map(|t| t.and_utc().timestamp_millis())
}

#[must_use]
pub(crate) fn day_ms(offsets: &[Offset], ms: i64) -> NaiveDate {
    let t = DateTime::from_timestamp_millis(ms).map_or(NaiveDateTime::MIN, |t| t.naive_utc());
    wall(offsets, t).date()
}

/// The owner's day an instant falls on; `None` for one that does not parse.
#[must_use]
pub(crate) fn day(offsets: &[Offset], at: &str) -> Option<NaiveDate> {
    parsed(at).map(|t| wall(offsets, t).date())
}

/// The first instant of the owner's next day, to the minute: when a list's "today" becomes
/// "yesterday".
#[must_use]
pub(crate) fn next_day_ms(offsets: &[Offset], now: i64) -> i64 {
    let today = day_ms(offsets, now);
    let start = (today + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .unwrap_or(NaiveDateTime::MAX);
    let guess = start.and_utc().timestamp_millis() - i64::from(offset_at(offsets, now)) * 60_000;
    // An offset that changes overnight moves midnight; walk to the minute the day turns.
    (0..=24 * 60)
        .map(|k| guess - 12 * 3_600_000 + k * 60_000)
        .find(|&t| t > now && day_ms(offsets, t) > today)
        .unwrap_or(guess)
}

fn offset_at(offsets: &[Offset], ms: i64) -> i32 {
    offsets
        .iter()
        .take_while(|o| o.from <= ms)
        .last()
        .or(offsets.first())
        .map_or(0, |o| o.minutes)
}

/// Its minute today, `yesterday`, or its day; `""` for `""`, and one that does not parse as
/// written (`format.ts`'s `recent`).
#[must_use]
pub(crate) fn recent(offsets: &[Offset], at: &str, today: NaiveDate) -> String {
    if at.is_empty() {
        return String::new();
    }
    match parsed(at) {
        None => at.to_owned(),
        Some(t) => {
            let wall = wall(offsets, t);
            match (today - wall.date()).num_days() {
                0 => wall.format("%H:%M").to_string(),
                1 => "yesterday".to_owned(),
                _ => wall.format("%Y-%m-%d").to_string(),
            }
        }
    }
}
