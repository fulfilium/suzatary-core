//! The model's conformance suite over the deployment's interpreter, `ClaudeCli`, whose far end is
//! a script standing in for the `claude` binary: the prompt is built and sent down stdin as on
//! the box, the script keeps what it heard and prints one `result` line, and no model is asked.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use suzatary_core::capability::{Conversation, ToolCall, Turn};
use suzatary_core::conformance::model::{Case, Rig, check};
use suzatary_io::model::{ClaudeCli, Fence};

/// Each model gets its own script and the file it writes what it heard to; the rig keeps the
/// last, to say what it heard.
struct Scripts {
    dir: tempfile::TempDir,
    made: Mutex<(usize, Option<PathBuf>)>,
}

impl Scripts {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("a scratch directory"),
            made: Mutex::new((0, None)),
        }
    }

    fn fence() -> Fence {
        Fence::at(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.claude/secretary-settings.json"
        ))
    }
}

impl Rig for Scripts {
    type Model = ClaudeCli;

    fn answering(&self, text: &str) -> ClaudeCli {
        let mut made = self.made.lock().unwrap_or_else(PoisonError::into_inner);
        made.0 += 1;
        let at = self.dir.path().join(made.0.to_string());
        std::fs::create_dir_all(&at).expect("the script's directory");
        let (heard, answer) = (at.join("heard"), at.join("answer"));
        let result = serde_json::json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "num_turns": 1,
            "duration_ms": 5,
            "session_id": "00000000-0000-4000-8000-000000000001",
            "result": text,
        });
        std::fs::write(&answer, result.to_string()).expect("the answer");
        let script = suzatary_testkit::fake_program(
            &at,
            "claude",
            &format!(
                "#!/bin/sh\ncat > {}\ncat {}\n",
                heard.display(),
                answer.display()
            ),
        );
        made.1 = Some(heard);
        ClaudeCli::new("a-model", Self::fence()).with_binary(script.display().to_string())
    }

    fn unreachable(&self) -> ClaudeCli {
        ClaudeCli::new("a-model", Self::fence()).with_binary("/nonexistent/claude")
    }

    /// Everything that went down stdin: what the far end read.
    fn heard(&self, _model: &ClaudeCli) -> String {
        let heard = self
            .made
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .1
            .clone();
        heard
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default()
    }
}

#[test]
fn the_claude_cli_interpreter_keeps_every_law_over_a_scripted_far_end() {
    let conversation = Conversation {
        system: "You are the secretary. Propose; never send.".to_owned(),
        turns: vec![
            Turn::Person {
                text: "When does the lease renew?".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            Turn::Model {
                text: "Let me look.".to_owned(),
                calls: vec![ToolCall {
                    verb: "search".into(),
                    args: std::collections::BTreeMap::new(),
                }],
            },
            Turn::Tool {
                verb: "search".into(),
                answer: "one message: the lease renews on 1 October".to_owned(),
                outcome: suzatary_core::secretary::outcome::Outcome::Answered,
                files: Vec::new(),
            },
            Turn::Act {
                text: "Ada dismissed proposal 1.".to_owned(),
                references: Vec::new(),
            },
        ],
    };
    let cases = [
        Case {
            conversation: conversation.clone(),
            answer: r#"{"say": "It renews on 1 October.", "calls": []}"#.to_owned(),
        },
        Case {
            conversation: conversation.clone(),
            answer: "```json\n{\"say\": \"Checking.\", \"calls\": [{\"verb\": \"context\", \
                     \"args\": {\"day\": \"today\"}}]}\n```"
                .to_owned(),
        },
        Case {
            conversation,
            answer: "Plain prose, which is what was said.".to_owned(),
        },
    ];
    let tools = serde_json::json!([{"name": "context"}, {"name": "search"}]);
    assert_eq!(check(&Scripts::new(), &tools, &cases), Ok(()));
}
