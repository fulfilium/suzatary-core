//! A week of the owner's calendars, drawn by Typst on the box; every title is a stranger's words.

use serde::Deserialize;
use suzatary_api::wire::CalendarPayload;
use ts_rs::TS;
use viewtree::{Node, view};

use crate::chrome::{link, rich};
use crate::places::{Place, Question};
use crate::reading::{Msg, View, json};
use crate::runtime::Answer;
use crate::zone::{Offset, shift};

/// The router resolves `""` to today.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CalendarAt")]
pub struct At {
    /// `YYYY-MM-DD`.
    pub from: String,
}

#[derive(Debug)]
pub enum Calendar {}

impl View for Calendar {
    type At = At;
    type Of = CalendarPayload;
    const CLASS: &'static str = "calendarpage";

    fn path(a: &At) -> String {
        Question::Calendar(&a.from).path()
    }

    fn read(a: Answer) -> Result<CalendarPayload, String> {
        json(a)
    }

    fn title(_: &At, _: Option<&CalendarPayload>) -> String {
        "Calendar".to_owned()
    }

    fn legend(_: &At, _: &[Offset], p: Option<&CalendarPayload>) -> Option<String> {
        p.map(|p| {
            format!(
                "{} days from {}: the calendars at their last sync.",
                p.days, p.from
            )
        })
    }

    fn body(_: &At, _: &[Offset], p: &CalendarPayload) -> Vec<Node<Msg>> {
        let week = |n| {
            link(
                "",
                Place::Calendar(&shift(&p.from, n)),
                if n < 0 { "Previous week" } else { "Next week" },
            )
        };
        vec![
            view! { P "calendarsteps" [week(-7), " · ", link("", Place::Calendar(""), "Today"), " · ", week(7)] },
            rich(p.rich.as_deref(), &p.text, "calendarweek"),
        ]
    }
}
