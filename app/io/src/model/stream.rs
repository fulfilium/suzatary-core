//! One line of the `claude` CLI's stream-json (`--output-format stream-json --verbose`), read
//! into the events this box acts on; every other is [`Event::Other`], and a line that is not
//! JSON, or not the shape its `type` promises, is no event.

use serde::Deserialize;

use suzatary_core::capability::{SessionId, TokenUsage};

use super::stream_event::StreamEvent;

/// One line, tagged by its `type`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The session's start, or a status.
    System(System),
    /// A whole assistant message.
    Assistant {
        /// The message.
        message: Message,
    },
    /// A Messages API stream event, as the CLI relays it.
    StreamEvent {
        /// The event.
        event: StreamEvent,
    },
    /// The run's end.
    Result(Ended),
    /// A line this box does not read (a tool's answer, a rate-limit notice).
    #[serde(other)]
    Other,
}

/// A `system` line, tagged by its `subtype`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum System {
    /// The session began.
    Init {
        /// The tools it was offered, by name.
        tools: Vec<String>,
    },
    /// A status or any other notice.
    #[serde(other)]
    Other,
}

/// An assistant message, read for the tools it called.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Message {
    /// Its blocks.
    pub content: Vec<Block>,
}

/// One block of an assistant message.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// A call of a tool.
    ToolUse {
        /// The name it called, as the model said it.
        name: String,
    },
    /// Text or thinking.
    #[serde(other)]
    Other,
}

/// The final `result` line.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Ended {
    /// How it ended (`success`, `error_max_turns`, …), verbatim.
    pub subtype: String,
    /// Whether the CLI called the run an error.
    pub is_error: bool,
    /// The session it ran as.
    pub session_id: SessionId,
    /// The turns it took.
    pub num_turns: u64,
    /// How long it ran, in milliseconds.
    pub duration_ms: u64,
    /// The reply's text; an error that ended the run before a reply has none.
    pub result: Option<String>,
    /// What it cost; `None` where the line did not say.
    pub usage: Option<Usage>,
    /// The calls a permission refused; an absent list is none refused.
    #[serde(default)]
    pub permission_denials: Vec<Denial>,
}

/// Token counts off a `result` line's `usage` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Usage {
    /// Fresh prompt tokens.
    pub input_tokens: u64,
    /// Prompt tokens written to the cache.
    pub cache_creation_input_tokens: u64,
    /// Prompt tokens read from the cache.
    pub cache_read_input_tokens: u64,
    /// Reply tokens.
    pub output_tokens: u64,
}

/// One refused call.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Denial {
    /// The tool it named.
    pub tool_name: String,
}

impl From<Usage> for TokenUsage {
    fn from(usage: Usage) -> Self {
        Self {
            prompt_tokens: usage.input_tokens,
            completion_tokens: usage.output_tokens,
            cached_prompt_tokens: Some(usage.cache_read_input_tokens),
        }
    }
}

/// A `result` line whose shape is not the one this box reads. It names the shape and never a
/// value, since a value of the wrong type may be the model's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "the model printed a result line this box cannot read: one without a subtype, is_error, \
     session_id, num_turns or duration_ms, or with one of another type"
)]
pub struct Unreadable;

/// A line's `type` alone, so a result line of another shape is known for one.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Tag {
    Result,
    #[serde(other)]
    Other,
}

impl Event {
    /// `line` read, or `None` for one that is not an event.
    #[must_use]
    pub fn of(line: &str) -> Option<Self> {
        serde_json::from_str(line).ok()
    }

    /// `line` read: `None` for a line that is not an event, and [`Unreadable`] for a `result`
    /// line of another shape, which is never read as no result.
    #[must_use]
    pub fn read(line: &str) -> Option<Result<Self, Unreadable>> {
        match serde_json::from_str(line) {
            Ok(event) => Some(Ok(event)),
            Err(_) => match serde_json::from_str(line).ok()? {
                Tag::Result => Some(Err(Unreadable)),
                Tag::Other => None,
            },
        }
    }
}

/// The reply's text a line adds, or `None` for any other line, including one that is not JSON.
#[must_use]
pub fn delta_of(line: &str) -> Option<String> {
    match Event::of(line)? {
        Event::StreamEvent { event } => event.text().map(str::to_owned),
        Event::System(_) | Event::Assistant { .. } | Event::Result(_) | Event::Other => None,
    }
}

/// The final `result` line, or `None` for every other line.
///
/// # Errors
/// [`Unreadable`] for a `result` line of another shape.
pub fn result_of(line: &str) -> Result<Option<Ended>, Unreadable> {
    match Event::read(line).transpose()? {
        Some(Event::Result(ended)) => Ok(Some(ended)),
        Some(
            Event::System(_) | Event::Assistant { .. } | Event::StreamEvent { .. } | Event::Other,
        )
        | None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every line of a real `claude --output-format stream-json` run reads as an event, none
    /// dropped for a shape this box misread: one text delta, none from thinking or a
    /// signature, and one result line carrying the session, the text and the usage.
    #[test]
    fn a_recorded_run_reads_line_by_line() {
        let lines: Vec<&str> = include_str!("../../tests/fixtures/claude-stream-json.jsonl")
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        for line in &lines {
            assert!(Event::of(line).is_some(), "{line}");
            if line.contains("thinking_delta") || line.contains("signature_delta") {
                assert_eq!(delta_of(line), None, "{line}");
            }
        }
        let texts: Vec<String> = lines.iter().filter_map(|line| delta_of(line)).collect();
        assert_eq!(texts, vec!["hello there".to_owned()]);
        let results: Vec<Ended> = lines
            .iter()
            .filter_map(|line| result_of(line).expect("readable"))
            .collect();
        let [ended] = results.as_slice() else {
            panic!("one result line, not {}", results.len());
        };
        assert_eq!(
            ended.session_id.as_str(),
            "00000000-0000-4000-8000-000000000001"
        );
        assert_eq!(ended.result.as_deref(), Some("hello there"));
        assert!(ended.usage.as_ref().expect("usage").output_tokens > 0);
    }

    /// A result missing what every result carries is a result this box cannot read, never no
    /// result and never one of zero turns; and its error quotes none of the line.
    #[test]
    fn a_result_missing_its_counts_is_unreadable_and_quotes_nothing() {
        let line = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":"PRIVATE-7731","session_id":"00000000-0000-4000-8000-000000000001"}"#;
        assert_eq!(result_of(line), Err(Unreadable));
        assert!(!Unreadable.to_string().contains("PRIVATE"));
        assert_eq!(result_of("not json"), Ok(None));
        assert_eq!(result_of(r#"{"type":"user","message":7}"#), Ok(None));
    }
}
