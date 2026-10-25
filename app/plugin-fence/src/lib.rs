//! A source plugin that tries to break the plugin host's fences, one mode per call.
//!
//! The cursor names the mode and its argument, `<mode> <argument>`, so one component carries
//! every attack and `app/plugin/tests/fence.rs` picks one per test. Its world adds the WASI
//! monotonic clock to the source world, so one mode can sleep. Built with the `escapee`
//! feature it targets a world that also imports a filesystem and a network, which the host must
//! refuse to load at all.
//!
//! The component-model glue `wit-bindgen` generates, and its `export!` expands where it is
//! called, is `unsafe`, so this crate cannot forbid `unsafe` as the app's crates do. It is the
//! adversary in these tests and never loaded by the app: what its unsafety could do is exactly
//! what the host's fences are tested to contain.

mod source;
