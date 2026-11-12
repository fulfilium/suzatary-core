//! One message: its body as text (mail is never markup here), its links, headers and todos.

use serde::Deserialize;
use suzatary_api::wire::MailPage;
use ts_rs::TS;
use viewtree::Attr::{Class, Href, Title};
use viewtree::kit::para;
use viewtree::{Node, view};

use crate::chrome::{Shape, ask, mark};
use crate::mailrow::{account, body, decided, folder, onward, subject};
use crate::places::{Place, Question};
use crate::reading::{Msg, View, json};
use crate::runtime::Answer;
use crate::zone::{Offset, minute};

/// Its notmuch id or a prefix of its hash.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "MessageAt")]
pub struct At {
    pub id: String,
}

/// A 404 is a fact about the maildir (its account removed with its mail, or never synced), drawn
/// as `Gone`, never a Retry.
#[derive(Debug, Clone)]
pub enum Read {
    Found(Box<MailPage>),
    Gone(String),
}

#[derive(Debug)]
pub enum Message {}

impl View for Message {
    type At = At;
    type Of = Read;
    const CLASS: &'static str = "mailmessage";
    const SHAPE: Shape = Shape::Lines;

    fn path(a: &At) -> String {
        Question::Message(&a.id).path()
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

    fn read(a: Answer) -> Result<Read, String> {
        match a {
            Answer::Refused { status: 404, words } => Ok(Read::Gone(words)),
            Answer::Refused { words, .. } => Err(words),
            Answer::Done { body } => json(Answer::Done { body }).map(|p| Read::Found(Box::new(p))),
        }
    }

    fn title(_: &At, v: Option<&Read>) -> String {
        match v {
            Some(Read::Found(p)) => subject(&p.message.row).to_owned(),
            Some(Read::Gone(_)) | None => "Message".to_owned(),
        }
    }

    fn legend(_: &At, offsets: &[Offset], v: Option<&Read>) -> Option<String> {
        Some(match v {
            Some(Read::Found(p)) => {
                let row = &p.message.row;
                match row.date.map(|d| minute(offsets, &d.isoformat())) {
                    Some(at) if !at.is_empty() => format!("{} · {at}", row.sender),
                    Some(_) | None => row.sender.clone(),
                }
            }
            Some(Read::Gone(_)) | None => String::new(),
        })
    }

    fn body(a: &At, offsets: &[Offset], v: &Read) -> Vec<Node<Msg>> {
        match v {
            Read::Found(p) => found(offsets, p),
            Read::Gone(said) => gone(&a.id, said),
        }
    }
}

fn capital(s: &str) -> String {
    let mut cs = s.chars();
    cs.next()
        .map(|c| c.to_uppercase().chain(cs).collect())
        .unwrap_or_default()
}

fn gone(id: &str, said: &str) -> Vec<Node<Msg>> {
    vec![
        view! { Code "mailid" [id] },
        para("legend mailgone", format!("{}.", capital(said))),
        para(
            "legend",
            "The sync window is INBOX + 7 days; anything older or elsewhere needs a backfill.",
        ),
    ]
}

fn found(_: &[Offset], p: &MailPage) -> Vec<Node<Msg>> {
    let message = &p.message;
    let row = &message.row;
    let thread = (!message.thread.is_empty()).then(|| {
        onward(
            "threadlink",
            Place::Thread(&message.thread),
            "Whole conversation",
        )
    });
    // Every URL is shown beside its label, which is the sender's claim.
    let links = (!message.links.is_empty()).then(|| {
        let items: Vec<_> = message
            .links
            .iter()
            .map(|l| {
                let label = (!l.label.is_empty()).then(|| view! { Span "linklabel" [l.label.as_str()] });
                view! { Li [label, view! { A "linkurl" {Href => &l.url, Title => &l.url} [l.url.as_str()] }] }
            })
            .collect();
        view! { Div [view! { H2 [format!("Links ({})", message.links.len())] }, view! { Ul "maillinks" [items] }] }
    });
    let n = p.todos.len();
    let said = if n == 0 {
        "No todo on the chain names this message as its source."
    } else {
        "Every todo whose latest record names this message. The numbers are today's."
    };
    // A print of entries, whose maps are keyed by strings, does not fail.
    let cut = message.truncated.then(|| {
        para(
            "legend",
            "Cut at 200 KB — the rest is in the maildir, not on this page.",
        )
    });
    [
        Some(view! { Div "mailchips" [account(row), folder(row), decided(row), thread] }),
        Some(ask(&format!("mail:{}", row.hash))),
        Some(body(&message.body)),
        cut,
        links,
        Some(view! { H2 [format!("Todos from this mail ({n})")] }),
        Some(para("legend", said)),
        Some(crate::part::part(
            &row.hash,
            &crate::part::Part::Todos {
                entries: p.todos.clone(),
            },
        )),
        Some(headers(p)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

// Collapsed: open, it would repeat the title and push the body below a phone's fold.
fn headers(p: &MailPage) -> Node<Msg> {
    let row = &p.message.row;
    let fact = |name: &str, value: Node<Msg>| vec![view! { Dt [name] }, view! { Dd [value] }];
    let mut facts: Vec<Node<Msg>> = p
        .message
        .headers
        .iter()
        .flat_map(|h| {
            fact(
                &h.name.to_lowercase(),
                viewtree::node::text(h.value.as_str()),
            )
        })
        .collect();
    let account = if row.account.is_empty() {
        "—"
    } else {
        &row.account
    };
    let short: String = row.hash.chars().take(16).collect();
    facts.extend(fact("account", viewtree::node::text(account)));
    facts.extend(fact("folder", viewtree::node::text(row.container.as_str())));
    facts.extend(fact(
        "hash",
        view! { Code "hash" {Title => &row.hash} [short] },
    ));
    facts.extend(fact("id", view! { Code "mailid" [row.id.as_str()] }));
    view! { Details "why" [
        view! { Summary ["Headers", mark(crate::part::Mark::Chevron)] },
        view! { Dl {Class => "facts mailfacts"} [facts] },
    ] }
}
