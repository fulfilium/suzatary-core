//! The fake compactor every test uses: no model, the span's lines joined and cut to a node, as
//! the digest post's own test (`OptMem`) compresses. Test support, never production.

use crate::capability::{Compact, ModelError};
use crate::model_name::ModelName;
use crate::secretary::compact_ask::Ask;
use crate::secretary::digest_line::NODE_BYTES;

/// Joins a span's lines with ` / ` and cuts the result at [`NODE_BYTES`].
#[derive(Debug, Clone)]
pub struct Joining(ModelName);

impl Default for Joining {
    fn default() -> Self {
        Self(ModelName::parse("joining").expect("a model name"))
    }
}

impl Compact for Joining {
    fn model(&self) -> &ModelName {
        &self.0
    }

    fn compact(&self, ask: &Ask) -> Result<String, ModelError> {
        let joined = ask.span().join(" / ");
        Ok(joined[..joined.floor_char_boundary(NODE_BYTES)].to_owned())
    }
}
