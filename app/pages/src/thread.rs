//! One mail thread, oldest first, with only the newest open. Bodies are text.

use serde::Deserialize;
use suzatary_api::wire::{MailPage, ThreadPayload};
use ts_rs::TS;
use viewtree::Attr::{Class, Href, Open};
use viewtree::kit::para;
use viewtree::{Node, view};

use crate::chrome::{ask, mark};
use crate::mailrow::{account, arrived, body, decided, sender, subject, todo};
use crate::places::{Place, Question};
use crate::reading::{Msg, View, json};
use crate::runtime::Answer;
use crate::zone::Offset;

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ThreadAt")]
pub struct At {
    pub id: String,
}

#[derive(Debug)]
pub enum Thread {}

impl View for Thread {
    type At = At;
    type Of = ThreadPayload;
    const CLASS: &'static str = "mailthread";

    fn path(a: &At) -> String {
        Question::Thread(&a.id).path()
    }

    fn back(_: &At) -> Option<(&'static str, Place<'_>)> {
        Some((
            "Mail",
            Place::Mail {
                query: "",
                account: "",
            },
        ))
    }

    fn read(a: Answer) -> Result<ThreadPayload, String> {
        json(a)
    }

    fn title(_: &At, v: Option<&ThreadPayload>) -> String {
        v.and_then(|t| t.messages.first())
            .map_or("Conversation", |p| subject(&p.message.row))
            .to_owned()
    }

    fn legend(_: &At, _: &[Offset], v: Option<&ThreadPayload>) -> Option<String> {
        Some(v.map_or_else(String::new, |t| {
            let n = t.messages.len();
            format!(
                "{n} message{}, oldest first · notmuch thread {}",
                if n == 1 { "" } else { "s" },
                t.thread
            )
        }))
    }

    fn body(_: &At, offsets: &[Offset], v: &ThreadPayload) -> Vec<Node<Msg>> {
        let n = v.messages.len();
        // The whole-conversation reference is one only this page can offer.
        std::iter::once(ask(&format!("thread:{}", v.thread)))
            .chain(
                v.messages
                    .iter()
                    .enumerate()
                    .map(|(i, p)| block(offsets, p, i + 1 == n)),
            )
            .collect()
    }
}

fn block(offsets: &[Offset], p: &MailPage, open: bool) -> Node<Msg> {
    let row = &p.message.row;
    let cut = p
        .message
        .truncated
        .then(|| para("legend", "Cut at 200 KB."));
    view! { Details {Class => "why threadmsg", Open => open} [
        view! { Summary [
            view! { Span "threadwho" [sender(row)] },
            view! { Span "meta" [arrived(offsets, row)] },
            mark(crate::part::Mark::Chevron),
        ] },
        view! { Div "mailchips" [
            account(row),
            decided(row),
            todo(row),
            view! { A "hashlink" {Href => Place::Message(&row.id).href()} ["open"] },
        ] },
        body(&p.message.body),
        cut,
    ] }
}
