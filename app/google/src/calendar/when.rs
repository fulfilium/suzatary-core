//! When an occurrence is: a span of UTC instants, or of whole days.

use chrono::{NaiveDate, NaiveDateTime};

use super::NotAnOccurrence;

/// A span: timed, in UTC, or all day, in the calendar's own days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum When {
    /// From `start` until `end`, UTC instants, `end` never before `start`.
    Timed {
        /// The start.
        start: NaiveDateTime,
        /// The end.
        end: NaiveDateTime,
    },
    /// From the day `start` until the day before `end`, `end` after `start`.
    AllDay {
        /// The first day.
        start: NaiveDate,
        /// The day after the last.
        end: NaiveDate,
    },
}

/// The spelling a stored interval gives a UTC instant.
const INSTANT: &str = "%Y-%m-%dT%H:%M:%SZ";

/// The spelling it gives a day.
const DAY: &str = "%Y-%m-%d";

impl When {
    /// A timed span.
    ///
    /// # Errors
    /// `NotAnOccurrence::Span` for an end before its start.
    pub fn timed(start: NaiveDateTime, end: NaiveDateTime) -> Result<Self, NotAnOccurrence> {
        if end < start {
            return Err(NotAnOccurrence::Span);
        }
        Ok(When::Timed { start, end })
    }

    /// A span of whole days.
    ///
    /// # Errors
    /// `NotAnOccurrence::Span` for an end not after its start.
    pub fn all_day(start: NaiveDate, end: NaiveDate) -> Result<Self, NotAnOccurrence> {
        if end <= start {
            return Err(NotAnOccurrence::Span);
        }
        Ok(When::AllDay { start, end })
    }

    /// Read one as the store spells it: an ISO 8601 interval, `start/end`, of two UTC instants
    /// (`2026-10-05T13:00:00Z/2026-10-05T13:30:00Z`) or of two days (`2026-10-09/2026-10-11`).
    ///
    /// # Errors
    /// `NotAnOccurrence::Span` for anything else, a mixed pair or an inverted span.
    pub fn parse(interval: &str) -> Result<Self, NotAnOccurrence> {
        let (start, end) = interval.split_once('/').ok_or(NotAnOccurrence::Span)?;
        let instant = |text| NaiveDateTime::parse_from_str(text, INSTANT);
        let day = |text| NaiveDate::parse_from_str(text, DAY);
        match (instant(start), instant(end)) {
            (Ok(start), Ok(end)) => When::timed(start, end),
            _ => When::all_day(
                day(start).map_err(|_| NotAnOccurrence::Span)?,
                day(end).map_err(|_| NotAnOccurrence::Span)?,
            ),
        }
    }

    /// Its one spelling, as [`When::parse`] reads it.
    #[must_use]
    pub fn print(&self) -> String {
        match self {
            When::Timed { start, end } => {
                format!("{}/{}", start.format(INSTANT), end.format(INSTANT))
            }
            When::AllDay { start, end } => format!("{}/{}", start.format(DAY), end.format(DAY)),
        }
    }

    /// The days it touches in a reader's clock, `to_local` placing a UTC instant there; an
    /// instant span that ends at a midnight does not touch the day it ends on.
    #[must_use]
    pub fn days(&self, to_local: &dyn Fn(NaiveDateTime) -> NaiveDateTime) -> Vec<NaiveDate> {
        let (first, last) = match self {
            When::AllDay { start, end } => (*start, end.pred_opt().unwrap_or(*start)),
            When::Timed { start, end } => {
                let (start, end) = (to_local(*start), to_local(*end));
                let last = if end > start && end.time() == chrono::NaiveTime::MIN {
                    end.date().pred_opt().unwrap_or(start.date())
                } else {
                    end.date()
                };
                (start.date(), last.max(start.date()))
            }
        };
        first
            .iter_days()
            .take_while(|day| *day <= last)
            .take(400)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_prints_as_it_parses_and_never_runs_backwards() {
        for interval in [
            "2026-10-05T13:00:00Z/2026-10-05T13:30:00Z",
            "2026-10-05T13:00:00Z/2026-10-05T13:00:00Z",
            "2026-10-09/2026-10-11",
        ] {
            assert_eq!(When::parse(interval).expect("a span").print(), interval);
        }
        for bad in [
            "2026-10-05T13:00:00Z/2026-10-05T12:00:00Z",
            "2026-10-09/2026-10-09",
            "2026-10-05 13:00/2026-10-05 13:30",
            "2026-10-09/2026-10-10T00:00:00Z",
            "2026-10-09",
            "",
        ] {
            assert!(When::parse(bad).is_err(), "{bad:?}");
        }
    }
}
