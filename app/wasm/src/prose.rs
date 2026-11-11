//! Escaped markup back to the prose it stands for, so a Typst view's state holds text, never markup.

use suzatary_base::escape::{Escaped, unescape_markup};
use wasm_bindgen::prelude::*;

/// The prose `escape_markup` made this markup from, or `undefined` when it is markup a person wrote.
#[wasm_bindgen]
#[must_use]
pub fn prose(markup: &str) -> Option<String> {
    unescape_markup(&Escaped::from_chain(markup)).ok()
}
