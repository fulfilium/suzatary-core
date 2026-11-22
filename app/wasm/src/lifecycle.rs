//! §4, so a replica can spell an act with no network; the box re-verifies everything it is
//! given, so nothing here widens what a page may write.

use prodrome_core::event::{
    canonical, mk_cancelled, mk_completed, mk_created, mk_reopened, mk_tended,
};
use prodrome_core::fpl::datetime_of;
use prodrome_wasm_exports::wire::parse_instant;
use wasm_bindgen::prelude::*;

use crate::{Event, refused};

/// `at` is the caller's claim and nothing clamps it: a wrong clock is `verify`'s to report,
/// never this module's to rewrite.
#[wasm_bindgen]
pub fn lifecycle(
    kind: &str,
    todo: &str,
    at: &str,
    actor: &str,
    note: &str,
) -> Result<String, JsError> {
    let at = parse_instant("at", at).map_err(refused)?;
    let at = datetime_of(at).map_err(|e| refused(format!("at: {e}")))?;
    let event: Event = match kind {
        "Created" => mk_created(todo, at, actor, "", note),
        "Completed" => mk_completed(todo, at, actor, note),
        "Cancelled" => mk_cancelled(todo, at, actor, note),
        "Reopened" => mk_reopened(todo, at, actor, note),
        "Tended" => mk_tended(todo, at, actor, note),
        other => {
            return Err(refused(format!(
                "kind must be one of Created, Completed, Cancelled, Reopened, Tended, got {other:?}"
            )));
        }
    }
    .map_err(|e| refused(e.to_string()))?;
    Ok(canonical(&event))
}
