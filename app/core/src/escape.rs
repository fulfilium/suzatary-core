//! Prose into typst, as two types for its two source languages.
//!
//! [`Escaped`] is markup (inside `[..]`, where `#`, `[` and `]` are structure); [`Quoted`] is
//! the inside of a `".."` string, where only `"` and `\` are. Escaping one as the other is
//! silent damage, hence two newtypes.
//!
//! This is containment, not cosmetics: the triage agent's prose comes from attacker-controlled
//! mail, and with every structural character backslashed it cannot author markup at all. That
//! is stated as a law over arbitrary strings in the property tests below.

use std::fmt;

/// The characters typst markup would otherwise read as structure.
///
/// `]` matters for containment (it closes a content block); the rest are escaped so prose reads
/// as prose.
pub(crate) const MARKUP_SPECIALS: &[char] =
    &['\\', '[', ']', '#', '$', '@', '*', '_', '<', '>', '`'];

/// The characters a typst string would otherwise read as structure: deliberately not the
/// markup set.
pub(crate) const STRING_SPECIALS: &[char] = &['\\', '"'];

/// Typst markup that prose went through [`escape_markup`] to become: the one prose → markup
/// door, and the type [`crate::capability::Render`] takes.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct Escaped(String);

impl Escaped {
    /// Markup read back off the chain, already escaped when it was authored (re-escaping would
    /// double every backslash). Never hand it raw prose or a composed string: that would put
    /// markup back in the agent's reach.
    #[must_use]
    pub fn from_chain(markup: impl Into<String>) -> Self {
        Self(markup.into())
    }

    /// The markup, for typst.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Escaped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The inside of a typst `".."` string that prose went through [`escape_string`] to become.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Quoted(String);

impl Quoted {
    /// String source read back off the chain; see [`Escaped::from_chain`].
    #[must_use]
    pub fn from_chain(source: impl Into<String>) -> Self {
        Self(source.into())
    }

    /// The escaped text, to be placed between two `"`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Quoted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Prose → typst content, with every structural character neutralised: the only way plain
/// prose becomes [`Escaped`].
#[must_use]
pub fn escape_markup(text: &str) -> Escaped {
    Escaped(escape_with(text, MARKUP_SPECIALS))
}

/// Prose → the inside of a typst string: the only way plain prose becomes [`Quoted`].
#[must_use]
pub fn escape_string(text: &str) -> Quoted {
    Quoted(escape_with(text, STRING_SPECIALS))
}

/// Typst content → the prose it stands for: the inverse of [`escape_markup`]. It refuses
/// anything outside the escape's image, so `unescape_markup ∘ escape_markup = id` is total.
pub fn unescape_markup(markup: &Escaped) -> Result<String, EscapeError> {
    unescape_with(markup.as_str(), MARKUP_SPECIALS)
}

/// The inside of a typst string → the prose it stands for. The inverse of
/// [`escape_string`].
pub fn unescape_string(source: &Quoted) -> Result<String, EscapeError> {
    unescape_with(source.as_str(), STRING_SPECIALS)
}

/// The text handed to an unescape is not in the escape's image.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EscapeError {
    /// A backslash with nothing after it. The escape never emits one.
    #[error("a trailing backslash at byte {at}: nothing to unescape")]
    DanglingBackslash {
        /// Where it is.
        at: usize,
    },
    /// A structural character standing on its own: the failure that matters.
    #[error("an unescaped {found:?} at byte {at}")]
    UnescapedSpecial {
        /// The character.
        found: char,
        /// Where it is.
        at: usize,
    },
}

fn escape_with(text: &str, specials: &[char]) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if specials.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn unescape_with(source: &str, specials: &[char]) -> Result<String, EscapeError> {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.char_indices();
    while let Some((at, c)) = chars.next() {
        if c == '\\' {
            let (_, escaped) = chars.next().ok_or(EscapeError::DanglingBackslash { at })?;
            out.push(escaped);
        } else if specials.contains(&c) {
            return Err(EscapeError::UnescapedSpecial { found: c, at });
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// No unescaped special, as a predicate over one string: every special is immediately preceded
/// by an odd-length run of backslashes, and every other run of backslashes is even.
///
/// The image is full of special characters on purpose (an escape that removed them would be
/// lossy); the law is that none of them is structure.
#[must_use]
pub fn no_unescaped_special(image: &str, specials: &[char]) -> bool {
    let mut backslashes = 0usize;
    for c in image.chars() {
        if c == '\\' {
            backslashes += 1;
            continue;
        }
        let escaped = !backslashes.is_multiple_of(2);
        if specials.contains(&c) != escaped {
            return false;
        }
        backslashes = 0;
    }
    backslashes.is_multiple_of(2)
}

/// Typst markup source → readable one-line plain text, for the open list and
/// the briefing, never for rendering.
///
/// Not the inverse of [`escape_markup`] (that is [`unescape_markup`]): this reads markup as a
/// person skims it. A call gives the text of its content blocks (`#link("u")[t]` → `t`) or else
/// its string arguments. Total, so one odd body cannot break the agent's dedup list.
#[must_use]
pub fn markup_text(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut parts = String::new();
    let mut at = 0;
    while at < chars.len() {
        let (chunk, next) = read_markup(&chars, at);
        parts.push_str(&chunk);
        at = next;
        if at < chars.len() {
            // An unmatched `]`: prose the document escaped, not structure.
            parts.push(']');
            at += 1;
        }
    }
    parts.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The inside of a typst `".."` literal → the prose it stands for.
///
/// Only `\\` and `\"` are undone: those are a string's only escapes, and undoing more would
/// invent text the document never showed.
#[must_use]
pub fn string_text(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '\\' && at + 1 < chars.len() && STRING_SPECIALS.contains(&chars[at + 1]) {
            out.push(chars[at + 1]);
            at += 2;
        } else {
            out.push(chars[at]);
            at += 1;
        }
    }
    out
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_body(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// Markup from `at` to the first unmatched `]` (or the end): the text, and the
/// index of that `]`.
fn read_markup(chars: &[char], mut at: usize) -> (String, usize) {
    let mut out = String::new();
    while at < chars.len() {
        let c = chars[at];
        if c == ']' {
            break;
        }
        if c == '\\' && at + 1 < chars.len() && MARKUP_SPECIALS.contains(&chars[at + 1]) {
            out.push(chars[at + 1]);
            at += 2;
        } else if c == '#' && at + 1 < chars.len() && is_ident_start(chars[at + 1]) {
            let mut end = at + 1;
            while end < chars.len() && is_ident_body(chars[end]) {
                end += 1;
            }
            let (chunk, next) = read_call(chars, end);
            out.push_str(&chunk);
            at = next;
        } else if c == '[' {
            let (chunk, next) = read_markup(chars, at + 1);
            out.push_str(&chunk);
            at = if next < chars.len() { next + 1 } else { next };
        } else if c == '`' {
            let (chunk, next) = read_raw(chars, at);
            out.push_str(&chunk);
            at = next;
        } else if (c == '*' || c == '_') && !inside_word(chars, at) {
            at += 1;
        } else {
            out.push(c);
            at += 1;
        }
    }
    (out, at)
}

/// `*` and `_` are dropped only where typst would take them for emphasis, never between two
/// word characters: `ms_imap_sync` is a name, not an emphasised `imap`.
fn inside_word(chars: &[char], at: usize) -> bool {
    at > 0
        && chars[at - 1].is_alphanumeric()
        && at + 1 < chars.len()
        && chars[at + 1].is_alphanumeric()
}

/// A backtick run and its contents, kept verbatim: no escapes, no markup.
fn read_raw(chars: &[char], at: usize) -> (String, usize) {
    let mut fence = 0;
    while at + fence < chars.len() && chars[at + fence] == '`' {
        fence += 1;
    }
    let start = at + fence;
    let mut close = start;
    while close < chars.len() {
        if chars[close] == '`' && chars[close..].iter().take(fence).all(|c| *c == '`') {
            let mut inner: String = chars[start..close].iter().collect();
            if fence >= 3 {
                // ```lang\n..``` — the first line is the language tag.
                inner = inner
                    .split_once('\n')
                    .map_or(String::new(), |(_, rest)| rest.to_owned());
            }
            return (inner, close + fence);
        }
        close += 1;
    }
    (String::new(), start)
}

/// A `#name` call, `at` being just past the name.
fn read_call(chars: &[char], mut at: usize) -> (String, usize) {
    let mut contents: Vec<String> = Vec::new();
    let mut strings: Vec<String> = Vec::new();
    if at < chars.len() && chars[at] == '(' {
        let read = read_args(chars, at + 1);
        contents = read.0;
        strings = read.1;
        at = read.2;
    }
    while at < chars.len() && chars[at] == '[' {
        let (chunk, next) = read_markup(chars, at + 1);
        contents.push(chunk);
        at = if next < chars.len() { next + 1 } else { next };
    }
    let kept: Vec<String> = contents.into_iter().filter(|c| !c.is_empty()).collect();
    let shown = if kept.is_empty() { strings } else { kept };
    (shown.join(" "), at)
}

/// The content blocks and string literals inside an argument list, `at` being
/// just past the `(`. Everything else in there is code, which the reader did not come for.
fn read_args(chars: &[char], mut at: usize) -> (Vec<String>, Vec<String>, usize) {
    let mut contents = Vec::new();
    let mut strings = Vec::new();
    let mut depth = 1;
    while at < chars.len() && depth > 0 {
        match chars[at] {
            '(' => {
                depth += 1;
                at += 1;
            }
            ')' => {
                depth -= 1;
                at += 1;
            }
            '"' => {
                let (chunk, next) = read_quoted(chars, at + 1);
                strings.push(chunk);
                at = next;
            }
            '[' => {
                let (chunk, next) = read_markup(chars, at + 1);
                contents.push(chunk);
                at = if next < chars.len() { next + 1 } else { next };
            }
            _ => at += 1,
        }
    }
    (contents, strings, at)
}

/// A `".."` literal, `at` being just past the opening quote.
fn read_quoted(chars: &[char], at: usize) -> (String, usize) {
    let mut end = at;
    while end < chars.len() {
        if chars[end] == '\\' && end + 1 < chars.len() {
            end += 2;
        } else if chars[end] == '"' {
            return (
                string_text(&chars[at..end].iter().collect::<String>()),
                end + 1,
            );
        } else {
            end += 1;
        }
    }
    (string_text(&chars[at..end].iter().collect::<String>()), end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The set is closed: a structural character not backslashed here is one an email can use
    /// to leave its own field.
    #[test]
    fn every_markup_special_is_backslashed_and_nothing_else_is() {
        assert_eq!(
            escape_markup("a\\b[c]d#e$f@g*h_i<j>k`l").as_str(),
            "a\\\\b\\[c\\]d\\#e\\$f\\@g\\*h\\_i\\<j\\>k\\`l"
        );
        assert_eq!(
            escape_markup("plain prose, 1 & 2.").as_str(),
            "plain prose, 1 & 2."
        );
    }

    /// The string language is a different language: `"` is structure and `@`, `$`, `#` are not.
    #[test]
    fn a_string_escapes_the_two_characters_a_string_has() {
        assert_eq!(
            escape_string(r#"say "hi" \ #now"#).as_str(),
            r#"say \"hi\" \\ #now"#
        );
    }

    /// String source read as markup is caught by the law, not the compiler: `#` alone is not
    /// markup.
    #[test]
    fn string_source_is_not_markup_source() {
        let quoted = escape_string("#link");
        assert!(!no_unescaped_special(quoted.as_str(), MARKUP_SPECIALS));
        assert!(no_unescaped_special(quoted.as_str(), STRING_SPECIALS));
    }

    proptest! {
        /// The escape is injective, because it has a total inverse: `unescape ∘ escape = id`.
        #[test]
        fn the_markup_escape_round_trips(text: String) {
            let back = unescape_markup(&escape_markup(&text));
            prop_assert_eq!(back.as_deref(), Ok(text.as_str()));
        }

        /// Injectivity stated directly, over pairs.
        #[test]
        fn distinct_prose_escapes_distinctly(a: String, b: String) {
            prop_assume!(a != b);
            prop_assert_ne!(escape_markup(&a), escape_markup(&b));
        }

        /// No unescaped special: nothing the agent writes can reach typst as structure.
        #[test]
        fn the_image_holds_no_structure(text: String) {
            prop_assert!(no_unescaped_special(escape_markup(&text).as_str(), MARKUP_SPECIALS));
        }

        /// The same three for the string language, whose specials are two.
        #[test]
        fn the_string_escape_round_trips(text: String) {
            let back = unescape_string(&escape_string(&text));
            prop_assert_eq!(back.as_deref(), Ok(text.as_str()));
        }

        #[test]
        fn distinct_prose_quotes_distinctly(a: String, b: String) {
            prop_assume!(a != b);
            prop_assert_ne!(escape_string(&a), escape_string(&b));
        }

        #[test]
        fn the_quoted_image_holds_no_structure(text: String) {
            prop_assert!(no_unescaped_special(escape_string(&text).as_str(), STRING_SPECIALS));
        }

        /// The predicate is not vacuously true: a bare special fails it, and
        /// so does a lone trailing backslash.
        #[test]
        fn a_bare_special_is_caught(prefix: String, special in proptest::sample::select(MARKUP_SPECIALS)) {
            let text = format!("{}{special}", escape_markup(&prefix));
            prop_assert!(!no_unescaped_special(&text, MARKUP_SPECIALS));
        }
    }
}
