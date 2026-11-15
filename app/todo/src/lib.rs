//! One todo as the folds read it: authoring, views, decisions, renders and the templates that
//! draw them.

#![forbid(unsafe_code)]

pub mod authoring;
pub mod decisions;
pub mod reading;
pub mod render;
pub mod render_cache;
pub mod rendering;
pub mod template;
pub mod todos;
pub mod view;
pub mod wire {
    //! The web's payloads of a todo.
    pub mod todo;
}

// The inert renderer: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
