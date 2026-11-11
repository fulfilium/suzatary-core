use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::refusal::Refusal;
use crate::secretary::model::Turn;
use crate::secretary::store::Entry;
use crate::secretary::transcript;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Prose set aside for the owner to copy; nothing performs it.
pub struct DraftNote;

#[derive(Deserialize, JsonSchema)]
pub struct DraftNoteArgs {
    title: String,
    note: String,
}

impl Verb for DraftNote {
    type Surface = SecretarySurface;
    type Args = DraftNoteArgs;
    type Answer = Value;
    const NAME: &'static str = "draft_note";
    const DESCRIPTION: &'static str = "Set a piece of prose aside in this conversation so the owner can copy it \
        whole — a paragraph for somebody else's thread, a plan, wording they \
        asked you to try. It is NOT a proposal: nothing performs a note, so it \
        has no tap and needs none. Use it when the thing you are writing is not \
        an email this box can send.";

    fn run(call: &Call<'_>, args: DraftNoteArgs) -> Result<Value, Refusal> {
        transcript::push(
            call.caps.conversations,
            call.conversation,
            &Entry::Said {
                at: call.caps.clock.now(),
                turn: Turn::Tool {
                    verb: crate::secretary::SecretaryVerb::DraftNote.into(),
                    answer: format!("{}\n\n{}", args.title, args.note),
                    outcome: crate::secretary::outcome::Outcome::Answered,
                    files: Vec::new(),
                },
                by: None,
            },
        )?;
        Ok(json!({
            "drafted": args.title,
            "next": "It is in the conversation for the owner to copy. Nothing performs a note.",
        }))
    }
}
