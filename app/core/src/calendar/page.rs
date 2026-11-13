//! The calendar's days as `reading`'s page and as plain text.

use crate::reading::{CalendarDay, CalendarPage, CalendarRow};

/// The calendar's page.
#[must_use]
pub fn calendar(days: &[super::Day]) -> CalendarPage {
    CalendarPage {
        days: days
            .iter()
            .map(|day| CalendarDay {
                heading: day.date.format("%A %-d %B").to_string(),
                entries: day.entries.iter().map(calendar_row).collect(),
            })
            .collect(),
    }
}

fn calendar_row(entry: &super::Entry) -> CalendarRow {
    let status = entry.content.status().as_str();
    let place = entry.content.place();
    CalendarRow {
        time: entry.time(),
        title: entry.content.title().to_owned(),
        place: (!place.is_empty()).then(|| place.to_owned()),
        calendar: entry.id.calendar().label().to_owned(),
        status: status.to_owned(),
        notes: (status != "confirmed")
            .then(|| status.to_owned())
            .into_iter()
            .chain(entry.conflicted.then(|| "two syncs disagree".to_owned()))
            .collect(),
    }
}

/// The same days as plain text, for a page that has no rendering yet.
#[must_use]
pub fn calendar_text(days: &[super::Day]) -> String {
    days.iter()
        .map(|day| {
            let head = day.date.format("%A %-d %B").to_string();
            let lines: Vec<String> = day
                .entries
                .iter()
                .map(|entry| {
                    let place = if entry.content.place().is_empty() {
                        String::new()
                    } else {
                        format!(", {}", entry.content.place())
                    };
                    let status = match entry.content.status() {
                        super::Status::Confirmed => String::new(),
                        other => format!(" ({})", other.as_str()),
                    };
                    format!(
                        "  {} {}{place}{status}",
                        entry.time(),
                        entry.content.title()
                    )
                })
                .collect();
            if lines.is_empty() {
                format!("{head}\n  nothing")
            } else {
                format!("{head}\n{}", lines.join("\n"))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
