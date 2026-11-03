//! What every page asks of the tab and what the tab hands back, the same for each page: the
//! effects a page's `update` answers, as data the host (`web/src/pages/program.ts`) performs, and
//! the inputs it answers with. A page names each of its questions by its own `Ask`, which the host
//! hands back untouched, and reads each answer with serde, once, where it lands.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use viewtree::kit::Reading;

use crate::zone::Offset;

/// Whether this login's acts are offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    /// It writes: each act is a button.
    Writes,
    /// It only reads: an act is not offered.
    Reads,
}

/// What every page reads of the tab: whether its login writes, and the owner's zone.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct Context {
    /// Whether the acts are offered.
    pub gate: Gate,
    /// The owner's zone as offsets from UTC, oldest first, each from the instant it starts.
    pub offsets: Vec<Offset>,
}

/// What a page asks the tab to do. `H` names the page's held questions and `A` its one-time
/// asks and acts, each handed back with its answer, so an answer to a question of the other kind
/// is not a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Effect<H, A> {
    /// Hold the box's answer to a GET of `path`, as the tab's register of that question holds it:
    /// handed over now, and again whenever it moves (an answer lands, the box moves, it is asked
    /// again), as [`Input::Held`]. A refusal (a 4xx) is the box's answer and stands as one; no
    /// answer (the network, a 5xx) is the register's failure, and leaves the answer that stood.
    Hold {
        /// Which question.
        ask: H,
        /// The box's path.
        path: String,
    },
    /// Ask a held question again (a Retry).
    Again {
        /// The box's path, as `Hold` named it.
        path: String,
    },
    /// One GET of `path`, answered once as [`Input::Answered`].
    Get {
        /// Which question.
        ask: A,
        /// The box's path.
        path: String,
    },
    /// An act: one POST of `{}` to `path`, answered once as [`Input::Answered`].
    Post {
        /// Which act.
        ask: A,
        /// The box's path.
        path: String,
    },
    /// Go to a place in the app.
    Go {
        /// Its address, `#/…`.
        href: String,
    },
    /// Go back, or to `fallback` when the app has nowhere to go back to.
    Back {
        /// The address, `#/…`.
        fallback: String,
    },
    /// Say something briefly.
    Toast {
        /// What.
        words: String,
    },
}

/// No question: the `A` of a page that asks nothing once, so it has no answer to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Never {}

/// Whether what a held question shows may be behind the box (the tab's `reading.Behind`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
pub struct Behind {
    /// Why the last ask failed, or `None` while one is long on its way.
    pub why: Option<String>,
    /// When what is shown was read, in epoch milliseconds.
    #[ts(type = "number")]
    pub since: i64,
}

/// What a GET or a POST came to.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Answer {
    /// The box answered this.
    Done {
        /// Its answer, read by the page that asked.
        #[ts(type = "unknown")]
        body: serde_json::Value,
    },
    /// The box refused it, or no answer came.
    Refused {
        /// The HTTP status, 0 when no answer came.
        status: u16,
        /// The box's words, or why none came.
        words: String,
    },
}

/// What the tab hands a page.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Input<H, A> {
    /// The login or the zone changed.
    Context(Context),
    /// A held question, as its register now stands.
    Held {
        /// Which.
        ask: H,
        /// What it holds.
        reading: Reading<Answer>,
        /// Whether that may be behind.
        behind: Option<Behind>,
    },
    /// A GET or a POST answered.
    Answered {
        /// Which.
        ask: A,
        /// What it came to.
        answer: Answer,
    },
}

/// A held answer read as the page's type: a refusal, or an answer of another shape, is an error
/// in its words.
#[must_use]
pub fn read<T: DeserializeOwned>(r: Reading<Answer>) -> Reading<T> {
    match r {
        Reading::Loading => Reading::Loading,
        Reading::Error { e } => Reading::Error { e },
        Reading::Val { v } => match answered(v) {
            Ok(v) => Reading::Val { v },
            Err(e) => Reading::Error { e },
        },
    }
}

/// An answer read as the page's type, or why not.
///
/// # Errors
/// The box refused it, or answered in another shape.
pub fn answered<T: DeserializeOwned>(a: Answer) -> Result<T, String> {
    match a {
        Answer::Done { body } => serde_json::from_value(body)
            .map_err(|e| format!("the box answered in a shape this page does not read ({e})")),
        Answer::Refused { words, .. } => Err(words),
    }
}

/// `s` as `encodeURIComponent` spells it, so a path or an address the page prints reads back as
/// the TypeScript's did.
#[must_use]
pub fn component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xf)]));
        }
    }
    out
}

/// `?name=value&…` of the fields that are not empty, each value encoded; `""` for none.
#[must_use]
pub fn search(fields: &[(&str, &str)]) -> String {
    let pairs: Vec<String> = fields
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(n, v)| format!("{n}={}", component(v)))
        .collect();
    if pairs.is_empty() {
        String::new()
    } else {
        format!("?{}", pairs.join("&"))
    }
}

/// A held question as it stands: what it holds, and whether that may be behind.
#[derive(Debug, Clone)]
pub struct Held<T> {
    /// What it holds.
    pub reading: Reading<T>,
    /// Whether that may be behind.
    pub behind: Option<Behind>,
}

impl<T> Held<T> {
    /// Nothing held yet.
    #[must_use]
    pub const fn loading() -> Self {
        Held {
            reading: Reading::Loading,
            behind: None,
        }
    }
}

impl<T: DeserializeOwned> Held<T> {
    /// What the host handed over, read as the page's type.
    #[must_use]
    pub fn of(reading: Reading<Answer>, behind: Option<Behind>) -> Self {
        Held {
            reading: read(reading),
            behind,
        }
    }
}
