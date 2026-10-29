use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::attachment::AttachmentId;
use crate::capability::Turn;
use crate::held_value::HeldFile;
use crate::refusal::Refusal;
use crate::secretary::file_search;
use crate::secretary::upload_standing::UploadStanding;
use crate::secretary::verb_answer::VerbAnswer;
use crate::secretary::verbs::kept;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Open one file the owner sent in a chat.
pub struct UploadRead;

#[derive(Deserialize, JsonSchema)]
pub struct UploadReadArgs {
    file: String,
}

impl Verb for UploadRead {
    type Surface = SecretarySurface;
    type Args = UploadReadArgs;
    type Answer = VerbAnswer;
    const NAME: &'static str = "upload_read";
    const DESCRIPTION: &'static str = "OPEN A FILE THE OWNER SENT in a chat: `file` is the id of a \
        `files` row `search` answered. A PDF or a photo on the box is handed to you AS ITSELF, \
        attached to this answer, so read it there. A file he kept private in his Drive comes \
        back as its text, every identity number a placeholder, and as its own `held` \
        placeholder, which a propose_email body carries to send it on his tap; \
        `drive_read` of its `private` path reads the file itself, on his tap. A file whose turn was erased, or that \
        the box holds back, is refused by name.";

    fn run(call: &Call<'_>, args: UploadReadArgs) -> Result<VerbAnswer, Refusal> {
        let id: AttachmentId = args
            .file
            .trim()
            .parse()
            .map_err(|_| Refusal::bad_request("file is a `files` row's id: 64 hex digits"))?;
        let conversations = crate::secretary::list(call.caps.conversations)?;
        let named = file_search::named(&conversations);
        if !named.contains(&id) {
            return Err(Refusal::not_found(format!(
                "no live turn sent file {id}: it was erased, or never sent in a chat"
            )));
        }
        let index = call.caps.upload_index;
        let failed = |failed: crate::capability::IndexError| Refusal::internal(failed.to_string());
        match index.refresh(&named).map_err(failed)?.get(&id) {
            Some(UploadStanding::Indexed(_)) => {}
            Some(UploadStanding::Held) => {
                return Err(Refusal::forbidden(format!(
                    "file {id} names an identity number and is held back until the owner keeps it \
                     private in his Drive; it is not read"
                )));
            }
            Some(UploadStanding::Erased) | None => {
                return Err(Refusal::not_found(format!(
                    "the box no longer holds file {id}"
                )));
            }
        }
        let read = index
            .open(&id)
            .map_err(failed)?
            .ok_or_else(|| Refusal::not_found(format!("the box no longer holds file {id}")))?;
        let attachment = conversations
            .iter()
            .flat_map(|stored| &stored.turns)
            .find_map(|said| match &said.turn {
                Turn::Person { attachments, .. } => {
                    attachments.iter().find(|attachment| attachment.id == id)
                }
                Turn::Model { .. } | Turn::Tool { .. } | Turn::Act { .. } | Turn::Erased { .. } => {
                    None
                }
            });
        let name = attachment
            .and_then(|attachment| attachment.name.clone())
            .unwrap_or_else(|| "photo".to_owned());
        let mut head = Map::new();
        head.insert("file".to_owned(), Value::from(id.as_str()));
        if let Some(copy) = attachment.and_then(|attachment| attachment.private.as_ref()) {
            head.insert("private".to_owned(), Value::from(copy.path.to_string()));
        }
        VerbAnswer::reading(
            head,
            &HeldFile::Upload { id: id.clone() },
            &name,
            read,
            |bytes| kept(call.caps, bytes),
        )
    }
}
