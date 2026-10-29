//! The secretary's routes: its conversations, a person's turns, photos and memory.

use axum::Json;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use prodrome_core::event::Actor;
use suzatary_core::attachment::{Attachment, AttachmentId};
use suzatary_core::file_read::Withheld;
use suzatary_core::reference::Reference;
use suzatary_core::refusal::{Refusal, shown};
use suzatary_core::secretary::readings;
use suzatary_core::secretary::routes as secretary_routes;
use suzatary_core::secretary::stamp::{self, Stamp};
use suzatary_core::secretary::store::ConversationId;
use suzatary_core::wire;
use suzatary_io::attachments::{Stored, UploadError};

use super::body::{Body, MAX_ACT_BYTES};
use super::call::Call;
use super::guard::{Reader, Writer};
use super::params::{Asked, Params};
use super::reply::Accepted;
use super::{Chained, State};
use suzatary_core::secretary::memory::FactId;

/// A block of an answer, named by its sha.
pub struct Block {
    id: ConversationId,
    sha: String,
}

/// A person's turn: the words, the chips and the files it names.
pub struct Said {
    say: String,
    references: Vec<Reference>,
    attachments: Vec<Attachment>,
    /// How many turns the asker saw; absent, a question is always a new line.
    after: Option<usize>,
}

/// A photo or PDF, decoded; the name it was picked with, and where the owner said to keep it.
pub struct Upload {
    bytes: Vec<u8>,
    name: String,
    keep: Keep,
}

/// Where an upload is kept.
enum Keep {
    /// In the box's store, unless it names an identity number, which holds it back.
    OnTheBox,
    /// In the owner's Drive, on his say-so for a file the box held back.
    Private,
}

/// A stored upload's bytes, served so they never read as a page.
pub struct Served(Attachment, Vec<u8>);

/// A conversation's body is a person's turn, not a document.
const MAX_SAY_BYTES: usize = 32_768;

/// An upload: the largest PDF, grown by a third as base64, and room for the JSON around it.
const MAX_UPLOAD_BYTES: usize = suzatary_core::file_read::MAX_PDF / 3 * 4 + 65_536;

/// The conversation a path's segment names, parsed.
///
/// # Errors
/// [`Refusal::bad_request`] for a name no conversation could have.
pub fn conversation_named(asked: &Asked) -> Result<ConversationId, Refusal> {
    ConversationId::new(asked.segment("conversation")?)
        .map_err(|failed| Refusal::bad_request(failed.to_string()))
}

impl Params for ConversationId {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        conversation_named(asked)
    }
}

impl Params for Block {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        Ok(Self {
            id: conversation_named(asked)?,
            sha: asked.segment("sha")?.to_owned(),
        })
    }
}

impl Params for FactId {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        let id = asked.segment("id")?;
        id.parse().map_err(|_| {
            Refusal::bad_request(format!("a fact is named by its number, not {}", shown(id)))
        })
    }
}

/// A digest node, by its content name.
impl Params for prodrome_core::event::Hash {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        let node = asked.segment("node")?;
        prodrome_core::event::Hash::new(node).map_err(|_| {
            Refusal::bad_request(format!(
                "a summary is named by its node's 64-hex name, not {}",
                shown(node)
            ))
        })
    }
}

impl Params for AttachmentId {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        asked.segment("id")?.parse().map_err(
            |failed: suzatary_core::attachment::AttachmentError| {
                Refusal::bad_request(failed.to_string())
            },
        )
    }
}

impl Body for wire::Titling {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = Self;

    fn parse(titling: Self, _: &State) -> Result<Self, Refusal> {
        Ok(titling)
    }
}

impl Body for secretary_routes::Erasing {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = Self;

    fn parse(erasing: Self, _: &State) -> Result<Self, Refusal> {
        Ok(erasing)
    }
}

impl Body for Said {
    const CAP: usize = MAX_SAY_BYTES;

    type Wire = wire::Saying;

    /// Each photo reread from the store, so its type is the store's.
    fn parse(saying: wire::Saying, state: &State) -> Result<Self, Refusal> {
        let wire::Saying {
            say,
            references,
            attachments,
            after,
        } = saying;
        let attachments = if attachments.is_empty() {
            Vec::new()
        } else {
            let store = &uploads(state)?.store;
            attachments
                .iter()
                .map(|one| {
                    let refused = |failed: &dyn std::fmt::Display| {
                        Refusal::bad_request(format!("attachments: {failed}"))
                    };
                    match store
                        .attachment(&one.id)
                        .map_err(|failed| refused(&failed))?
                        .ok_or_else(|| refused(&format!("no upload {} in the store", one.id)))?
                    {
                        Stored::Here { attachment, .. } => attachment
                            .named(&one.name)
                            .map_err(|failed| refused(&failed)),
                        // Named when it was kept; the page cannot rename it.
                        Stored::Private { attachment, .. } => Ok(attachment),
                    }
                })
                .collect::<Result<_, _>>()?
        };
        Ok(Self {
            say,
            references,
            attachments,
            after,
        })
    }
}

impl Body for Upload {
    const CAP: usize = MAX_UPLOAD_BYTES;

    type Wire = wire::Uploading;

    fn parse(uploading: wire::Uploading, _: &State) -> Result<Self, Refusal> {
        if uploading.file.is_empty() {
            return Err(Refusal::bad_request("file: base64 text is required"));
        }
        Ok(Self {
            bytes: suzatary_io::attachments::decoded(&uploading.file)
                .map_err(|failed| Refusal::bad_request(format!("file: {failed}")))?,
            name: uploading.name,
            keep: if uploading.private {
                Keep::Private
            } else {
                Keep::OnTheBox
            },
        })
    }
}

impl IntoResponse for Served {
    fn into_response(self) -> Response {
        let Served(attachment, bytes) = self;
        (
            [
                (
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static(attachment.media_type.as_str()),
                ),
                (
                    header::X_CONTENT_TYPE_OPTIONS,
                    header::HeaderValue::from_static("nosniff"),
                ),
                (
                    header::CONTENT_SECURITY_POLICY,
                    header::HeaderValue::from_static("default-src 'none'; sandbox"),
                ),
            ],
            bytes,
        )
            .into_response()
    }
}

fn uploads(state: &State) -> Result<&super::Uploads, Refusal> {
    state
        .uploads
        .as_deref()
        .ok_or_else(|| Refusal::not_found("this server keeps no uploads"))
}

/// `GET /api/secretary/memory`: every standing fact.
pub fn memory(call: &Call<Reader, (), ()>) -> Result<Json<wire::MemoryPayload>, Refusal> {
    secretary_routes::memory(call.chain.caps.secretary.memories).map(Json)
}

/// `POST /api/secretary/memory/<id>/forget`: erase one fact.
pub fn forget(call: &Call<Actor, FactId, ()>) -> Result<Json<wire::MemoryPayload>, Refusal> {
    let caps = call.chain.caps;
    secretary_routes::forget(
        caps.secretary.memories,
        caps.secretary.clock,
        &call.guard,
        call.params,
    )
    .map(Json)
}

/// `GET /api/secretary/record`: the summaries a session is told of the owner's words.
pub fn record(call: &Call<Reader, (), ()>) -> Result<Json<wire::RecordPayload>, Refusal> {
    secretary_routes::record(&call.chain.caps.secretary.memories.digests).map(Json)
}

/// `POST /api/secretary/record/<node>/retract`: the owner's tap on a wrong summary.
pub fn retract(
    call: &Call<suzatary_core::secretary::tap::Tap, prodrome_core::event::Hash, ()>,
) -> Result<Json<wire::RecordPayload>, Refusal> {
    let caps = call.chain.caps;
    secretary_routes::retract(
        caps.secretary.memories,
        caps.secretary.clock,
        &call.guard,
        &call.params,
    )
    .map(Json)
}

/// `POST /api/secretary/conversations`: open a topic.
pub fn open(
    call: &Call<Writer, (), wire::Titling>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    call.state
        .with_cards(|caps, cards| secretary_routes::open(caps, cards, &call.body.title))
        .map(Json)
}

/// `POST …/<id>/title`: rename one, as the gate's actor.
pub fn rename(
    call: &Call<Actor, ConversationId, wire::Titling>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    call.state
        .with_cards(|caps, cards| {
            secretary_routes::rename(caps, cards, &call.guard, &call.params, &call.body.title)
        })
        .inspect(|_| call.state.heard(&call.params))
        .map(Json)
}

/// `POST …/<id>/erase`: erase turns of one, or all of it, as the owner's tap.
pub fn erase(
    call: &Call<suzatary_core::secretary::tap::Tap, ConversationId, secretary_routes::Erasing>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    call.state
        .with_cards(|caps, cards| {
            secretary_routes::erase(caps, cards, &call.guard, &call.params, &call.body)
        })
        .inspect(|_| {
            // The erased turns' uploads leave the index now, not at the next search.
            let caps = call.chain.caps;
            if let Err(failed) = suzatary_core::secretary::file_search::reindex(
                caps.secretary.conversations,
                caps.secretary.upload_index,
            ) {
                tracing::warn!(
                    "secretary: reindexing the uploads after an erasure: {}",
                    failed.message()
                );
            }
        })
        .map(Json)
}

/// `GET …/blocks/<sha>`: one block of an answer as typst's HTML, a running round's or a stored
/// one's: rendering is asked by content, and the box renders only what a model wrote.
///
/// A sha no answer holds is a 404, and a block kept as text is a 422 saying why.
pub fn block(call: &Call<Reader, Block, ()>) -> Result<Json<wire::BlockRender>, Refusal> {
    let Block { id, sha } = &call.params;
    let source = match call.state.workers.closed(id, sha) {
        Some(source) => Some(source),
        None => readings::block_named(
            &suzatary_core::secretary::read(call.chain.caps.secretary.conversations, id)?,
            sha,
        ),
    };
    let Some(source) = source else {
        return Err(Refusal::not_found(format!(
            "no answer in {id} holds a block {}",
            shown(sha)
        )));
    };
    let block = suzatary_core::secretary::answer::Renderable::of(&source);
    call.chain
        .caps
        .answers
        .block(&block)
        .map(|html| {
            Json(wire::BlockRender {
                html: html.as_str().to_owned(),
                key: suzatary_core::rendering::key(
                    &suzatary_core::rendering::ANSWER,
                    block.as_str(),
                ),
            })
        })
        .map_err(|refused| Refusal::unprocessable(refused.to_string()))
}

/// The stamp the gate's `actor` puts on a turn pointing at `references`
/// ([`stamp::pointing`]); the chains are read only when one is a todo.
fn stamped(state: &State, actor: &Actor, references: &[Reference]) -> Stamp {
    if references
        .iter()
        .any(|reference| matches!(reference, Reference::Todo(_)))
    {
        let read = state.chains_read();
        stamp::pointing(
            actor.as_str(),
            references,
            &read.set(state.first.caps.secretary.clock.now()),
        )
    } else {
        Stamp::Actor(actor.as_str().to_owned())
    }
}

/// `POST …/say`: 202 and a background turn, so a phone that gives up never retries into
/// a second turn; a `say` mid-round only nudges, so two make one exchange.
pub fn say(
    call: &Call<Actor, ConversationId, Said>,
) -> Result<Accepted<wire::SayPayload>, Refusal> {
    let Said {
        say,
        references,
        attachments,
        after,
    } = &call.body;
    let id = &call.params;
    let by = stamped(&call.state, &call.guard, references);
    call.state
        .with_cards(|caps, cards| {
            secretary_routes::say(caps, cards, id, say, references, attachments, by, *after)
        })
        .inspect(|_| call.state.heard(id))
        .map(Accepted)
}

/// `POST …/regenerate`: ask the last round again; a round that wrote anything, or one
/// still running, is a 409.
pub fn regenerate(
    call: &Call<Actor, ConversationId, ()>,
) -> Result<Accepted<wire::ConversationPayload>, Refusal> {
    let id = &call.params;
    call.state
        .with_cards(|caps, cards| secretary_routes::regenerate(caps, cards, id, &call.guard))
        .inspect(|_| call.state.heard(id))
        .map(Accepted)
}

/// `POST …/edit`: replace the last round's question, setting aside all after it.
pub fn edit(
    call: &Call<Actor, ConversationId, Said>,
) -> Result<Accepted<wire::ConversationPayload>, Refusal> {
    let Said {
        say,
        references,
        attachments,
        ..
    } = &call.body;
    let id = &call.params;
    let by = stamped(&call.state, &call.guard, references);
    call.state
        .with_cards(|caps, cards| {
            secretary_routes::edit(
                caps,
                cards,
                id,
                say,
                references,
                attachments,
                &call.guard,
                by,
            )
        })
        .inspect(|_| call.state.heard(id))
        .map(Accepted)
}

/// `POST …/stop`: stop the answer being worked on and answer once the stop is written, so the
/// payload reads the act and nothing queued; with no round running it is a 409, never a halt left
/// for the next question.
pub fn stop(
    call: &Call<Actor, ConversationId, ()>,
) -> Result<Accepted<wire::ConversationPayload>, Refusal> {
    let id = &call.params;
    call.state
        .workers
        .stop(call.state.first.caps, id, call.guard.clone())?;
    call.state
        .with_cards(|caps, cards| secretary_routes::newest(caps, cards, id))
        .map(Accepted)
}

/// `POST /api/secretary/attachments`: keep one photo or PDF, answer what it is.
pub fn upload(call: &Call<Writer, (), Upload>) -> Result<Json<wire::Uploaded>, Refusal> {
    let uploads = uploads(&call.state)?;
    let Upload { bytes, name, keep } = &call.body;
    let kept = match keep {
        Keep::Private => uploads
            .store
            .keep_private(bytes, name, uploads.vault.as_ref()),
        Keep::OnTheBox => uploads.store.put(bytes),
    };
    match kept {
        Ok(file) => Ok(Json(wire::Uploaded::Kept {
            file: wire::AttachmentRow::from(&file),
        })),
        // Held back for the owner, who may keep it private in his Drive or not send it.
        Err(UploadError::Withheld(why @ Withheld::HeldNumber)) => {
            Ok(Json(wire::Uploaded::Sensitive {
                why: why.to_string(),
                folder: suzatary_core::vault::FOLDER.to_owned(),
            }))
        }
        Err(
            failed @ (UploadError::Refused(_)
            | UploadError::Withheld(_)
            | UploadError::Unread(_)
            | UploadError::Drive(_)),
        ) => Err(Refusal::bad_request(failed.to_string())),
        Err(failed @ UploadError::Store(_)) => Err(Refusal::internal(failed.to_string())),
    }
}

/// `GET /api/secretary/attachments/<id>`: one upload.
pub fn photo(call: &Call<Reader, AttachmentId, ()>) -> Result<Served, Refusal> {
    match uploads(&call.state)?
        .store
        .attachment(&call.params)?
        .ok_or_else(|| Refusal::not_found(format!("no upload {} in the store", call.params)))?
    {
        Stored::Here { attachment, bytes } => Ok(Served(attachment, bytes)),
        Stored::Private { copy, .. } => Err(Refusal::not_found(format!(
            "a private file is kept in your Drive at {}, not here",
            copy.path
        ))),
    }
}
