//! A recurrence set expanded in wall-clock time, so one expansion serves an event in any zone and
//! a `VTIMEZONE`'s observances alike; the caller places each start in UTC through its zone.

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use rrule::{RRule, RRuleSet, Tz, Unvalidated};

use crate::item::Window;

/// A wall-clock reading.
type At = NaiveDateTime;

/// The most instances one set's walk visits from `DTSTART`, since rrule walks every instance
/// from there even when only a window is asked for; past it the walk is cut and the cut said.
pub const MAX_STEPS: usize = 200_000;

fn wall(at: NaiveDateTime) -> DateTime<Tz> {
    at.and_utc().with_timezone(&Tz::UTC)
}

/// The rule `text` from `start`; `local` reads a UTC `UNTIL` on the set's clock, since rrule
/// would read a local `UNTIL` in the host's zone.
///
/// # Errors
/// A rule rrule does not read, or an `UNTIL` that is no date, as the reason.
pub fn rule(text: &str, start: At, local: &dyn Fn(At) -> At) -> Result<RRule, String> {
    let time = |text: &str| NaiveDateTime::parse_from_str(text, "%Y%m%dT%H%M%S").ok();
    let day = |text: &str| NaiveDate::parse_from_str(text, "%Y%m%d").ok();
    let until = |text: &str| match text.strip_suffix('Z') {
        Some(utc) => time(utc).map(local),
        None => time(text).or_else(|| day(text)?.and_hms_opt(23, 59, 59)),
    };
    let parts = text.trim().split(';');
    let (ends, parts): (Vec<&str>, Vec<&str>) = parts.partition(|part| part.starts_with("UNTIL="));
    let parsed: RRule<Unvalidated> = parts.join(";").parse().map_err(|e| format!("{e}"))?;
    let valid = |rule: RRule<Unvalidated>| rule.validate(wall(start)).map_err(|e| format!("{e}"));
    match ends.first().map(|part| &part["UNTIL=".len()..]) {
        None => valid(parsed),
        Some(text) => match until(text) {
            Some(end) => valid(parsed.until(wall(end))),
            None => Err(format!("its UNTIL {text:?} is no date")),
        },
    }
}

/// The starts within `[from, to)` of the set `start`, `rule`, `dates` and less `except`, and
/// whether [`MAX_STEPS`] cut the walk before it reached `to`.
pub fn starts(
    start: At,
    rule: Option<RRule>,
    dates: &[At],
    except: &[At],
    (from, to): Window,
) -> (Vec<At>, bool) {
    // DTSTART is always an instance (RFC 5545 §3.8.5.3); rrule's duplicate of it is removed below.
    let set = RRuleSet::new(wall(start)).rdate(wall(start));
    let set = rule.into_iter().fold(set, RRuleSet::rrule);
    let set = dates.iter().fold(set, |set, at| set.rdate(wall(*at)));
    let set = except.iter().fold(set, |set, at| set.exdate(wall(*at)));
    let mut walk = set.into_iter().map(|at| at.naive_utc());
    let mut starts: Vec<At> = walk
        .by_ref()
        .take(MAX_STEPS)
        .take_while(|at| *at < to)
        .filter(|at| *at >= from)
        .collect();
    let cut = walk.next().is_some_and(|at| at < to);
    starts.dedup();
    (starts, cut)
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").expect("a literal time")
    }

    fn expand(rule: &str, dtstart: &str, window: Window) -> (Vec<At>, bool) {
        let start = at(dtstart);
        let rule = super::rule(rule, start, &|utc| utc).expect("a rule");
        starts(start, Some(rule), &[], &[], window)
    }

    /// Each row is a rule, its `DTSTART`, a window, the starts it keeps and whether the walk was
    /// cut: only the window is kept and COUNT still counts from `DTSTART`, the end is exclusive,
    /// an endless rule stops at the end, and a rule too fine for any calendar is cut.
    #[test]
    fn each_rule_keeps_only_its_window_and_a_walk_too_long_is_cut() {
        let window = (at("2026-10-04 00:00"), at("2026-12-01 00:00"));
        let rows: [(&str, &str, Window, &[&str], bool); 5] = [
            (
                "FREQ=DAILY;COUNT=5",
                "2026-10-01 09:00",
                window,
                &["2026-10-04 09:00", "2026-10-05 09:00"],
                false,
            ),
            (
                "FREQ=DAILY;COUNT=5",
                "2026-10-01 09:00",
                (at("2026-10-01 00:00"), at("2026-10-01 09:00")),
                &[],
                false,
            ),
            (
                "FREQ=WEEKLY;BYDAY=MO",
                "1990-01-01 09:00",
                (at("2026-10-01 00:00"), at("2026-10-15 00:00")),
                &["2026-10-05 09:00", "2026-10-12 09:00"],
                false,
            ),
            ("FREQ=SECONDLY", "2000-01-01 00:00", window, &[], true),
            (
                "FREQ=DAILY;UNTIL=20261005T235959",
                "2026-10-03 09:00",
                window,
                &["2026-10-04 09:00", "2026-10-05 09:00"],
                false,
            ),
        ];
        for (rule, dtstart, window, kept, cut) in rows {
            let kept: Vec<At> = kept.iter().map(|text| at(text)).collect();
            assert_eq!(
                expand(rule, dtstart, window),
                (kept, cut),
                "{rule} from {dtstart}"
            );
        }
        assert!(super::rule("FREQ=DAILY;UNTIL=soon", at("2026-10-01 09:00"), &|utc| utc).is_err());
    }

    proptest::proptest! {
        /// A window is the sum of its halves: what `[from, to)` keeps is what `[from, m)` keeps
        /// followed by what `[m, to)` keeps, for any split, since each instance is decided by the
        /// rule and `DTSTART` alone and never by where the reader looks.
        #[test]
        fn a_window_is_the_sum_of_its_halves(
            freq in 0usize..4,
            interval in 1u32..4,
            days in proptest::collection::btree_set(0u32..7, 0..4),
            count in proptest::option::of(1u32..40),
            start in 0i64..400,
            split in 0i64..700,
        ) {
            let freq = ["DAILY", "WEEKLY", "MONTHLY", "YEARLY"][freq];
            let names = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];
            let byday: Vec<&str> = days.iter().map(|day| names[*day as usize]).collect();
            let byday = if byday.is_empty() || freq == "DAILY" {
                String::new()
            } else {
                format!(";BYDAY={}", byday.join(","))
            };
            let count = count.map_or_else(String::new, |n| format!(";COUNT={n}"));
            let rule = format!("FREQ={freq};INTERVAL={interval}{byday}{count}");
            let dtstart = (at("2025-01-01 09:30") + Duration::days(start)).format("%Y-%m-%d %H:%M").to_string();
            let (from, to) = (at("2025-06-01 00:00"), at("2027-06-01 00:00"));
            let middle = from + Duration::days(split);
            let (whole, _) = expand(&rule, &dtstart, (from, to));
            let (mut halves, _) = expand(&rule, &dtstart, (from, middle));
            halves.extend(expand(&rule, &dtstart, (middle, to)).0);
            proptest::prop_assert_eq!(&whole, &halves);
            proptest::prop_assert!(whole.windows(2).all(|pair| pair[0] < pair[1]), "ascending, no twins");
        }
    }
}
