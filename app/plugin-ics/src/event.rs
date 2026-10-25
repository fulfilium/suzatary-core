//! A feed's events as occurrences: each `VEVENT` expanded through its `RRULE` and `RDATE`, less
//! its `EXDATE`, each `RECURRENCE-ID` override standing in for the instance it names, and only
//! what falls in the window kept.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use icalendar::parser::{Component, ParseString, Property};

use crate::item::{DAY, INSTANT, Item, MAX_ATTENDEES, MAX_NAME, MAX_TEXT, Status, When, Window};
use crate::recurrence;
use crate::zone::{Zone, Zones};

/// Occurrences by UID and occurrence, so an override replaces the instance it names.
type Items = BTreeMap<(String, String), Item>;

/// What is said about a feed while reading it: a line for the operator, never an item.
pub type Said<'s> = &'s mut dyn FnMut(String);

/// The most occurrences one calendar emits; past it the earliest are kept and the cut said.
pub const MAX_OCCURRENCES: usize = 2000;

fn props<'c, 'a>(component: &'c Component<'a>, name: &str) -> Vec<&'c Property<'a>> {
    let all = component.properties.iter();
    let named = |property: &&Property<'_>| property.name.as_str().eq_ignore_ascii_case(name);
    all.filter(named).collect()
}

/// The children `name` of `component`, its name in any case.
pub fn children<'c, 'a>(component: &'c Component<'a>, name: &str) -> Vec<&'c Component<'a>> {
    let all = component.components.iter();
    all.filter(|child| child.name.as_str().eq_ignore_ascii_case(name))
        .collect()
}

/// The trimmed value of the first property `name` of `component`, TEXT unescaped.
pub fn text<'c>(component: &'c Component<'_>, name: &str) -> Option<&'c str> {
    Some(props(component, name).first()?.val.as_str().trim())
}

fn param<'p>(property: &'p Property<'_>, key: &str) -> Option<&'p str> {
    let found = property
        .params
        .iter()
        .find(|param| param.key.as_str().eq_ignore_ascii_case(key))?;
    found.val.as_ref().map(ParseString::as_str)
}

/// A DATE, or a DATE-TIME as a reading of its zone's clock.
#[derive(Debug, Clone, Copy)]
enum Moment<'z> {
    Date(NaiveDate),
    Time(NaiveDateTime, Zone<'z>),
}

impl Moment<'_> {
    /// Its instant, a date read as its midnight in UTC.
    fn utc(self) -> NaiveDateTime {
        match self {
            Moment::Date(day) => day.and_time(NaiveTime::MIN),
            Moment::Time(at, zone) => zone.to_utc(at),
        }
    }

    /// The identity of the instance it starts: a date, or a UTC instant.
    fn key(self) -> String {
        match self {
            Moment::Date(day) => day.format("%Y%m%d").to_string(),
            Moment::Time(..) => self.utc().format("%Y%m%dT%H%M%SZ").to_string(),
        }
    }

    /// The span of an instance starting here, an all-day one at least a day; `None` for one
    /// that would end past every date.
    fn span(self, length: Duration) -> Option<When> {
        let print = |at: NaiveDateTime, format| at.format(format).to_string();
        let start = self.utc();
        Some(match self {
            Moment::Date(_) => {
                let end = start.checked_add_signed(Duration::days(length.num_days().max(1)))?;
                let (start, end) = (print(start, DAY), print(end, DAY));
                When::AllDay { start, end }
            }
            Moment::Time(..) => {
                let end = start.checked_add_signed(length.max(Duration::zero()))?;
                let (start, end) = (print(start, INSTANT), print(end, INSTANT));
                When::Timed { start, end }
            }
        })
    }
}

/// One DATE or DATE-TIME of a list; a PERIOD counts by its start.
fn moment<'z>(item: &str, date: bool, zone: Zone<'z>) -> Option<Moment<'z>> {
    let text = item.split('/').next().unwrap_or(item).trim();
    let time = text.strip_suffix(['Z', 'z']);
    let (time, zone) = time.map_or((text, zone), |utc| (utc, Zone::Utc));
    let day = || NaiveDate::parse_from_str(text, "%Y%m%d").ok();
    match NaiveDateTime::parse_from_str(time, "%Y%m%dT%H%M%S") {
        Ok(at) if !date => Some(Moment::Time(at, zone)),
        Ok(_) | Err(_) => day().map(Moment::Date),
    }
}

/// Every occurrence of `calendar`'s events in `window`, named for `name`, sorted by UID and
/// occurrence, at most [`MAX_OCCURRENCES`].
pub fn occurrences(
    calendar: &Component<'_>,
    name: &str,
    window: Window,
    said: Said<'_>,
) -> Vec<Item> {
    let zones = Zones::of(calendar);
    let all = children(calendar, "VEVENT");
    let events: Vec<_> = all
        .iter()
        .filter_map(|event| Some((text(event, "UID")?, *event)))
        .collect();
    if events.len() < all.len() {
        said(format!(
            "{} events with no UID were left out",
            all.len() - events.len()
        ));
    }
    let events = events.into_iter();
    let (overrides, masters): (Vec<_>, Vec<_>) =
        events.partition(|(_, event)| text(event, "RECURRENCE-ID").is_some());
    let masters: BTreeMap<&str, &Component<'_>> = masters.into_iter().collect();
    let mut out = Items::new();
    for (uid, event) in &masters {
        zones.read((name, window), uid, event, None, &mut out, said);
    }
    for (uid, event) in &overrides {
        let master = masters.get(uid).copied();
        zones.read((name, window), uid, event, master, &mut out, said);
    }
    let mut items: Vec<Item> = out
        .into_values()
        .filter(|item| item.overlaps(window))
        .collect();
    if items.len() > MAX_OCCURRENCES {
        said(format!(
            "{} occurrences in the window, the earliest {MAX_OCCURRENCES} kept",
            items.len()
        ));
        items.sort_by_key(|item| item.when.span().map(|(start, _)| start));
        items.truncate(MAX_OCCURRENCES);
        items.sort_by(|a, b| (&a.uid, &a.occurrence).cmp(&(&b.uid, &b.occurrence)));
    }
    items
}

impl Zones {
    fn moments(&self, event: &Component<'_>, name: &str, said: Said<'_>) -> Vec<Moment<'_>> {
        let mut out = Vec::new();
        for property in props(event, name) {
            let date =
                param(property, "VALUE").is_some_and(|value| value.eq_ignore_ascii_case("DATE"));
            let tzid = param(property, "TZID").or(self.1.as_deref());
            let zone = tzid.and_then(|tzid| {
                let zone = self.named(tzid);
                if zone.is_none() {
                    said(format!("TZID {tzid} names no zone known here; read as UTC"));
                }
                zone
            });
            let items = property.val.as_str().split(',');
            out.extend(items.filter_map(|item| moment(item, date, zone.unwrap_or(Zone::Utc))));
        }
        out
    }

    fn moment(&self, event: &Component<'_>, name: &str, said: Said<'_>) -> Option<Moment<'_>> {
        self.moments(event, name, said).into_iter().next()
    }

    /// `DTEND` less `DTSTART`, or `DURATION`.
    fn length(&self, event: &Component<'_>, start: Moment<'_>, said: Said<'_>) -> Option<Duration> {
        if let Some(end) = self.moment(event, "DTEND", said) {
            return Some(end.utc() - start.utc());
        }
        let seconds = match iso8601::duration(text(event, "DURATION")?).ok()? {
            iso8601::Duration::Weeks(weeks) => i64::from(weeks) * 7 * 86_400,
            iso8601::Duration::YMDHMS {
                year,
                month,
                day,
                hour,
                minute,
                second,
                ..
            } => {
                let days = i64::from(year) * 365 + i64::from(month) * 30 + i64::from(day);
                days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second)
            }
        };
        Duration::try_seconds(seconds)
    }

    /// Every instance of a master event under its start, or an override (of `master`) under the
    /// instance it replaces; its fields read from `event`, or from `master` where it says none.
    fn read(
        &self,
        (calendar, window): (&str, Window),
        uid: &str,
        event: &Component<'_>,
        master: Option<&Component<'_>>,
        out: &mut Items,
        said: Said<'_>,
    ) {
        let original = self.moment(event, "RECURRENCE-ID", said);
        let Some(start) = self.moment(event, "DTSTART", said).or(original) else {
            said(format!("UID {uid} has no DTSTART and was left out"));
            return;
        };
        let length = match self.length(event, start, said) {
            Some(length) => Some(length),
            None => master.and_then(|master| {
                let first = self.moment(master, "DTSTART", said)?;
                self.length(master, first, said)
            }),
        };
        let length = length.unwrap_or_default();
        let (local, zone) = match start {
            Moment::Date(day) => (day.and_time(NaiveTime::MIN), Zone::Utc),
            Moment::Time(at, zone) => (at, zone),
        };
        // Every date of the set as a reading of the start's clock; a DATE at its time of day.
        let wall = |moment| match moment {
            Moment::Date(day) => day.and_time(local.time()),
            Moment::Time(at, own) => zone.local(own.to_utc(at)),
        };
        let mut walls = |name| -> Vec<NaiveDateTime> {
            self.moments(event, name, said)
                .into_iter()
                .map(wall)
                .collect()
        };
        let (dates, excluded) = (walls("RDATE"), walls("EXDATE"));
        let rule = match text(event, "RRULE")
            .map(|rule| recurrence::rule(rule, local, &|at| zone.local(at)))
        {
            Some(Err(why)) => {
                said(format!("UID {uid}: RRULE not read ({why}), DTSTART alone"));
                None
            }
            Some(Ok(rule)) => Some(rule),
            None => None,
        };
        // Padded so an instance that began before the window and runs into it is found; an
        // override is read wherever it is, so it replaces its instance.
        let padding = Duration::days(2).checked_add(&length.abs());
        let span = padding.and_then(|padding| match original {
            None => Some((
                window.0.checked_sub_signed(padding)?,
                window.1 + Duration::days(2),
            )),
            Some(_) => Some((
                local.checked_sub_signed(padding)?,
                local.checked_add_signed(padding)?,
            )),
        });
        let Some(span) = span else {
            said(format!(
                "UID {uid}: it lasts past every date, and was left out"
            ));
            return;
        };
        let (starts, cut) = recurrence::starts(local, rule, &dates, &excluded, span);
        if cut {
            said(format!(
                "UID {uid}: its rule was cut after {} instances",
                recurrence::MAX_STEPS
            ));
        }
        let read = |name| clipped(text(event, name).or_else(|| text(master?, name)), MAX_TEXT);
        let status = match (status(event), master.map(status)) {
            (_, Some(Status::Cancelled)) => Status::Cancelled,
            (own, _) => own,
        };
        let attendees = match (props(event, "ATTENDEE").is_empty(), master) {
            (true, Some(master)) => props(master, "ATTENDEE"),
            (true, None) | (false, _) => props(event, "ATTENDEE"),
        };
        let name = |attendee: &&Property<'_>| clipped(param(attendee, "CN"), MAX_NAME);
        let names = attendees.iter().filter_map(name);
        let names: Vec<String> = names.take(MAX_ATTENDEES).collect();
        for local in starts {
            let instance = match start {
                Moment::Date(_) => Moment::Date(local.date()),
                Moment::Time(_, zone) => Moment::Time(local, zone),
            };
            let Some(when) = instance.span(length) else {
                said(format!(
                    "UID {uid}: an instance ends past every date, and was left out"
                ));
                continue;
            };
            let item = Item {
                calendar: calendar.to_owned(),
                uid: clipped(Some(uid), MAX_TEXT).unwrap_or_default(),
                occurrence: original.unwrap_or(instance).key(),
                title: read("SUMMARY").unwrap_or_default(),
                when,
                place: read("LOCATION").unwrap_or_default(),
                attendees: names.clone(),
                status,
            };
            out.insert((uid.to_owned(), item.occurrence.clone()), item);
        }
    }
}

/// The first `max` characters of `text`, trimmed, or `None` for none.
fn clipped(text: Option<&str>, max: usize) -> Option<String> {
    let text: String = text?.trim().chars().take(max).collect();
    (!text.is_empty()).then_some(text)
}

fn status(event: &Component<'_>) -> Status {
    let status = text(event, "STATUS").map(str::to_ascii_uppercase);
    match status.as_deref() {
        Some("CANCELLED") => Status::Cancelled,
        Some("TENTATIVE") => Status::Tentative,
        Some(_) | None => Status::Confirmed,
    }
}
