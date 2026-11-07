//! Searching mail older than the routine sync's window, on the mail server itself.
//!
//! - The query is the tool's own small language, parsed once into an [`OlderQuery`] of
//!   [`Term`]s, and each server kind gets it in its own syntax ([`imap_criteria`],
//!   [`gmail_q`]); no text the model wrote reaches a server except as a [`Term`] inside quotes,
//!   and a [`Term`] holds no quote, backslash or control character.
//! - The [`Range`] ends where the routine INBOX pull begins, on the same internal date, so a
//!   message an older search pulls is one the routine sync never fetches.
//! - [`plan`] takes at most [`MAX_MATCHES`] of what the box does not already hold, newest
//!   first, and counts the rest; [`pull`] runs that plan over an IMAP [`Session`].

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde::Serialize;

use super::imap::{ImapError, Maildir, Session, SyncState, imap_date, maildir_name, reconcile};
use super::verbs::Summary;

/// The most messages one search pulls per account; the rest are counted, not fetched.
pub const MAX_MATCHES: usize = 25;

/// The largest message pulled; a larger one is counted as too large and not kept.
pub const MAX_MESSAGE_BYTES: usize = 5 * 1024 * 1024;

/// The longest [`Term`], in characters.
pub(crate) const MAX_TERM_CHARS: usize = 100;

/// The most [`Term`]s one query holds.
pub(crate) const MAX_TERMS: usize = 12;

/// The server folder an older search reads on an IMAP account. Sent is not searched: the
/// routine sync already keeps the whole of it on the box.
pub const IMAP_FOLDER: &str = crate::mail::imap::INBOX;

/// The maildir leaf, under an account's folder, that an older search writes into.
#[must_use]
pub fn leaf(folder: &str) -> String {
    format!("older-{}", super::account::folder_slug(folder))
}

/// Why a text is not a query.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    /// No from, to, subject or body word: a search for everything is not a search.
    #[error("the query names nothing to look for: give from:, to:, subject: or a word of the body")]
    Empty,
    /// A `name:` that is not one of the fields.
    #[error(
        "{0:?} is not a field; the fields are from:, to:, subject: and before:, and a word or \
         \"a quoted phrase\" alone is searched for in the body (quote one that holds a colon)"
    )]
    UnknownField(String),
    /// A field with nothing after it.
    #[error("{0}: needs a value right after the colon")]
    Missing(&'static str),
    /// A quote opened and never closed, or one inside a word.
    #[error("a quote must open and close a whole phrase: {0:?}")]
    Quote(String),
    /// A term that is not one.
    #[error(transparent)]
    Term(#[from] TermError),
    /// More terms than one search takes.
    #[error("a query holds at most {MAX_TERMS} words or phrases, got {0}")]
    TooMany(usize),
    /// `before:` that is not a date.
    #[error("before: takes a date as YYYY-MM-DD, got {0:?}")]
    Date(String),
    /// `before:` twice.
    #[error("before: may be given once")]
    TwoBefores,
}

/// Why a text is not a [`Term`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TermError {
    /// Longer than `MAX_TERM_CHARS`.
    #[error("a word or phrase is at most {MAX_TERM_CHARS} characters, got {0}")]
    TooLong(usize),
    /// A quote, a backslash or a control character, none of which a server reads as text.
    #[error("a word or phrase holds no quote, backslash or control character: {0:?}")]
    Forbidden(String),
}

/// A word or phrase to look for: non-empty, at most `MAX_TERM_CHARS` characters, and free of
/// `"`, `\` and control characters, so quoting it for any server keeps it one string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Term(String);

impl Term {
    /// A term, or why `text` is not one; `None` for text that is only whitespace.
    ///
    /// # Errors
    /// [`TermError`].
    pub fn new(text: &str) -> Result<Option<Self>, TermError> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let chars = text.chars().count();
        if chars > MAX_TERM_CHARS {
            return Err(TermError::TooLong(chars));
        }
        if text
            .chars()
            .any(|c| c == '"' || c == '\\' || c.is_control())
        {
            return Err(TermError::Forbidden(text.to_owned()));
        }
        Ok(Some(Self(text.to_owned())))
    }

    /// The term as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What to look for in older mail: every term must match (a conjunction), and `before` narrows
/// the date range from above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OlderQuery {
    from: Vec<Term>,
    to: Vec<Term>,
    subject: Vec<Term>,
    body: Vec<Term>,
    before: Option<NaiveDate>,
}

/// One field of the query language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    From,
    To,
    Subject,
    Before,
}

impl Field {
    fn named(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "from" => Some(Self::From),
            "to" => Some(Self::To),
            "subject" => Some(Self::Subject),
            "before" => Some(Self::Before),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::From => "from",
            Self::To => "to",
            Self::Subject => "subject",
            Self::Before => "before",
        }
    }
}

impl std::str::FromStr for OlderQuery {
    type Err = QueryError;

    /// `from:ada@example.test subject:"team invite" budget before:2025-01-01`: `from:`, `to:`
    /// and `subject:` take a word or a quoted phrase, a bare word or phrase is looked for in
    /// the body, and `before:` takes a date.
    fn from_str(text: &str) -> Result<Self, QueryError> {
        let mut query = Self {
            from: Vec::new(),
            to: Vec::new(),
            subject: Vec::new(),
            body: Vec::new(),
            before: None,
        };
        let mut rest = text.trim_start();
        while !rest.is_empty() {
            let (field, after) = field_of(rest)?;
            let (value, after) = value_of(after, field)?;
            rest = after.trim_start();
            match field {
                Some(Field::Before) => {
                    let date = NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                        .map_err(|_| QueryError::Date(value.clone()))?;
                    if query.before.replace(date).is_some() {
                        return Err(QueryError::TwoBefores);
                    }
                }
                Some(Field::From) => query.from.extend(Term::new(&value)?),
                Some(Field::To) => query.to.extend(Term::new(&value)?),
                Some(Field::Subject) => query.subject.extend(Term::new(&value)?),
                None => query.body.extend(Term::new(&value)?),
            }
        }
        let terms = query.terms().count();
        if terms == 0 {
            return Err(QueryError::Empty);
        }
        if terms > MAX_TERMS {
            return Err(QueryError::TooMany(terms));
        }
        Ok(query)
    }
}

/// The field `text` opens with, if it opens with `letters:`, and what follows the colon.
fn field_of(text: &str) -> Result<(Option<Field>, &str), QueryError> {
    let letters = text.len()
        - text
            .trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .len();
    match text[letters..].strip_prefix(':') {
        Some(after) if letters > 0 => match Field::named(&text[..letters]) {
            Some(field) => Ok((Some(field), after)),
            None => Err(QueryError::UnknownField(format!("{}:", &text[..letters]))),
        },
        _ => Ok((None, text)),
    }
}

/// One value: a quoted phrase, or a word up to the next whitespace.
fn value_of(text: &str, field: Option<Field>) -> Result<(String, &str), QueryError> {
    let missing = || match field {
        Some(field) => QueryError::Missing(field.name()),
        None => QueryError::Quote(text.to_owned()),
    };
    if let Some(open) = text.strip_prefix('"') {
        let close = open
            .find('"')
            .ok_or_else(|| QueryError::Quote(text.to_owned()))?;
        let after = &open[close + 1..];
        if after.chars().next().is_some_and(|c| !c.is_whitespace()) {
            return Err(QueryError::Quote(text.to_owned()));
        }
        return Ok((open[..close].to_owned(), after));
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    let word = &text[..end];
    if word.is_empty() {
        return Err(missing());
    }
    if word.contains('"') {
        return Err(QueryError::Quote(word.to_owned()));
    }
    Ok((word.to_owned(), &text[end..]))
}

impl OlderQuery {
    /// Every term, whichever field holds it.
    fn terms(&self) -> impl Iterator<Item = &Term> {
        self.from
            .iter()
            .chain(&self.to)
            .chain(&self.subject)
            .chain(&self.body)
    }

    /// The upper date bound the query itself asked for, if any.
    #[must_use]
    pub fn before(&self) -> Option<NaiveDate> {
        self.before
    }
}

/// Why a date range is no range.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RangeError {
    /// `since` is on or after where the routine sync begins.
    #[error(
        "since {since} is not older than the routine sync's window, which keeps mail from \
         {window} on; `search` reads that"
    )]
    InsideWindow {
        /// What was asked.
        since: NaiveDate,
        /// Where the routine sync begins.
        window: NaiveDate,
    },
    /// `before` is on or before `since`.
    #[error("before {before} is not after since {since}")]
    Backwards {
        /// What was asked.
        since: NaiveDate,
        /// The end asked for.
        before: NaiveDate,
    },
}

/// The dates an older search covers: from `since`, up to but not including `before`, which is
/// never later than the day the routine sync's window begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Range {
    since: NaiveDate,
    before: NaiveDate,
}

impl Range {
    /// From `since` up to the earlier of `asked` and `window`, where the routine sync's window
    /// begins.
    ///
    /// # Errors
    /// [`RangeError`] when that leaves no day.
    pub fn older(
        since: NaiveDate,
        asked: Option<NaiveDate>,
        window: NaiveDate,
    ) -> Result<Self, RangeError> {
        if since >= window {
            return Err(RangeError::InsideWindow { since, window });
        }
        let before = asked.map_or(window, |asked| asked.min(window));
        if before <= since {
            return Err(RangeError::Backwards { since, before });
        }
        Ok(Self { since, before })
    }

    /// The first day searched.
    #[must_use]
    pub const fn since(self) -> NaiveDate {
        self.since
    }

    /// The day after the last day searched.
    #[must_use]
    pub const fn before(self) -> NaiveDate {
        self.before
    }
}

/// A query this server kind cannot be asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Untranslatable {
    /// IMAP's quoted strings carry ASCII only, and this box sends no literals.
    #[error(
        "this IMAP server is asked in ASCII only, and {0:?} is not; search it with ASCII words"
    )]
    NotAscii(String),
}

/// The query and range as IMAP `UID SEARCH` criteria: every key joined by AND, every term a quoted
/// string, dates on the internal date as the routine pull's `SINCE` is.
///
/// # Errors
/// [`Untranslatable::NotAscii`] for a term outside ASCII.
pub fn imap_criteria(query: &OlderQuery, range: Range) -> Result<String, Untranslatable> {
    let mut keys = vec![
        format!("SINCE {}", imap_date(range.since)),
        format!("BEFORE {}", imap_date(range.before)),
    ];
    for (key, terms) in [
        ("FROM", &query.from),
        ("TO", &query.to),
        ("SUBJECT", &query.subject),
        ("BODY", &query.body),
    ] {
        for term in terms {
            if !term.as_str().is_ascii() {
                return Err(Untranslatable::NotAscii(term.as_str().to_owned()));
            }
            keys.push(format!("{key} \"{}\"", term.as_str()));
        }
    }
    Ok(keys.join(" "))
}

/// The query and range as the Gmail API's `q`: each term a quoted phrase under its operator, a
/// body term a bare phrase, and the dates as `after:` and `before:`.
#[must_use]
pub fn gmail_q(query: &OlderQuery, range: Range) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (operator, terms) in [
        ("from:", &query.from),
        ("to:", &query.to),
        ("subject:", &query.subject),
        ("", &query.body),
    ] {
        for term in terms {
            parts.push(format!("{operator}\"{}\"", term.as_str()));
        }
    }
    parts.push(format!("after:{}", range.since.format("%Y/%m/%d")));
    parts.push(format!("before:{}", range.before.format("%Y/%m/%d")));
    parts.join(" ")
}

/// Which of a server's matches to fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan<Id> {
    /// At most `cap` of the matches the box does not hold, newest first.
    pub wanted: Vec<Id>,
    /// Matches the box already holds, fetched by no one.
    pub already: usize,
    /// Matches the box does not hold that the cap left out.
    pub beyond: usize,
}

/// The newest `cap` of `newest_first` that `held` does not name, and the counts of the rest.
#[must_use]
pub fn plan<Id: Ord + Clone>(newest_first: &[Id], held: &BTreeSet<Id>, cap: usize) -> Plan<Id> {
    let missing: Vec<&Id> = newest_first
        .iter()
        .filter(|id| !held.contains(id))
        .collect();
    let wanted: Vec<Id> = missing.iter().take(cap).map(|id| (*id).clone()).collect();
    Plan {
        already: newest_first.len() - missing.len(),
        beyond: missing.len() - wanted.len(),
        wanted,
    }
}

/// How many messages a server matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matched {
    /// This many, counted.
    Exactly(usize),
    /// At least this many: the server's listing was cut at a page.
    AtLeast(usize),
}

impl Serialize for Matched {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Exactly(n) => serializer.serialize_u64(*n as u64),
            Self::AtLeast(n) => serializer.serialize_str(&format!("{n} or more")),
        }
    }
}

impl std::fmt::Display for Matched {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exactly(n) => write!(f, "{n}"),
            Self::AtLeast(n) => write!(f, "{n} or more"),
        }
    }
}

/// What one pull over an IMAP folder did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pulled {
    /// How many the server matched.
    pub matched: usize,
    /// How many of those the box already held.
    pub already: usize,
    /// How many were written into the leaf.
    pub landed: usize,
    /// How many were fetched and were larger than [`MAX_MESSAGE_BYTES`].
    pub too_large: usize,
    /// How many the cap left on the server.
    pub beyond: usize,
    /// The older leaf's state to persist.
    pub state: SyncState,
}

/// Search `folder` read-only with `criteria`, and write the newest [`MAX_MATCHES`] matches
/// that neither `routine` (the routine pull's state for that folder) nor `state` (this leaf's
/// own) holds into `maildir`, already read.
///
/// # Errors
/// [`ImapError`] from the session or the maildir.
pub fn pull<S: Session + ?Sized, M: Maildir + ?Sized>(
    session: &mut S,
    maildir: &M,
    routine: &SyncState,
    state: SyncState,
    folder: &str,
    criteria: &str,
) -> Result<Pulled, ImapError> {
    let selected = session.select(folder, true)?;
    let state = reconcile(state, &selected.uidvalidity);
    let mut held: BTreeSet<u32> = state.synced.clone();
    if routine.uidvalidity.as_deref() == Some(selected.uidvalidity.as_str()) {
        held.extend(&routine.synced);
    }
    // The server answers in ascending UID order, oldest first.
    let mut found = session.uid_search(criteria)?;
    found.reverse();
    let Plan {
        wanted,
        already,
        beyond,
    } = plan(&found, &held, MAX_MATCHES);
    let mut fetched: BTreeSet<u32> = BTreeSet::new();
    let mut too_large = 0;
    for uid in wanted {
        let Some(raw) = session.uid_fetch(uid)? else {
            continue;
        };
        if raw.len() > MAX_MESSAGE_BYTES {
            too_large += 1;
            continue;
        }
        maildir.write(&maildir_name(&raw, uid, false), false, &raw)?;
        fetched.insert(uid);
    }
    Ok(Pulled {
        matched: found.len(),
        already,
        landed: fetched.len(),
        too_large,
        beyond,
        state: SyncState {
            synced: state.synced.union(&fetched).copied().collect(),
            ..state
        },
    })
}

/// What one account's server search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Found {
    /// The server folders searched.
    pub folders: Vec<String>,
    /// How many messages matched there.
    pub matched: Matched,
    /// How many of those the box already held, which `search` finds.
    pub already_here: usize,
    /// The messages pulled now, as the index reads them; `show` takes each `id`.
    pub pulled: Vec<Summary>,
    /// Messages pulled that the index names by no `id` this answer could ask for.
    pub unnamed: usize,
    /// Matches larger than [`MAX_MESSAGE_BYTES`], not kept.
    pub too_large: usize,
    /// Matches the cap of [`MAX_MATCHES`] left on the server; a narrower query reaches them.
    pub beyond_cap: usize,
    /// The `search` query that reads everything older searches have pulled for this account.
    pub local: String,
}

/// Why an account was not searched on its server.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unsearched {
    /// The account's sync kind offers this box no server search.
    #[error(
        "{0} is pulled by mbsync through the Proton Bridge, which offers this box no server \
         search; the folders mbsync syncs are on the box, and `search` reads them"
    )]
    NoServerSearch(String),
    /// The query cannot be put to this server.
    #[error(transparent)]
    Untranslatable(#[from] Untranslatable),
    /// The account's sign-in lapsed; only a person gives one.
    #[error("{0}")]
    NeedsLogin(String),
    /// The server or the box failed, in its own words.
    #[error("{0}")]
    Failed(String),
}

/// One account's search: what was found, or why it was not searched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Searched on the server.
    Searched(Found),
    /// Not searched.
    NotSearched(Unsearched),
}

/// One account and its [`Outcome`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountOlder {
    /// The account's address.
    pub account: String,
    /// What its search came to.
    pub outcome: Outcome,
}

impl Serialize for AccountOlder {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Searched<'a> {
            account: &'a str,
            #[serde(flatten)]
            found: &'a Found,
        }
        #[derive(Serialize)]
        struct NotSearched<'a> {
            account: &'a str,
            not_searched: String,
        }
        match &self.outcome {
            Outcome::Searched(found) => Searched {
                account: &self.account,
                found,
            }
            .serialize(serializer),
            Outcome::NotSearched(why) => NotSearched {
                account: &self.account,
                not_searched: why.to_string(),
            }
            .serialize(serializer),
        }
    }
}

/// The answer to an older search: the range, each account, and what was covered in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Older {
    /// Which accounts, folders and dates were searched, and what was not, in words.
    pub searched: String,
    /// The dates searched.
    pub range: Range,
    /// Each account asked.
    pub accounts: Vec<AccountOlder>,
}

impl Older {
    /// The answer for `accounts` over `range`.
    #[must_use]
    pub fn of(range: Range, accounts: Vec<AccountOlder>) -> Self {
        Self {
            searched: coverage(range, &accounts),
            range,
            accounts,
        }
    }
}

/// The accounts, folders and dates searched, and what was not, as sentences.
#[must_use]
pub fn coverage(range: Range, accounts: &[AccountOlder]) -> String {
    let mut said = vec![format!(
        "Searched on the mail server: mail dated from {} up to, not including, {}.",
        range.since, range.before
    )];
    for one in accounts {
        said.push(match &one.outcome {
            Outcome::Searched(found) => format!(
                "{}: {} searched; {} matched, {} already on the box, {} pulled now{}{}.",
                one.account,
                found.folders.join(", "),
                found.matched,
                found.already_here,
                found.pulled.len() + found.unnamed,
                match found.too_large {
                    0 => String::new(),
                    n => format!(", {n} too large to keep"),
                },
                match found.beyond_cap {
                    0 => String::new(),
                    n => format!(", {n} more left on the server past the cap of {MAX_MATCHES}"),
                },
            ),
            Outcome::NotSearched(why) => format!("{}: not searched, because {why}.", one.account),
        });
    }
    if accounts.is_empty() {
        said.push("No account was searched.".to_owned());
    }
    said.push(format!(
        "Not searched here: mail before {} (not asked for), and mail from {} on (the routine \
         sync's, which `search` reads).",
        range.since, range.before
    ));
    said.join(" ")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use super::*;
    use crate::mail::imap::Selected;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a date")
    }

    fn range() -> Range {
        Range::older(day("2023-01-01"), None, day("2026-10-10")).expect("a range")
    }

    fn query(text: &str) -> OlderQuery {
        text.parse().expect("a query")
    }

    // --- the translations, as properties -----------------------------------------

    /// One IMAP search key and its argument, as a server's parser reads them.
    #[derive(Debug, PartialEq, Eq)]
    enum ImapToken {
        Atom(String),
        Quoted(String),
    }

    /// RFC 3501's atoms and quoted strings, the only two shapes the criteria use; `None` for
    /// anything a server would read otherwise (an escape, a literal, a parenthesis, a line end).
    fn imap_tokens(text: &str) -> Option<Vec<ImapToken>> {
        let mut tokens = Vec::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                ' ' => {}
                '"' => {
                    let mut quoted = String::new();
                    loop {
                        match chars.next()? {
                            '"' => break,
                            '\\' | '\r' | '\n' => return None,
                            other => quoted.push(other),
                        }
                    }
                    tokens.push(ImapToken::Quoted(quoted));
                }
                c if c.is_ascii_graphic() && !"(){%*\\]".contains(c) => {
                    let mut atom = c.to_string();
                    while let Some(&next) = chars.peek() {
                        if next == ' ' || next == '"' {
                            break;
                        }
                        if !next.is_ascii_graphic() || "(){%*\\]".contains(next) {
                            return None;
                        }
                        atom.push(next);
                        chars.next();
                    }
                    tokens.push(ImapToken::Atom(atom));
                }
                _ => return None,
            }
        }
        Some(tokens)
    }

    /// One piece of a Gmail `q`: an operator's quoted phrase, a bare phrase, or a date.
    #[derive(Debug, PartialEq, Eq)]
    enum GmailPiece {
        Phrase(String, String),
        Date(String, String),
    }

    fn gmail_pieces(q: &str) -> Option<Vec<GmailPiece>> {
        let mut pieces = Vec::new();
        let mut rest = q;
        while !rest.is_empty() {
            rest = rest.strip_prefix(' ').unwrap_or(rest);
            let operator_end = rest.find(['"', ' ']).unwrap_or(rest.len());
            let operator = &rest[..operator_end];
            if let Some(open) = rest[operator_end..].strip_prefix('"') {
                let close = open.find('"')?;
                if !matches!(operator, "" | "from:" | "to:" | "subject:") {
                    return None;
                }
                pieces.push(GmailPiece::Phrase(
                    operator.to_owned(),
                    open[..close].to_owned(),
                ));
                rest = &open[close + 1..];
                if !(rest.is_empty() || rest.starts_with(' ')) {
                    return None;
                }
            } else {
                let (name, value) = operator.split_once(':')?;
                if !matches!(name, "after" | "before") {
                    return None;
                }
                pieces.push(GmailPiece::Date(name.to_owned(), value.to_owned()));
                rest = &rest[operator_end..];
            }
        }
        Some(pieces)
    }

    /// Any text the term grammar admits, the hostile kind included: operators, parentheses,
    /// IMAP keywords and Gmail's own syntax.
    fn hostile() -> impl Strategy<Value = String> {
        prop_oneof![
            "[ -~]{1,40}",
            "\\PC{1,30}",
            Just("OR FROM x".to_owned()),
            Just(") UID 1:* (".to_owned()),
            Just("{5}".to_owned()),
            Just("in:anywhere OR label:x".to_owned()),
            Just("-subject:(a)".to_owned()),
        ]
    }

    fn terms() -> impl Strategy<Value = Vec<Term>> {
        proptest::collection::vec(hostile(), 0..3).prop_map(|texts| {
            texts
                .iter()
                .filter_map(|text| Term::new(text).ok().flatten())
                .collect()
        })
    }

    fn queries() -> impl Strategy<Value = OlderQuery> {
        (terms(), terms(), terms(), terms())
            .prop_filter("a query names something", |(a, b, c, d)| {
                !(a.is_empty() && b.is_empty() && c.is_empty() && d.is_empty())
            })
            .prop_map(|(from, to, subject, body)| OlderQuery {
                from,
                to,
                subject,
                body,
                before: None,
            })
    }

    proptest! {
        /// Whatever the terms hold, the IMAP criteria read back as the two dates and then one
        /// key per term with that term, whole, as its quoted argument: nothing injected.
        #[test]
        fn imap_criteria_carry_each_term_as_one_quoted_string(query in queries()) {
            match imap_criteria(&query, range()) {
                Err(Untranslatable::NotAscii(term)) => prop_assert!(!term.is_ascii()),
                Ok(criteria) => {
                    let tokens = imap_tokens(&criteria).expect("criteria a server reads plainly");
                    let mut expected = vec![
                        ImapToken::Atom("SINCE".to_owned()),
                        ImapToken::Atom("01-Jan-2023".to_owned()),
                        ImapToken::Atom("BEFORE".to_owned()),
                        ImapToken::Atom("10-Oct-2026".to_owned()),
                    ];
                    for (key, list) in [
                        ("FROM", &query.from),
                        ("TO", &query.to),
                        ("SUBJECT", &query.subject),
                        ("BODY", &query.body),
                    ] {
                        for term in list {
                            expected.push(ImapToken::Atom(key.to_owned()));
                            expected.push(ImapToken::Quoted(term.as_str().to_owned()));
                        }
                    }
                    prop_assert_eq!(tokens, expected);
                }
            }
        }

        /// Whatever the terms hold, Gmail's `q` reads back as one quoted phrase per term under
        /// its own operator, then the two dates: nothing injected.
        #[test]
        fn gmail_q_carries_each_term_as_one_quoted_phrase(query in queries()) {
            let pieces = gmail_pieces(&gmail_q(&query, range())).expect("a q of phrases and dates");
            let mut expected = Vec::new();
            for (operator, list) in [
                ("from:", &query.from),
                ("to:", &query.to),
                ("subject:", &query.subject),
                ("", &query.body),
            ] {
                for term in list {
                    expected.push(GmailPiece::Phrase(operator.to_owned(), term.as_str().to_owned()));
                }
            }
            expected.push(GmailPiece::Date("after".to_owned(), "2023/01/01".to_owned()));
            expected.push(GmailPiece::Date("before".to_owned(), "2026/10/10".to_owned()));
            prop_assert_eq!(pieces, expected);
        }

        /// A parsed query's terms are the terms the text named, and no term holds what a server
        /// could read as syntax.
        #[test]
        fn a_parsed_term_never_holds_a_quote_or_a_line_end(text in "\\PC{0,60}") {
            if let Ok(query) = text.parse::<OlderQuery>() {
                for term in query.terms() {
                    prop_assert!(!term.as_str().contains(['"', '\\', '\r', '\n']));
                    prop_assert!(!term.as_str().is_empty());
                }
            }
        }
    }

    // --- the pull, against a fake server -----------------------------------------

    struct Server {
        uidvalidity: String,
        messages: BTreeMap<u32, Vec<u8>>,
        searched: Vec<String>,
        readonly: Option<bool>,
    }

    impl Session for Server {
        fn list(&mut self) -> Result<Vec<String>, ImapError> {
            Ok(vec![crate::mail::imap::INBOX.to_owned()])
        }
        fn select(&mut self, _folder: &str, readonly: bool) -> Result<Selected, ImapError> {
            self.readonly = Some(readonly);
            Ok(Selected {
                uidvalidity: self.uidvalidity.clone(),
            })
        }
        fn uid_search(&mut self, criteria: &str) -> Result<Vec<u32>, ImapError> {
            self.searched.push(criteria.to_owned());
            Ok(self.messages.keys().copied().collect())
        }
        fn uid_fetch(&mut self, uid: u32) -> Result<Option<Vec<u8>>, ImapError> {
            Ok(self.messages.get(&uid).cloned())
        }
        fn uid_mark_seen(&mut self, _uid: u32) -> Result<(), ImapError> {
            panic!("an older search never stores a flag");
        }
    }

    #[derive(Default)]
    struct Leaf(std::cell::RefCell<BTreeMap<String, bool>>);

    impl Maildir for Leaf {
        fn write(&self, name: &str, unread: bool, _raw: &[u8]) -> Result<(), ImapError> {
            self.0.borrow_mut().insert(name.to_owned(), unread);
            Ok(())
        }
        fn file_names(&self) -> Result<Vec<String>, ImapError> {
            Ok(self.0.borrow().keys().cloned().collect())
        }
    }

    fn server(count: u32) -> Server {
        Server {
            uidvalidity: "7".to_owned(),
            messages: (1..=count)
                .map(|uid| (uid, format!("Subject: {uid}\r\n\r\nbody").into_bytes()))
                .collect(),
            searched: Vec::new(),
            readonly: None,
        }
    }

    fn routine(validity: &str, synced: &[u32]) -> SyncState {
        SyncState {
            uidvalidity: Some(validity.to_owned()),
            synced: synced.iter().copied().collect(),
            seen_pushed: BTreeSet::new(),
        }
    }

    /// The cap: thirty matches, two already held by the routine pull, so the newest
    /// twenty-five of the rest land read and three are counted past the cap.
    #[test]
    fn a_pull_takes_the_newest_within_the_cap_and_counts_the_rest() {
        let mut server = server(30);
        let leaf = Leaf::default();
        let done = pull(
            &mut server,
            &leaf,
            &routine("7", &[29, 30]),
            SyncState::default(),
            IMAP_FOLDER,
            "SINCE 01-Jan-2023",
        )
        .expect("a pull");
        assert_eq!(server.readonly, Some(true), "EXAMINE, never SELECT");
        assert_eq!(
            (done.matched, done.already, done.landed, done.beyond),
            (30, 2, MAX_MATCHES, 3)
        );
        assert!(leaf.0.borrow().values().all(|unread| !unread), "lands read");
        assert_eq!(
            done.state.synced,
            (4..=28).collect::<BTreeSet<u32>>(),
            "the newest twenty-five not held"
        );
    }

    /// A second search over the same range fetches nothing it already pulled, and a routine
    /// state of another UIDVALIDITY holds nothing.
    #[test]
    fn a_second_pull_fetches_nothing_twice() {
        let mut first = server(3);
        let leaf = Leaf::default();
        let done = pull(
            &mut first,
            &leaf,
            &routine("6", &[1, 2, 3]),
            SyncState::default(),
            IMAP_FOLDER,
            "x",
        )
        .expect("a pull");
        assert_eq!(
            done.landed, 3,
            "a routine state of another generation holds nothing"
        );
        let again = pull(
            &mut server(3),
            &leaf,
            &SyncState::default(),
            done.state,
            IMAP_FOLDER,
            "x",
        )
        .expect("a second pull");
        assert_eq!((again.landed, again.already), (0, 3));
    }

    /// A message over the byte cap is fetched, counted and not kept.
    #[test]
    fn a_message_over_the_byte_cap_is_not_kept() {
        let mut big = server(2);
        big.messages.insert(2, vec![b'x'; MAX_MESSAGE_BYTES + 1]);
        let leaf = Leaf::default();
        let done = pull(
            &mut big,
            &leaf,
            &SyncState::default(),
            SyncState::default(),
            IMAP_FOLDER,
            "x",
        )
        .expect("a pull");
        assert_eq!((done.landed, done.too_large), (1, 1));
        assert_eq!(leaf.0.borrow().len(), 1);
    }

    /// Whatever the older search pulled, the routine INBOX pull that runs after it fetches only
    /// within its window, so no message lands in both leaves: the routine's `SINCE` and the
    /// older search's `BEFORE` are the same day on the same internal date.
    #[test]
    fn the_routine_pull_and_an_older_search_never_meet() {
        let window = day("2026-10-10");
        let r = Range::older(day("2023-01-01"), None, window).expect("a range");
        let criteria = imap_criteria(&query("invite"), r).expect("ascii");
        assert!(criteria.contains(&format!("BEFORE {}", imap_date(window))));
        assert_eq!(
            crate::mail::imap::Pull::Window {
                folder: crate::mail::imap::INBOX.to_owned(),
                since: window
            }
            .criteria(),
            format!("SINCE {}", imap_date(window))
        );
    }

    // --- the coverage words ------------------------------------------------------

    #[test]
    fn the_answer_names_every_account_folder_and_date() {
        let answer = Older::of(
            range(),
            vec![
                AccountOlder {
                    account: "ada@example.test".to_owned(),
                    outcome: Outcome::Searched(Found {
                        folders: vec![IMAP_FOLDER.to_owned()],
                        matched: Matched::Exactly(30),
                        already_here: 2,
                        pulled: Vec::new(),
                        unnamed: 25,
                        too_large: 0,
                        beyond_cap: 3,
                        local: "path:ada/older-inbox/**".to_owned(),
                    }),
                },
                AccountOlder {
                    account: "bo@example.test".to_owned(),
                    outcome: Outcome::NotSearched(Unsearched::NoServerSearch(
                        "bo@example.test".to_owned(),
                    )),
                },
            ],
        );
        assert_eq!(
            answer.searched,
            "Searched on the mail server: mail dated from 2023-01-01 up to, not including, \
             2026-10-10. \
             ada@example.test: INBOX searched; 30 matched, 2 already on the box, 25 pulled now, \
             3 more left on the server past the cap of 25. bo@example.test: not searched, \
             because bo@example.test is pulled by mbsync through the Proton Bridge, which offers \
             this box no server search; the folders mbsync syncs are on the box, and `search` \
             reads them. Not searched here: mail before 2023-01-01 (not asked for), and mail \
             from 2026-10-10 on (the routine sync's, which `search` reads)."
        );
        let json = serde_json::to_value(&answer).expect("json");
        assert_eq!(json["range"]["since"], "2023-01-01");
        assert_eq!(json["accounts"][0]["folders"][0], "INBOX");
        assert_eq!(json["accounts"][0]["beyond_cap"], 3);
        assert!(json["accounts"][1]["not_searched"].is_string());
    }
}
