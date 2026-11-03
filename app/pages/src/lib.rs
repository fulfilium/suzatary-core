//! Suzatary's pages in the view-tree shape (docs/FRAMEWORK-PLAN-2026-11-02.md): each a
//! [`viewtree::Program`], run as wasm in the tab, its patches painted by `web/src/painter`.
#![forbid(unsafe_code)]

pub mod chrome;
pub mod mail;
pub mod mailrow;
pub mod message;
pub mod places;
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

hosted!(mail::Mail, message::Message, thread::Thread);

#[cfg(target_arch = "wasm32")]
viewtree::export!(SettingsPage = settings::Settings);
#[cfg(target_arch = "wasm32")]
viewtree::export!(MailPage = mail::Mail);
#[cfg(target_arch = "wasm32")]
viewtree::export!(MessagePage = message::Message);
#[cfg(target_arch = "wasm32")]
viewtree::export!(ThreadPage = thread::Thread);
