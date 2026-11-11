//! The owner's zone: where a wall-clock reading (a day, an hour) is taken.
//!
//! An instant is a point in time, stored naive and UTC by definition; it never needs a zone. A
//! day is the owner's, read in the named zone the instance gives (`[persona] zone`), never the
//! machine's or a device's; it is his, so it moves when he does, and daylight saving is the
//! zone's own rule, never a hand edit.

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Offset as _, TimeZone as _};
use chrono_tz::Tz;
use prodrome_core::literal::Datetime;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A named zone, `America/New_York`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Zone(Tz);

/// A name no zone has.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0:?} names no time zone (an IANA name, such as America/New_York)")]
pub struct NoZone(pub String);

impl Zone {
    /// UTC itself.
    pub const UTC: Zone = Zone(Tz::UTC);

    /// The zone `name` names.
    ///
    /// # Errors
    /// [`NoZone`] for a name the zone database does not hold.
    pub fn named(name: &str) -> Result<Self, NoZone> {
        name.parse::<Tz>()
            .map(Zone)
            .map_err(|_| NoZone(name.to_owned()))
    }

    /// The zone a person most likely meant by `text`: the name in another case, or a city alone
    /// (`London` → `Europe/London`, `new york` → `America/New_York`), spaces read as `_`. Only
    /// names of a region and a city are offered, so `GB` or `Etc/GMT` never are.
    #[must_use]
    pub fn closest(text: &str) -> Option<Self> {
        let wanted = text.trim().replace(' ', "_").to_lowercase();
        let named = || {
            chrono_tz::TZ_VARIANTS
                .iter()
                .copied()
                .filter(|tz| tz.name().contains('/') && !tz.name().starts_with("Etc/"))
        };
        named()
            .find(|tz| tz.name().to_lowercase() == wanted)
            .or_else(|| {
                named().find(|tz| {
                    tz.name()
                        .rsplit('/')
                        .next()
                        .is_some_and(|city| city.to_lowercase() == wanted)
                })
            })
            .map(Zone)
    }

    /// Its name, as the instance spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.0.name()
    }

    /// What the owner's clock on the wall reads at `instant`.
    #[must_use]
    pub fn wall(self, instant: NaiveDateTime) -> NaiveDateTime {
        instant + Duration::seconds(i64::from(self.east_seconds(instant)))
    }

    fn east_seconds(self, instant: NaiveDateTime) -> i32 {
        self.0
            .offset_from_utc_datetime(&instant)
            .fix()
            .local_minus_utc()
    }

    /// The owner's day at `instant`.
    #[must_use]
    pub fn day(self, instant: NaiveDateTime) -> NaiveDate {
        self.wall(instant).date()
    }

    /// The instant the owner's `day` begins: its first wall-clock hour that exists, so a day
    /// whose midnight a clock change skips begins when its clock does.
    #[must_use]
    pub fn start(self, day: NaiveDate) -> NaiveDateTime {
        (0..24)
            .find_map(|hour| {
                let wall = day.and_time(NaiveTime::MIN) + Duration::hours(hour);
                self.0.from_local_datetime(&wall).earliest()
            })
            .map_or_else(|| day.and_time(NaiveTime::MIN), |at| at.naive_utc())
    }

    /// The instant the owner's `day` ends, which is when the next begins.
    #[must_use]
    pub fn end(self, day: NaiveDate) -> NaiveDateTime {
        day.succ_opt()
            .map_or(NaiveDateTime::MAX, |next| self.start(next))
    }

    /// Hours east of UTC at `instant`, which a zone with daylight saving changes twice a year.
    #[must_use]
    pub fn east_at(self, instant: NaiveDateTime) -> i32 {
        self.0
            .offset_from_utc_datetime(&instant)
            .fix()
            .local_minus_utc()
            / 3600
    }
}

impl Serialize for Zone {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Zone {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Zone::named(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// When an act is written and where the owner's days are read: what lowers a day (a deadline, a
/// deferral, today) to an instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    /// The instant, UTC.
    pub at: Datetime,
    /// The owner's zone.
    pub zone: Zone,
}

impl Moment {
    /// The owner's day at this moment.
    #[must_use]
    pub fn today(self) -> NaiveDate {
        self.zone.day(prodrome_core::fpl::instant_of(self.at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").expect("an instant")
    }

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a day")
    }

    fn new_york() -> Zone {
        Zone::named("America/New_York").expect("a zone")
    }

    #[test]
    fn an_evening_in_new_york_is_the_next_utc_day_and_the_same_owners_day() {
        let utc = at("2026-10-07 01:00");
        assert_eq!(new_york().wall(utc), at("2026-10-06 21:00"));
        assert_eq!(new_york().day(utc), day("2026-10-06"));
        assert_eq!(Zone::UTC.day(utc), day("2026-10-07"));
        let berlin = Zone::named("Europe/Berlin").expect("a zone");
        assert_eq!(berlin.wall(utc), at("2026-10-07 03:00"));
    }

    #[test]
    fn a_day_across_new_yorks_fall_back_is_twenty_five_hours_with_no_hand_edit() {
        let sunday = day("2026-11-01");
        assert_eq!(
            new_york().start(sunday),
            at("2026-11-01 04:00"),
            "midnight EDT"
        );
        assert_eq!(
            new_york().end(sunday),
            at("2026-11-02 05:00"),
            "midnight EST"
        );
        assert_eq!(
            new_york().end(sunday) - new_york().start(sunday),
            Duration::hours(25)
        );
        assert_eq!(new_york().east_at(at("2026-10-31 12:00")), -4);
        assert_eq!(new_york().east_at(at("2026-11-02 12:00")), -5);
        // 00:30 EST on the 2nd is still the 1st an hour earlier in EDT terms: the day is the zone's.
        assert_eq!(new_york().day(at("2026-11-02 04:30")), sunday);
        assert_eq!(new_york().day(at("2026-11-02 05:00")), day("2026-11-02"));
    }

    #[test]
    fn a_day_begins_when_its_clock_does_where_midnight_is_skipped() {
        // Havana springs forward at midnight: 2026-03-08 begins at 01:00 local, 05:00 UTC.
        let havana = Zone::named("America/Havana").expect("a zone");
        let start = havana.start(day("2026-03-08"));
        assert_eq!(havana.day(start), day("2026-03-08"));
        assert_eq!(havana.day(start - Duration::seconds(1)), day("2026-03-07"));
    }

    #[test]
    fn a_zone_reads_back_as_its_name_and_an_unknown_name_is_refused() {
        for name in ["America/New_York", "Europe/London", "Europe/Berlin", "UTC"] {
            assert_eq!(Zone::named(name).map(Zone::name), Ok(name));
        }
        assert_eq!(
            Zone::named("Mars/Olympus"),
            Err(NoZone("Mars/Olympus".to_owned()))
        );
        assert_eq!(Zone::named("-4"), Err(NoZone("-4".to_owned())));
    }

    #[test]
    fn every_instant_falls_in_the_day_it_reads_as() {
        let zones = [
            "America/New_York",
            "Europe/London",
            "Europe/Berlin",
            "Pacific/Chatham",
            "UTC",
        ];
        let from = at("2026-01-01 00:00");
        for name in zones {
            let zone = Zone::named(name).expect("a zone");
            for step in 0..(366 * 24 * 4) {
                let instant = from + Duration::minutes(15 * step);
                let today = zone.day(instant);
                assert!(
                    zone.start(today) <= instant && instant < zone.end(today),
                    "{name} {instant}"
                );
            }
        }
    }
}
