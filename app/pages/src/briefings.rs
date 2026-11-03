//! Briefings: the secretary's morning briefs, newest first, and the files saved before them.

use suzatary_core::wire::BriefingsPayload;
use viewtree::{Node, view};

use crate::chrome::{Masthead, empty, link};
use crate::places::{Place, Question};
use crate::reading::{Msg, Nothing, View, json};
use crate::runtime::Answer;
use crate::zone::Offset;

/// The briefings.
#[derive(Debug)]
pub enum Briefings {}

fn row(date: &str, words: u32, title: &str) -> Node<Msg> {
    let meta = view! { Div "briefingmeta" [view! { Span "briefingdate" [date] }, view! { Span [format!("{words} words")] }] };
    link(
        "briefingrow",
        Place::Briefing(date),
        vec![meta, view! { Div "briefingtitle" [title] }],
    )
}

impl View for Briefings {
    type At = Nothing;
    type Of = BriefingsPayload;
    const CLASS: &'static str = "briefingspage";
    const MASTHEAD: Masthead = Masthead::Destination;

    fn path(_: &Nothing) -> String {
        Question::Briefings.path()
    }

    fn read(a: Answer) -> Result<BriefingsPayload, String> {
        json(a)
    }

    fn title(_: &Nothing, _: Option<&BriefingsPayload>) -> String {
        "Briefings".to_owned()
    }

    fn legend(_: &Nothing, _: &[Offset], _: Option<&BriefingsPayload>) -> Option<String> {
        Some("The secretary's morning brief, every day at 07:03, newest first.".to_owned())
    }

    fn body(_: &Nothing, _: &[Offset], p: &BriefingsPayload) -> Vec<Node<Msg>> {
        let mut briefs: Vec<Node<Msg>> = p
            .briefs
            .iter()
            .map(|b| row(&b.date, b.words, &b.title))
            .collect();
        if briefs.is_empty() {
            briefs.push(empty("No morning brief has been written yet."));
        }
        let saved: Vec<Node<Msg>> = p
            .briefings
            .iter()
            .map(|b| row(&b.date, b.words, &b.title))
            .collect();
        let mut out = vec![view! { Div "briefinglist" [briefs] }];
        if !saved.is_empty() {
            out.extend([
                view! { H2 ["Saved before 11 September 2026"] },
                view! { Div "briefinglist" [saved] },
            ]);
        }
        out
    }
}
