//! A topic's name: typed by the owner, or guessed once by the stateless `title` job.
//!
//! The job holds no verb and no session, and an answer that is not a [`Title`] is
//! refused, never repaired past the first line.

use crate::secretary::conversations;
use prodrome_core::event::Actor;

use crate::secretary::model::{Conversation, Model, Turn};
use crate::secretary::store::{Entry, Kind, Namer, Stored, Title};
use crate::secretary::transcript;
use suzatary_base::capability::Clock;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::refusal::Refusal;

/// The opening words of the job's one question, which a scripted fixture keys on.
pub const ASK: &str = "Title this conversation";

/// The job's rules.
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
    Some(Conversation {
        system: SYSTEM.to_owned(),
        turns: vec![Turn::Person {
            text: format!(
                "{ASK}.\n\nThe question:\n{}\n\nThe answer:\n{}",
                clip(asked),
                clip(answered)
            ),
            references: Vec::new(),
            attachments: Vec::new(),
        }],
    })
}

/// Read a title off the job's answer: its first line, unquoted, cut before `JOB_MAX`.
///
/// # Errors
/// [`crate::secretary::store::NotATitle`] when nothing is left.
pub(crate) fn from_answer(say: &str) -> Result<Title, crate::secretary::store::NotATitle> {
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
    conversations: crate::secretary::conversations::Conversations<'_>,
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
///
/// # Errors
/// [`Refusal::bad_request`], [`Refusal::not_found`], or the store's own.
pub fn rename(
    conversations: crate::secretary::conversations::Conversations<'_>,
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
    conversations: crate::secretary::conversations::Conversations<'_>,
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
            title: crate::secretary::words::Words::kept(title),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretary::store::Said;

    fn stored(kind: Kind, title: &str, turns: Vec<Turn>) -> Stored {
        let mut entries = vec![Entry::Opened {
            at: suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0)
                .expect("an instant"),
            title: crate::secretary::words::Words::kept(title.to_owned()),
            model: "m".to_owned(),
            kind,
        }];
        entries.extend(turns.into_iter().map(|turn| {
            Entry::Said {
                at: suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 1, 0, 0)
                    .expect("an instant"),
                turn,
                by: None,
            }
        }));
        Stored::of(
            &ConversationId::new("2026-09-25-1").expect("a slug"),
            &entries,
        )
        .expect("a conversation")
    }

    fn person(text: &str) -> Turn {
        Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        }
    }

    fn answer(text: &str) -> Turn {
        Turn::Model {
            text: text.to_owned(),
            calls: Vec::new(),
        }
    }

    /// The question carries the cue, the first question and the first answer.
    #[test]
    fn the_question_is_the_first_exchange() {
        let asked = question(&stored(
            Kind::Topic,
            "",
            vec![
                person("When is the permit due?"),
                Turn::Tool {
                    verb: "search".into(),
                    answer: "secret mail".to_owned(),
                    outcome: crate::secretary::outcome::Outcome::Answered,
                    files: Vec::new(),
                },
                answer("On the 3rd."),
                person("And the fee?"),
            ],
        ))
        .expect("a question");
        let [Turn::Person { text, .. }] = asked.turns.as_slice() else {
            panic!("one person turn");
        };
        assert!(text.starts_with(ASK));
        assert!(text.contains("When is the permit due?") && text.contains("On the 3rd."));
        assert!(!text.contains("secret mail") && !text.contains("fee"));
    }

    /// The one conversation the job runs over, and the clock it stamps with.
    struct Lines(crate::secretary::conversations::InMemory);

    impl Clock for Lines {
        fn now(&self) -> suzatary_base::capability::Datetime {
            suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 5, 0, 0).expect("an instant")
        }
    }

    struct Says(&'static str);

    impl Model for Says {
        fn name(&self) -> String {
            "titler".to_owned()
        }
        fn turn(
            &self,
            _: &Conversation,
            _: &serde_json::Value,
        ) -> Result<crate::secretary::model::Reply, crate::secretary::model::ModelError> {
            Ok(crate::secretary::model::Reply::parse(self.0))
        }
    }

    fn lines_of(stored: &Stored) -> Lines {
        let mut entries = vec![Entry::Opened {
            at: stored.opened,
            title: stored.title.clone(),
            model: stored.model.clone(),
            kind: stored.kind,
        }];
        entries.extend(
            stored
                .turns
                .iter()
                .map(|Said { at, turn, by }| Entry::Said {
                    at: *at,
                    turn: turn.clone(),
                    by: by.clone(),
                }),
        );
        let lines = crate::secretary::conversations::InMemory::default();
        let id = ConversationId::new("2026-09-25-1").expect("a slug");
        transcript::start(lines.conversations(), &id, &entries[0]).expect("opens");
        for entry in &entries[1..] {
            transcript::push(lines.conversations(), &id, entry).expect("pushes");
        }
        Lines(lines)
    }

    /// The job names an owed topic once, and a rename wins after it.
    #[test]
    fn the_job_names_once_and_a_rename_is_an_append() {
        let id = ConversationId::new("2026-09-25-1").expect("a slug");
        let caps = lines_of(&stored(
            Kind::Topic,
            "",
            vec![person("When is the permit due?"), answer("On the 3rd.")],
        ));
        let named = run(caps.0.conversations(), &caps, &Says("Permit deadline"), &id)
            .expect("the job ran")
            .expect("a title landed");
        assert_eq!(
            named.title.get().map(String::as_str),
            Some("Permit deadline")
        );
        assert_eq!(
            run(caps.0.conversations(), &caps, &Says("Other"), &id).expect("ran"),
            None
        );
        let ada = Actor::new("ada").expect("an actor");
        let renamed =
            rename(caps.0.conversations(), &caps, &ada, &id, "  The permit  ").expect("renamed");
        assert_eq!(renamed.title.get().map(String::as_str), Some("The permit"));
        assert!(matches!(
            transcript::lines(caps.0.conversations(), &id).expect("reads").last().cloned(),
            Some(Entry::Titled { by: Namer::Person { actor }, .. }) if actor == "ada"
        ));
        assert_eq!(
            rename(caps.0.conversations(), &caps, &ada, &id, "two\nlines")
                .expect_err("refused")
                .status(),
            suzatary_base::refusal::Status::BadRequest
        );
    }

    /// An answer that is no title is a refusal and writes nothing.
    #[test]
    fn a_non_title_writes_nothing() {
        let id = ConversationId::new("2026-09-25-1").expect("a slug");
        let caps = lines_of(&stored(
            Kind::Topic,
            "",
            vec![person("When?"), answer("Soon.")],
        ));
        let before = transcript::lines(caps.0.conversations(), &id)
            .expect("reads")
            .len();
        assert!(run(caps.0.conversations(), &caps, &Says("#panel[]"), &id).is_err());
        assert_eq!(
            transcript::lines(caps.0.conversations(), &id)
                .expect("reads")
                .len(),
            before
        );
    }
}
