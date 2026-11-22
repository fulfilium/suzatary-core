//! A conversation set up from a template (`SetUp`) is owed the box's opening note: the template's
//! steps the box still owes, as an act line stamped by the box. Owed is a reading of the
//! conversation, a `SetUp` with no such note after it, and the note is written under the lines'
//! lock only while it is still owed, so each setup gets one note whatever restarts and workers
//! meet it.

use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_connect::connect::TemplateId;
use suzatary_connect::secrets::SecretState;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Begun, Entry};
use suzatary_inbox::secretary::stamp::Stamp;
use suzatary_lines::secretary::conversations::Conversations;
use suzatary_lines::secretary::transcript;
use suzatary_lines::secretary::transcript::Transcript;
use suzatary_store::store::StoreError;

/// The template whose opening `reading` is owed: none for a conversation not set up, or one the
/// box has noted in.
#[must_use]
pub fn owed(reading: &Transcript) -> Option<TemplateId> {
    let entries = reading.entries();
    let Begun::SetUp(template) = entries.first()?.start()?.begun else {
        return None;
    };
    let noted = entries.iter().skip(1).any(|entry| {
        matches!(
            entry,
            Entry::Said {
                turn: Turn::Act { .. },
                by: Some(Stamp::Box),
                ..
            }
        )
    });
    (!noted).then_some(template)
}

/// The opening `note` of `id`'s template written at `at`, where it is still owed; whether it
/// was.
pub fn open(
    conversations: Conversations<'_>,
    id: &ConversationId,
    at: Datetime,
    note: impl FnOnce(TemplateId) -> String,
) -> Result<bool, StoreError> {
    transcript::push_owed(conversations, id, |reading| {
        owed(reading).map(|template| Entry::noted(at, note(template), Vec::new()))
    })
}

/// Every conversation owed its opening, as a restart finds them: the note a worker was to write
/// when the box stopped.
pub fn owing(conversations: Conversations<'_>) -> Result<Vec<ConversationId>, StoreError> {
    Ok(transcript::each(conversations)?
        .into_iter()
        .filter(|(_, read)| read.as_ref().is_ok_and(|read| owed(read).is_some()))
        .map(|(id, _)| id)
        .collect())
}

/// Whether the box's Google client is stored, its id and its secret both.
#[must_use]
pub fn client_kept(
    secrets: &dyn suzatary_connect::secrets::Secrets,
    settables: &suzatary_connect::settings::Settables,
) -> SecretState {
    SecretState::all(
        [
            suzatary_google::google::CLIENT_ID,
            suzatary_google::google::CLIENT_SECRET,
        ]
        .map(|name| {
            suzatary_connect::settings::Settable::mint(settables, name)
                .map_or(SecretState::Missing, |name| secrets.secret_state(&name))
        }),
    )
}
