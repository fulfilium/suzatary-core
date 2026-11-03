//! One message: its body as text (mail is never markup here), its links, its headers, and the
//! todos whose latest record names it. The todos are drawn by the list's own cards, the host's
//! part `todos`, until the todo pages are programs (plan §7 stage 8).

use serde::{Deserialize, Serialize};
use suzatary_core::wire::MailPage;
use ts_rs::TS;
use viewtree::Attr::{Class, Href, Title};
use viewtree::kit::{Reading, para};
use viewtree::node::foreign;
use viewtree::{Node, Program, view};

use crate::chrome::{self, Chrome, Shape, mark};
use crate::mailrow::{account, ask, body, decided, folder, onward, subject};
use crate::places::{Place, Question};
use crate::runtime::{Answer, Behind, Context, Effect, Held, Input, Never, answered};
use crate::zone::minute;

/// Which message, by its notmuch id or a prefix of its hash, as the address names it.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "MessageFlags")]
pub struct Flags {
    /// The tab's context.
    pub context: Context,
    /// The message.
    pub id: String,
}

/// The page's one question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "MessageHold")]
pub enum Hold {
    /// The message, held.
    Message,
}

/// The message, or the box's words for one it no longer keeps: a 404 is a fact about the maildir
/// (its account removed with its mail, or never synced), drawn as one, never a Retry.
#[derive(Debug, Clone)]
enum Read {
    Found(Box<MailPage>),
    Gone(String),
}

/// It asks nothing once.
pub type Ask = Never;

/// Everything the page knows.
#[derive(Debug, Clone)]
pub struct Model {
    flags: Flags,
    read: Held<Read>,
}

/// Everything that can happen to the page.
#[derive(Debug, Clone)]
pub enum Msg {
    /// From the host.
    Host(Box<Input<Hold, Ask>>),
    /// Back pressed.
    Back,
    /// Retry pressed.
    Retry,
}

/// The page as a program.
#[derive(Debug)]
pub enum Message {}

fn path(m: &Flags) -> String {
    Question::Message(&m.id).path()
}

fn read(r: Reading<Answer>, behind: Option<Behind>) -> Held<Read> {
    let reading = match r {
        Reading::Loading => Reading::Loading,
        Reading::Error { e } => Reading::Error { e },
        Reading::Val {
            v: Answer::Refused { status: 404, words },
        } => Reading::Val {
            v: Read::Gone(words),
        },
        Reading::Val { v } => match answered::<MailPage>(v) {
            Ok(p) => Reading::Val {
                v: Read::Found(Box::new(p)),
            },
            Err(e) => Reading::Error { e },
        },
    };
    Held { reading, behind }
}

impl Program for Message {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect<Hold, Ask>;
    type Flags = Flags;

    fn init(flags: Flags) -> (Model, Vec<Effect<Hold, Ask>>) {
        let hold = Effect::Hold {
            ask: Hold::Message,
            path: path(&flags),
        };
        (
            Model {
                flags,
                read: Held::loading(),
            },
            vec![hold],
        )
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect<Hold, Ask>> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(c) => m.flags.context = c,
                Input::Held {
                    ask: Hold::Message,
                    reading,
                    behind,
                } => m.read = read(reading, behind),
                Input::Answered { ask, .. } => match ask {},
            },
            Msg::Back => {
                return vec![Effect::Back {
                    fallback: Place::Mail {
                        query: "",
                        account: "",
                    }
                    .href(),
                }];
            }
            Msg::Retry => {
                return vec![Effect::Again {
                    path: path(&m.flags),
                }];
            }
        }
        Vec::new()
    }

    fn view(m: &Model) -> Node<Msg> {
        let offsets = &m.flags.context.offsets;
        let (title, legend, body) = match &m.read.reading {
            Reading::Val { v: Read::Found(p) } => {
                let row = &p.message.row;
                let at = row
                    .date
                    .map_or_else(String::new, |d| minute(offsets, &d.isoformat()));
                let legend = if at.is_empty() {
                    row.sender.clone()
                } else {
                    format!("{} · {at}", row.sender)
                };
                (subject(row).to_owned(), legend, found(p))
            }
            Reading::Val {
                v: Read::Gone(said),
            } => ("Message".to_owned(), String::new(), gone(&m.flags.id, said)),
            Reading::Loading | Reading::Error { .. } => {
                ("Message".to_owned(), String::new(), Vec::new())
            }
        };
        let chrome = Chrome {
            class: "mailmessage",
            title,
            back: Some(("Mail", Msg::Back)),
            legend: Some(legend),
            now: Vec::new(),
            shape: Shape::Lines,
        };
        let status = chrome::status(&m.read.reading, m.read.behind.as_ref(), offsets, Msg::Retry);
        chrome::page(chrome, status, body)
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

fn found(p: &MailPage) -> Vec<Node<Msg>> {
    let message = &p.message;
    let row = &message.row;
    let thread = (!message.thread.is_empty()).then(|| {
        onward(
            "threadlink",
            Place::Thread(&message.thread),
            "Whole conversation",
        )
    });
    // Every URL is shown beside its label, because a label is the sender's claim about where a
    // link goes. The URL is clamped by CSS, never cut: cutting would be this app deciding what a
    // link says.
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
    // The box's entries as it printed them, for the cards the host draws; a print of entries,
    // whose maps are keyed by strings, does not fail.
    let todos = serde_json::to_string(&p.todos).unwrap_or_default();
    let cut = message.truncated.then(|| {
        para(
            "legend",
            "Cut at 200 KB — the rest is in the maildir, not on this page.",
        )
    });
    [
        Some(view! { Div "mailchips" [account(row), folder(row), decided(row), thread] }),
        Some(ask(&format!("mail:{}", row.hash))),
        Some(body(row, &message.body)),
        cut,
        links,
        Some(view! { H2 [format!("Todos from this mail ({n})")] }),
        Some(para("legend", said)),
        Some(foreign("todos", &row.hash, todos)),
        Some(headers(p)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

// Collapsed: open, it would repeat the title and the line under it and push the body below a
// phone's fold.
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
        view! { Summary ["Headers", mark("chevron")] },
        view! { Dl {Class => "facts mailfacts"} [facts] },
    ] }
}
