//! Time zones: a `TZID` is read as an IANA name, or else as a `VTIMEZONE` the feed defines
//! (Outlook's "Pacific Standard Time").

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, FixedOffset, NaiveDateTime, Offset, TimeZone};
use chrono_tz::Tz;
use icalendar::parser::Component;

use crate::event::{children, text};
use crate::recurrence;

/// A zone a wall-clock time is read in.
#[derive(Debug, Clone, Copy)]
pub enum Zone<'a> {
    /// UTC, or what an unknown name is read as.
    Utc,
    /// A zone of the IANA database.
    Iana(Tz),
    /// A zone the feed defines.
    Defined(&'a [Observance]),
}

/// One `STANDARD` or `DAYLIGHT`: from `start` and each onset its rule makes, read in the offset
/// `from` it replaces, the offset is `to`.
#[derive(Debug, Clone)]
pub struct Observance {
    start: NaiveDateTime,
    from: i32,
    to: i32,
    rule: Option<String>,
}

/// The feed's `VTIMEZONE`s by `TZID`, and the `X-WR-TIMEZONE` a floating time is read in.
#[derive(Debug, Clone, Default)]
pub struct Zones(BTreeMap<String, Vec<Observance>>, pub Option<String>);

impl Zones {
    /// Every `VTIMEZONE` in `calendar`; an observance missing a start or an offset is left out.
    pub fn of(calendar: &Component<'_>) -> Self {
        let offset = |child: &Component<'_>, name| text(child, name)?.parse::<FixedOffset>().ok();
        let observance = |child: &Component<'_>| {
            let start = NaiveDateTime::parse_from_str(text(child, "DTSTART")?, "%Y%m%dT%H%M%S");
            let from = offset(child, "TZOFFSETFROM")?.local_minus_utc();
            let to = offset(child, "TZOFFSETTO")?.local_minus_utc();
            let rule = text(child, "RRULE").map(str::to_owned);
            Some(Observance {
                start: start.ok()?,
                from,
                to,
                rule,
            })
        };
        let zone = |zone: &Component<'_>| {
            let observances = zone.components.iter().filter_map(observance).collect();
            Some((text(zone, "TZID")?.to_owned(), observances))
        };
        let zones = children(calendar, "VTIMEZONE").into_iter();
        let floating = text(calendar, "X-WR-TIMEZONE").map(str::to_owned);
        Self(zones.filter_map(zone).collect(), floating)
    }

    /// The zone `tzid` names, an IANA name ending a longer path included, or `None`.
    pub fn named(&self, tzid: &str) -> Option<Zone<'_>> {
        let defined = || Some(Zone::Defined(self.0.get(tzid)?));
        let path = || {
            tzid.match_indices('/')
                .find_map(|(at, _)| tzid[at + 1..].parse().ok())
        };
        tzid.parse::<Tz>()
            .ok()
            .or_else(path)
            .map(Zone::Iana)
            .or_else(defined)
    }
}

impl Observance {
    /// Its latest onset at or before `utc`, in UTC.
    fn onset(&self, utc: NaiveDateTime) -> Option<NaiveDateTime> {
        let from = Duration::seconds(i64::from(self.from));
        let rule = self.rule.as_deref();
        // A yearly rule that neither counts nor ends makes the same onsets from any year, so it
        // is walked from last year rather than from Outlook's 1601.
        let endless = rule.is_some_and(|rule| {
            rule.contains("YEARLY") && !rule.contains("COUNT") && !rule.contains("UNTIL")
        });
        let start = match self.start.with_year(utc.year() - 1) {
            Some(recent) if endless && recent > self.start => recent,
            Some(_) | None => self.start,
        };
        let rule = rule.and_then(|rule| recurrence::rule(rule, start, &|at| at + from).ok());
        let (starts, _) = recurrence::starts(start, rule, &[], &[], (start, utc + from));
        starts.last().map(|local| *local - from)
    }
}

impl Zone<'_> {
    /// The offset east of UTC in force at `utc`, in seconds; before every onset, the offset the
    /// earliest one replaces.
    pub fn offset_at(self, utc: NaiveDateTime) -> i32 {
        match self {
            Zone::Utc => 0,
            Zone::Iana(tz) => tz.offset_from_utc_datetime(&utc).fix().local_minus_utc(),
            Zone::Defined(observances) => {
                let onsets = observances
                    .iter()
                    .filter_map(|o| Some((o.onset(utc)?, o.to)));
                let earliest = || observances.iter().min_by_key(|o| o.start).map(|o| o.from);
                let latest = onsets.max_by_key(|(at, _)| *at).map(|(_, to)| to);
                latest.or_else(earliest).unwrap_or(0)
            }
        }
    }

    /// What this zone's clock reads at `utc`.
    pub fn local(self, utc: NaiveDateTime) -> NaiveDateTime {
        utc + Duration::seconds(i64::from(self.offset_at(utc)))
    }

    /// `local`, a reading of this zone's clock, in UTC: a reading the clock shows twice is the
    /// first, and one it skips is read with the offset before the jump (RFC 5545 §3.3.5).
    pub fn to_utc(self, local: NaiveDateTime) -> NaiveDateTime {
        let utc = |offset: i32| local - Duration::seconds(i64::from(offset));
        let before = utc(self.offset_at(local - Duration::hours(36)));
        let after = utc(self.offset_at(local + Duration::hours(36)));
        let valid = [before, after]
            .into_iter()
            .filter(|at| self.local(*at) == local);
        valid.min().unwrap_or(before)
    }
}

#[cfg(test)]
mod tests {
    use icalendar::parser::{read_components, unfold};

    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").expect("a literal time")
    }

    /// Microsoft's spelling of New York, with the United States' rules since 2007.
    const EASTERN: &str = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Eastern Standard Time\r\n\
        BEGIN:STANDARD\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0400\r\nTZOFFSETTO:-0500\r\n\
        RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\n\
        DTSTART:16010101T020000\r\nTZOFFSETFROM:-0500\r\nTZOFFSETTO:-0400\r\n\
        RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n\
        END:VCALENDAR\r\n";

    /// A defined zone reads every local time as the IANA database does, on both sides of each
    /// change: a time the clock skips with the offset before the jump, and one it repeats as its
    /// first reading (RFC 5545 §3.3.5). Each row is a local time and its UTC instant.
    #[test]
    fn a_defined_zone_reads_each_time_as_the_database_does() {
        let text = unfold(EASTERN);
        let roots = read_components(&text).expect("a calendar");
        let zones = Zones::of(&roots[0]);
        let defined = zones.named("Eastern Standard Time").expect("defined");
        let iana = Zone::Iana(chrono_tz::America::New_York);
        for (local, utc) in [
            ("2026-01-15 09:00", "2026-01-15 14:00"),
            ("2026-03-07 23:00", "2026-03-08 04:00"),
            ("2026-03-08 02:30", "2026-03-08 07:30"),
            ("2026-03-08 03:30", "2026-03-08 07:30"),
            ("2026-07-04 12:00", "2026-07-04 16:00"),
            ("2026-11-01 01:30", "2026-11-01 05:30"),
            ("2026-11-01 03:00", "2026-11-01 08:00"),
            ("2026-12-31 23:59", "2027-01-01 04:59"),
        ] {
            assert_eq!(defined.to_utc(at(local)), at(utc), "defined, {local}");
            assert_eq!(iana.to_utc(at(local)), at(utc), "IANA, {local}");
        }
        assert!(zones.named("Somewhere Standard Time").is_none());
    }
}
