//! A topic's name: typed by the owner, or guessed once by the stateless `title` job.
//!
//! The job holds no verb and no session, and an answer that is not a [`Title`] is
//! refused, never repaired past the first line.

use prodrome_data::event::Actor;
use suzatary_lines::secretary::conversations;
use suzatary_lines::secretary::conversations::Conversations;

use suzatary_base::capability::Clock;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::refusal::Refusal;
use suzatary_conversations::secretary::model::{Conversation, Model, Turn};
use suzatary_conversations::secretary::store::{Entry, Kind, Namer, Stored, Title};
use suzatary_lines::secretary::transcript;

/// The opening words of the job's one question, which a scripted fixture keys on.
pub const ASK: &str = "Title this conversation";

pub(crate) const SYSTEM: &str = "You name conversations for a list. Answer with the title alone: \
                          at most six words, no quotes, no punctuation at the end, no markup.";

/// How much of the question and of the answer the job reads.
const EXCERPT: usize = 2_000;

/// How long a title the job may give, in characters; shorter than a person's
/// [`Title::MAX`] because it is a guess.
const JOB_MAX: usize = 60;

/// Whether the job owes this conversation a name: an unnamed topic with a
/// finished answer.
#[must_use]
pub fn owed(stored: &Stored) -> bool {
    stored.kind == Kind::Topic && stored.untitled() && question(stored).is_some()
}

/// The job's one question from the first person turn and first finished answer.
#[must_use]
pub fn question(stored: &Stored) -> Option<Conversation> {
    let live = stored.live();
    let asked = live.iter().find_map(|(_, said)| match &said.turn {
        Turn::Person { text, .. } if !text.trim().is_empty() => Some(text.as_str()),
        _ => None,
    })?;
    let answered = live.iter().find_map(|(_, said)| match &said.turn {
        Turn::Model { text, calls } if calls.is_empty() && !text.trim().is_empty() => {
            Some(text.as_str())
        }
        _ => None,
    })?;
    let clip = |text: &str| text.chars().take(EXCERPT).collect::<String>();
    Some(Conversation::bare(
        SYSTEM,
        vec![Turn::Person {
            text: format!(
                "{ASK}.\n\nThe question:\n{}\n\nThe answer:\n{}",
                clip(asked),
                clip(answered)
            ),
            references: Vec::new(),
            attachments: Vec::new(),
        }],
    ))
}

/// Read a title off the job's answer: its first line, unquoted, cut before `JOB_MAX`.
pub(crate) fn from_answer(
    say: &str,
) -> Result<Title, suzatary_conversations::secretary::store::NotATitle> {
    let line = typst_prose_line(say).unwrap_or("");
    let bare = line
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '“' | '”' | '‘' | '’' | '`'))
        .trim_end_matches(['.', ':'])
        .trim();
    let cut = if bare.chars().count() <= JOB_MAX {
        bare.to_owned()
    } else {
        let head: String = bare.chars().take(JOB_MAX).collect();
        head.rsplit_once(' ')
            .map_or(head.as_str(), |(words, _)| words)
            .to_owned()
    };
    Title::new(cut)
}

/// Run the job on one conversation; a name the person set meanwhile wins.
///
/// # Errors
/// The model's failure or a non-title answer, as [`Refusal::unprocessable`], or the store's.
pub fn run(
    conversations: Conversations<'_>,
    clock: &dyn Clock,
    model: &dyn Model,
    id: &ConversationId,
) -> Result<Option<Stored>, Refusal> {
    let stored = conversations::read(conversations, id)?;
    let Some(asked) = question(&stored).filter(|_| owed(&stored)) else {
        return Ok(None);
    };
    let reply = model
        .turn(&asked, &serde_json::json!([]))
        .map_err(|failed| Refusal::unprocessable(failed.to_string()))?;
    let title = from_answer(&reply.say).map_err(|refused| {
        Refusal::unprocessable(format!("the title job answered no title: {refused}"))
    })?;
    if !owed(&conversations::read(conversations, id)?) {
        return Ok(None);
    }
    titled(
        conversations,
        clock,
        id,
        title,
        Namer::Job {
            model: model.name(),
        },
    )
    .map(Some)
}

/// Rename one conversation, as the actor the gate resolved.
pub fn rename(
    conversations: Conversations<'_>,
    clock: &dyn Clock,
    by: &Actor,
    id: &ConversationId,
    text: &str,
) -> Result<Stored, Refusal> {
    let title = Title::new(text).map_err(|refused| Refusal::bad_request(refused.to_string()))?;
    conversations::read(conversations, id)?;
    titled(
        conversations,
        clock,
        id,
        title,
        Namer::Person {
            actor: by.as_str().to_owned(),
        },
    )
}

fn titled(
    conversations: Conversations<'_>,
    clock: &dyn Clock,
    id: &ConversationId,
    title: Title,
    by: Namer,
) -> Result<Stored, Refusal> {
    transcript::push(
        conversations,
        id,
        &Entry::Titled {
            at: clock.now(),
            title: suzatary_memory::secretary::words::Words::kept(title),
            by,
        },
    )?;
    conversations::read(conversations, id)
}

/// The first line of typst `text` that reads as prose.
pub(crate) fn typst_prose_line(text: &str) -> Option<&str> {
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let leading = line.chars().next();
        let mut candidate = line
            .trim_start_matches(['=', '-', '+', '*', '_', '/', ' '])
            .trim();
        if let Some(mark @ ('*' | '_')) = leading
            && let Some(rest) = candidate.strip_suffix(mark)
        {
            candidate = rest.trim();
        }
        if !candidate.is_empty() {
            return Some(candidate);
        }
    }
    None
}
