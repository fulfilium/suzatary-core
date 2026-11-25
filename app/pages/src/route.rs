//! The app's address of one page: the hash `web/src/shell/route.ts` reads, parsed once into a
//! closed sum and printed back, and what the page declares to the layout (`tiles::Page`).

use std::collections::BTreeMap;

use tiles::{Address, Home, Page, Pin, Px, Shape, Share};

use crate::measure;
use crate::runtime::{component, search};

/// Every string reads as one; `print` gives the hash back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Todos,
    /// The list with the add sheet up, its words in the fragment.
    Add {
        text: String,
        title: String,
    },
    Todo {
        id: String,
    },
    /// The Typst list, with the todo it has open (`""` for none).
    Typst {
        open: String,
    },
    Inbox {
        open: Option<String>,
    },
    /// `""` is the inbox, and every account.
    Mail {
        query: String,
        account: String,
    },
    Message {
        id: String,
    },
    Thread {
        id: String,
    },
    About,
    Settings,
    Briefings,
    Briefing {
        date: String,
    },
    /// `""` is today.
    Calendar {
        date: String,
    },
    Secretary {
        refs: Vec<String>,
        topics: Vec<String>,
        focus: bool,
    },
    Memory,
    NewTopic,
    /// The conversations, as a list.
    Conversations,
    Conversation {
        id: String,
        refs: Vec<String>,
        focus: bool,
        at: Option<u64>,
    },
    /// The secretary beside the pages of the focused workspace: the default conversation's talk
    /// about `about`, a reference's text, or the whole conversation for `""`.
    Aside {
        about: String,
    },
    /// The address as it was written, without `#`.
    Unknown {
        hash: String,
    },
}

/// The largest integer a JavaScript number holds exactly, `Number.MAX_SAFE_INTEGER`.
const SAFE: u64 = (1 << 53) - 1;

/// What opens here: a page of the kind of the focused one replaces it (`tiles::Act::Open`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Todos,
    Todo,
    Inbox,
    Mail,
    About,
    Settings,
    Briefings,
    Calendar,
    Secretary,
    Conversation,
    Aside,
}

/// `decodeURIComponent`: `None` where a `%` escape is malformed or the bytes are not UTF-8.
fn decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let [hi, lo] = b.get(i + 1..i + 3)? else {
                return None;
            };
            // Two hex digits, never a sign: `from_str_radix` would take `%+1`.
            let digit = |c: &u8| char::from(*c).to_digit(16);
            out.push(u8::try_from(digit(hi)? * 16 + digit(lo)?).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A hash's query half; a pair that will not decode reads as absent, and the last of a name wins.
fn params(query: &str) -> BTreeMap<String, String> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            let (name, value) = (decode(name)?, decode(value)?);
            (!name.is_empty()).then_some((name, value))
        })
        .collect()
}

/// A comma-separated field, empty entries dropped.
fn list(value: Option<&String>) -> Vec<String> {
    value
        .map(|v| {
            v.split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

impl Route {
    /// Total, as `parseRoute`: `hash` with or without its `#`.
    #[must_use]
    pub fn parse(hash: &str) -> Route {
        Route::read(hash.strip_prefix('#').unwrap_or(hash))
    }

    /// A hash without its `#`.
    fn read(bare: &str) -> Route {
        let unknown = || Route::Unknown {
            hash: bare.to_owned(),
        };
        let whole = bare.strip_prefix('/').unwrap_or(bare);
        let (path, query) = whole.split_once('?').unwrap_or((whole, ""));
        let q = params(query);
        let field = |n: &str| q.get(n).cloned().unwrap_or_default();
        let (head, rest) = path.split_once('/').unwrap_or((path, ""));
        let id = |s: &str| decode(s).filter(|d| !d.is_empty());
        let focus = q.get("focus").is_some_and(|f| f == "1");
        let parsed = match head {
            "" | "todos" => Some(Route::Todos),
            "add" => Some(Route::Add {
                text: field("text"),
                title: field("title"),
            }),
            "inbox" => Some(Route::Inbox {
                open: q.get("open").filter(|o| !o.is_empty()).cloned(),
            }),
            "todo" => id(rest).map(|id| Route::Todo { id }),
            "typst" => decode(rest).map(|open| Route::Typst { open }),
            "mail" if rest.is_empty() => Some(Route::Mail {
                query: field("q"),
                account: field("account"),
            }),
            "mail" => match rest.strip_prefix("thread/") {
                Some(t) => id(t).map(|id| Route::Thread { id }),
                None => id(rest).map(|id| Route::Message { id }),
            },
            "secretary" => match rest {
                "" => Some(Route::Secretary {
                    refs: list(q.get("ref")),
                    topics: list(q.get("topics")),
                    focus,
                }),
                "memory" => Some(Route::Memory),
                "new" => Some(Route::NewTopic),
                _ => id(rest).map(|id| Route::Conversation {
                    id,
                    refs: list(q.get("ref")),
                    focus,
                    at: q
                        .get("at")
                        .filter(|a| !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit()))
                        .and_then(|a| a.parse().ok())
                        // Digits a JavaScript number holds exactly, as `parseRoute` reads them.
                        .filter(|a| *a <= SAFE),
                }),
            },
            "conversations" if rest.is_empty() => Some(Route::Conversations),
            "aside" => decode(rest).map(|about| Route::Aside { about }),
            "about" if rest.is_empty() => Some(Route::About),
            "settings" if rest.is_empty() => Some(Route::Settings),
            "calendar" => decode(rest).map(|date| Route::Calendar { date }),
            "briefings" if rest.is_empty() => Some(Route::Briefings),
            "briefings" => id(rest).map(|date| Route::Briefing { date }),
            _ => None,
        };
        parsed.unwrap_or_else(unknown)
    }

    /// The hash, `#` first: `hrefOf`.
    #[must_use]
    pub fn href(&self) -> String {
        match self {
            Route::Todos => "#/".to_owned(),
            Route::Add { text, title } => {
                format!("#/add{}", search(&[("text", text), ("title", title)]))
            }
            Route::Inbox { open: None } => "#/inbox".to_owned(),
            Route::Inbox { open: Some(open) } => format!("#/inbox{}", search(&[("open", open)])),
            Route::Todo { id } => format!("#/todo/{}", component(id)),
            Route::Typst { open } if open.is_empty() => "#/typst".to_owned(),
            Route::Typst { open } => format!("#/typst/{}", component(open)),
            Route::Mail { query, account } => {
                format!("#/mail{}", search(&[("q", query), ("account", account)]))
            }
            Route::Message { id } => format!("#/mail/{}", component(id)),
            Route::Thread { id } => format!("#/mail/thread/{}", component(id)),
            Route::Secretary {
                refs,
                topics,
                focus,
            } => format!(
                "#/secretary{}",
                search(&[
                    ("ref", &refs.join(",")),
                    ("topics", &topics.join(",")),
                    ("focus", if *focus { "1" } else { "" }),
                ])
            ),
            Route::Memory => "#/secretary/memory".to_owned(),
            Route::NewTopic => "#/secretary/new".to_owned(),
            Route::Conversations => "#/conversations".to_owned(),
            Route::Conversation {
                id,
                refs,
                focus,
                at,
            } => format!(
                "#/secretary/{}{}",
                component(id),
                search(&[
                    ("ref", &refs.join(",")),
                    ("focus", if *focus { "1" } else { "" }),
                    ("at", &at.map(|a| a.to_string()).unwrap_or_default()),
                ])
            ),
            Route::Aside { about } if about.is_empty() => "#/aside".to_owned(),
            Route::Aside { about } => format!("#/aside/{}", component(about)),
            Route::About => "#/about".to_owned(),
            Route::Settings => "#/settings".to_owned(),
            Route::Briefings => "#/briefings".to_owned(),
            Route::Briefing { date } => format!("#/briefings/{}", component(date)),
            Route::Calendar { date } if date.is_empty() => "#/calendar".to_owned(),
            Route::Calendar { date } => format!("#/calendar/{}", component(date)),
            Route::Unknown { hash } => format!("#{hash}"),
        }
    }

    /// The bar's destinations, in its order: the root's children.
    #[must_use]
    pub fn destinations() -> [Route; 8] {
        [
            Route::Todos,
            Route::Inbox { open: None },
            Route::Secretary {
                refs: Vec::new(),
                topics: Vec::new(),
                focus: false,
            },
            Route::Briefings,
            Route::Mail {
                query: String::new(),
                account: String::new(),
            },
            Route::Calendar {
                date: String::new(),
            },
            Route::Settings,
            Route::About,
        ]
    }
}

impl Route {
    /// The page the secretary beside the pages is about, where its reference names one.
    pub(crate) fn subject(&self) -> Option<Route> {
        match self {
            Route::Aside { about } => crate::places::page_of(about).map(|h| Route::parse(&h)),
            Route::Todos
            | Route::Add { .. }
            | Route::Typst { .. }
            | Route::Unknown { .. }
            | Route::Todo { .. }
            | Route::Inbox { .. }
            | Route::Mail { .. }
            | Route::Message { .. }
            | Route::Thread { .. }
            | Route::About
            | Route::Settings
            | Route::Briefings
            | Route::Briefing { .. }
            | Route::Calendar { .. }
            | Route::Secretary { .. }
            | Route::Conversations
            | Route::Memory
            | Route::NewTopic
            | Route::Conversation { .. } => None,
        }
    }
}

impl Page for Route {
    type Kind = Kind;

    fn kind(&self) -> Kind {
        match self {
            Route::Todos | Route::Add { .. } | Route::Typst { .. } | Route::Unknown { .. } => {
                Kind::Todos
            }
            Route::Todo { .. } => Kind::Todo,
            Route::Inbox { .. } => Kind::Inbox,
            Route::Mail { .. } | Route::Message { .. } | Route::Thread { .. } => Kind::Mail,
            Route::About => Kind::About,
            Route::Settings => Kind::Settings,
            Route::Briefings | Route::Briefing { .. } => Kind::Briefings,
            Route::Calendar { .. } => Kind::Calendar,
            Route::Secretary { .. } | Route::Conversations => Kind::Secretary,
            Route::Memory | Route::NewTopic | Route::Conversation { .. } => Kind::Conversation,
            Route::Aside { .. } => Kind::Aside,
        }
    }

    /// A list reads well from a list's width; a page read as a column needs the column's; the
    /// secretary beside a page, the width it had as the window's panel.
    fn shape(&self) -> Shape {
        let least = match self {
            Route::Aside { .. } => measure::BESIDE,
            Route::Todos
            | Route::Add { .. }
            | Route::Typst { .. }
            | Route::Inbox { .. }
            | Route::Mail { .. }
            | Route::Briefings
            | Route::Calendar { .. }
            | Route::Secretary { .. }
            | Route::Conversations => measure::LIST,
            Route::Todo { .. }
            | Route::Message { .. }
            | Route::Thread { .. }
            | Route::About
            | Route::Settings
            | Route::Briefing { .. }
            | Route::Memory
            | Route::NewTopic
            | Route::Conversation { .. }
            | Route::Unknown { .. } => measure::COLUMN,
        };
        Shape::Flows { least: Px(least) }
    }

    fn pin(&self) -> Pin {
        match self.kind() {
            Kind::Aside => Pin::Pinned,
            Kind::Todos
            | Kind::Todo
            | Kind::Inbox
            | Kind::Mail
            | Kind::About
            | Kind::Settings
            | Kind::Briefings
            | Kind::Calendar
            | Kind::Secretary
            | Kind::Conversation => Pin::Free,
        }
    }

    /// A list and the secretary beside a page take one share; a page read as a column, two.
    fn share(&self) -> Share {
        match (self.kind(), self.shape()) {
            (Kind::Aside, _) => Share::ONE,
            (_, Shape::Flows { least }) if least == Px(measure::LIST) => Share::ONE,
            (_, Shape::Flows { .. } | Shape::Paged { .. }) => Share::TWO,
        }
    }

    fn opens(&self) -> Shape {
        Shape::Flows {
            least: Px(measure::COLUMN),
        }
    }
}

impl Home for Route {
    fn home(&self) -> Route {
        let [
            todos,
            inbox,
            secretary,
            briefings,
            mail,
            calendar,
            settings,
            about,
        ] = Route::destinations();
        match self.kind() {
            Kind::Conversation => Route::Conversations,
            // The secretary is opened from the page it is about, so a bare address of it stands
            // beside that page, in its workspace, and a hide goes back to it.
            Kind::Aside => self.subject().unwrap_or(todos),
            Kind::Todos | Kind::Todo => todos,
            Kind::Inbox => inbox,
            Kind::Secretary => secretary,
            Kind::Briefings => briefings,
            Kind::Mail => mail,
            Kind::Calendar => calendar,
            Kind::Settings => settings,
            Kind::About => about,
        }
    }
}

impl Address for Route {
    fn print(&self) -> String {
        let href = self.href();
        href.strip_prefix('#').unwrap_or(&href).to_owned()
    }

    fn parse(s: &str) -> Option<Route> {
        Some(Route::read(s))
    }
}
