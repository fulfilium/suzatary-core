//! Suzatary's pages as `viewtree::Program`s, run as wasm in the tab and painted by
//! `web/src/painter`.
#![forbid(unsafe_code)]
// The crate's dependents are its own tests and the wasm boundary: an item is `pub` only where one
// names it, and the types reachable through it stay unnameable outside.
#![allow(private_interfaces)]

pub mod about;
pub mod answer;
pub mod authoring;
pub mod briefing;
pub mod briefings;
pub mod calendar;
pub mod card;
pub mod chats;
pub mod chrome;
pub mod composer;
pub mod conversation;
pub mod conversations;
pub mod desk;
pub mod digest;
pub mod draft;
pub mod form;
pub mod held;
pub mod inbox;
pub mod intents;
pub mod mail;
pub mod mailrow;
mod measure;
pub mod memory;
pub mod message;
pub mod needsyou;
pub mod part;
pub mod places;
pub mod reading;
pub mod refused;
pub mod releases;
pub mod route;
pub mod runtime;
pub mod secretary;
pub mod settings;
pub mod slices;
pub mod thread;
pub mod todo;
pub mod todoparts;
pub mod todos;
pub mod typstlist;
pub mod waiting;
pub mod zone;

/// Each page's `Hosted`, with the rehearsal file under `tests/rehearsal/` its first readings' code runs
/// through, where it has one.
macro_rules! hosted {
    ($($page:ident :: $program:ident $(= $rehearsal:literal)?),*) => {$(
        impl viewtree::host::Hosted for $page::$program {
            type Input = runtime::Input<$page::Hold, $page::Ask, runtime::Never, runtime::Beside>;
            fn input(input: Self::Input) -> $page::Msg {
                $page::Msg::Host(Box::new(input.bare()))
            }
            type Reply = runtime::Reply<$page::Hold, $page::Ask>;
            fn reply(reply: Self::Reply, text: Vec<u8>) -> $page::Msg {
                $page::Msg::Host(Box::new(reply.with(text)))
            }
            $(fn rehearsal() -> Option<&'static str> {
                Some(include_str!(concat!("../tests/rehearsal/", $rehearsal)))
            })?
        }
    )*};
}

hosted!(
    mail::Mail,
    memory::Memory,
    about::About,
    settings::Settings = "settings.json",
    inbox::Inbox = "inbox.json",
    secretary::Secretary = "secretary.json",
    draft::Draft,
    conversations::Conversations,
    needsyou::NeedsYou = "needsyou.json",
    todo::Todo,
    typstlist::Typst
);

impl viewtree::host::Hosted for todos::Todos {
    type Input = runtime::Input<todos::Hold, todos::Ask, todos::Listen, runtime::Beside>;
    fn input(input: Self::Input) -> todos::Msg {
        todos::Msg::Host(Box::new(input.bare()))
    }
    type Reply = runtime::Reply<todos::Hold, todos::Ask>;
    fn reply(reply: Self::Reply, text: Vec<u8>) -> todos::Msg {
        todos::Msg::Host(Box::new(reply.with(text)))
    }
    /// Flags alone: the page a visit opens on is constructed, painted and freed, so leaving it
    /// for the stream compiles nothing on the page's thread.
    fn rehearsal() -> Option<&'static str> {
        Some(include_str!("../tests/rehearsal/todos.json"))
    }
}

impl viewtree::host::Hosted for conversation::Conversation {
    type Input = runtime::Input<
        conversation::Hold,
        conversation::Ask,
        conversation::Stream,
        runtime::Beside,
    >;
    fn input(input: Self::Input) -> conversation::Msg {
        conversation::Msg::Host(Box::new(input.bare()))
    }
    type Reply = runtime::Reply<conversation::Hold, conversation::Ask>;
    fn reply(reply: Self::Reply, text: Vec<u8>) -> conversation::Msg {
        conversation::Msg::Host(Box::new(reply.with(text)))
    }
    fn rehearsal() -> Option<&'static str> {
        Some(include_str!("../tests/rehearsal/conversation.json"))
    }
}

impl<V: reading::View> viewtree::host::Hosted for reading::One<V> {
    type Input = runtime::Input<reading::Hold, runtime::Never, runtime::Never, runtime::Beside>;
    fn input(input: Self::Input) -> reading::Msg {
        reading::Msg::Host(Box::new(input.bare()))
    }
    type Reply = runtime::Reply<reading::Hold, runtime::Never>;
    fn reply(reply: Self::Reply, text: Vec<u8>) -> reading::Msg {
        reading::Msg::Host(Box::new(reply.with(text)))
    }
}

#[cfg(target_arch = "wasm32")]
viewtree::export!(SettingsPage = settings::Settings);
#[cfg(target_arch = "wasm32")]
viewtree::export!(MailPage = mail::Mail);
#[cfg(target_arch = "wasm32")]
viewtree::export!(MessagePage = reading::One<message::Message>);
#[cfg(target_arch = "wasm32")]
viewtree::export!(ThreadPage = reading::One<thread::Thread>);
#[cfg(target_arch = "wasm32")]
viewtree::export!(MemoryPage = memory::Memory);
#[cfg(target_arch = "wasm32")]
viewtree::export!(AboutPage = about::About);
#[cfg(target_arch = "wasm32")]
viewtree::export!(CalendarPage = reading::One<calendar::Calendar>);
#[cfg(target_arch = "wasm32")]
viewtree::export!(BriefingsPage = reading::One<briefings::Briefings>);
#[cfg(target_arch = "wasm32")]
viewtree::export!(BriefingPage = reading::One<briefing::Briefing>);
#[cfg(target_arch = "wasm32")]
viewtree::export!(InboxPage = inbox::Inbox);
#[cfg(target_arch = "wasm32")]
viewtree::export!(TodoPage = todo::Todo);
#[cfg(target_arch = "wasm32")]
viewtree::export!(TodosPage = todos::Todos);
#[cfg(target_arch = "wasm32")]
viewtree::export!(TypstPage = typstlist::Typst);
#[cfg(target_arch = "wasm32")]
viewtree::export!(SecretaryPage = secretary::Secretary);
#[cfg(target_arch = "wasm32")]
viewtree::export!(ConversationPage = conversation::Conversation);
#[cfg(target_arch = "wasm32")]
viewtree::export!(DraftPage = draft::Draft);
#[cfg(target_arch = "wasm32")]
viewtree::export!(ConversationsPage = conversations::Conversations);
#[cfg(target_arch = "wasm32")]
viewtree::export!(NeedsYouPage = needsyou::NeedsYou);
