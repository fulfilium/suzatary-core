//! One day's briefing, rendered by the box: the secretary's brief, or the saved Typst file.

use serde::Deserialize;
use suzatary_api::wire::{BriefingDay, BriefingPayload};
use ts_rs::TS;
use viewtree::kit::para;
use viewtree::{Node, view};

use crate::chrome::{link, mark, rich};
use crate::places::{Place, Question};
use crate::reading::{Msg, View, json};
use crate::runtime::Answer;
use crate::zone::Offset;

/// The path's word for the owner's day now; the box resolves it.
pub const TODAY: &str = "today";

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "BriefingAt")]
pub struct At {
    /// `YYYY-MM-DD`.
    pub date: String,
}

/// The box says `None` with a 404.
#[derive(Debug, Clone)]
pub enum Day {
    Written(BriefingPayload),
    None,
}

#[derive(Debug)]
pub enum Briefing {}

impl View for Briefing {
    type At = At;
    type Of = Day;
    const CLASS: &'static str = "briefingpage";
    const SHAPE: crate::chrome::Shape = crate::chrome::Shape::Lines;

    fn path(a: &At) -> String {
        Question::Briefing(&a.date).path()
    }

    fn back(_: &At) -> Option<(&'static str, Place<'_>)> {
        Some(("Briefings", Place::Briefings))
    }

    // The box decides the kind: a brief wins over a saved file.
    fn read(a: Answer) -> Result<Day, String> {
        match a {
            Answer::Refused { status: 404, .. } => Ok(Day::None),
            Answer::Refused { words, .. } => Err(words),
            Answer::Done { body } => json(Answer::Done { body }).map(Day::Written),
        }
    }

    fn title(a: &At, _: Option<&Day>) -> String {
        if a.date == TODAY {
            "Today".to_owned()
        } else {
            a.date.clone()
        }
    }

    fn body(a: &At, _: &[Offset], d: &Day) -> Vec<Node<Msg>> {
        let mut out = Self::day(a, d);
        out.push(
            view! { P "legend" [link("earlierbriefings", Place::Briefings, "Earlier briefings")] },
        );
        out
    }
}

impl Briefing {
    fn day(a: &At, d: &Day) -> Vec<Node<Msg>> {
        let Day::Written(p) = d else {
            return vec![para(
                "legend",
                if a.date == TODAY {
                    "No briefing has been written yet today."
                } else {
                    "No briefing was written for this day."
                },
            )];
        };
        match &p.day {
            BriefingDay::Brief {
                conversation,
                text,
                blocks,
                ..
            } => {
                let reply = Place::Conversation {
                    id: conversation,
                    reference: "",
                    focus: false,
                    at: None,
                };
                vec![
                    crate::answer::blocks(text, blocks, Msg::Copy),
                    crate::answer::more(crate::answer::copies(text, blocks, Msg::Copy), false)
                        .unwrap_or_else(|| viewtree::node::text("")),
                    view! { P "legend" [link("", reply, "Reply in the conversation"), ", where its proposals are."] },
                ]
            }
            BriefingDay::Saved { source, rich: html } => vec![
                rich(html.as_deref(), source, "todobody"),
                para(
                    "legend",
                    if html.is_some() {
                        "Rendered by typst."
                    } else {
                        "Shown as typst markup source. Nothing has rendered it."
                    },
                ),
                // The source stays reachable even when the rendering came through.
                view! { Details "why" [view! { Summary ["Source", mark(crate::part::Mark::Chevron)] }, view! { Pre "briefingsource" [source.as_str()] }] },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 404 means no briefing was written that day; any other refusal is a failure, so the
    /// frame offers Retry.
    #[test]
    fn a_404_is_no_briefing_and_nothing_else_is() {
        let refused = |status| Answer::Refused {
            status,
            words: "no".to_owned(),
        };
        assert!(matches!(Briefing::read(refused(404)), Ok(Day::None)));
        assert_eq!(Briefing::read(refused(502)).err(), Some("no".to_owned()));
    }

    /// The path's `today` is titled and answered as today, not as a date nobody wrote.
    #[test]
    fn today_is_named_as_today() {
        let today = At {
            date: TODAY.to_owned(),
        };
        let other = At {
            date: "2026-09-04".to_owned(),
        };
        assert_eq!(Briefing::title(&today, None), "Today");
        assert_eq!(Briefing::title(&other, None), "2026-09-04");
        let words = |a: &At| format!("{:?}", Briefing::body(a, &[], &Day::None));
        assert!(words(&today).contains("yet today"));
        assert!(words(&other).contains("for this day"));
    }
}
