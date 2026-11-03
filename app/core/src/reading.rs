//! What a page reads, as the typed values its templates draw (`rendering::TODO`,
//! `rendering::CALENDAR`).
//!
//! Every datum is a string in the value, read by the template as data, so it is text whatever it
//! spells; the one exception is a trusted record's markup, evaluated in place (see [`Prose`]).
//! The HTML carries classes and no colour, so one render serves both themes.

use std::cmp::Ordering;
use std::num::NonZeroU32;

use crate::zone::Zone;
use chrono::{NaiveDateTime, NaiveTime};
use prodrome_core::fpl::{Delta, Explanation, Instant, Note, Scalar, total_seconds};
use prodrome_core::observe::{Observation, Rounding};
use prodrome_core::term::{CurvePoint, TermF};
use schemars::JsonSchema;
use serde::Serialize;

use crate::escape::Escaped;
use crate::wire;

/// What a page shows of a fulfillment: tenths of a percent, to the nearest. A page is printed from
/// this observation of every price on it, so its bytes, and the memo that keys a page by them,
/// move only where the observed value does, whatever the clock says between.
pub const SHOWN: Observation = Observation::new(
    match NonZeroU32::new(1000) {
        Some(levels) => levels,
        None => unreachable!(),
    },
    Rounding::Nearest,
);

/// A fulfillment as a page prints one: [`SHOWN`], "62.3%".
fn pct(value: f64) -> String {
    let step = SHOWN.observe(Some(value)).unwrap_or_default();
    format!("{}.{}%", step / 10, step % 10)
}

/// A node's reading: its number, or `∅` where it has none, never a number standing in.
fn reads(value: Option<f64>) -> String {
    value.map_or_else(|| "∅".to_owned(), pct)
}

// --- Prose: a record's field, rendered or shown as its source ------------------

/// One field of a record, as a page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Prose {
    /// A trusted actor's markup that compiles on its own: evaluated in place,
    /// so it reads exactly as its own rendering does.
    Rendered {
        /// The markup.
        source: Escaped,
    },
    /// Anything else, shown as its source text: an untrusted actor's (rendered means
    /// confirmed), or markup Typst refused.
    Source {
        /// The source, as text.
        text: String,
    },
}

// --- Why this number -----------------------------------------------------------

/// The core's explanation of one price, read by its closed shape, so a kind the core gains
/// fails a match here rather than printing its fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Explained(Explanation);

impl Explained {
    /// The explanation, to be read.
    #[must_use]
    pub fn of(explanation: Explanation) -> Self {
        Self(explanation)
    }

    /// The first recurrence's last pass, depth first: "tended Sep 13, 12 days
    /// ago", "not tended yet", or `None` for a price with no recurrence.
    #[must_use]
    pub fn pass(&self, zone: Zone) -> Option<String> {
        pass(zone, &self.0)
    }
}

fn pass(z: Zone, node: &Explanation) -> Option<String> {
    match &*node.node {
        TermF::Recur { .. } => Some(tended(z, node)),
        _ => parts(node).into_iter().find_map(|(_, kid)| pass(z, kid)),
    }
}

/// A note the core wrote beside a node, by the key it wrote it under.
fn note<'a>(node: &'a Explanation, key: &str) -> Option<&'a Scalar> {
    match node.notes.get(key)? {
        Note::One(scalar) => Some(scalar),
        Note::Many(_) | Note::Maps(_) => None,
    }
}

fn number(node: &Explanation, key: &str) -> Option<f64> {
    match note(node, key)? {
        Scalar::Float(x) => Some(*x),
        #[allow(clippy::cast_precision_loss)]
        Scalar::Int(n) => Some(*n as f64),
        Scalar::Text(_) | Scalar::Bool(_) => None,
    }
}

/// An instant the core noted, which it prints as `fpl::iso`; "" is none.
fn instant(node: &Explanation, key: &str) -> Option<Instant> {
    match note(node, key)? {
        Scalar::Text(t) => NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.f").ok(),
        Scalar::Float(_) | Scalar::Int(_) | Scalar::Bool(_) => None,
    }
}

fn shares(node: &Explanation) -> Vec<f64> {
    match node.notes.get("shares") {
        Some(Note::Many(xs)) => xs
            .iter()
            .filter_map(|x| match x {
                Scalar::Float(x) => Some(*x),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A parameter in [0, 1] as a reader says it: "60%", "12.5%".
fn share(x: f64) -> String {
    let s = format!("{:.1}", x * 100.0);
    format!("{}%", s.strip_suffix(".0").unwrap_or(&s))
}

/// A plain number, "−4", "1.5".
fn plain(x: f64) -> String {
    let s = format!("{:.2}", x.abs());
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if x < 0.0 {
        format!("−{s}")
    } else {
        s.to_owned()
    }
}

/// An instant as a reader says it: "Sep 30, 02:57", or "Sep 30" at midnight.
/// In UTC, the zone every instant on the chain is in.
fn when(z: Zone, t: Instant) -> String {
    let t = z.wall(t);
    if t.time() == NaiveTime::MIN {
        t.format("%b %-d").to_string()
    } else {
        t.format("%b %-d, %H:%M").to_string()
    }
}

/// A deadline as a reader says it: one reached at the owner's midnight is the end of the day
/// before, the day it is due.
fn due(z: Zone, t: Instant) -> String {
    let wall = z.wall(t);
    match wall.date().pred_opt() {
        Some(day) if wall.time() == NaiveTime::MIN => {
            format!("the end of {}", day.format("%b %-d"))
        }
        _ => when(z, t),
    }
}

fn counted(n: f64, one: &str) -> String {
    let s = plain(n);
    if s == "1" {
        format!("1 {one}")
    } else {
        format!("{s} {one}s")
    }
}

/// A span as a reader says it: "40 minutes", "12 hours", "14 days".
fn span(d: Delta) -> String {
    let hours = total_seconds(d).abs() / 3600.0;
    if hours < 1.0 {
        counted((hours * 60.0).round(), "minute")
    } else if hours < 48.0 {
        counted(hours.round(), "hour")
    } else {
        counted((hours / 24.0 * 10.0).round() / 10.0, "day")
    }
}

fn tended(z: Zone, node: &Explanation) -> String {
    match (instant(node, "tended"), number(node, "agoHours")) {
        (Some(at), Some(ago)) => {
            let days = (ago / 24.0).floor();
            let ago = if days < 1.0 {
                "today".to_owned()
            } else {
                format!("{} ago", counted(days, "day"))
            };
            format!("tended {}, {ago}", z.wall(at).format("%b %-d"))
        }
        _ => "not tended yet".to_owned(),
    }
}

/// Each subterm under the words its slot gives it; "" where the slot is the
/// node's only one and the sentence already says what it is.
fn parts(node: &Explanation) -> Vec<(String, &Explanation)> {
    let one = |kid| vec![(String::new(), kid)];
    match &*node.node {
        TermF::Flat { .. }
        | TermF::Decay { .. }
        | TermF::Curve { .. }
        | TermF::Ref { .. }
        | TermF::Absent => Vec::new(),
        TermF::Conj { terms, .. } => {
            let shares = shares(node);
            terms
                .iter()
                .enumerate()
                .map(|(i, kid)| {
                    let pull = shares
                        .get(i)
                        .map(|s| format!(", {} of the pull", share(*s)))
                        .unwrap_or_default();
                    (format!("Part {}{pull}: ", i + 1), kid)
                })
                .collect()
        }
        TermF::Least { terms } => terms
            .iter()
            .enumerate()
            .map(|(i, kid)| (format!("Version {}: ", i + 1), kid))
            .collect(),
        TermF::Offset { term, .. }
        | TermF::Importance { term, .. }
        | TermF::Shift { term, .. }
        | TermF::Within { term, .. }
        | TermF::Periodic { term, .. } => one(term),
        TermF::Piecewise(schedule) => one(schedule.head()),
        TermF::Gate { gate, body } => vec![
            ("The gate: ".to_owned(), gate),
            ("The body: ".to_owned(), body),
        ],
        TermF::OffsetBy { delta, term } => vec![
            ("Moved by: ".to_owned(), delta),
            ("The part moved: ".to_owned(), term),
        ],
        TermF::After { term, pending, .. } => vec![
            ("Once it is done: ".to_owned(), term),
            ("Until then: ".to_owned(), pending),
        ],
        TermF::Recur { term, pending, .. } => vec![
            ("Since the last pass: ".to_owned(), term),
            ("Before the first pass: ".to_owned(), pending),
        ],
    }
}

/// Which of a todo's specs is in force: "the spec revised on Sep 16 (2
/// revisions in all)", or "the spec as first written".
fn spec(z: Zone, node: &Explanation) -> String {
    instant(node, "since").map_or_else(
        || "the spec as first written".to_owned(),
        |t| {
            format!(
                "the spec revised on {} ({} in all)",
                when(z, t),
                counted(number(node, "pieces").unwrap_or(0.0), "revision")
            )
        },
    )
}

/// A drawn line's points: "80% on Sep 1, then 40% on Oct 1, 09:30 (filing)".
fn through(z: Zone, points: &[CurvePoint]) -> String {
    points
        .iter()
        .map(|pt| {
            let label = if pt.label.is_empty() {
                String::new()
            } else {
                format!(" ({})", pt.label)
            };
            format!("{} on {}{label}", share(pt.value), when(z, pt.at))
        })
        .collect::<Vec<_>>()
        .join(", then ")
}

/// What history says of the event an `After` waits on.
fn bound(node: &Explanation) -> &'static str {
    match note(node, "bound") {
        Some(Scalar::Text(t)) if t == "completed" => {
            "is done, so the follow-up runs from when it was"
        }
        Some(Scalar::Text(t)) if t == "cancelled" => "was cancelled, so this is moot",
        _ => "is not done yet",
    }
}

/// A conflict's sentence: `Least` is how the chain prices writes nobody has settled.
fn least(v: &str, versions: usize) -> String {
    format!(
        "{v}: the most urgent of {versions} versions written concurrently, until the next write \
         settles them."
    )
}

/// One node in one plain sentence, its own fulfillment first.
fn says(z: Zone, node: &Explanation) -> String {
    let v = reads(node.value);
    match &*node.node {
        TermF::Absent => format!("{v}: nothing prices it, so it has no number."),
        TermF::Flat { .. } => format!("{v}, flat: the same whatever the date."),
        TermF::Decay {
            start,
            end,
            end_date,
            lead_up,
            start_date,
        } => {
            let from = start_date
                .map(|t| format!("; 100% before {}", when(z, t)))
                .unwrap_or_default();
            format!(
                "{v} now, falling from {} to {} by {}, over {}{from}.",
                share(*start),
                share(*end),
                due(z, *end_date),
                span(*lead_up)
            )
        }
        TermF::Curve { points } => format!("{v} now, along a drawn line: {}.", through(z, points)),
        TermF::Least { terms } => least(&v, terms.len()),
        TermF::Conj { terms, p } => {
            let floor = number(node, "certifies")
                .map(|c| format!(", so none of them is below {}", share(c)))
                .unwrap_or_default();
            format!(
                "{v}: {} taken together at strictness {}, the lowest pulling hardest{floor}.",
                counted(
                    f64::from(u32::try_from(terms.len()).unwrap_or(u32::MAX)),
                    "part"
                ),
                plain(*p)
            )
        }
        TermF::Offset { delta, .. } => {
            let (way, to) = if *delta < 0.0 {
                ("lowered", "0%")
            } else {
                ("raised", "100%")
            };
            format!(
                "{v}: the part below, {way} {} of the way to {to}.",
                share(delta.abs())
            )
        }
        TermF::Gate { .. } => {
            format!("{v}: the body counts only as far as its gate is met.")
        }
        TermF::Shift { delta, .. } => {
            let at = match delta.cmp(&Delta::zero()) {
                Ordering::Greater => format!("as it will stand in {}", span(*delta)),
                Ordering::Less => format!("as it stood {} earlier", span(*delta)),
                Ordering::Equal => "as it stands now".to_owned(),
            };
            format!("{v}: the part below, {at}.")
        }
        TermF::Within { window, p, .. } => {
            let peak = instant(node, "peakAt")
                .map(|t| format!("; it pulls hardest on {}", when(z, t)))
                .unwrap_or_default();
            format!(
                "{v}: the part below across the next {}, at strictness {}{peak}.",
                span(*window),
                plain(*p)
            )
        }
        TermF::Importance { w, .. } => {
            let how = if *w > 1.0 {
                "sharpened toward urgent"
            } else if *w < 1.0 {
                "softened toward done"
            } else {
                "unchanged"
            };
            format!("{v}: the part below, weighted {}: {how}.", plain(*w))
        }
        TermF::After { event, needs, .. } => {
            let needs = needs
                .map(|n| format!(", and needs {} after it", span(n)))
                .unwrap_or_default();
            format!("{v}: waits on {event}, which {}{needs}.", bound(node))
        }
        TermF::Recur { .. } => format!("{v}: it recurs, {}.", tended(z, node)),
        TermF::Periodic { period, .. } => {
            let round = instant(node, "cycleStart")
                .map(|t| format!("; this round began {}", when(z, t)))
                .unwrap_or_default();
            format!("{v}: it repeats every {}{round}.", span(*period))
        }
        TermF::Piecewise { .. } => format!("{v}, by {}.", spec(z, node)),
        TermF::OffsetBy { .. } => {
            format!("{v}: one part, moved by what the other gives.")
        }
        TermF::Ref { store, entity } => {
            format!("{v}: the price of {}.", referred(store.as_ref(), entity))
        }
    }
}

/// What a reference names: a todo of this chain, or an entity of another store.
fn referred(store: Option<&String>, entity: &str) -> String {
    store.map_or_else(
        || format!("todo {entity}"),
        |store| format!("{entity} in {store}"),
    )
}

/// One part of a price and its own parts, each in one sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Part {
    /// Its label and its sentence.
    pub says: String,
    /// Its parts.
    pub parts: Vec<Part>,
}

fn part(z: Zone, label: &str, node: &Explanation) -> Part {
    Part {
        says: format!("{label}{}", says(z, node)),
        parts: parts(node)
            .iter()
            .map(|(label, kid)| part(z, label, kid))
            .collect(),
    }
}

/// "Why this number": the price in one sentence, then its parts in words, or the sentence for
/// a todo with none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Why {
    /// The todo has no number, and the sentence says why.
    Unpriced {
        /// Why.
        says: String,
    },
    /// The piece in force's sentence, which piece that is and the last pass, and its parts.
    Priced {
        /// The heading, with the price.
        heading: String,
        /// The sentence.
        says: String,
        /// Which spec prices it, and its last pass, a line each.
        passes: Vec<String>,
        /// Its parts.
        parts: Vec<Part>,
    },
}

fn why(z: Zone, tree: Option<&Explained>) -> Why {
    let Some(Explained(root)) = tree else {
        return Why::Unpriced {
            says: "This todo's price names a todo that cannot be read, so it has no number."
                .to_owned(),
        };
    };
    if matches!(*root.node, TermF::Absent) {
        return Why::Unpriced {
            says: "The chain records no priority spec for this todo.".to_owned(),
        };
    }
    let mut price = root;
    let mut revision = None;
    while let TermF::Piecewise(schedule) = &*price.node {
        revision.get_or_insert_with(|| spec(z, price));
        price = schedule.head();
    }
    Why::Priced {
        heading: format!("Why this number — {}", reads(root.value)),
        says: says(z, price),
        passes: revision
            .map(|revision| format!("Priced by {revision}."))
            .into_iter()
            .chain(pass(z, root).map(|pass| format!("Last pass: {pass}.")))
            .collect(),
        parts: parts(price)
            .iter()
            .map(|(label, kid)| part(z, label, kid))
            .collect(),
    }
}

// --- A todo's reading part -------------------------------------------------------

/// What a todo's content record gives its page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    /// The action line: the page's title.
    pub body: Prose,
    /// The detail, `None` when absent.
    pub detail: Option<Prose>,
}

/// Why the body reads as written, in one line under the header; a body typst drew needs no line.
fn render_note(body: &Prose, unconfirmed: bool) -> Option<String> {
    match (body, unconfirmed) {
        (Prose::Rendered { .. }, _) => None,
        (Prose::Source { .. }, true) => {
            Some("Shown the way it was written until you confirm it.".to_owned())
        }
        (Prose::Source { .. }, false) => {
            Some("Shown the way it was written: Typst could not draw it.".to_owned())
        }
    }
}

/// What a chip says of a todo, which its class names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChipKind {
    /// Its state while open, or a count.
    Plain,
    /// Its state once closed.
    Done,
    /// Its category.
    Cat,
    /// Its record's kind.
    Kind,
    /// Its provenance.
    Prov,
}

/// One chip of the header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Chip {
    /// Its words.
    pub label: String,
    /// What it says.
    pub kind: ChipKind,
}

fn chip(label: impl Into<String>, kind: ChipKind) -> Chip {
    Chip {
        label: label.into(),
        kind,
    }
}

/// The number at the head of a todo's page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Fulfil {
    /// Its fulfillment, as [`SHOWN`] prints it.
    Priced {
        /// "62.3%".
        shown: String,
    },
    /// Nothing prices it.
    Unpriced,
    /// Its price names a todo that cannot be read.
    Unlinked,
}

/// The header: the number, the body as the title, the id, the chips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Head {
    /// The number.
    pub fulfil: Fulfil,
    /// The body, where the page has the record.
    pub body: Option<Prose>,
    /// The todo's id.
    pub id: String,
    /// Its chips.
    pub chips: Vec<Chip>,
    /// Why the body reads as written, where it is not drawn.
    pub note: Option<String>,
}

fn head(entry: &wire::Entry, content: Option<&Content>) -> Head {
    let authored = entry.authored.as_ref();
    let closed = |word: &str| {
        let day = entry.at.get(..10).unwrap_or_default();
        chip(format!("{word} {day}").trim_end(), ChipKind::Done)
    };
    let state = match entry.state {
        wire::EntryState::Open => chip("open", ChipKind::Plain),
        wire::EntryState::Completed => closed("completed"),
        wire::EntryState::Cancelled => closed("cancelled"),
    };
    let subs = authored.map_or(0, |a| a.subtodos.len());
    let done = authored.map_or(0, |a| a.subtodos.iter().filter(|s| s.done).count());
    let category = authored.map_or("", |a| a.category.as_str());
    let chips = std::iter::once(state)
        .chain((!category.is_empty()).then(|| chip(category, ChipKind::Cat)))
        .chain(authored.map(|a| chip(a.block.name(), ChipKind::Kind)))
        .chain(
            entry
                .unconfirmed
                .then(|| chip("unconfirmed", ChipKind::Prov)),
        )
        .chain((subs > 0).then(|| chip(format!("{done} of {subs} done"), ChipKind::Plain)))
        .collect();
    Head {
        fulfil: match entry.value {
            Some(wire::Fulfillment::Number(v)) => Fulfil::Priced { shown: pct(v) },
            Some(wire::Fulfillment::Absent(_)) => Fulfil::Unpriced,
            None => Fulfil::Unlinked,
        },
        body: content.map(|c| c.body.clone()),
        id: entry.todo.clone(),
        chips,
        note: content.and_then(|c| render_note(&c.body, entry.unconfirmed)),
    }
}

/// A todo's reading part, one page: the header first (the page puts its acts under it), then
/// the detail and "Why this number".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TodoPage {
    /// The header.
    pub head: Head,
    /// The detail, where it has one.
    pub detail: Option<Prose>,
    /// Why this number.
    pub why: Why,
}

/// A todo's page.
#[must_use]
pub fn todo(
    entry: &wire::Entry,
    content: Option<&Content>,
    explained: Option<&Explained>,
    z: Zone,
) -> TodoPage {
    TodoPage {
        head: head(entry, content),
        detail: content.and_then(|c| c.detail.clone()),
        why: why(z, explained),
    }
}

// --- The calendar ----------------------------------------------------------------

/// The days of the calendar, one section each. Every title, place and name is data, so a
/// stranger's invite is text whatever it spells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CalendarPage {
    /// The days.
    pub days: Vec<CalendarDay>,
}

/// One day of the calendar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CalendarDay {
    /// "Monday 5 October".
    pub heading: String,
    /// What is on it.
    pub entries: Vec<CalendarRow>,
}

/// One entry of a day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CalendarRow {
    /// When, on the owner's clock.
    pub time: String,
    /// Its title, "" for none.
    pub title: String,
    /// Its place, where it has one.
    pub place: Option<String>,
    /// The calendar it is on.
    pub calendar: String,
    /// Its status, as its class.
    pub status: String,
    /// What the reader is told beside it: a status but confirmed, two syncs that disagree.
    pub notes: Vec<String>,
}

/// The calendar's page.
#[must_use]
pub fn calendar(days: &[crate::calendar::Day]) -> CalendarPage {
    CalendarPage {
        days: days
            .iter()
            .map(|day| CalendarDay {
                heading: day.date.format("%A %-d %B").to_string(),
                entries: day.entries.iter().map(calendar_row).collect(),
            })
            .collect(),
    }
}

fn calendar_row(entry: &crate::calendar::Entry) -> CalendarRow {
    let status = entry.content.status().as_str();
    let place = entry.content.place();
    CalendarRow {
        time: entry.time(),
        title: entry.content.title().to_owned(),
        place: (!place.is_empty()).then(|| place.to_owned()),
        calendar: entry.id.calendar().label().to_owned(),
        status: status.to_owned(),
        notes: (status != "confirmed")
            .then(|| status.to_owned())
            .into_iter()
            .chain(entry.conflicted.then(|| "two syncs disagree".to_owned()))
            .collect(),
    }
}

/// The same days as plain text, for a page that has no rendering yet.
#[must_use]
pub fn calendar_text(days: &[crate::calendar::Day]) -> String {
    days.iter()
        .map(|day| {
            let head = day.date.format("%A %-d %B").to_string();
            let lines: Vec<String> = day
                .entries
                .iter()
                .map(|entry| {
                    let place = if entry.content.place().is_empty() {
                        String::new()
                    } else {
                        format!(", {}", entry.content.place())
                    };
                    let status = match entry.content.status() {
                        crate::calendar::Status::Confirmed => String::new(),
                        other => format!(" ({})", other.as_str()),
                    };
                    format!(
                        "  {} {}{place}{status}",
                        entry.time(),
                        entry.content.title()
                    )
                })
                .collect();
            if lines.is_empty() {
                format!("{head}\n  nothing")
            } else {
                format!("{head}\n{}", lines.join("\n"))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Duration;
    use prodrome_core::fpl::{Env, explained, mk_decay};
    use proptest::prelude::*;

    fn at(s: &str) -> Instant {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").expect("an instant")
    }

    proptest! {
        /// Between an instant and its next change as [`SHOWN`] sees the price, a todo's page
        /// is the same value whatever the clock says, so the memo keyed by it hits; at an exact
        /// change it is another.
        #[test]
        fn a_page_is_the_same_value_until_its_observed_value_moves(
            start in 0.0f64..0.98,
            end in 0.0f64..0.9,
            days in 1i64..30,
            from in 0i64..40 * 24 * 60,
            within in 0.0f64..1.0,
        ) {
            let term = mk_decay(start, end, at("2026-09-30T00:00:00"), Duration::days(days), None)
                .expect("a decay");
            let closed = prodrome_core::fpl::Closed::of(term).expect("closed");
            let env = Env::new();
            let page = |t: Instant| {
                let why = Explained::of(explained(&closed, t, &env));
                let value = prodrome_core::fpl::fulfillment(&closed, t, &env);
                todo(
                    &wire::Entry {
                        value: Some(wire::Fulfillment::of_value(value)),
                        ..entry(wire::EntryState::Open, false)
                    },
                    None,
                    Some(&why),
                    Zone::UTC,
                )
            };
            let t0 = at("2026-09-01T00:00:00") + Duration::minutes(from);
            let next = prodrome_core::observe::next_change(&closed, t0, &env, SHOWN);
            let Some(changes) = next.at else {
                prop_assert_eq!(page(t0 + Duration::days(400)), page(t0));
                return Ok(());
            };
            let span = (changes - t0).num_microseconds().expect("a span");
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let t = t0 + Duration::microseconds((within * (span - 1) as f64) as i64);
            prop_assert_eq!(page(t), page(t0));
            prop_assert!(next.exact, "a decay is exact");
            prop_assert_ne!(page(changes), page(t0));
        }
    }

    fn entry(state: wire::EntryState, unconfirmed: bool) -> wire::Entry {
        wire::Entry {
            todo: "file-permit".to_owned(),
            state,
            states: vec![state],
            at: "2026-09-20T10:00:00".to_owned(),
            claimed: wire::ClaimedState::Agrees,
            value: Some(wire::Fulfillment::Number(0.623)),
            recurring: false,
            unconfirmed,
            conflicts: std::collections::BTreeMap::new(),
            authored: None,
            candidates: Vec::new(),
            stream: Vec::new(),
        }
    }
}
