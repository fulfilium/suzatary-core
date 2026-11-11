//! A mail query as the owner's files are matched by it: the plain words, `word*` prefixes and
//! quoted phrases notmuch reads, case-folded and every one required, as notmuch requires them.
//!
//! A file has no sender, date or folder, so a query that names a notmuch field, or joins terms
//! with an operator, is not one a file can answer, and the search says so rather than match a
//! different question. Notmuch also stems English words (`permits` finds `permit`); a file's
//! words are matched whole, which a `word*` prefix makes up for.

use crate::mail::mailbox::Query;
use crate::secretary::search::fold;

/// What a file must hold to match: every phrase, in any order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileQuery(Vec<Phrase>);

/// Words that must stand next to each other, in order; one word is a phrase of one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Phrase(Vec<Word>);

/// One word of a phrase, case-folded.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Word {
    /// Matches this word and no other.
    Whole(String),
    /// Matches every word that starts with this, as `lease*` does.
    Starting(String),
}

/// A query no file can answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unanswerable {
    /// It names a field only mail has, such as `from:` or `date:`.
    #[error("`{0}:` is a field of mail, which a file does not have")]
    Field(String),
    /// It joins its terms with a notmuch operator.
    #[error("`{0}` is a notmuch operator; a file is matched when it holds every word")]
    Operator(String),
    /// It holds no word, only punctuation.
    #[error("the query holds no word a file could match")]
    NoWords,
}

impl FileQuery {
    /// Read `query` as notmuch reads its free text.
    ///
    /// # Errors
    /// [`Unanswerable`], naming the first term no file can match.
    pub fn of(query: &Query) -> Result<Self, Unanswerable> {
        let mut phrases = Vec::new();
        for term in terms(query.as_str()) {
            let phrase = match term {
                Term::Quoted(text) => Phrase(words(&text).map(Word::Whole).collect()),
                Term::Bare(text) => bare(&text)?,
            };
            if !phrase.0.is_empty() {
                phrases.push(phrase);
            }
        }
        if phrases.is_empty() {
            Err(Unanswerable::NoWords)
        } else {
            Ok(Self(phrases))
        }
    }

    /// Where the first phrase first matches in `text`, as a character offset and width, when
    /// `text` holds every phrase; `None` when it misses one.
    #[must_use]
    pub fn find(&self, text: &str) -> Option<(usize, usize)> {
        let spans = spans(text);
        let mut first = None;
        for phrase in &self.0 {
            let at = (0..spans.len()).find(|&start| phrase.matches(&spans[start..]))?;
            let last = &spans[at + phrase.0.len() - 1];
            first.get_or_insert((spans[at].start, last.end - spans[at].start));
        }
        first
    }
}

impl Phrase {
    /// Whether these words open with this phrase.
    fn matches(&self, words: &[Span]) -> bool {
        words.len() >= self.0.len()
            && self.0.iter().zip(words).all(|(word, span)| match word {
                Word::Whole(whole) => &span.folded == whole,
                Word::Starting(prefix) => span.folded.starts_with(prefix.as_str()),
            })
    }
}

/// One term of a query as typed.
enum Term {
    /// Inside double quotes: a phrase.
    Quoted(String),
    /// Between spaces.
    Bare(String),
}

/// The query's terms: runs between whitespace, and quoted runs, a missing closing quote ending
/// at the end as notmuch reads it.
fn terms(query: &str) -> Vec<Term> {
    let mut terms = Vec::new();
    let mut chars = query.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            terms.push(Term::Quoted(
                chars.by_ref().take_while(|&c| c != '"').collect(),
            ));
        } else {
            let mut bare = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '"' {
                    break;
                }
                bare.push(c);
                chars.next();
            }
            terms.push(Term::Bare(bare));
        }
    }
    terms
}

/// One bare term as a phrase: `AND` and a leading `+` say nothing more than the default; an
/// operator, a negation, a group or a field is refused; a trailing `*` makes the last word a
/// prefix; `lease-agreement` is the phrase of its two words, as notmuch splits it.
fn bare(term: &str) -> Result<Phrase, Unanswerable> {
    let term = term.strip_prefix('+').unwrap_or(term);
    match term {
        "AND" => return Ok(Phrase(Vec::new())),
        "OR" | "NOT" | "XOR" | "NEAR" | "ADJ" => {
            return Err(Unanswerable::Operator(term.to_owned()));
        }
        _ if term.starts_with('-') || term.contains(['(', ')']) => {
            return Err(Unanswerable::Operator(term.to_owned()));
        }
        _ => {}
    }
    if let Some((field, _)) = term.split_once(':')
        && !field.is_empty()
        && field.chars().all(|c| c.is_ascii_alphabetic())
    {
        return Err(Unanswerable::Field(field.to_owned()));
    }
    let mut words: Vec<Word> = words(term).map(Word::Whole).collect();
    if term.ends_with('*')
        && let Some(Word::Whole(last)) = words.pop()
    {
        words.push(Word::Starting(last));
    }
    Ok(Phrase(words))
}

/// A word of a text: where it stands, in characters, and its case-folded spelling.
struct Span {
    start: usize,
    end: usize,
    folded: String,
}

/// Every word of `text`, a word being a run of letters and digits.
fn spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut open: Option<Span> = None;
    for (at, c) in text.chars().enumerate() {
        match (c.is_alphanumeric(), open.as_mut()) {
            (true, Some(span)) => {
                span.folded.push(fold(c));
                span.end = at + 1;
            }
            (true, None) => {
                open = Some(Span {
                    start: at,
                    end: at + 1,
                    folded: fold(c).to_string(),
                });
            }
            (false, Some(_)) => spans.extend(open.take()),
            (false, None) => {}
        }
    }
    spans.extend(open);
    spans
}

/// The case-folded words of `text`.
fn words(text: &str) -> impl Iterator<Item = String> {
    spans(text).into_iter().map(|span| span.folded)
}
