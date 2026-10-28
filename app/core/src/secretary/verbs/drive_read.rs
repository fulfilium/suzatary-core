use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::refusal::Refusal;
use crate::secretary::acts::Act;
use crate::secretary::store::Proposal;
use crate::secretary::verb_answer::VerbAnswer;
use crate::secretary::verbs::{Reasoned, drive_path, drive_refusal, kept, propose};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Read one document of the owner's Drive, or ask to.
pub struct DriveRead;

#[derive(Deserialize, JsonSchema)]
pub struct DriveReadArgs {
    path: String,
    #[serde(flatten)]
    reasoned: Reasoned,
}

impl Verb for DriveRead {
    type Surface = SecretarySurface;
    type Args = DriveReadArgs;
    type Answer = VerbAnswer;
    const NAME: &'static str = "drive_read";
    const DESCRIPTION: &'static str = "READ ONE DOCUMENT from the owner's Drive, by its full `path` as \
        `drive_list` printed it: a PDF or an image is handed to you as itself, \
        attached to the answer, and a text file comes back as its text. Any \
        other type is refused by name. In a folder the owner has allowed it reads \
        at once; anywhere else it is ASKED, on a card naming the file, and the \
        file arrives in this conversation if they tap Read. Every SSN, ITIN, TIN \
        and EIN, code, and card or bank number in text is already `[redacted]`, and a PDF whose text names one \
        comes back as that text, not as itself: never try to recover one, and \
        never quote an ID number back, from text or from a scan. When a task needs the number, say \
        which file holds it and let them copy it. `why` and `sources` are \
        required as for proposals.";

    fn run(call: &Call<'_>, args: DriveReadArgs) -> Result<VerbAnswer, Refusal> {
        let reasons = args.reasoned.reasons()?;
        let path = drive_path(&args.path)?;
        if crate::drive::allowed(&call.caps.drive.allowed_folders(), &path) {
            let read = call
                .caps
                .drive
                .read(&path)
                .map_err(|failed| drive_refusal(&failed))?;
            let mut head = Map::new();
            head.insert("path".to_owned(), Value::from(path.to_string()));
            return VerbAnswer::reading(head, path.name().unwrap_or_default(), read, |bytes| {
                kept(call.caps, bytes)
            });
        }
        // Outside the private folder its entry comes first, so the card names a file that is
        // there. Inside it nothing is listed: every name answers with the same card, there or
        // not, so no answer tells the model which private files exist, and the owner's tap is
        // the only read.
        let size = if crate::vault::holds(&path) {
            None
        } else {
            call.caps
                .drive
                .port()
                .entry(&path)
                .map_err(|failed| drive_refusal(&failed))?
                .size
        };
        let act = Act::DriveRead { path, size };
        let sentence = act.sentence();
        let index = propose(
            call.caps,
            call.chains,
            call.conversation,
            Proposal::Act(act),
            &reasons,
            None,
            None,
        )?;
        Ok(json!({
            "asked": index,
            "request": sentence,
            "next": "that folder is not allowed, so the owner was asked; the file arrives here as a turn if they tap Read",
        })
        .into())
    }
}
