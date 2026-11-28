//! A moment a provider names: a UTC instant, or a whole day.

use chrono::{NaiveDate, NaiveDateTime};

use super::When;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    At(NaiveDateTime),
    Day(NaiveDate),
}

impl Point {
    /// The text an occurrence's key holds for it: `YYYYMMDDTHHMMSSZ` or `YYYYMMDD`.
    #[must_use]
    pub fn key(self) -> String {
        match self {
            Point::At(at) => at.format("%Y%m%dT%H%M%SZ").to_string(),
            Point::Day(day) => day.format("%Y%m%d").to_string(),
        }
    }
}

impl From<When> for Point {
    /// Where the span starts.
    fn from(when: When) -> Self {
        match when {
            When::Timed { start, .. } => Point::At(start),
            When::AllDay { start, .. } => Point::Day(start),
        }
    }
}
