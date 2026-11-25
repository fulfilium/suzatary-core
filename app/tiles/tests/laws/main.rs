//! The tiled layout: its laws over generated runs, and the design note's worked cases.

mod cases;
mod check;
mod dock;
mod laws;
/// The example, whose test runs its `main` and the arrangement it asserts.
#[path = "../../examples/notes.rs"]
mod notes;
mod page;
mod policy;
mod runs;
mod words;
