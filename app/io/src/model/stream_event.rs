//! One event of the model's stream, as the `claude` CLI's `stream_event` lines wrap it: the
//! events this box reads, and every other as [`StreamEvent::Other`].

use serde::Deserialize;

/// The far end's own words for a failure.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Said {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// The message began: what the prompt cost.
    MessageStart {
        message: Started,
    },
    ContentBlockDelta {
        delta: Delta,
    },
    /// The message's running totals.
    MessageDelta {
        usage: Delivered,
    },
    /// The stream failed, which ends the turn.
    Error {
        error: Said,
    },
    /// An event this box does not read (`ping`, a block's start or stop, `message_stop`).
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Started {
    pub usage: Prompted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Prompted {
    pub input_tokens: u64,
    /// Those served from the prompt cache; `None` where the API did not say.
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Delivered {
    /// Its tokens, cumulative.
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    /// Text the reply says.
    TextDelta { text: String },
    /// Thinking, a signature or a tool's input: never the reply's text.
    #[serde(other)]
    Other,
}

impl StreamEvent {
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            StreamEvent::ContentBlockDelta {
                delta: Delta::TextDelta { text },
            } => Some(text),
            StreamEvent::ContentBlockDelta {
                delta: Delta::Other,
            }
            | StreamEvent::MessageStart { .. }
            | StreamEvent::MessageDelta { .. }
            | StreamEvent::Error { .. }
            | StreamEvent::Other => None,
        }
    }
}
