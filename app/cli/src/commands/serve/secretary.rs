//! The secretary's routes.

use axum::Json;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use prodrome_core::event::Actor;
use prodrome_core::event::Hash;
use suzatary_api::wire::BlockRender;
use suzatary_api::wire::Erasing;
use suzatary_api::wire::RecordPayload;
use suzatary_api::wire::Saying;
use suzatary_api::wire::Uploaded;
use suzatary_api::wire::Uploading;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::Reference;
use suzatary_base::refusal::{Refusal, shown};
use suzatary_held::attachment::{Attachment, AttachmentId};
use suzatary_held::file_read::Withheld;
use suzatary_inbox::secretary::stamp;
use suzatary_inbox::secretary::stamp::{Speaker, Stamp};
use suzatary_io::attachments::{Stored, UploadError};
use suzatary_server::secretary::routes as secretary_routes;
use suzatary_signatures::signatures::Seat;
use suzatary_signatures::signatures::Tap;
use suzatary_topics::secretary::readings;
use suzatary_topics::wire::topics as wire;

use super::body::{Body, MAX_ACT_BYTES};
use super::call::Call;
use super::guard::{Reader, Writer};
use super::params::{Asked, Params};
use super::reply::Accepted;
use super::{Chained, State};
use suzatary_memory::secretary::memory::FactId;

pub struct Block {
    id: ConversationId,
    sha: String,
}

pub struct Said {
    say: String,
    references: Vec<Reference>,
    attachments: Vec<Attachment>,
}

/// A photo or PDF, decoded.
pub struct Upload {
    bytes: Vec<u8>,
    name: String,
    keep: Keep,
}

enum Keep {
    /// Held back if it names an identity number.
    OnTheBox,
    /// In the owner's Drive.
    Private,
}

/// Upload bytes, served so they never read as a page.
pub struct Served(Attachment, Vec<u8>);

/// The largest PDF as base64, plus JSON room.
const MAX_UPLOAD_BYTES: usize = suzatary_held::file_read::MAX_PDF / 3 * 4 + 65_536;

/// The `conversation` segment.
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

impl Params for Hash {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        let node = asked.segment("node")?;
        Hash::new(node).map_err(|_| {
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
            |failed: suzatary_held::attachment::AttachmentError| {
                Refusal::bad_request(failed.to_string())
            },
        )
    }
}

impl Body for Erasing {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = Self;

    fn parse(erasing: Self, _: &State) -> Result<Self, Refusal> {
        Ok(erasing)
    }
}

impl Body for Said {
    const CAP: usize = suzatary_lines::secretary::lines_rule::MAX_SAID_BYTES;

    type Wire = Saying;

    /// Each photo's type is reread from the store.
    fn parse(saying: Saying, state: &State) -> Result<Self, Refusal> {
        let Saying {
            say,
            references,
            attachments,
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
        })
    }
}

impl Body for Upload {
    const CAP: usize = MAX_UPLOAD_BYTES;

    type Wire = Uploading;

    fn parse(uploading: Uploading, _: &State) -> Result<Self, Refusal> {
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

/// `GET …/memory`.
pub fn memory(call: &Call<Reader, (), ()>) -> Result<Json<wire::MemoryPayload>, Refusal> {
    secretary_routes::memory(call.chain.caps.secretary.memories).map(Json)
}

/// `POST …/memory/<id>/forget`.
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

/// `GET …/record`.
pub fn record(call: &Call<Reader, (), ()>) -> Result<Json<RecordPayload>, Refusal> {
    secretary_routes::record(&call.chain.caps.secretary.memories.digests).map(Json)
}

/// `POST …/record/<node>/retract`, by tap.
pub fn retract(call: &Call<Tap, Hash, ()>) -> Result<Json<RecordPayload>, Refusal> {
    let caps = call.chain.caps;
    secretary_routes::retract(
        caps.secretary.memories,
        caps.secretary.clock,
        &call.guard,
        &call.params,
    )
    .map(Json)
}

/// A stored secret's handle.
pub struct Handle(String);

impl Params for Handle {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        Ok(Self(asked.segment("handle")?.to_owned()))
    }
}

/// `POST …/replace/<handle>`: the value comes only in the card's tap.
pub fn replace(call: &Call<Writer, Handle, ()>) -> Result<Json<serde_json::Value>, Refusal> {
    call.state
        .with_cards(|caps, cards| secretary_routes::replace(caps, cards, &call.params.0))?;
    call.state.heard(&suzatary_base::conversation_id::SETUP);
    Ok(Json(serde_json::json!({})))
}

/// `POST …/<id>/erase`, by tap, after any running round stops.
pub fn erase(
    call: &Call<Tap, ConversationId, Erasing>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    call.state
        .workers
        .halt_round(&call.params, call.guard.who().clone());
    call.state
        .with_cards(|caps, cards| {
            secretary_routes::erase(caps, cards, &call.guard, &call.params, &call.body)
        })
        .inspect(|_| {
            let caps = call.chain.caps;
            if let Err(failed) = suzatary_secretary::secretary::file_search::reindex(
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

/// `GET …/blocks/<sha>`: the box renders only blocks a model wrote.
pub fn block(call: &Call<Reader, Block, ()>) -> Result<Json<BlockRender>, Refusal> {
    let Block { id, sha } = &call.params;
    let source = match call.state.workers.closed(id, sha) {
        Some(source) => Some(source),
        None => readings::block_named(
            &suzatary_lines::secretary::conversations::read(
                call.chain.caps.secretary.conversations,
                id,
            )?,
            sha,
        ),
    };
    let Some(source) = source else {
        return Err(Refusal::not_found(format!(
            "no answer in {id} holds a block {}",
            shown(sha)
        )));
    };
    let block = suzatary_base::answer::Renderable::of(&source);
    let key = suzatary_todo::rendering::ANSWER
        .key(&block)
        .map_err(|e| Refusal::internal(e.to_string()))?;
    call.chain
        .caps
        .answers
        .block(&block)
        .map(|html| {
            Json(BlockRender {
                html: html.as_str().to_owned(),
                key,
            })
        })
        .map_err(|refused| Refusal::unprocessable(refused.to_string()))
}

fn stamped<P, B>(call: &Call<Actor, P, B>, references: &[Reference]) -> Stamp {
    let speaker = Speaker::at(call.guard.as_str(), Seat::of(&call.admitted));
    if references
        .iter()
        .any(|reference| matches!(reference, Reference::Todo(_)))
    {
        let read = call.state.chains_read();
        stamp::pointing(
            speaker,
            references,
            &read.set(call.state.first.caps.secretary.clock.now()),
        )
    } else {
        Stamp::Actor(speaker)
    }
}

/// `POST …/regenerate`: 409 if the round wrote or runs.
pub fn regenerate(
    call: &Call<Actor, ConversationId, ()>,
) -> Result<Accepted<wire::ConversationPayload>, Refusal> {
    let id = &call.params;
    call.state
        .with_cards(|caps, cards| secretary_routes::regenerate(caps, cards, id, &call.guard))
        .inspect(|_| call.state.heard(id))
        .map(Accepted)
}

/// `POST …/edit`.
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
    let by = stamped(call, references);
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

/// `POST …/attachments`.
pub fn upload(call: &Call<Writer, (), Upload>) -> Result<Json<Uploaded>, Refusal> {
    let uploads = uploads(&call.state)?;
    let Upload { bytes, name, keep } = &call.body;
    let kept = match keep {
        Keep::Private => uploads
            .store
            .keep_private(bytes, name, uploads.vault.as_ref()),
        Keep::OnTheBox => uploads.store.put(bytes),
    };
    match kept {
        Ok(file) => Ok(Json(Uploaded::Kept {
            file: wire::AttachmentRow::from(&file),
            attachment: file,
        })),
        // Held back; the owner may keep it in his Drive.
        Err(UploadError::Withheld(why @ (Withheld::HeldNumber | Withheld::Form))) => {
            Ok(Json(Uploaded::Sensitive {
                why: why.to_string(),
                folder: suzatary_held::vault::FOLDER.to_owned(),
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

/// `GET …/attachments/<id>`.
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
