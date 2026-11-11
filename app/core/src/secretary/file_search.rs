//! Finding the files the owner sent in a chat: each live turn's uploads, matched by their name
//! and the words the upload index holds of them, newest first like mail.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::attachment::AttachmentId;
use crate::conversation_id::ConversationId;
use crate::secretary::file_query::FileQuery;
use crate::secretary::model::Turn;
use crate::secretary::search::around;
use crate::secretary::store::Stored;
use crate::secretary::upload_standing::UploadStanding;

/// One file that matched, on the turn it came in on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileHit {
    /// Its id in the upload store, which `upload_read` opens and `propose_email` attaches.
    pub file: AttachmentId,
    /// The name it was sent under; `None` for a photo, which has none.
    pub name: Option<String>,
    /// `application/pdf` or an image type.
    pub media: String,
    /// Where it is kept in the owner's Drive, for a file he kept private.
    pub private: Option<String>,
    /// The conversation it was sent in.
    pub conversation: ConversationId,
    /// That conversation's heading.
    pub title: String,
    /// The index of the turn that carried it, which a conversation opens at.
    pub turn: usize,
    /// When it was sent.
    pub at: String,
    /// What the owner said on that turn, cut to `SAID_CHARS`.
    pub said: String,
    /// The match in its name or text, with `FILE_CONTEXT` characters either side.
    pub excerpt: String,
}

/// How much of a file's words a hit keeps either side of the match, in characters.
pub(crate) const FILE_CONTEXT: usize = 120;

/// How much of the owner's words on the turn a hit keeps, in characters.
pub(crate) const SAID_CHARS: usize = 280;

/// The most hits one search answers, so the answer stays well within what a line stores.
pub(crate) const MAX_FILE_HITS: usize = 25;

/// Every upload a live turn names: the uploads the index holds an entry for.
#[must_use]
pub fn named(conversations: &[Stored]) -> BTreeSet<AttachmentId> {
    sent(conversations)
        .map(|(_, _, attachment)| attachment.id.clone())
        .collect()
}

/// Bring `index` to the uploads the stored conversations name, answering how many it holds words
/// of: what the box runs at startup, which indexes uploads stored before the index existed, and
/// after an erasure, so an erased upload's words leave the index at once.
///
/// # Errors
/// [`crate::refusal::Refusal::internal`] when the conversations or the index cannot be read.
pub fn reindex(
    conversations: crate::secretary::conversations::Conversations<'_>,
    index: &dyn crate::upload_index::UploadIndex,
) -> Result<usize, crate::refusal::Refusal> {
    let standings = index
        .refresh(&named(&crate::secretary::conversations::list(
            conversations,
        )?))
        .map_err(|failed| crate::refusal::Refusal::internal(failed.to_string()))?;
    Ok(standings
        .values()
        .filter(|standing| match standing {
            UploadStanding::Indexed(_) => true,
            UploadStanding::Held | UploadStanding::Erased => false,
        })
        .count())
}

/// Every upload on a live person turn whose name and indexed words hold `query`, newest first,
/// at most `limit` (itself at most `MAX_FILE_HITS`); an upload held back or erased is never
/// found, whatever it says.
#[must_use]
pub fn find(
    conversations: &[Stored],
    standings: &BTreeMap<AttachmentId, UploadStanding>,
    query: &FileQuery,
    limit: usize,
) -> Vec<FileHit> {
    let mut hits: Vec<FileHit> = sent(conversations)
        .filter_map(|(stored, index, attachment)| {
            let text = match standings.get(&attachment.id) {
                Some(UploadStanding::Indexed(text)) => text,
                Some(UploadStanding::Held | UploadStanding::Erased) | None => return None,
            };
            // The name leads, so a match in it is shown before one in the text.
            let words = format!("{}\n{text}", attachment.name.as_deref().unwrap_or_default());
            let (at, width) = query.find(&words)?;
            let chars: Vec<char> = words.chars().collect();
            let said = &stored.turns[index];
            Some(FileHit {
                file: attachment.id.clone(),
                name: attachment.name.clone(),
                media: attachment.media_type.as_str().to_owned(),
                private: attachment
                    .private
                    .as_ref()
                    .map(|copy| copy.path.to_string()),
                conversation: stored.id.clone(),
                title: stored.heading().into_owned(),
                turn: index,
                at: crate::secretary::stamp::stamp(said.at),
                said: match &said.turn {
                    Turn::Person { text, .. } => cut(text),
                    Turn::Model { .. }
                    | Turn::Tool { .. }
                    | Turn::Act { .. }
                    | Turn::Erased { .. } => String::new(),
                },
                excerpt: around(&chars, at, width.min(FILE_CONTEXT), FILE_CONTEXT),
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then_with(|| a.conversation.cmp(&b.conversation))
            .then_with(|| b.turn.cmp(&a.turn))
            .then_with(|| a.file.cmp(&b.file))
    });
    hits.truncate(limit.min(MAX_FILE_HITS));
    hits
}

/// Each upload on a live person turn, with its conversation and the turn's index. A set-aside
/// turn was never delivered, and an erased turn keeps only its uploads' ids, so neither counts.
fn sent(
    conversations: &[Stored],
) -> impl Iterator<Item = (&Stored, usize, &crate::attachment::Attachment)> {
    conversations.iter().flat_map(|stored| {
        stored
            .turns
            .iter()
            .enumerate()
            .filter(|(index, _)| stored.is_live(*index))
            .flat_map(move |(index, said)| {
                let attachments: &[crate::attachment::Attachment] = match &said.turn {
                    Turn::Person { attachments, .. } => attachments,
                    Turn::Model { .. }
                    | Turn::Tool { .. }
                    | Turn::Act { .. }
                    | Turn::Erased { .. } => &[],
                };
                attachments
                    .iter()
                    .map(move |attachment| (stored, index, attachment))
            })
    })
}

/// The first [`SAID_CHARS`] characters of the owner's words, on one line.
fn cut(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    around(&chars, 0, 0, SAID_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachment::Attachment;
    use crate::mail::mailbox::Query;
    use crate::secretary::store::Entry;

    const PDF: &[u8] = b"%PDF-1.7\n%%EOF\n";

    fn pdf(seed: &str, name: &str) -> Attachment {
        Attachment::accept(&[PDF, seed.as_bytes()].concat())
            .expect("a pdf")
            .named(name)
            .expect("a name")
    }

    fn conversation(slug: &str, turns: &[(&str, Turn)]) -> Stored {
        let mut entries = vec![Entry::Opened {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
            title: crate::secretary::words::Words::kept(format!("About {slug}")),
            model: "m".to_owned(),
            kind: crate::secretary::store::Kind::Topic,
        }];
        entries.extend(turns.iter().map(|(at, turn)| Entry::Said {
            at: crate::secretary::store::instant(at),
            turn: turn.clone(),
            by: None,
        }));
        Stored::of(&ConversationId::new(slug).expect("a slug"), &entries).expect("a conversation")
    }

    fn sending(text: &str, attachments: Vec<Attachment>) -> Turn {
        Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments,
        }
    }

    fn query(text: &str) -> FileQuery {
        FileQuery::of(&Query::new(text).expect("a query")).expect("answerable")
    }

    /// A file is found by its text and by its name, with the conversation and turn it came in on;
    /// a held or erased file is not, and neither is one the index has no standing for.
    #[test]
    fn a_file_is_found_with_its_conversation_and_turn_and_only_when_indexed() {
        let lease = pdf("a", "lease.pdf");
        let w2 = pdf("b", "w2.pdf");
        let gone = pdf("c", "deed.pdf");
        let all = [conversation(
            "2026-09-25-1",
            &[
                (
                    "2026-09-25T09:01:00",
                    sending("here is the lease", vec![lease.clone()]),
                ),
                (
                    "2026-09-25T09:02:00",
                    sending("and my tax form", vec![w2.clone(), gone.clone()]),
                ),
            ],
        )];
        let standings = BTreeMap::from([
            (
                lease.id.clone(),
                UploadStanding::Indexed("Residential lease for unit 4".to_owned()),
            ),
            (w2.id.clone(), UploadStanding::Held),
            (gone.id.clone(), UploadStanding::Erased),
        ]);
        let hits = find(&all, &standings, &query("unit 4"), 10);
        assert_eq!(hits.len(), 1, "{hits:?}");
        let hit = &hits[0];
        assert_eq!(hit.file, lease.id);
        assert_eq!(hit.conversation.as_str(), "2026-09-25-1");
        assert_eq!(hit.turn, 0);
        assert_eq!(hit.said, "here is the lease");
        assert!(hit.excerpt.contains("unit 4"), "{}", hit.excerpt);
        assert_eq!(find(&all, &standings, &query("lease.pdf"), 10).len(), 1);
        for hidden in ["w2", "deed"] {
            assert!(
                find(&all, &standings, &query(hidden), 10).is_empty(),
                "{hidden} is held or erased"
            );
        }
        assert!(
            find(&all, &BTreeMap::new(), &query("lease"), 10).is_empty(),
            "nothing the index does not hold is found"
        );
        assert_eq!(
            named(&all),
            BTreeSet::from([lease.id.clone(), w2.id.clone(), gone.id.clone()])
        );
    }

    /// An erased turn keeps its upload's id but not its words, so the index keeps nothing of it.
    #[test]
    fn an_erased_turn_names_no_upload() {
        let lease = pdf("a", "lease.pdf");
        let all = [conversation(
            "2026-09-25-1",
            &[(
                "2026-09-25T09:01:00",
                Turn::Erased {
                    shape: crate::secretary::turn_shape::TurnShape::Person {
                        references: Vec::new(),
                        attachments: vec![crate::secretary::turn_shape::AttachmentShape {
                            id: lease.id.clone(),
                            media_type: lease.media_type,
                        }],
                    },
                    words: Vec::new(),
                },
            )],
        )];
        assert!(named(&all).is_empty());
    }
}
