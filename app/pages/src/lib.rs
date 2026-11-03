//! Suzatary's pages in the view-tree shape (docs/FRAMEWORK-PLAN-2026-11-02.md): each a
//! [`viewtree::Program`], run as wasm in the tab, its patches painted by `web/src/painter`.
#![forbid(unsafe_code)]

pub mod settings;

impl viewtree::host::Hosted for settings::Settings {
    type Input = settings::Input;
    fn input(input: settings::Input) -> settings::Msg {
        settings::Msg::Host(Box::new(input))
    }
}

#[cfg(target_arch = "wasm32")]
viewtree::export!(SettingsPage = settings::Settings);
