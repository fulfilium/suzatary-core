use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;
use suzatary_conversations::secretary::store::Entry;
use suzatary_lines::secretary::spoken::Spoken;
use suzatary_lines::secretary::transcript;

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
                turn: Spoken::tool(
                    crate::secretary::SecretaryVerb::DraftNote.into(),
                    format!("{}\n\n{}", args.title, args.note),
                    suzatary_conversations::secretary::outcome::Outcome::Answered,
                    Vec::new(),
                ),
                by: None,
            },
        )?;
        Ok(json!({
            "drafted": args.title,
            "next": "It is in the conversation for the owner to copy. Nothing performs a note.",
        }))
    }
}
