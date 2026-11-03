//! One day's briefing, rendered by the box: the secretary's brief block by block, or the saved
//! Typst file.

use serde::{Deserialize, Serialize};
use suzatary_core::wire::{BlockRow, BriefingDay, BriefingPayload};
use ts_rs::TS;
use viewtree::kit::para;
use viewtree::node::foreign;
use viewtree::{Node, view};

use crate::chrome::{ask, link, mark, rich};
use crate::places::{Place, Question};
use crate::reading::{Msg, View, json};
use crate::runtime::Answer;
use crate::zone::Offset;

/// The day.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "BriefingAt")]
pub struct At {
    /// `YYYY-MM-DD`.
    pub date: String,
}

/// What the day holds: what was written, or nothing, which the box says with a 404.
#[derive(Debug, Clone)]
pub enum Day {
    /// The brief or the saved file.
    Written(BriefingPayload),
    /// No briefing was written for the day.
    None,
}

/// One day's briefing.
#[derive(Debug)]
pub enum Briefing {}

/// What an answer's parts are drawn from: the conversation's own drawing of a turn.
#[derive(Serialize)]
struct Turn<'a> {
    text: &'a str,
    blocks: &'a [BlockRow],
}

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

    // Which kind the day is, the box decides: a brief wins over a saved file.
    fn read(a: Answer) -> Result<Day, String> {
        match a {
            Answer::Refused { status: 404, .. } => Ok(Day::None),
            Answer::Refused { words, .. } => Err(words),
            Answer::Done { body } => json(Answer::Done { body }).map(Day::Written),
        }
    }

    fn title(a: &At, _: Option<&Day>) -> String {
        a.date.clone()
    }

    fn body(_: &At, _: &[Offset], d: &Day) -> Vec<Node<Msg>> {
        let Day::Written(p) = d else {
            return vec![para("legend", "No briefing was written for this day.")];
        };
        match &p.day {
            BriefingDay::Brief {
                conversation,
                text,
                blocks,
                ..
            } => {
                let turn = serde_json::to_string(&Turn { text, blocks })
                    .expect("an answer prints as JSON");
                let reply = Place::Conversation {
                    id: conversation,
                    reference: "",
                    focus: false,
                    at: None,
                };
                vec![
                    foreign("answer", conversation, turn.clone()),
                    foreign("answercopies", conversation, turn),
                    view! { P "legend" [link("", reply, "Reply in the conversation"), ", where its proposals are."] },
                ]
            }
            BriefingDay::Saved { source, rich: html } => vec![
                ask(&format!("briefing:{}", p.date)),
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
                view! { Details "why" [view! { Summary ["Source", mark("chevron")] }, view! { Pre "briefingsource" [source.as_str()] }] },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 404 is the box saying no briefing was written that day; any other refusal is a
    /// failure, so the frame offers Retry.
    #[test]
    fn a_404_is_no_briefing_and_nothing_else_is() {
        let refused = |status| Answer::Refused {
            status,
            words: "no".to_owned(),
        };
        assert!(matches!(Briefing::read(refused(404)), Ok(Day::None)));
        assert_eq!(Briefing::read(refused(502)).err(), Some("no".to_owned()));
    }
}
