//! Suzatary's pages in the view-tree shape (docs/FRAMEWORK-PLAN-2026-11-02.md): each a
//! [`viewtree::Program`], run as wasm in the tab, its patches painted by `web/src/painter`.
#![forbid(unsafe_code)]

pub mod about;
pub mod briefing;
pub mod briefings;
pub mod calendar;
pub mod chrome;
pub mod mail;
pub mod mailrow;
pub mod memory;
pub mod message;
pub mod places;
pub mod reading;
pub mod releases;
pub mod runtime;
pub mod settings;
pub mod thread;
pub mod zone;

impl viewtree::host::Hosted for settings::Settings {
    type Input = settings::Input;
    fn input(input: settings::Input) -> settings::Msg {
        settings::Msg::Host(Box::new(input))
    }
}

/// A page whose inputs are the runtime's: each is its message `Host`.
macro_rules! hosted {
    ($($page:ident :: $program:ident),*) => {$(
        impl viewtree::host::Hosted for $page::$program {
            type Input = runtime::Input<$page::Hold, $page::Ask>;
            fn input(input: Self::Input) -> $page::Msg {
                $page::Msg::Host(Box::new(input))
            }
        }
    )*};
}

hosted!(mail::Mail, memory::Memory, about::About);

impl<V: reading::View> viewtree::host::Hosted for reading::One<V> {
    type Input = runtime::Input<reading::Hold, runtime::Never>;
    fn input(input: Self::Input) -> reading::Msg {
        reading::Msg::Host(Box::new(input))
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
