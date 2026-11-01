//! WHAT A JOURNAL MAY SAY OF A MODEL'S RUN: its counts and ids, read off the CLI's final
//! `result` object, and never its words, which quote the mail it read.

use std::collections::{BTreeMap, BTreeSet};

use suzatary_core::capability::SessionId;

use super::stream::{Block, Ended, Event, System, Unreadable, Usage};

/// A run, summarised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// The session it ran as.
    pub session_id: SessionId,
    /// How the CLI said it ended (`success`, `error_max_turns`, …), verbatim; printed, never
    /// read.
    pub ended: String,
    /// What that ending was.
    pub outcome: Outcome,
    /// The turns it took.
    pub turns: u64,
    /// How long it ran, in milliseconds.
    pub duration_ms: u64,
    /// The tokens it cost; `None` where the result did not say.
    pub usage: Option<Usage>,
    /// The tools a permission refused, by name, in the order refused, among those the session
    /// was offered; a refused name it was not offered is counted in `unknown`.
    pub refused: Vec<String>,
    /// The tools it called, by name, with how often, among those the session's `init` event
    /// offered it: a model an email steered can call a name made of the email's words, and a
    /// name no tool has is counted in `unknown` and never printed.
    pub called: BTreeMap<String, u64>,
    /// The calls to, and refusals of, a name the session was never offered: never printed, since
    /// a steered model names a call after the words it read.
    pub unknown: u64,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The CLI did not call it an error.
    Clean,
    /// The CLI called it an error.
    Erred,
    /// An error because the CLI is not signed in: an error whose text BEGINS so, so a narration
    /// quoting an email that says it is not one.
    SignedOut,
}

impl Outcome {
    fn of(ended: &Ended) -> Self {
        let signed_out = ended.result.as_deref().is_some_and(|text| {
            text.trim_start()
                .to_lowercase()
                .starts_with("not logged in")
        });
        match (ended.is_error, signed_out) {
            (false, _) => Outcome::Clean,
            (true, false) => Outcome::Erred,
            (true, true) => Outcome::SignedOut,
        }
    }
}

/// The `result` object `claude -p` prints, summarised, from its one object (`--output-format
/// json`) or from the last of its stream (`stream-json --verbose`), with the calls the stream
/// names; `None` for text that holds no result (an unauthenticated CLI's prose, an empty run).
///
/// # Errors
/// [`Unreadable`] when the last `result` line is not the shape this box reads.
pub fn summary_of(text: &str) -> Result<Option<Summary>, Unreadable> {
    let read: Vec<Result<Event, Unreadable>> = match Event::read(text.trim()) {
        Some(one) => vec![one],
        None => text.lines().filter_map(Event::read).collect(),
    };
    let last = read.iter().rev().find_map(|line| match line {
        Ok(Event::Result(ended)) => Some(Ok(ended)),
        Err(unreadable) => Some(Err(*unreadable)),
        Ok(
            Event::System(_)
            | Event::Assistant { .. }
            | Event::StreamEvent { .. }
            | Event::RateLimitEvent { .. }
            | Event::Other,
        ) => None,
    });
    let Some(ended) = last.transpose()? else {
        return Ok(None);
    };
    let events: Vec<&Event> = read.iter().filter_map(|line| line.as_ref().ok()).collect();
    let offered: BTreeSet<&str> = events
        .iter()
        .flat_map(|event| match event {
            Event::System(System::Init { tools }) => tools.as_slice(),
            Event::System(System::Other)
            | Event::Assistant { .. }
            | Event::StreamEvent { .. }
            | Event::Result(_)
            | Event::RateLimitEvent { .. }
            | Event::Other => &[],
        })
        .map(String::as_str)
        .collect();
    let mut called = BTreeMap::new();
    let mut unknown = 0;
    let calls = events
        .iter()
        .flat_map(|event| match event {
            Event::Assistant { message } => message.content.as_slice(),
            Event::System(_)
            | Event::StreamEvent { .. }
            | Event::Result(_)
            | Event::RateLimitEvent { .. }
            | Event::Other => &[],
        })
        .filter_map(|block| match block {
            Block::ToolUse { name } => Some(name.as_str()),
            Block::Other => None,
        });
    for name in calls {
        if offered.contains(name) {
            *called.entry(name.to_owned()).or_insert(0) += 1;
        } else {
            unknown += 1;
        }
    }
    let mut refused = Vec::new();
    for denial in &ended.permission_denials {
        if offered.contains(denial.tool_name.as_str()) {
            refused.push(denial.tool_name.clone());
        } else {
            unknown += 1;
        }
    }
    Ok(Some(Summary {
        session_id: ended.session_id.clone(),
        ended: ended.subtype.clone(),
        outcome: Outcome::of(ended),
        turns: ended.num_turns,
        duration_ms: ended.duration_ms,
        usage: ended.usage,
        refused,
        called,
        unknown,
    }))
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "session {} ended {}{} after {} turns in {} ms; ",
            self.session_id.as_str(),
            self.ended,
            match self.outcome {
                Outcome::Clean => "",
                Outcome::Erred | Outcome::SignedOut => " (an error)",
            },
            self.turns,
            self.duration_ms,
        )?;
        match &self.usage {
            Some(usage) => write!(
                f,
                "tokens: {} in, {} cached, {} written to cache, {} out",
                usage.input_tokens,
                usage.cache_read_input_tokens,
                usage.cache_creation_input_tokens,
                usage.output_tokens,
            )?,
            None => write!(f, "tokens: not reported")?,
        }
        write!(f, "; {} tool call(s) refused", self.refused.len())?;
        if !self.refused.is_empty() {
            write!(f, " ({})", self.refused.join(", "))?;
        }
        if !self.called.is_empty() {
            let called: Vec<String> = self
                .called
                .iter()
                .map(|(name, times)| format!("{name}×{times}"))
                .collect();
            write!(f, "; called {}", called.join(", "))?;
        }
        if self.unknown > 0 {
            write!(f, "; {} call(s) to a name it was not offered", self.unknown)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runs_summary_holds_its_counts_and_ids_and_none_of_its_words() {
        let offered = serde_json::json!({
            "type": "system", "subtype": "init", "tools": ["ToolSearch", "Bash"],
        });
        let ended = serde_json::json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "duration_ms": 41_000,
            "num_turns": 12,
            "result": "Triaged 3 messages from someone@example.test about the lease",
            "session_id": "0f0e0d0c-0b0a-4009-8008-070605040302",
            "usage": {
                "input_tokens": 10,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 900,
                "output_tokens": 70,
            },
            "permission_denials": [
                {"tool_name": "Bash", "tool_input": {"command": "cat secrets"}},
                {"tool_name": "mcp__suzatary__SECRETWORD@example.org", "tool_input": {}},
            ],
        });
        let said = format!("{offered}\n{ended}\n");
        let summary = summary_of(&said)
            .expect("readable")
            .expect("a result object");
        assert_eq!(summary.turns, 12);
        assert_eq!(summary.outcome, Outcome::Clean);
        assert_eq!(summary.refused, ["Bash"]);
        assert_eq!(
            summary.unknown, 1,
            "a refused name the session was not offered"
        );
        let line = summary.to_string();
        for words in ["lease", "someone@", "cat secrets", "SECRETWORD"] {
            assert!(!line.contains(words), "{line}");
        }
        assert!(
            line.contains("0f0e0d0c-0b0a-4009-8008-070605040302"),
            "{line}"
        );
        assert!(line.contains("tokens: 10 in, 900 cached"), "{line}");
        assert_eq!(summary_of("Not logged in · Please run /login"), Ok(None));
        assert_eq!(summary_of(""), Ok(None));
    }
}
