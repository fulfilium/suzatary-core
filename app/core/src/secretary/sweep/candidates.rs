//! The open list, swept: for each open todo, the later mail that plausibly bears on it, with why
//! it was found typed.
//!
//! Searches are built from what mail carries (the source and body referents), never the todo's id,
//! and age never gates; this module reads no clock and writes nothing.

use std::sync::LazyLock;

use prodrome_core::event::TodoId;
use regex::Regex;

use crate::capability::{Corpus, CorpusError, Datetime, Query};
use crate::record::{MarkupSource, Source};
use crate::reference::MailHash;

/// A message's canonical hash, the same type a `mail:<hash>` reference validates.
pub type MessageHash = MailHash;

/// One open todo, as [`candidates`] needs it to search; the caller looks up its content, so this
/// module never reads the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTodo {
    /// The todo's id.
    pub id: TodoId,
    /// The floor every candidate's message must be strictly after: the start of the day it was
    /// created, or its latest trusted reopen when that is later, so a reopen stands until mail
    /// newer than it arrives.
    pub since: Datetime,
    /// The message it came from; `None` for a todo typed by hand, which then searches only its
    /// referents.
    pub source: Option<Source>,
    /// The todo's body, in typst markup, where `referents_in` looks.
    pub body: MarkupSource,
}

/// A thing a todo's body names that other mail would also name: a closed, conservative sum.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Referent {
    /// A confirmation, order, booking or record-locator code.
    Code(String),
    /// A person's name, read off a "Dear …"/"Passenger: …" label.
    Person(String),
    /// A flight number, read off a "Flight …" label.
    Flight(String),
}

impl Referent {
    /// The text [`queries_for`] hands the corpus for this referent.
    fn query_term(&self) -> String {
        match self {
            Referent::Code(text) | Referent::Flight(text) => text.clone(),
            // Quoted, so notmuch reads a multi-word name as one phrase.
            Referent::Person(text) => format!("\"{text}\""),
        }
    }
}

/// Why a message was found.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Match {
    /// Found by `thread:<id>`, the conversation the todo came from.
    SameThread,
    /// Found by `from:<address>`, the same sender.
    SameSender,
    /// Found by a referent the todo's body names.
    NamesReferent(Referent),
}

/// A later message that plausibly bears on one open todo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The todo it might bear on.
    pub todo: TodoId,
    /// The message it rests on.
    pub message: MessageHash,
    /// When that message arrived, always strictly after the todo's `since`.
    pub at: Datetime,
    /// Why it was found.
    pub matched: Match,
}

/// [`candidates`] refused.
#[derive(Debug, thiserror::Error)]
pub enum SweepError {
    /// The corpus itself refused a search this function built.
    #[error("searching for todo:{todo}: {source}")]
    Corpus {
        /// Which todo the search was for.
        todo: String,
        /// The corpus's own refusal.
        #[source]
        source: CorpusError,
    },
}

/// A sender header's search term: the address inside `<…>` when there is one, else the header
/// trimmed.
fn sender_query_term(sender: &str) -> Option<String> {
    let trimmed = sender.trim();
    if trimmed.is_empty() {
        return None;
    }
    let inner = match (trimmed.find('<'), trimmed.find('>')) {
        (Some(open), Some(close)) if open < close => &trimmed[open + 1..close],
        _ => trimmed,
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return None;
    }
    Some(if inner.contains(char::is_whitespace) {
        format!("\"{inner}\"")
    } else {
        inner.to_owned()
    })
}

/// Whether a field can be spliced into a notmuch query bare without changing its meaning.
fn query_safe(text: &str) -> bool {
    !text.is_empty() && !text.contains(char::is_whitespace) && !text.contains('"')
}

/// Every query [`candidates`] will run for one open todo, each paired with the [`Match`] a hit
/// carries.
fn queries_for(todo: &OpenTodo) -> Vec<(Match, Query)> {
    let mut out = Vec::new();
    if let Some(source) = &todo.source {
        let thread = source.thread_id.as_str();
        if query_safe(thread)
            && let Ok(query) = Query::new(format!("thread:{thread}"))
        {
            out.push((Match::SameThread, query));
        }
        if let Some(term) = sender_query_term(source.sender.as_str())
            && let Ok(query) = Query::new(format!("from:{term}"))
        {
            out.push((Match::SameSender, query));
        }
    }
    for referent in referents_in(todo.body.as_str()) {
        if let Ok(query) = Query::new(referent.query_term()) {
            out.push((Match::NamesReferent(referent), query));
        }
    }
    out
}

// The referent extractor: five labelled, conservative patterns, never a general NLP pass.

static CONFIRMATION_CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)confirmation\s*(?:code|number|no\.?|#)?\s*[:#]?\s*([A-Za-z0-9]{4,10})")
        .expect("a fixed pattern")
});

static BOOKING_CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:booking|reservation)\s*(?:code|number|no\.?|#|reference)?\s*[:#]?\s*([A-Za-z0-9]{4,10})",
    )
    .expect("a fixed pattern")
});

static ORDER_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)order\s*(?:number|no\.?|#)?\s*[:#]?\s*([A-Za-z0-9]{4,10})")
        .expect("a fixed pattern")
});

static RECORD_LOCATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:record locator|PNR)\s*[:#]?\s*([A-Za-z0-9]{4,10})")
        .expect("a fixed pattern")
});

static FLIGHT_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)flight\s*(?:no\.?|number|#|:)?\s*([A-Za-z]{2,3})\s?(\d{1,4})\b")
        .expect("a fixed pattern")
});

static PERSON_AFTER_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:dear|attn|attention|passenger|guest|traveler|traveller)\s*[:,]?\s*([A-Z][a-z]+(?:\s[A-Z][a-z]+){0,2})")
        .expect("a fixed pattern")
});

/// A code a labelled pattern captured, kept only when it carries a digit.
fn code_from(regex: &Regex, text: &str) -> Option<String> {
    let captured = regex.captures(text)?.get(1)?.as_str();
    captured
        .chars()
        .any(|c| c.is_ascii_digit())
        .then(|| captured.to_uppercase())
}

/// The referents [`queries_for`] searches on, deduped in a stable order.
fn referents_in(text: &str) -> Vec<Referent> {
    let mut found = std::collections::BTreeSet::new();
    for regex in [
        &*CONFIRMATION_CODE,
        &*BOOKING_CODE,
        &*ORDER_NUMBER,
        &*RECORD_LOCATOR,
    ] {
        if let Some(code) = code_from(regex, text) {
            found.insert(Referent::Code(code));
        }
    }
    if let Some(captures) = FLIGHT_NUMBER.captures(text) {
        let airline = captures.get(1).map_or("", |m| m.as_str());
        let number = captures.get(2).map_or("", |m| m.as_str());
        found.insert(Referent::Flight(format!(
            "{}{number}",
            airline.to_uppercase()
        )));
    }
    if let Some(captures) = PERSON_AFTER_LABEL.captures(text)
        && let Some(name) = captures.get(1)
    {
        found.insert(Referent::Person(name.as_str().to_owned()));
    }
    found.into_iter().collect()
}

/// `verbs::shown_timestamp`'s spelling read back as the instant it names: an offset is taken
/// off, so the mail's moment is UTC as a todo's `created` is; one with no offset is UTC already.
fn parse_shown_timestamp(text: &str) -> Option<Datetime> {
    let naive =
        chrono::NaiveDateTime::parse_from_str(text.get(0..19)?, "%Y-%m-%d %H:%M:%S").ok()?;
    let utc = match text.get(19..)? {
        "" => naive,
        offset => chrono::DateTime::parse_from_str(
            &format!("{} {offset}", &text[..19]),
            "%Y-%m-%d %H:%M:%S %:z",
        )
        .ok()?
        .naive_utc(),
    };
    prodrome_core::fpl::datetime_of(utc).ok()
}

/// For every open todo, the later mail that plausibly bears on it; proposes and writes nothing.
///
/// # Errors
/// The corpus refused a search.
pub fn candidates(
    open: &[OpenTodo],
    corpus: &dyn Corpus,
    now: Datetime,
) -> Result<Vec<Candidate>, SweepError> {
    let mut found = Vec::new();
    for todo in open {
        for (matched, query) in queries_for(todo) {
            let rows = corpus
                .search(&query, None)
                .map_err(|source| SweepError::Corpus {
                    todo: todo.id.as_str().to_owned(),
                    source,
                })?;
            for row in rows {
                let Some(at) = row.timestamp.as_deref().and_then(parse_shown_timestamp) else {
                    continue;
                };
                // Never after `now` either: a message a skewed clock dates in the future is not
                // evidence yet.
                if at <= todo.since || at > now {
                    continue;
                }
                let Ok(message) = MessageHash::new(row.hash) else {
                    continue;
                };
                found.push(Candidate {
                    todo: todo.id.clone(),
                    message,
                    at,
                    matched: matched.clone(),
                });
            }
        }
    }
    // A total order, so the result is deterministic whatever order the corpus answered in.
    found.sort_by(|a, b| {
        (a.todo.as_str(), a.at, a.message.as_str(), &a.matched).cmp(&(
            b.todo.as_str(),
            b.at,
            b.message.as_str(),
            &b.matched,
        ))
    });
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::state::Decision;
    use crate::mail::verbs::{Marked, NoiseAdded, Show, Summary, SyncReport};

    fn dt(y: i32, m: u32, d: u32) -> Datetime {
        Datetime::new(y, m, d, 0, 0, 0, 0).expect("a real date")
    }

    fn source(sender: &str, subject: &str, thread_id: &str) -> Source {
        crate::record::mk_source(sender, subject, dt(2026, 9, 1), "", thread_id, "")
    }

    fn open(id: &str, created: Datetime, src: Option<Source>, body: &str) -> OpenTodo {
        OpenTodo {
            id: TodoId::new(id).expect("a valid id"),
            since: created,
            source: src,
            body: MarkupSource::from_chain(body),
        }
    }

    fn row(hash: &str, timestamp: &str) -> Summary {
        Summary {
            hash: hash.to_owned(),
            id: format!("id-{hash}"),
            sender: "noreply@airline.example".to_owned(),
            title: "Re: your trip".to_owned(),
            timestamp: Some(timestamp.to_owned()),
            container: "inbox".to_owned(),
            sources: 1,
            held: crate::held_value::Kinds::empty(),
        }
    }

    /// A corpus over fixed answers, matching `thread:`/`from:` exactly and a bare term by
    /// substring.
    struct FakeCorpus {
        messages: Vec<(String, String, String, String, String)>, // hash, thread, sender, text, timestamp
    }

    impl Corpus for FakeCorpus {
        fn sync(&self) -> Result<SyncReport, CorpusError> {
            Err(CorpusError::Refused("the sweep never syncs".to_owned()))
        }
        fn queue(&self, _limit: Option<usize>) -> Result<Vec<Summary>, CorpusError> {
            Ok(Vec::new())
        }
        fn search(
            &self,
            query: &Query,
            _limit: Option<usize>,
        ) -> Result<Vec<Summary>, CorpusError> {
            let text = query.as_str();
            let hits: Vec<Summary> = if let Some(thread) = text.strip_prefix("thread:") {
                self.messages
                    .iter()
                    .filter(|(_, t, ..)| t == thread)
                    .map(|(hash, _, _, _, at)| row(hash, at))
                    .collect()
            } else if let Some(sender) = text.strip_prefix("from:") {
                self.messages
                    .iter()
                    .filter(|(_, _, s, ..)| s == sender)
                    .map(|(hash, _, _, _, at)| row(hash, at))
                    .collect()
            } else {
                let term = text.trim_matches('"').to_lowercase();
                self.messages
                    .iter()
                    .filter(|(_, _, _, body, _)| body.to_lowercase().contains(&term))
                    .map(|(hash, _, _, _, at)| row(hash, at))
                    .collect()
            };
            Ok(hits)
        }
        fn show(&self, id: &str) -> Result<Show, CorpusError> {
            Ok(Show::NotFound { id: id.to_owned() })
        }
        fn mark(
            &self,
            _hash: &str,
            _decision: Decision,
            _note: Option<&str>,
            _native: Option<&str>,
        ) -> Result<Marked, CorpusError> {
            Err(CorpusError::Refused("the sweep never marks".to_owned()))
        }
        fn noise_add(&self, _sender: &str) -> Result<NoiseAdded, CorpusError> {
            Err(CorpusError::Refused(
                "the sweep never adds noise".to_owned(),
            ))
        }
        fn last_sync(&self) -> Option<SyncReport> {
            None
        }
        fn last_triage_pass(&self) -> Option<String> {
            None
        }
    }

    #[test]
    fn law1_a_todo_with_no_later_mail_yields_no_candidate() {
        let corpus = FakeCorpus {
            messages: vec![(
                "aaaaaaaa".into(),
                "th1".into(),
                "noreply@airline.example".into(),
                "nothing relevant here".into(),
                "2026-09-01 10:00:00".into(),
            )],
        };
        let todo = open(
            "flightcheckin",
            dt(2026, 9, 5),
            Some(source("noreply@airline.example", "Your trip", "th1")),
            "check in for the flight",
        );
        let found = candidates(&[todo], &corpus, dt(2026, 9, 10)).expect("candidates");
        assert!(
            found.is_empty(),
            "the only message in the thread predates `created`, so absence of \
             later evidence must not manufacture a candidate"
        );
    }

    #[test]
    fn law2_a_candidates_message_is_named_and_dated_strictly_after_since() {
        let corpus = FakeCorpus {
            messages: vec![(
                "bbbbbbbb".into(),
                "th1".into(),
                "noreply@airline.example".into(),
                "checked in".into(),
                "2026-09-06 08:00:00".into(),
            )],
        };
        let created = dt(2026, 9, 5);
        // No sender named, so only the thread query runs.
        let todo = open(
            "flightcheckin",
            created,
            Some(source("", "Your trip", "th1")),
            "check in for the flight",
        );
        let found = candidates(&[todo], &corpus, dt(2026, 9, 10)).expect("candidates");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].message.as_str(), "bbbbbbbb");
        assert_eq!(found[0].matched, Match::SameThread);
        assert!(
            found[0].at > created,
            "a candidate's message must be dated strictly after the todo's `since`"
        );
    }

    #[test]
    fn law4_a_handauthored_todo_finds_candidates_only_through_its_referents() {
        let corpus = FakeCorpus {
            messages: vec![(
                "eeeeeeee".into(),
                String::new(),
                String::new(),
                "your confirmation code: AB12CD".into(),
                "2026-09-06 08:00:00".into(),
            )],
        };
        let created = dt(2026, 9, 5);

        let names_nothing = open("handwritten", created, None, "call the plumber sometime");
        let empty = candidates(&[names_nothing], &corpus, dt(2026, 9, 10)).expect("candidates");
        assert!(
            empty.is_empty(),
            "a source-less todo whose body names no referent yields nothing"
        );

        let names_a_code = open(
            "handwritten2",
            created,
            None,
            "waiting on confirmation code AB12CD",
        );
        let found = candidates(&[names_a_code], &corpus, dt(2026, 9, 10)).expect("candidates");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].message.as_str(), "eeeeeeee");
        assert_eq!(
            found[0].matched,
            Match::NamesReferent(Referent::Code("AB12CD".to_owned()))
        );
    }
}
