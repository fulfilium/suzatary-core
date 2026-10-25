//! The app's reference calendar plugin: a `source` that reads each calendar's secret iCal link
//! and emits one item per event occurrence in a window around today, each an event of
//! `ics.schema`.
//!
//! A calendar is a credential its manifest declares (`PLUGIN_ICS_*`), fetched through the host,
//! so the link is never in this plugin's memory, only the feed it answers. `guest` is the one
//! module that touches the component model; the rest is plain Rust tested natively.

#![deny(unsafe_code)]

mod event;
pub mod item;
mod recurrence;
pub mod sync;
mod zone;

/// The component's side: the world's `fetch`, the WASI clock and standard error lent to
/// [`sync::sync`].
///
/// wit-bindgen's generated lifting and its `export!` expand to `unsafe` where they are written,
/// so this module, and only this one, allows it.
#[allow(unsafe_code)]
mod guest {
    // The generated lifting rebuilds each list from the pointer and length its own allocator
    // handed out, which this lint cannot see.
    #[allow(clippy::same_length_and_capacity)]
    mod bindings {
        wit_bindgen::generate!({
            path: "../wit",
            world: "suzatary:plugin/source@0.4.0",
        });
    }

    use std::io::Write as _;

    use bindings::exports::suzatary::plugin::sync::{Guest, Synced};
    use bindings::suzatary::plugin::fetch::{Target, fetch};

    struct Ics;

    impl Guest for Ics {
        fn sync(cursor: Option<Vec<u8>>, credentials: Vec<String>) -> Result<Synced, String> {
            let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
            let at = crate::item::at(now.naive_utc()).map_err(|refused| refused.to_string())?;
            let mut get = |name: &str| {
                let answer = fetch(&Target::Credential(name.to_owned()));
                answer.map(|response| (response.status, response.body))
            };
            let mut log = |line: String| drop(writeln!(std::io::stderr(), "{line}"));
            let (now, cursor) = (now.naive_utc(), cursor.as_deref());
            let learned = crate::sync::sync(cursor, &credentials, now, &mut get, &mut log);
            let items = learned.items.iter().map(|item| item.event(at)).collect();
            let cursor = learned.cursor;
            Ok(Synced { items, cursor })
        }
    }

    bindings::export!(Ics with_types_in bindings);
}
