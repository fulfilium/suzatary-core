//! THE LEAK GUARD'S MATCHER: whether a file of a tree that may be published names anything of
//! this deployment. `scripts/public-tree.sh --check` hands it every file through `suzatary
//! leaks`; a hit is a file and a line number, never the text, so the guard never prints what it
//! guards. Secrets are not its business: gitleaks reads the same files for them, with its own
//! maintained rules and its own decoding (`scripts/secret-rules.toml`).
//!
//! THE INVARIANT the two hold together: every byte of a published tree is either text both of
//! them read, or in a file a person admitted by its path and SHA-256 at a terminal
//! ([`Admitted`]). So the matcher decodes nothing. A file it cannot read as words ([`Opaque`]) is
//! a hit until it is admitted, and what an admitted file encodes is the admitting person's to see.
//!
//! Text is read as words ([`Words`]): compatibility-decomposed, lowercased, its accents and
//! Unicode's format characters dropped, and every run of whitespace one space, a line break and
//! the comment marker opening the next line included. A term of the data repository's lists, or
//! a name of this deployment, leaks where it stands as a whole word of that text, at the line it
//! starts on. Outside [`Scope::Lists`], so does a line holding a path of the private history
//! beside a docs reference, a home path other than the cast's, a `.ts.net` name that is no
//! placeholder, an email address outside the reserved example domains, a tailnet node's address
//! or a telephone number.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;

/// Which rules a scan applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every rule: the check a public tree passes.
    Whole,
    /// The data repository's term lists alone: what a scan of another repository's range asks.
    Lists,
}

/// Which rule a hit is, so a refusal names its layer and never what it matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// A term of the data repository's lists.
    Term,
    /// A name of this deployment: its box, tailnet or user.
    Name,
    /// A path of the private history, beside a docs reference.
    Link,
    /// A home path other than the cast's.
    Home,
    /// A `.ts.net` name that is no placeholder.
    Tailnet,
    /// An email address outside the reserved example domains.
    Address,
    /// A tailnet node's address.
    Node,
    /// A telephone number that is not fictional.
    Phone,
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Layer::Term => "names a term of the lists",
            Layer::Name => "names a name of this deployment",
            Layer::Link => "links to the private history",
            Layer::Home => "names a home path",
            Layer::Tailnet => "names a tailnet",
            Layer::Address => "names an email address",
            Layer::Node => "names a tailnet node's address",
            Layer::Phone => "names a telephone number",
        })
    }
}

/// What a scan matches against, read once: the private lists, this deployment's names and the
/// private history's paths, each read as words with the layer it is.
#[derive(Debug, Clone)]
pub struct Matcher {
    terms: Vec<(String, Layer)>,
    links: Vec<String>,
    scope: Scope,
}

impl Matcher {
    /// A matcher over `terms` and this deployment's `words` (one per entry, blank ones dropped;
    /// the words are read only under [`Scope::Whole`]) and the private history's `links`.
    #[must_use]
    pub fn new(terms: &[String], words: &[String], links: &[String], scope: Scope) -> Self {
        let named = match scope {
            Scope::Whole => words,
            Scope::Lists => &[],
        };
        Matcher {
            terms: terms
                .iter()
                .map(|one| (one, Layer::Term))
                .chain(named.iter().map(|one| (one, Layer::Name)))
                .map(|(one, layer)| (Words::of(one).text, layer))
                .filter(|(one, _)| !one.is_empty())
                .collect(),
            links: links.iter().filter(|l| !l.is_empty()).cloned().collect(),
            scope,
        }
    }

    /// The 1-based lines of `text` where a leak starts, each with the least layer it is.
    #[must_use]
    pub fn hits(&self, text: &str) -> BTreeMap<usize, Layer> {
        let words = Words::of(text);
        let mut hits = BTreeMap::new();
        let mut hit = |at: usize, layer: Layer| {
            hits.entry(at)
                .and_modify(|least: &mut Layer| *least = (*least).min(layer))
                .or_insert(layer);
        };
        for (term, layer) in &self.terms {
            for at in words.find(term) {
                hit(at, *layer);
            }
        }
        if self.scope == Scope::Whole {
            for (at, line) in text.lines().enumerate() {
                if let Some(layer) = self.says(line) {
                    hit(at + 1, layer);
                }
            }
        }
        hits
    }

    /// The first rule that is not the lists' this one line breaks.
    fn says(&self, raw: &str) -> Option<Layer> {
        let line = raw.to_lowercase();
        [
            ((line.contains("docs/") || line.contains("claude-history"))
                && self.links.iter().any(|link| raw.contains(link.as_str())))
            .then_some(Layer::Link),
            homed(raw).then_some(Layer::Home),
            tailnet(&line).then_some(Layer::Tailnet),
            addressed(&line).then_some(Layer::Address),
            tailnet_address(raw).then_some(Layer::Node),
            phone(raw).then_some(Layer::Phone),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// Text as words: every character folded (see [`fold`]) and every run of whitespace, a line break
/// and the comment marker opening the next line included, one space; each line's start kept, so
/// a match is named by the line it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    text: String,
    /// `(offset, line)` where each line's words start, offsets ascending.
    starts: Vec<(usize, usize)>,
}

/// A comment marker a line may open with, after its indentation.
static MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?://[/!]?|#+|\*+|--|;+|>+)").expect("a valid pattern"));

impl Words {
    /// `text` read as words.
    #[must_use]
    pub fn of(text: &str) -> Self {
        let mut words = Words {
            text: String::with_capacity(text.len()),
            starts: Vec::new(),
        };
        for (at, line) in text.lines().enumerate() {
            let mut gap = !words.text.is_empty();
            for c in fold(&MARKER.replace(line, "")).chars() {
                if c.is_whitespace() {
                    gap = !words.text.is_empty();
                    continue;
                }
                if gap {
                    words.text.push(' ');
                    gap = false;
                }
                if words.starts.last().is_none_or(|(_, line)| *line != at + 1) {
                    words.starts.push((words.text.len(), at + 1));
                }
                words.text.push(c);
            }
        }
        words
    }

    /// The lines on which `term`, already read as words, starts as a whole word (no letter, digit
    /// or `_` of any script on either side).
    fn find<'a>(&'a self, term: &'a str) -> impl Iterator<Item = usize> + 'a {
        let wordy = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        self.text
            .match_indices(term)
            .filter(move |(at, _)| {
                !wordy(self.text[..*at].chars().next_back())
                    && !wordy(self.text[at + term.len()..].chars().next())
            })
            .map(|(at, _)| {
                let line = self.starts.partition_point(|(start, _)| *start <= at);
                self.starts[line - 1].1
            })
    }
}

/// Arabic's tatweel, a stretch of the line between letters that is no letter itself.
const TATWEEL: char = '\u{0640}';

/// One character as the matcher reads it: compatibility-decomposed (so `ﬁ` is `fi` and a
/// full-width letter its ASCII one), lowercased, with every combining mark (so `Zoë` is `zoe`)
/// and every format character, Unicode's `Cf` (a zero-width space, a soft hyphen, a joiner,
/// shown as nothing) and the tatweel, dropped. A term and a text are folded alike.
#[must_use]
pub fn fold(text: &str) -> String {
    use unicode_general_category::{GeneralCategory, get_general_category};
    use unicode_normalization::UnicodeNormalization as _;
    use unicode_normalization::char::is_combining_mark;
    text.nfkd()
        .filter(|c| {
            !is_combining_mark(*c)
                && get_general_category(*c) != GeneralCategory::Format
                && *c != TATWEEL
        })
        .collect::<String>()
        .to_lowercase()
}

/// Why the matcher cannot vouch for a file by reading it, judged over the WHOLE file, so no way
/// of cutting an encoding up (wrapped lines, quoted chunks, escaped line breaks) reads as words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opaque {
    /// Not text: a NUL byte, or bytes that are not UTF-8.
    Binary,
    /// Text holding base64 or hex: tokens of [`TOKEN`] or more characters whose case and digits
    /// alternate as base64's do, [`ENCODED`] characters of them in all (a token that is exactly
    /// an integrity hash, `sha256-…` or Go's `h1:…`, not counted while its bytes are no text, nor
    /// a sealed slot's box of ciphertext); a hex
    /// token of [`HEX`] or more digits whose bytes, read from either digit, are [`textual`]; or
    /// [`PAIRS`] hex pairs in a row, separated by `:`, `-` or a space.
    Encoded,
    /// Text that spells with escapes: [`ESCAPES`] or more of a letter, a digit or a byte past
    /// ASCII (`%XX`, `&#…;`, `\uXXXX`, `\xXX`); one of a letter or digit inside a word, those or
    /// quoted-printable's `=XX`; or two quoted-printable escapes in a row, or an escaped `=`.
    Escaped,
}

impl std::fmt::Display for Opaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Opaque::Binary => "not text",
            Opaque::Encoded => "base64 or hex",
            Opaque::Escaped => "text spelled with escapes",
        })
    }
}

/// The shortest token read as a piece of an encoding: as short as a line a wrapped encoding is
/// cut into.
pub const TOKEN: usize = 20;

/// The characters of such tokens past which a file is opaque: a few digests' worth, short of
/// a payload worth hiding a sentence in.
pub const ENCODED: usize = 160;

/// The shortest hex token read for the text it may spell: sixteen bytes.
pub const HEX: usize = 32;

/// The hex pairs in a row past which a file is opaque: a dump, never a MAC address.
pub const PAIRS: usize = 16;

/// The spelling escapes past which a file is opaque.
pub const ESCAPES: usize = 16;

/// A token of base64's alphabet, standard or URL-safe, padding included.
static ALPHABET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9+/_=\-]+").expect("a valid pattern"));

/// A hex token.
static HEXES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"[0-9a-fA-F]{{{HEX},}}")).expect("a valid pattern"));

/// Hex pairs in a row behind separators.
static PAIRED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"[0-9a-fA-F]{{2}}(?:[: \-][0-9a-fA-F]{{2}}){{{},}}",
        PAIRS - 1
    ))
    .expect("a valid pattern")
});

/// A token that is exactly an integrity hash of its algorithm's length, its digest in base64: a
/// Subresource Integrity or Nix hash (`sha256-`, `sha384-`, `sha512-`), or a Go module's after
/// `h1:` (a SHA-256 alone). Anything longer is a token like any other.
static HASH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:sha256-(?P<s256>[A-Za-z0-9+/]{43}=)|sha384-(?P<s384>[A-Za-z0-9+/]{64})|sha512-(?P<s512>[A-Za-z0-9+/]{86}==)|(?P<h1>[A-Za-z0-9+/]{43}=))$",
    )
    .expect("a valid pattern")
});

/// Is this token, at `start` in `text`, exactly an integrity hash whose bytes are no text? Then
/// it is random bytes and hides no word.
fn hash(text: &str, start: usize, token: &str) -> bool {
    use base64::Engine as _;
    HASH.captures(token).is_some_and(|found| {
        let digest = ["s256", "s384", "s512"]
            .iter()
            .find_map(|name| found.name(name))
            .or_else(|| found.name("h1").filter(|_| text[..start].ends_with("h1:")));
        digest.is_some_and(|digest| {
            base64::engine::general_purpose::STANDARD
                .decode(digest.as_str())
                .is_ok_and(|bytes| !textual(&bytes))
        })
    })
}

/// A sealed slot's literal (`secretary::line_event`'s `SealedSlot`): the name of its key, a
/// digest, its box, the sealed words in base64, and in the `Scrubbed` form the name of the
/// rules its words passed, a digest.
static LOCKED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:Locked\(key='[0-9a-f]{64}', box='(?P<locked>[A-Za-z0-9+/]+={0,2})'\)",
        r"|Scrubbed\(key='[0-9a-f]{64}', box='(?P<scrubbed>[A-Za-z0-9+/]+={0,2})', rules='[0-9a-f]{64}'\))"
    ))
    .expect("a valid pattern")
});

/// A sealed slot of a text the guard recognises as ciphertext: where its whole call stands, and
/// where its box does. One recognition, read by the matcher (the box is not counted) and by the
/// secret scanner's copy (the call is a placeholder), so the two cannot disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// The whole `Locked(key='…', box='…')` or `Scrubbed(key='…', box='…', rules='…')` call.
    pub call: std::ops::Range<usize>,
    /// Its box, the sealed words in base64.
    pub sealed: std::ops::Range<usize>,
}

/// The sealed slots of `text` that are ciphertext: words padded to whole 256-byte buckets and
/// sealed, so a box of 256·k + 16 bytes (the Poly1305 tag), k ≥ 1, that are no text. Random
/// bytes under a key that is destroyed or never leaves the box hide no word.
#[must_use]
pub fn slots(text: &str) -> Vec<Slot> {
    use base64::Engine as _;
    LOCKED
        .captures_iter(text)
        .filter_map(|found| {
            let call = found.get(0)?;
            let sealed = found.name("locked").or_else(|| found.name("scrubbed"))?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(sealed.as_str())
                .ok()?;
            (bytes.len() >= 256 + 16 && bytes.len() % 256 == 16 && !textual(&bytes)).then(|| Slot {
                call: call.range(),
                sealed: sealed.range(),
            })
        })
        .collect()
}

/// What the secret scanner reads of a sealed slot: the call, its key's name and its box gone.
pub const PLACEHOLDER: &str = "Locked(<sealed>)";

/// `text` with every sealed slot [`slots`] recognises replaced by [`PLACEHOLDER`], and nothing
/// else touched: a key before or after a call on its line, or a slot of any other shape, is
/// still there to be found. No slot spans a line, so every line keeps its number.
#[must_use]
pub fn placeheld(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for slot in slots(text) {
        out.push_str(&text[from..slot.call.start]);
        out.push_str(PLACEHOLDER);
        from = slot.call.end;
    }
    out.push_str(&text[from..]);
    out
}

/// Bytes that read as text: at least 95 in 100 printable ASCII or whitespace, at least half
/// letters, and four different bytes or more. Random bytes (a digest, a hash) are not printable,
/// a placeholder of digits is not letters, and one of a repeated byte (`0707…`) spells nothing.
#[must_use]
pub fn textual(bytes: &[u8]) -> bool {
    let printable = bytes
        .iter()
        .filter(|b| b.is_ascii_graphic() || b.is_ascii_whitespace())
        .count();
    let letters = bytes.iter().filter(|b| b.is_ascii_alphabetic()).count();
    let different = bytes.iter().collect::<BTreeSet<_>>().len();
    different >= 4 && printable * 100 >= bytes.len() * 95 && letters * 2 >= bytes.len()
}

/// The bytes a hex token spells read from its first or its second digit, as text or not.
fn spells(hex: &str) -> bool {
    [0, 1].iter().any(|from| {
        hex.get(*from..).is_some_and(|digits| {
            let bytes: Vec<u8> = digits
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .filter_map(|pair| {
                    std::str::from_utf8(pair)
                        .ok()
                        .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                })
                .collect();
            textual(&bytes)
        })
    })
}

/// An escape, by the digits of the character it names: percent (`p`), `\x` (`x`), a
/// quoted-printable one (`q`), `\u` (`u`), and HTML's decimal (`d`) and hex (`h`) references.
static ESCAPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"%(?P<p>[0-9A-Fa-f]{2})|\\x(?P<x>[0-9A-Fa-f]{2})|=(?P<q>[0-9A-F]{2})|\\u(?P<u>[0-9A-Fa-f]{4})|&#(?P<d>[0-9]{1,7});|&#[xX](?P<h>[0-9A-Fa-f]{1,6});",
    )
    .expect("a valid pattern")
});

/// Two quoted-printable escapes in a row, or an escaped `=` (its `D` a class, so this source is
/// no quoted-printable body itself).
static QUOTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:=[0-9A-F]{2}){2}|=3[D]").expect("a valid pattern"));

/// A character's class, as base64 mixes them.
#[derive(PartialEq, Eq)]
enum Class {
    Upper,
    Lower,
    Digit,
}

/// Does this token hold all three classes (upper case, lower case, digit) and change class at
/// least once every three characters, as base64 does? A path or a name
/// (`docs/PLAN-2026-09-16`) changes rarely, and a lowercase hex digest holds two classes.
fn alternates(token: &str) -> bool {
    let classes: Vec<Class> = token
        .chars()
        .filter_map(|c| match c {
            'A'..='Z' => Some(Class::Upper),
            'a'..='z' => Some(Class::Lower),
            '0'..='9' => Some(Class::Digit),
            _ => None,
        })
        .collect();
    let changes = classes.windows(2).filter(|pair| pair[0] != pair[1]).count();
    token.len() >= TOKEN
        && [Class::Upper, Class::Lower, Class::Digit]
            .iter()
            .all(|class| classes.contains(class))
        && changes * 3 >= token.len()
}

/// The character an escape names, and whether it is quoted-printable's.
fn named(found: &regex::Captures<'_>) -> Option<(char, bool)> {
    let hex = |name: &str| {
        found
            .name(name)
            .and_then(|m| u32::from_str_radix(m.as_str(), 16).ok())
    };
    let code = hex("p")
        .or_else(|| hex("x"))
        .or_else(|| hex("u"))
        .or_else(|| hex("h"))
        .or_else(|| found.name("d").and_then(|m| m.as_str().parse().ok()));
    match (code, hex("q")) {
        (Some(code), _) => char::from_u32(code).map(|c| (c, false)),
        (None, Some(code)) => char::from_u32(code).map(|c| (c, true)),
        (None, None) => None,
    }
}

impl Opaque {
    /// Why `bytes` are opaque to the matcher, or `None` when they are text it reads whole.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Option<Self> {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Some(Opaque::Binary);
        };
        if text.contains('\0') {
            return Some(Opaque::Binary);
        }
        let boxes: Vec<_> = slots(text).into_iter().map(|slot| slot.sealed).collect();
        let encoded: usize = ALPHABET
            .find_iter(text)
            .filter(|token| !hash(text, token.start(), token.as_str()))
            .filter(|token| !boxes.contains(&token.range()))
            .map(|token| token.as_str())
            .filter(|token| alternates(token))
            .map(str::len)
            .sum();
        if encoded >= ENCODED
            || HEXES.find_iter(text).any(|hex| spells(hex.as_str()))
            || PAIRED.is_match(text)
        {
            return Some(Opaque::Encoded);
        }
        let lettered = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphabetic());
        let mut spelled = 0;
        for found in ESCAPE.captures_iter(text) {
            let (Some((c, quoted)), Some(whole)) = (named(&found), found.get(0)) else {
                continue;
            };
            // Inside a word: a letter on both sides, as when an escape stands for one letter of a
            // term, never a value assigned to a name (`EPOCH=315532`).
            let inside_a_word = lettered(text[..whole.start()].chars().next_back())
                && lettered(text[whole.end()..].chars().next());
            if c.is_ascii_alphanumeric() && inside_a_word {
                return Some(Opaque::Escaped);
            }
            // A quoted-printable body is QUOTED's (two in a row, an escaped `=`) or spells inside
            // a word; a lone `=` before hex digits is a value (`FREQ=DAILY`, `EPOCH=315532`).
            if !quoted && (c.is_ascii_alphanumeric() || !c.is_ascii()) {
                spelled += 1;
            }
        }
        (spelled >= ESCAPES || QUOTED.is_match(text)).then_some(Opaque::Escaped)
    }
}

/// The files a person admitted, each by its path and the SHA-256 of its bytes: the manifest,
/// `<digest>  <path>` per line as `sha256sum` prints it, blank and `#` lines skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Admitted(BTreeSet<(String, String)>);

/// A line of the manifest, or of `.gitleaksignore`, that is not one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {0} is malformed")]
pub struct Malformed(pub usize);

impl Admitted {
    /// Each admitted file as its path and digest.
    pub fn admissions(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(digest, path)| (path.as_str(), digest.as_str()))
    }

    /// The manifest's admissions.
    ///
    /// # Errors
    /// [`Malformed`] at the first line that is not one.
    pub fn parse(manifest: &str) -> Result<Self, Malformed> {
        manifest
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'))
            .map(|(at, line)| match line.split_once("  ") {
                Some((digest, path))
                    if digest.len() == 64
                        && digest
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                        && !path.is_empty() =>
                {
                    Ok((digest.to_owned(), path.to_owned()))
                }
                _ => Err(Malformed(at + 1)),
            })
            .collect::<Result<_, _>>()
            .map(Admitted)
    }

    /// Did a person admit exactly these bytes at this path?
    #[must_use]
    pub fn holds(&self, path: &str, bytes: &[u8]) -> bool {
        use sha2::Digest as _;
        self.0
            .contains(&(hex::encode(sha2::Sha256::digest(bytes)), path.to_owned()))
    }
}

/// The files holding a secret scanner's finding a person vouched for: the paths the tree's
/// `.gitleaksignore` names, each line a fingerprint `<path>:<rule>:<line>` with no space at
/// either end, empty and `#` lines skipped. A fingerprint names a path, a rule and a line, never the bytes, so the bytes of
/// each such file are admitted by the manifest as an opaque file's are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Findings(BTreeSet<(String, String)>);

impl Findings {
    /// Each fingerprint, by the path it names: what a person vouched for, one at a time.
    pub fn admissions(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(path, line)| (path.as_str(), line.as_str()))
    }

    /// The paths `.gitleaksignore` names.
    ///
    /// # Errors
    /// [`Malformed`] at the first line that is not a fingerprint.
    pub fn parse(ignore: &str) -> Result<Self, Malformed> {
        ignore
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
            .map(|(at, line)| {
                let mut fields = line.rsplitn(3, ':');
                match (fields.next(), fields.next(), fields.next()) {
                    // gitleaks trims a line before it reads it, so an untrimmed one would admit a
                    // finding at a path this set never names.
                    (Some(number), Some(rule), Some(path))
                        if line.trim() == line
                            && number.parse::<usize>().is_ok()
                            && !rule.is_empty()
                            && !path.is_empty() =>
                    {
                        Ok((path.to_owned(), line.to_owned()))
                    }
                    _ => Err(Malformed(at + 1)),
                }
            })
            .collect::<Result<_, _>>()
            .map(Findings)
    }
}

/// Why the guard asks for a file's exact bytes to be admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vouch {
    /// The matcher cannot read it as words.
    Opaque(Opaque),
    /// It holds a finding a person vouched for by fingerprint.
    Finding,
}

impl Vouch {
    /// Why the file at `path`, whose bytes are `opaque` to the matcher or not, must be admitted,
    /// or `None` when the two checks reading it are enough.
    #[must_use]
    pub fn of(path: &str, opaque: Option<Opaque>, findings: &Findings) -> Option<Self> {
        opaque.map(Vouch::Opaque).or_else(|| {
            findings
                .0
                .iter()
                .any(|(named, _)| named == path)
                .then_some(Vouch::Finding)
        })
    }
}

impl std::fmt::Display for Vouch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Vouch::Opaque(opaque) => write!(f, "holds {opaque}"),
            Vouch::Finding => f.write_str("holds a finding .gitleaksignore admits"),
        }
    }
}

/// A home path: `/home/<user>` or `/Users/<user>`.
static HOME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"/(?:home|Users)/([A-Za-z_][A-Za-z0-9_.\-]*)").expect("a valid pattern")
});

/// A tailnet's `MagicDNS` name, its label first.
static TAILNET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-z0-9\-]+)\.ts\.net").expect("a valid pattern"));

/// An email address, lowercased.
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[a-z0-9._%+\-]+@([a-z0-9\-]+(?:\.[a-z0-9\-]+)*\.[a-z][a-z]+)")
        .expect("a valid pattern")
});

/// The end of a line that a URL's userinfo continues: `scheme://user`.
static USERINFO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-z][a-z0-9+.\-]*://[^/ ]*$").expect("a valid pattern"));

/// A dotted IPv4 address.
static IPV4: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b").expect("a valid pattern")
});

/// A node of a tailnet: an address in 100.64.0.0/10, the range Tailscale gives its nodes, other
/// than its own published resolver (100.100.100.100) and the cast's block, 100.64.0.0/24, which
/// the fixtures draw on as they draw on the example domains.
fn tailnet_address(raw: &str) -> bool {
    IPV4.captures_iter(raw).any(|found| {
        let octet = |i: usize| found[i].parse::<u16>().unwrap_or(u16::MAX);
        let address = [octet(1), octet(2), octet(3), octet(4)];
        address[0] == 100
            && (64..=127).contains(&address[1])
            && address.iter().all(|o| *o <= 255)
            && address != [100, 100, 100, 100]
            && !(address[1] == 64 && address[2] == 0)
    })
}

/// A telephone number: international (`+` and a country code, then eight to fifteen digits in
/// all, grouped by spaces, dots, dashes or brackets) or North American (`(nnn) nnn-nnnn` or
/// `nnn-nnn-nnnn`). Separators are required, so a digest or a timestamp is not one.
static PHONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:^|[^\w+])(\+[1-9][0-9]{0,2}(?:[ .\-]?\(?[0-9]{1,4}\)?){2,6})|\b(\(?[2-9][0-9]{2}\)?[ .\-]([0-9]{3})[ .\-][0-9]{4})\b",
    )
    .expect("a valid pattern")
});

/// A telephone number other than a fictional one: North America's 555 exchange, written either
/// way, or with the area code left out (`+1 555 0100`).
fn phone(raw: &str) -> bool {
    PHONE
        .captures_iter(raw)
        .any(|found| match (found.get(1), found.get(3)) {
            (Some(international), _) => {
                let digits: String = international
                    .as_str()
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect();
                let fictional = digits.starts_with('1')
                    && (digits.get(1..4) == Some("555")
                        || (digits.len() == 11 && digits.get(4..7) == Some("555")));
                (8..=15).contains(&digits.len()) && !fictional
            }
            (None, Some(exchange)) => exchange.as_str() != "555",
            (None, None) => false,
        })
}

/// A home path of a user other than the cast's `ada` or `root`.
fn homed(raw: &str) -> bool {
    HOME.captures_iter(raw).any(|found| {
        let user = found[1].to_lowercase();
        user != "ada" && user != "root"
    })
}

/// A tailnet whose name is not a placeholder.
fn tailnet(line: &str) -> bool {
    TAILNET
        .captures_iter(line)
        .any(|found| !matches!(&found[1], "example" | "tailnet"))
}

/// An address outside the reserved domains, and not a URL's userinfo.
fn addressed(line: &str) -> bool {
    EMAIL.captures_iter(line).any(|found| {
        let whole = found.get(0).expect("a match has a whole");
        let userinfo = USERINFO.is_match(&line[..whole.start()]);
        !userinfo && !reserved(&found[1]) && whole.as_str() != "noreply@anthropic.com"
    })
}

/// The example domains RFC 2606 and 6761 reserve, and any whose first label is `example`.
fn reserved(domain: &str) -> bool {
    ["example.com", "example.org", "example.net"]
        .iter()
        .any(|base| domain == *base || domain.ends_with(&format!(".{base}")))
        || [".test", ".invalid", ".example", ".localhost"]
            .iter()
            .any(|tld| domain.ends_with(tld))
        || domain.starts_with("example.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn matcher(scope: Scope) -> Matcher {
        let owned = |xs: &[&str]| xs.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        Matcher::new(
            &owned(&["ZzPerson", "zz  two words"]),
            &owned(&["zzbox"]),
            &owned(&["docs/private.md"]),
            scope,
        )
    }

    fn leaks(matcher: &Matcher, text: &str) -> bool {
        !matcher.hits(text).is_empty()
    }

    /// Every leak is spelled in pieces, so the tree this guard reads does not hold them.
    #[test]
    fn each_rule_finds_its_leak_and_a_near_miss_is_none() {
        let all = matcher(Scope::Whole);
        for leak in [
            "a zzperson mention".to_owned(),
            "the zzbox box".to_owned(),
            "see docs/private.md".to_owned(),
            concat!("/ho", "me/zzother/x").to_owned(),
            concat!("peer.zznet", ".ts.net").to_owned(),
            concat!("mail zz@realm", ".example-mail.com").to_owned(),
        ] {
            assert!(leaks(&all, &leak), "{leak}");
        }
        for clean in [
            "zzpersons and zzboxes are other words",
            "her@example.invalid, ops@mail.example.com, a@b.test",
            "/home/ada/Mail and /home/root/.ssh",
            "box.example.ts.net and peer.tailnet.ts.net",
            "https://user@calendar.example.org",
            "Co-Authored-By: Claude <noreply@anthropic.com>",
            "private.md without the docs",
        ] {
            assert!(!leaks(&all, clean), "{clean}");
        }
        assert!(!leaks(&matcher(Scope::Lists), "the zzbox box"));
        assert!(leaks(&matcher(Scope::Lists), "a ZZPERSON"));
    }

    /// A term is a word of the text however it is spelled: with an accent, written or left out,
    /// a full-width letter, an invisible format character (a zero-width space, a soft hyphen, a
    /// joiner) inside it, or any whitespace, a line break included, between its words. It is
    /// still a whole word.
    #[test]
    fn a_spelling_hides_no_term() {
        let lists = matcher(Scope::Lists);
        for spelled in [
            "a zzpérson",
            "ZZPERSÖN",
            "ｚｚperson",
            "zz\u{200B}person",
            "zz\u{00AD}person",
            "ZZ\u{200D}PERSON",
            "\u{FEFF}zzperson",
            "a zz two wörds term",
            "a zz\ttwo\u{00A0}words term",
            "a wrapped zz\ntwo words",
            "a wrapped zz two\n   // words term",
            "a quoted zz two\n> words",
        ] {
            assert!(leaks(&lists, spelled), "{spelled:?}");
        }
        assert!(
            leaks(&lists, "zz two\n\nwords"),
            "a blank line is whitespace too"
        );
        for clean in ["zz\u{200B}persona", "zz two\nwordsmith"] {
            assert!(!leaks(&lists, clean), "{clean:?}: still a whole word");
        }
        let accented = Matcher::new(&["Zzçàse".to_owned()], &[], &[], Scope::Lists);
        assert!(leaks(&accented, "the zzcase file"));
        assert!(!leaks(&accented, "the zzcases file"));
    }

    /// Each hit names its layer, the least where a line breaks two rules, and never what it
    /// matched.
    #[test]
    fn a_hit_names_its_layer() {
        let all = matcher(Scope::Whole);
        for (text, layer) in [
            ("a zzperson", Layer::Term),
            ("the zzbox box", Layer::Name),
            ("see docs/private.md", Layer::Link),
            (concat!("/ho", "me/zzother/x"), Layer::Home),
            (concat!("peer.zznet", ".ts.net"), Layer::Tailnet),
            (concat!("zz@realm", ".example-mail.com"), Layer::Address),
            (concat!("100.", "101.102.103"), Layer::Node),
            (concat!("(415) 867", "-5309"), Layer::Phone),
            (
                concat!("zzperson at zz@realm", ".example-mail.com"),
                Layer::Term,
            ),
        ] {
            assert_eq!(all.hits(text), BTreeMap::from([(1, layer)]), "{text}");
        }
        assert_eq!(Layer::Term.to_string(), "names a term of the lists");
    }

    /// A whole word is a whole word in any script, and a tatweel stretching a term hides it no
    /// more than an accent does.
    #[test]
    fn a_word_is_whole_in_any_script() {
        let greek = Matcher::new(&["ζζλ".to_owned()], &[], &[], Scope::Lists);
        assert!(leaks(&greek, "a note on ζζλ, here"));
        assert!(!leaks(&greek, "ζζλα is another word"));
        let arabic = Matcher::new(&["زز".to_owned()], &[], &[], Scope::Lists);
        assert!(leaks(&arabic, "a note on ز\u{0640}ز here"));
    }

    /// A match is named by the line it starts on, each line once.
    #[test]
    fn a_term_is_found_at_the_line_it_starts_on() {
        let text =
            "nothing\na wrapped zz two\n// words term\nzz two words alone\nzzperson, zzperson";
        assert_eq!(
            matcher(Scope::Lists)
                .hits(text)
                .into_keys()
                .collect::<Vec<_>>(),
            [2, 4, 5]
        );
    }

    /// Nothing is decoded: what a line holds encoded is not its words.
    #[test]
    fn an_encoding_is_not_read() {
        let all = matcher(Scope::Whole);
        // Built here, so this file spells nothing with escapes itself.
        for encoded in [
            "eyBhIG5vdGUgYWJvdXQgenpwZXJzb24gfQ==".to_owned(),
            ["zzpers=", "6Fn"].concat(),
            ["zzpers%", "6Fn"].concat(),
            ["zzpers\\", "u006fn"].concat(),
            ["zzpers&#", "111;n"].concat(),
        ] {
            assert!(!leaks(&all, &encoded), "{encoded}");
        }
    }

    /// A tailnet node's address and a telephone number are leaks; the tailnet's resolver, its
    /// range's name, a fictional number, a digest, a version and a date are not.
    #[test]
    fn a_tailnet_address_and_a_phone_number_are_found() {
        let all = matcher(Scope::Whole);
        for leak in [
            concat!("host = \"100.", "101.102.103\""),
            concat!("peer 100.", "127.0.9:443"),
            concat!("call +961 ", "3 123 456"),
            concat!("tel: +1 (415) ", "867-5309"),
            concat!("(415) 867", "-5309"),
            concat!("415.867", ".5309"),
        ] {
            assert!(leaks(&all, leak), "{leak}");
        }
        for clean in [
            "resolver 100.100.100.100 and range 100.64.0.0/10",
            "100.128.0.1 and 10.0.0.1 are not the tailnet's, and 100.64.0.9 is the cast's",
            "+1 617 555 1234 is fictional too",
            "tel:+15550100 and +1 555 0100 leave the area code out",
            "+1 2 3 is too short to be a number",
            "(555) 555-0100 is fictional",
            "a digest 3fa85f6457174562b3fc2c963f66afa6 and v1.2.3.4",
            "2026-10-09T12:30:00Z, line 1234567890",
            "+1 day",
        ] {
            assert!(!leaks(&all, clean), "{clean}");
        }
    }

    /// A file is opaque when it is not text or, over the whole file, holds what a reader would
    /// decode: however an encoding is cut up or escaped, it is not read as words. Every sample is
    /// built here, so this file holds none of them.
    #[test]
    fn whatever_cuts_an_encoding_up_it_is_opaque() {
        let b64 = |n: usize| "QmFzZTY0IGhpZGVzIHdvcmRz".repeat(n);
        let cut = |text: &str, width: usize, between: &str| {
            text.as_bytes()
                .chunks(width)
                .map(|piece| String::from_utf8_lossy(piece).into_owned())
                .collect::<Vec<_>>()
                .join(between)
        };
        let hex = "9f86d081884c7d659a2feaa0c55ad015".repeat(7);
        let spelled = hex::encode("a note about nobody in particular, said in hex. ".repeat(4));
        let pairs = |between: &str| cut(&hex[..64], 2, between);
        let escapes = |one: &str| one.repeat(20);
        let quoted = ["zzpers", "=6F", "n"].concat();
        for (text, opaque) in [
            (format!("payload: {}\n", &b64(9)[..199]), Opaque::Encoded),
            (cut(&b64(12), 39, "\n"), Opaque::Encoded),
            (cut(&b64(12), 76, "\n"), Opaque::Encoded),
            (
                format!("{{\"body\": \"{}\"}}", cut(&b64(12), 76, r"\r\n")),
                Opaque::Encoded,
            ),
            (
                format!(
                    "const B: &str = \"{}\";",
                    cut(&b64(12), 76, "\\r\\n\\\n    ")
                ),
                Opaque::Encoded,
            ),
            (
                format!("[\"{}\"]", cut(&b64(14), 150, "\",\n\"")),
                Opaque::Encoded,
            ),
            (
                format!("src: data:image/png;base64,{}", cut(&b64(9), 60, "\n  ")),
                Opaque::Encoded,
            ),
            (cut(&spelled, 60, "\n"), Opaque::Encoded),
            (
                format!("concat!(\n    \"{}\"\n)", cut(&spelled, 64, "\",\n    \"")),
                Opaque::Encoded,
            ),
            (cut(&spelled[1..], 60, "\n"), Opaque::Encoded),
            (
                format!("narHash = \"sha256-{}\"\n", b64(9)),
                Opaque::Encoded,
            ),
            (pairs(":"), Opaque::Encoded),
            (pairs(" "), Opaque::Encoded),
            (escapes(&["%", "41"].concat()), Opaque::Escaped),
            (escapes(&["&#", "65;"].concat()), Opaque::Escaped),
            (escapes(&["\\", "u0041"].concat()), Opaque::Escaped),
            (escapes(&["\\", "x41"].concat()), Opaque::Escaped),
            (format!("a note about {quoted}\n"), Opaque::Escaped),
            (["caf=C3", "=A9 au lait\n"].concat(), Opaque::Escaped),
        ] {
            assert_eq!(Opaque::of(text.as_bytes()), Some(opaque), "{text}");
        }
        assert_eq!(Opaque::of(b"a\0b"), Some(Opaque::Binary));
        assert_eq!(Opaque::of(&[0x66, 0x6f, 0xff]), Some(Opaque::Binary));
    }

    /// An integrity hash is exempt only as exactly itself: what follows one in the same token (a
    /// payload, after a `-` too, compressed bytes, UTF-16 text) is counted as any token is.
    #[test]
    fn a_hash_hides_nothing_past_its_length() {
        use base64::Engine as _;
        use sha2::Digest as _;
        let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let random = |n: u32| -> Vec<u8> {
            (0..n)
                .flat_map(|i| sha2::Sha256::digest(i.to_le_bytes()).to_vec())
                .collect()
        };
        let digest = b64(&random(1));
        let text = "a note about nobody in particular, written plainly. ".repeat(6);
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        for line in [
            format!("integrity: sha256-{digest}{}", b64(text.as_bytes())),
            format!("integrity: sha256-{digest}-{}", b64(text.as_bytes())),
            format!("integrity: sha512-{}{}", b64(&random(2)), b64(&random(12))),
            format!("integrity: sha256-{}", b64(&utf16)),
        ] {
            assert_eq!(Opaque::of(line.as_bytes()), Some(Opaque::Encoded), "{line}");
        }
        assert_eq!(
            Opaque::of(
                format!("integrity: sha256-{digest}\n")
                    .repeat(40)
                    .as_bytes()
            ),
            None,
            "an exact hash of random bytes is no encoding"
        );
    }

    /// A sealed slot's box is ciphertext, recognised only in its exact shape: the box of a whole
    /// `Locked(key='…', box='…')` call, 256·k + 16 bytes that are no text. A box of another
    /// length, a textual one, one outside a call, or a payload glued after the call is counted.
    #[test]
    fn a_sealed_box_is_ciphertext_in_its_shape_alone() {
        use base64::Engine as _;
        use sha2::Digest as _;
        let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let random = |n: usize| -> Vec<u8> {
            (0u32..)
                .flat_map(|i| sha2::Sha256::digest(i.to_le_bytes()).to_vec())
                .take(n)
                .collect()
        };
        let key = hex::encode(sha2::Sha256::digest(b"a key"));
        let locked = |sealed: &str| format!("Locked(key='{key}', box='{sealed}')");
        let scrubbed =
            |sealed: &str| format!("Scrubbed(key='{key}', box='{sealed}', rules='{key}')");
        let ciphertext = b64(&random(256 * 2 + 16));
        let object = format!(
            "Change(genesis='{key}', deps=(), event=Opened(words=({},)))",
            locked(&ciphertext)
        );
        let rescrubbed = format!(
            "Change(genesis='{key}', deps=(), event=Opened(words=({},)))",
            scrubbed(&ciphertext)
        );
        for clean in [
            object.clone(),
            rescrubbed.clone(),
            format!("      \"text\": \"{object}\",\n").repeat(4),
        ] {
            assert_eq!(Opaque::of(clean.as_bytes()), None, "{clean}");
        }
        let words = "a note about nobody in particular. ".repeat(16);
        for hit in [
            locked(&b64(&random(256 + 17))),
            locked(&b64(&words.as_bytes()[..256 + 16])),
            format!("box='{ciphertext}'"),
            format!("{}{}", locked(&ciphertext), b64(&random(200))),
            scrubbed(&b64(&words.as_bytes()[..256 + 16])),
            format!("Scrubbed(key='{key}', box='{ciphertext}')"),
        ] {
            assert_eq!(Opaque::of(hit.as_bytes()), Some(Opaque::Encoded), "{hit}");
        }
    }

    /// The scanner's copy hides a recognised slot and nothing else: a key before or after it on
    /// its line stays, and a near miss (a box of another length, an upper-case key name, a
    /// textual box, a call whose name only ends in `Locked`) is left as written, for the scanner
    /// to read.
    #[test]
    fn a_placeholder_hides_the_recognised_slot_alone() {
        use base64::Engine as _;
        use sha2::Digest as _;
        let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let random = |n: usize| -> Vec<u8> {
            (0u32..)
                .flat_map(|i| sha2::Sha256::digest(i.to_le_bytes()).to_vec())
                .take(n)
                .collect()
        };
        let key = hex::encode(sha2::Sha256::digest(b"a key"));
        let slot = |key: &str, sealed: &str| format!("Locked(key='{key}', box='{sealed}')");
        let good = slot(&key, &b64(&random(256 + 16)));
        let line = format!("before='x', words=({good},), after='y'\nnext\n");
        assert_eq!(
            placeheld(&line),
            format!("before='x', words=({PLACEHOLDER},), after='y'\nnext\n")
        );
        let words = "a note about nobody in particular. ".repeat(16);
        for near in [
            slot(&key, &b64(&random(256 + 17))),
            slot(&key.to_uppercase(), &b64(&random(256 + 16))),
            slot(&key, &b64(&words.as_bytes()[..256 + 16])),
            format!("Not{}", slot(&key, &b64(&random(256 + 16)))),
        ] {
            assert_eq!(placeheld(&near), near, "{near}");
        }
    }

    /// What a reader reads as words is no encoding: digests, a placeholder, integrity hashes of
    /// random bytes, identifiers, paths, values and control escapes.
    #[test]
    fn what_reads_as_words_is_not_opaque() {
        let b64 = |n: usize| "QmFzZTY0IGhpZGVzIHdvcmRz".repeat(n);
        let hex = "9f86d081884c7d659a2feaa0c55ad015".repeat(7);
        let escapes = |one: &str| one.repeat(20);
        let digests = |prefix: &str, encode: fn(&[u8]) -> String| {
            use sha2::Digest as _;
            (0..40u32)
                .map(|i| {
                    format!(
                        "{prefix}{}\n",
                        encode(&sha2::Sha256::digest(i.to_le_bytes()))
                    )
                })
                .collect::<Vec<_>>()
                .concat()
        };
        let b64_of = |bytes: &[u8]| {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(bytes)
        };
        let words = (0..80)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        for text in [
            format!("sha256 {}\n", &hex[..64]),
            "checksum = \"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08\"\n"
                .repeat(40),
            words,
            format!("{}\n{}\n", "=".repeat(300), "-".repeat(300)),
            "at=2026-09-28T10:00 and x=FF\n".to_owned(),
            format!("{}\n", b64(6)),
            "zoë, naïve, a MAC 00:1a:2b:3c:4d:5e\n".to_owned(),
            format!("{hex}\n"),
            format!("placeholder {}\n", "2".repeat(64)),
            format!("salt {}\n", "07".repeat(16)),
            digests("checksum ", |bytes| hex::encode(bytes)),
            digests("\"narHash\": \"sha256-", b64_of),
            digests("golang.org/x/mod v0.1.0 h1:", b64_of),
            "fn public_tree_guard_finds_each_leak_and_never_prints_it()\n".to_owned(),
            "see docs/WEBALGEBRA-PLAN-2026-09-16 and /fonts/CrimsonPro-200-900\n".repeat(20),
            "RRULE:FREQ=DAILY;COUNT=10\n".repeat(20),
            "SOURCE_DATE_EPOCH=315532800 and https://x.example/?uid=42&rev=54\n".repeat(20),
            escapes(&["\\", "x1b[0m"].concat()),
            escapes(&["%", "20"].concat()),
        ] {
            assert_eq!(Opaque::of(text.as_bytes()), None, "{text}");
        }
    }

    /// A manifest admits exactly the bytes it names at the path it names, and a line that is not
    /// `<digest>  <path>` is refused, never skipped.
    #[test]
    fn a_manifest_admits_its_bytes_at_its_path_alone() {
        use sha2::Digest as _;
        let digest = hex::encode(sha2::Sha256::digest(b"a picture"));
        let admitted =
            Admitted::parse(&format!("# the manifest\n\n{digest}  docs/a.png\n")).expect("parses");
        assert!(admitted.holds("docs/a.png", b"a picture"));
        assert!(!admitted.holds("docs/b.png", b"a picture"), "another path");
        assert!(
            !admitted.holds("docs/a.png", b"a picture, changed"),
            "other bytes"
        );
        for malformed in [
            format!("{digest} docs/a.png"),
            format!("{}  docs/a.png", digest.to_uppercase()),
            format!("{}  docs/a.png", &digest[1..]),
            format!("{digest}  "),
        ] {
            assert_eq!(
                Admitted::parse(&format!("# ok\n{malformed}\n")),
                Err(Malformed(2)),
                "{malformed}"
            );
        }
    }

    /// A file holding a vouched-for finding is admitted by its bytes, as an opaque one is; a
    /// fingerprint that is not `<path>:<rule>:<line>` is refused.
    #[test]
    fn a_vouched_finding_is_admitted_by_its_bytes() {
        let findings =
            Findings::parse("# vouched\n\nfixtures/a:b.pem:private-key:1\n").expect("parses");
        assert_eq!(
            Vouch::of("fixtures/a:b.pem", Opaque::of(b"text"), &findings),
            Some(Vouch::Finding)
        );
        assert_eq!(
            Vouch::of("elsewhere.pem", Opaque::of(b"text"), &findings),
            None
        );
        assert_eq!(
            Vouch::of("elsewhere.pem", Opaque::of(b"\0"), &findings),
            Some(Vouch::Opaque(Opaque::Binary))
        );
        for malformed in [
            "key.pem:private-key",
            "key.pem:private-key:one",
            ":private-key:1",
            "  key.pem:private-key:1",
            "key.pem:private-key:1 ",
            "   ",
        ] {
            assert_eq!(Findings::parse(malformed), Err(Malformed(1)), "{malformed}");
        }
    }

    /// Words of a text, joined by any whitespace.
    fn spaced() -> impl Strategy<Value = (Vec<String>, Vec<String>)> {
        prop::collection::vec("[a-z]{1,8}", 1..6).prop_flat_map(|words| {
            let gaps = prop::collection::vec("[ \t\n\u{a0}\u{3000}]{1,3}", words.len());
            (Just(words), gaps)
        })
    }

    fn joined(words: &[String], gaps: &[String]) -> String {
        words
            .iter()
            .zip(gaps)
            .flat_map(|(word, gap)| [word.as_str(), gap.as_str()])
            .collect()
    }

    proptest! {
        /// THE LAW OF WHITESPACE: a text's words are the same whatever whitespace stands between
        /// them, so a term is found however a text wraps or spaces it.
        #[test]
        fn words_are_the_same_whatever_the_whitespace((words, gaps) in spaced(), others in prop::collection::vec("[ \t\n]{1,3}", 6)) {
            let once = Words::of(&joined(&words, &gaps));
            let again = Words::of(&joined(&words, &others[..words.len()]));
            prop_assert_eq!(&once.text, &again.text);
            prop_assert_eq!(once.text, words.join(" "));
        }

        /// A term written into a text, between any words and with any whitespace, is found at
        /// the line it starts on.
        #[test]
        fn a_term_written_in_is_found((words, gaps) in spaced(), at in 0usize..6) {
            let at = at % (words.len() + 1);
            let mut written = words.clone();
            written.insert(at, "zz two words".to_owned());
            let mut spaces = gaps.clone();
            spaces.insert(at, "\n".to_owned());
            let text = joined(&written, &spaces);
            let line = 1 + joined(&written[..at], &spaces[..at]).matches('\n').count();
            prop_assert!(matcher(Scope::Lists).hits(&text).contains_key(&line), "{:?}", text);
        }

        /// A manifest printed from files admits each of them, and no other bytes at their paths.
        #[test]
        fn a_printed_manifest_admits_what_it_was_printed_from(
            files in prop::collection::btree_map("[a-z]{1,8}(/[a-z]{1,8}){0,2}", prop::collection::vec(any::<u8>(), 0..64), 0..8)
        ) {
            use sha2::Digest as _;
            let manifest = files
                .iter()
                .map(|(path, bytes)| format!("{}  {path}", hex::encode(sha2::Sha256::digest(bytes))))
                .collect::<Vec<_>>()
                .join("\n");
            let admitted = Admitted::parse(&manifest).expect("a printed manifest parses");
            for (path, bytes) in &files {
                prop_assert!(admitted.holds(path, bytes));
                let mut other = bytes.clone();
                other.push(0);
                prop_assert!(!admitted.holds(path, &other));
            }
        }

        /// Folding is idempotent: a folded text folds to itself, so a term and a text folded
        /// alike meet.
        #[test]
        fn fold_is_idempotent(text in any::<String>()) {
            prop_assert_eq!(fold(&fold(&text)), fold(&text));
        }
    }
}
