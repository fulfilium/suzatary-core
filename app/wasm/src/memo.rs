//! A node's content name for the tab's memoised fold: the Prodrome's `memo::name`, so a tree named in the
//! tab is named as the core names it.

use prodrome_data::event::Hash;
use wasm_bindgen::prelude::*;

/// `memo::name` of the node's own bytes and its children's names in order; a child that is not
/// 64 lowercase hex is refused.
#[wasm_bindgen]
pub fn memo_name(own: &str, children: Vec<String>) -> Result<String, JsError> {
    let children = children
        .into_iter()
        .map(Hash::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| crate::refused(e.to_string()))?;
    Ok(prodrome_memo::name(own.as_bytes(), &children)
        .as_str()
        .to_owned())
}
