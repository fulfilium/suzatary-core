//! When a row was written.

use crate::Error;
use std::fmt;

/// A UTC second since the Unix epoch, printed as `date -u +%FT%TZ` prints it
/// (`2026-11-19T10:03:00Z`), the form train.sh's rows were written in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Instant(pub u64);

const DAY: u64 = 86_400;

impl Instant {
    /// The clock's second; a clock before 1970 reads as the epoch.
    #[must_use]
    pub fn now() -> Instant {
        Instant(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
        )
    }

    pub fn parse(text: &str) -> Result<Instant, Error> {
        let malformed = || {
            Error::Malformed(format!(
                "\"{text}\" is not a UTC instant like 2026-11-19T10:03:00Z"
            ))
        };
        let bytes = text.as_bytes();
        let shape = b"dddd-dd-ddTdd:dd:ddZ";
        if bytes.len() != shape.len()
            || !bytes.iter().zip(shape).all(|(b, s)| match s {
                b'd' => b.is_ascii_digit(),
                _ => b == s,
            })
        {
            return Err(malformed());
        }
        let field = |at: std::ops::Range<usize>| -> u64 {
            text[at]
                .bytes()
                .fold(0, |n, b| n * 10 + u64::from(b - b'0'))
        };
        let (year, month, day) = (field(0..4), field(5..7), field(8..10));
        let (hour, minute, second) = (field(11..13), field(14..16), field(17..19));
        if year < 1970
            || !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return Err(malformed());
        }
        Ok(Instant(
            days_from_civil(year, month, day) * DAY + hour * 3600 + minute * 60 + second,
        ))
    }
}

impl fmt::Display for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day) = civil_from_days(self.0 / DAY);
        let second = self.0 % DAY;
        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
            second / 3600,
            second / 60 % 60,
            second % 60
        )
    }
}

fn days_in_month(year: u64, month: u64) -> u64 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a date on or after it, by the proleptic Gregorian calendar
/// (Howard Hinnant's `days_from_civil`, over eras of 400 years that start on 1 March).
fn days_from_civil(year: u64, month: u64, day: u64) -> u64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year / 400;
    let year_of_era = year % 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The inverse of [`days_from_civil`].
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let days = days + 719_468;
    let era = days / 146_097;
    let day_of_era = days % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = era * 400 + year_of_era + u64::from(month <= 2);
    (year, month, day)
}
