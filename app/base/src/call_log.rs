//! A model call's cost, and the journal's line for it: what it spent, never what it said.

use serde::{Deserialize, Serialize};

use crate::conversation_id::ConversationId;

/// The token counts of one call, as the model reported them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Prompt tokens read fresh, from neither cache nor a cache write.
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_prompt_tokens: u64,
    pub cache_written_prompt_tokens: u64,
}

/// Which call a line is about.
#[derive(Debug, Clone, Copy)]
pub enum Call<'a> {
    /// One ask of a round; `step` counts the round's asks from 1.
    Round {
        conversation: &'a ConversationId,
        step: usize,
    },
    Compaction,
}

/// Where the calls that returned are written down: the counts each reported (none where it
/// reported none) and the bytes it sent.
pub trait Journal: Sync {
    fn record(&self, call: Call<'_>, model: &str, usage: Option<TokenUsage>, bytes: u64);
}

/// The journal of the process: one `tracing` line per call at target `suzatary::calls`.
#[derive(Debug, Clone, Copy)]
pub struct Tracing;

impl Journal for Tracing {
    fn record(&self, call: Call<'_>, model: &str, usage: Option<TokenUsage>, bytes: u64) {
        let (kind, conversation, step) = match call {
            Call::Round { conversation, step } => {
                ("round", Some(conversation.as_str()), Some(step))
            }
            Call::Compaction => ("compaction", None, None),
        };
        tracing::info!(
            target: "suzatary::calls",
            kind,
            conversation,
            step,
            model,
            input_tokens = usage.map(|spent| spent.prompt_tokens),
            cache_read_tokens = usage.map(|spent| spent.cached_prompt_tokens),
            cache_written_tokens = usage.map(|spent| spent.cache_written_prompt_tokens),
            output_tokens = usage.map(|spent| spent.completion_tokens),
            bytes,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct Printed(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Printed {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("not poisoned")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn printed(call: Call<'_>, usage: Option<TokenUsage>) -> String {
        let out = Printed::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer({
                let out = out.clone();
                move || out.clone()
            })
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            Tracing.record(call, "a model", usage, 900);
        });
        let bytes = out.0.lock().expect("not poisoned").clone();
        String::from_utf8(bytes).expect("text")
    }

    #[test]
    fn a_round_call_prints_its_counts_and_a_compaction_none_of_a_conversation() {
        let id = ConversationId::new("2026-09-02-1").expect("a slug");
        let spent = TokenUsage {
            prompt_tokens: 11,
            completion_tokens: 22,
            cached_prompt_tokens: 33,
            cache_written_prompt_tokens: 44,
        };
        let line = printed(
            Call::Round {
                conversation: &id,
                step: 2,
            },
            Some(spent),
        );
        for part in [
            "suzatary::calls",
            "kind=\"round\"",
            "conversation=\"2026-09-02-1\"",
            "step=2",
            "model=\"a model\"",
            "input_tokens=11",
            "cache_read_tokens=33",
            "cache_written_tokens=44",
            "output_tokens=22",
            "bytes=900",
        ] {
            assert!(line.contains(part), "{part} in {line}");
        }
        let line = printed(Call::Compaction, None);
        assert!(
            line.contains("kind=\"compaction\"") && line.contains("bytes=900"),
            "{line}"
        );
        for absent in ["conversation", "step", "tokens"] {
            assert!(!line.contains(absent), "{absent} in {line}");
        }
    }
}
