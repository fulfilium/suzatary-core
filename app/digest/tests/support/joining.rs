//! The fake compactor every test uses, never production: the span's lines joined and cut to a
//! node.

use crate::model_name::ModelName;
use crate::secretary::compact::Compact;
use crate::secretary::compact_ask::Ask;
use crate::secretary::digest_line::NODE_BYTES;
use suzatary_conversations::secretary::model::{ModelError, Plain};

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

    fn compact(&self, ask: &Ask) -> Result<Plain, ModelError> {
        let joined = ask.span().join(" / ");
        Ok(Plain {
            text: joined[..joined.floor_char_boundary(NODE_BYTES)].to_owned(),
            usage: None,
        })
    }
}
