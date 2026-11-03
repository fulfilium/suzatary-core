//! The chain as a static site of its attestable history: every todo's
//! lifecycle, each event beside its object hash and the objects it rests on,
//! and the objects themselves under `objects/<hash>.py` for a stranger to
//! rehash. It reads only the objects and `view::entries`, so it is evidence
//! about the stored format.
//!
//! Each page is a value ([`SitePage`]) its template draws, which a renderer compiles
//! to HTML; every string on it came out of mail an attacker controls, so it
//! enters the page only as data, drawn as text.
//! Same inputs, same pages: the one moment printed is the one passed in.

use std::collections::BTreeMap;

use crate::believed::one_outcome;
use crate::chain_set::ChainSet;
use crate::chains::ChainName;
use crate::record::{Authored, Entry, Envelope, TodoEvent};
use prodrome_core::event::{Hash, parents_of};
use prodrome_core::literal::{Datetime, ProdromeError, Value, print_literal};
use prodrome_core::policy::Untrusted;
use prodrome_core::term::TermF;
use prodrome_core::{fpl, view};

pub mod page;

use page::{Expr, SitePage, Tag, facts, hash, hashes, link, lit, rows, span, strong, tag};

/// The whole site: pages to compile, and the objects copied as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Files {
    /// `index.html`, `chain.html` and `todo/<id>.html`, as values to draw.
    pub pages: BTreeMap<String, SitePage>,
    /// `objects/<hash>.py`: the canonical literal, byte for byte what the store holds.
    pub objects: BTreeMap<String, String>,
    /// The line every page ends on, for a page printed after the fact.
    pub footer: String,
}

// --- What the pages say ----------------------------------------------------------

const PROVISIONAL: &str = "The triage agent reads attacker-controlled mail, so its writes are \
     PROVISIONAL by construction (CLAUDE.md, Containment): recorded in full and surfaced here for \
     confirmation, but inert to the lifecycle and pricing folds, so an injected mis-completion \
     cannot silently close or reprice a todo. A provisional event binds only when a trusted actor \
     re-appends it.";

const LEGEND: &str = "The percentage IS a fulfillment: LOW MEANS URGENT. Open todos are sorted \
     ascending, so the top row is the one most in need of attention. A todo the chain prices no \
     opinion on shows —.";

/// How far back "recently resolved" reaches.
const RECENT_DAYS: i64 = 30;

/// One stored object with its own hash — the pair a reader verifies.
#[derive(Debug, Clone)]
pub struct Link {
    /// The object's name.
    pub hash: Hash,
    /// The object.
    pub envelope: Envelope,
}

/// The WHY an event's writer recorded. A free function because `TodoEvent` is
/// the Prodrome's, and nothing flows from the application into it.
fn note_of(event: &TodoEvent) -> &str {
    match event {
        TodoEvent::Created(e) => &e.note,
        TodoEvent::Completed(e)
        | TodoEvent::Cancelled(e)
        | TodoEvent::Reopened(e)
        | TodoEvent::Tended(e) => &e.note,
        TodoEvent::SpecRevised(e) => &e.note,
        TodoEvent::Authored(e) => &e.payload.note,
    }
}

impl Link {
    fn event(&self) -> Option<&TodoEvent> {
        self.envelope.event()
    }

    /// What an object rests on, said the way the object says it: one parent
    /// is a `prev`, several are `parents`.
    fn parentage(&self) -> Expr {
        let parents = parents_of(&self.envelope);
        match parents.as_slice() {
            [] => lit("prev (genesis)"),
            [one] => span([lit("prev "), hash(one)]),
            many => span([lit("parents "), hashes(many)]),
        }
    }
}

/// Everything the pages read, folded once.
pub struct Site<'a> {
    now: String,
    /// One row per todo the DAG has ever mentioned.
    pub entries: Vec<Entry>,
    /// The objects, in the linearisation's order.
    pub links: Vec<Link>,
    untrusted: &'a Untrusted,
    problems: Vec<String>,
    /// The content record each entry names (an entry carries a name, not a record).
    records: BTreeMap<Hash, Authored>,
    instant: fpl::Instant,
}

impl Site<'_> {
    /// The line every page ends on.
    fn footer(&self) -> String {
        format!(
            "generated {} from {} chain objects — suzatary web",
            self.now,
            self.links.len()
        )
    }

    fn record(&self, entry: &Entry) -> Option<&Authored> {
        entry
            .content()
            .first()
            .and_then(|name| self.records.get(name))
    }
}

/// A reading as the site prints one: the number, `∅` for a todo nothing
/// prices, and "—" for a price that does not link.
fn pct<E>(value: Result<Option<f64>, &E>) -> String {
    match value {
        Ok(Some(v)) => format!("{:.1}%", v * 100.0),
        Ok(None) => "∅".to_owned(),
        Err(_) => "—".to_owned(),
    }
}

fn spaced(at: Datetime) -> String {
    at.isoformat().replace('T', " ")
}

/// A field of the todo's latest content, "—" when the chain has none.
fn field(value: Option<&str>) -> Expr {
    lit(value.filter(|shown| !shown.is_empty()).unwrap_or("—"))
}

fn category(record: Option<&Authored>) -> Expr {
    field(
        record
            .and_then(|r| r.payload.category.as_ref())
            .map(prodrome_core::event::Name::as_str),
    )
}

fn state(entry: &Entry) -> Expr {
    let at = entry.at();
    let shown = if at.is_empty() {
        entry.state()
    } else {
        format!("{} {at}", entry.state())
    };
    let claimed = entry.claimed();
    if claimed.is_empty() {
        return lit(&shown);
    }
    span([
        lit(&format!("{shown} ")),
        strong(lit(&format!("provisional: {claimed}"))),
    ])
}

// --- index.html ---------------------------------------------------------------

const COLUMNS: &[&str] = &[
    "fulfillment",
    "todo",
    "kind",
    "category",
    "state",
    "last author",
];

fn row(entry: &Entry, site: &Site) -> Vec<Expr> {
    let record = site.record(entry);
    let author = lit(record.map_or("—", |r| r.actor.as_str()));
    vec![
        lit(&pct(entry.value())),
        link(
            &format!("todo/{}.html", entry.key.as_str()),
            lit(entry.key.as_str()),
        ),
        field(record.map(|r| r.payload.kind.as_str())),
        category(record),
        state(entry),
        if crate::view::provisional_content(entry) {
            span([author, lit(" "), strong(lit("provisional"))])
        } else {
            author
        },
    ]
}

fn table(entries: &[&Entry], site: &Site) -> Expr {
    rows(COLUMNS, entries.iter().map(|entry| row(entry, site)))
}

fn index_page(site: &Site) -> SitePage {
    let cutoff = spaced(
        fpl::datetime_of(site.instant - chrono::Duration::days(RECENT_DAYS))
            .unwrap_or_else(|_| site.instant_as_datetime()),
    );
    let mut open: Vec<&Entry> = site
        .entries
        .iter()
        .filter(|entry| one_outcome(entry.outcome()).is_none())
        .collect();
    open.sort_by(|left, right| view::list_order(left, right));
    let mut resolved: Vec<&Entry> = site
        .entries
        .iter()
        .filter(|entry| one_outcome(entry.outcome()).is_some() && entry.at() >= cutoff)
        .collect();
    resolved.sort_by(|left, right| {
        (right.at(), right.key.as_str()).cmp(&(left.at(), left.key.as_str()))
    });
    let all: Vec<&Entry> = site.entries.iter().collect();
    SitePage::of(
        "Todos on the chain",
        "",
        &site.footer(),
        vec![
            tag(Tag::H1, [lit("Todos on the chain")]),
            tag(Tag::P, [lit(LEGEND)]),
            tag(Tag::H2, [lit(&format!("Open ({})", open.len()))]),
            table(&open, site),
            tag(
                Tag::H2,
                [lit(&format!("Recently resolved ({})", resolved.len()))],
            ),
            table(&resolved, site),
            tag(Tag::H2, [lit(&format!("Everything ({})", all.len()))]),
            table(&all, site),
        ],
    )
}

// --- todo/<id>.html -------------------------------------------------------------

fn about(entry: &Entry, site: &Site) -> Expr {
    let record = site.record(entry);
    let created = record.map_or_else(|| "—".to_owned(), |r| spaced(r.payload.created));
    let authored = record.map_or_else(
        || "—".to_owned(),
        |r| format!("{} by {}", spaced(r.at), r.actor.as_str()),
    );
    let mut pairs = vec![
        ("state", state(entry)),
        ("kind", field(record.map(|r| r.payload.kind.as_str()))),
        ("category", category(record)),
        ("created", lit(&created)),
        ("last authored", lit(&authored)),
        (
            "waiting on",
            field(record.map(|r| r.payload.waiting_on.as_str())),
        ),
        ("fulfillment", lit(&pct(entry.value()))),
    ];
    if crate::view::provisional_content(entry) {
        pairs.push((
            "provisional",
            strong(lit("content authored by an untrusted actor")),
        ));
    }
    facts(pairs)
}

fn pre(source: &str) -> Expr {
    tag(Tag::Pre, [lit(source)])
}

fn source_block(record: Option<&Authored>) -> Vec<Expr> {
    let Some(source) = record.and_then(|r| r.payload.source.as_ref()) else {
        return Vec::new();
    };
    let or_dash = |value: &str| lit(if value.is_empty() { "—" } else { value });
    vec![
        tag(Tag::H2, [lit("Source message")]),
        facts(vec![
            ("sender", lit(source.sender.as_str())),
            ("subject", lit(source.subject.as_str())),
            ("date", lit(&spaced(source.date))),
            ("hash", or_dash(source.hash.as_str())),
            ("thread id", or_dash(source.thread_id.as_str())),
            ("message id", or_dash(source.message_id.as_str())),
        ]),
    ]
}

/// Body and detail are Typst markup source, shown as their source: un-escaping
/// it is the lossy codec the record type exists to avoid.
fn content_block(record: Option<&Authored>) -> Vec<Expr> {
    let Some(record) = record else {
        return vec![tag(
            Tag::P,
            [lit(
                "The chain holds no Authored record for this todo — only lifecycle events.",
            )],
        )];
    };
    let payload = &record.payload;
    let mut blocks = vec![
        tag(Tag::H2, [lit("Content (typst markup source)")]),
        pre(payload.body.as_str()),
    ];
    if !payload.detail.as_str().is_empty() {
        blocks.extend([tag(Tag::H3, [lit("detail")]), pre(payload.detail.as_str())]);
    }
    if !payload.rationale.is_empty() {
        blocks.extend([
            tag(Tag::H3, [lit("rationale")]),
            pre(&payload.rationale.join("\n")),
        ]);
    }
    if !payload.subtodos.is_empty() {
        blocks.push(tag(Tag::H3, [lit("subtodos")]));
        blocks.push(tag(
            Tag::Ul,
            payload.subtodos.iter().map(|sub| {
                let mark = if sub.done { "x" } else { " " };
                tag(Tag::Li, [lit(&format!("[{mark}] {}", sub.body))])
            }),
        ));
    }
    if !payload.notes.is_empty() {
        blocks.push(tag(Tag::H3, [lit("notes")]));
        blocks.push(tag(
            Tag::Ul,
            payload.notes.iter().map(|note| {
                tag(
                    Tag::Li,
                    [lit(&format!(
                        "on {}: {}",
                        note.on.as_str(),
                        note.lines.join(" / ")
                    ))],
                )
            }),
        ));
    }
    blocks
}

/// The price as the chain stores it, and its value at the site's moment; "Why
/// this number" is the app's reading page, not this site's.
fn price_block(entry: &Entry) -> Vec<Expr> {
    let without = |why: &str| vec![tag(Tag::H2, [lit("Price")]), tag(Tag::P, [lit(why)])];
    if matches!(entry.spec().out(), TermF::Absent) {
        return without("The chain records no priority spec for this todo.");
    }
    if entry.linked().is_none() {
        return without("This todo's price refers to another todo that does not link.");
    }
    vec![
        tag(Tag::H2, [lit(&format!("Price — {}", pct(entry.value())))]),
        tag(Tag::H3, [lit("spec, as the chain stores it")]),
        // The core's own print: a second spelling of a term is a second reading of it.
        // One line, so inline code, which a phone wraps where a `<pre>` would not.
        tag(
            Tag::P,
            [tag(Tag::Code, [lit(&fpl::print_term(entry.spec()))])],
        ),
    ]
}

/// The event's coordinates, not its content: they differ on every record.
const DIFF_SKIP: &[&str] = &["todo", "at", "actor"];

/// The canonical prints of "absent" on a shipped kind; on a first record
/// listing them would bury the fields the author filled.
const ABSENT_PRINTS: &[&str] = &["''", "()", "None"];

/// A record's content fields as canonical prints, the bytes the object holds.
fn prints(record: &Authored) -> Vec<(String, String)> {
    match record.to_value() {
        Value::Call(call) => call
            .fields
            .into_iter()
            .filter(|(name, _)| !DIFF_SKIP.contains(&name.as_str()))
            .map(|(name, value)| (name, print_literal(&value)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Which content fields this `Authored` changed against the previous one.
fn diff(previous: Option<&Authored>, current: &Authored) -> Vec<Expr> {
    let before: BTreeMap<String, String> = previous
        .map(|record| prints(record).into_iter().collect())
        .unwrap_or_default();
    let changed: Vec<(String, String, String)> = prints(current)
        .into_iter()
        .filter(|(name, value)| {
            before.get(name) != Some(value)
                && !(previous.is_none() && ABSENT_PRINTS.contains(&value.as_str()))
        })
        .map(|(name, value)| {
            let old = before.get(&name).cloned().unwrap_or_default();
            (name, old, value)
        })
        .collect();
    if changed.is_empty() {
        return vec![tag(
            Tag::P,
            [lit(
                "no content change against the previous Authored record",
            )],
        )];
    }
    let intro = if previous.is_none() {
        "first Authored record for this id".to_owned()
    } else {
        format!("{} field(s) changed", changed.len())
    };
    vec![
        tag(Tag::P, [lit(&intro)]),
        tag(
            Tag::Ul,
            changed.into_iter().map(|(name, old, new)| {
                let old = if old.is_empty() {
                    "—".to_owned()
                } else {
                    old
                };
                tag(Tag::Li, [lit(&format!("{name} {old} → {new}"))])
            }),
        ),
    ]
}

/// One event of a todo's history. A merge is on no todo's timeline, and the
/// stream never puts one here.
fn event_article(link: &Link, previous: Option<&Authored>) -> Option<Expr> {
    let event = link.event()?;
    let mut kids = vec![tag(
        Tag::H3,
        [lit(&format!(
            "{} — {} — {}",
            event.kind_name(),
            spaced(event.at()),
            event.actor().as_str()
        ))],
    )];
    if !note_of(event).is_empty() {
        kids.push(tag(Tag::P, [lit(&format!("note: {}", note_of(event)))]));
    }
    if let TodoEvent::Authored(record) = event {
        kids.extend(diff(previous, record));
    }
    kids.push(tag(Tag::P, [span([lit("hash "), hash(&link.hash)])]));
    kids.push(tag(Tag::P, [link.parentage()]));
    Some(tag(Tag::Article, kids))
}

fn history_block(entry: &Entry, site: &Site) -> Vec<Expr> {
    let by_hash: BTreeMap<&Hash, &Link> =
        site.links.iter().map(|link| (&link.hash, link)).collect();
    let mut blocks = vec![
        tag(
            Tag::H2,
            [lit(&format!("History ({} events)", entry.stream.len()))],
        ),
        tag(
            Tag::P,
            [lit(
                "Chronological. Every object is verifiable at objects/<hash>.py, a copy of events/objects/.",
            )],
        ),
    ];
    let mut previous: Option<&Authored> = None;
    for link in entry.stream.iter().filter_map(|name| by_hash.get(name)) {
        blocks.extend(event_article(link, previous));
        if let Some(TodoEvent::Authored(record)) = link.event() {
            previous = Some(record);
        }
    }
    blocks
}

fn todo_page(entry: &Entry, site: &Site) -> SitePage {
    let record = site.record(entry);
    let mut body = vec![tag(Tag::H1, [lit(entry.key.as_str())]), about(entry, site)];
    body.extend(content_block(record));
    body.extend(source_block(record));
    body.extend(price_block(entry));
    body.extend(history_block(entry, site));
    SitePage::of(entry.key.as_str(), "../", &site.footer(), body)
}

// --- chain.html and objects/ -------------------------------------------------------

const CHAIN_COLUMNS: &[&str] = &["hash", "parents", "kind", "todo", "actor", "at"];

/// One object. A merge has no event, so the columns that describe one are
/// empty rather than filled with something the store never said.
fn chain_row(link: &Link) -> Vec<Expr> {
    let event = link.event();
    let parents = parents_of(&link.envelope);
    let parentage = if parents.is_empty() {
        lit(match &link.envelope {
            Envelope::Change(_) => "(none: it rests on no earlier write)",
            _ => "(genesis)",
        })
    } else {
        hashes(&parents)
    };
    let kind = event.map_or_else(|| link.envelope.name(), TodoEvent::kind_name);
    vec![
        hash(&link.hash),
        parentage,
        lit(kind),
        match event {
            Some(event) => link_to_todo(event.todo().as_str()),
            None => lit(""),
        },
        lit(event.map_or("", |e| e.actor().as_str())),
        lit(&event.map_or_else(String::new, |e| spaced(e.at()))),
    ]
}

fn link_to_todo(todo: &str) -> Expr {
    link(&format!("todo/{todo}.html"), lit(todo))
}

fn health(problems: &[String]) -> Vec<Expr> {
    if problems.is_empty() {
        return vec![tag(
            Tag::P,
            [lit("healthy — store.verify() reported no problems.")],
        )];
    }
    vec![
        tag(
            Tag::P,
            [lit(&format!(
                "{} problem(s) reported by store.verify():",
                problems.len()
            ))],
        ),
        tag(
            Tag::Ul,
            problems.iter().map(|problem| tag(Tag::Li, [lit(problem)])),
        ),
    ]
}

fn chain_page(site: &Site) -> SitePage {
    let actors: Vec<&str> = site
        .untrusted
        .actors()
        .iter()
        .map(prodrome_core::event::Actor::as_str)
        .collect();
    let named = if actors.is_empty() {
        "(none — this deployment trusts every writer)".to_owned()
    } else {
        actors.join(", ")
    };
    let mut body = vec![
        tag(Tag::H1, [lit("The chain")]),
        tag(
            Tag::P,
            [lit(&format!(
                "{} objects, tip first. Each names the objects it rests on.",
                site.links.len()
            ))],
        ),
    ];
    body.extend(health(&site.problems));
    body.extend([
        tag(Tag::H2, [lit("Provisional writers")]),
        tag(Tag::P, [lit(&format!("untrusted actors: {named}"))]),
        tag(Tag::P, [lit(PROVISIONAL)]),
        tag(Tag::H2, [lit("Objects")]),
        rows(CHAIN_COLUMNS, site.links.iter().rev().map(chain_row)),
    ]);
    SitePage::of("The chain", "", &site.footer(), body)
}

/// `objects/<hash>.py` → the canonical literal, byte for byte what the store
/// holds: the store's own typed format, so there is nothing to escape, and a
/// `.py` file is served as text and never sniffed as a tag.
fn object_files(links: &[Link]) -> BTreeMap<String, String> {
    links
        .iter()
        .map(|link| {
            (
                format!("objects/{}.py", link.hash.as_str()),
                prodrome_core::event::canonical_envelope(&link.envelope),
            )
        })
        .collect()
}

// --- The pure generator --------------------------------------------------------------

impl Site<'_> {
    fn instant_as_datetime(&self) -> Datetime {
        fpl::datetime_of(self.instant).unwrap_or_else(|_| {
            Datetime::new(1, 1, 1, 0, 0, 0, 0).expect("year one is representable")
        })
    }
}

/// The moment as the pages print it, to the second.
fn generated(now: Datetime) -> String {
    let mut shown = spaced(now);
    if let Some(dot) = shown.find('.') {
        shown.truncate(dot);
    }
    shown
}

/// The whole site. PURE: no clock, no disk.
///
/// The chain `chain` as `set` read it: its objects, the envelopes and not just the register
/// nodes, because `objects/<hash>.py` is a byte-for-byte copy of each, in the linearisation's
/// order, and its rows priced through the whole set, as every page prices them.
///
/// # Errors
/// A chain the set did not read, and objects that do not order.
pub fn render_site(
    chain: &ChainName,
    set: &ChainSet<'_>,
    problems: &[String],
) -> Result<Files, ProdromeError> {
    let unread = |failed: crate::chain_set::NotAMember| ProdromeError::Store(failed.to_string());
    let held = set.held(chain).map_err(unread)?;
    let (now, untrusted) = (set.at(), held.untrusted);
    let links: Vec<Link> = held
        .nodes
        .iter()
        // A held chain's nodes are its dag's objects in causal order, so each is there.
        .map(|node| Link {
            hash: node.name().clone(),
            envelope: held.dag.objects()[node.name()].clone(),
        })
        .collect();
    let records: BTreeMap<Hash, Authored> = links
        .iter()
        .filter_map(|link| match link.event() {
            Some(TodoEvent::Authored(record)) => Some((link.hash.clone(), (**record).clone())),
            _ => None,
        })
        .collect();
    let site = Site {
        now: generated(now),
        entries: set.entries(chain).map_err(unread)?,
        links,
        untrusted,
        problems: problems.to_vec(),
        records,
        instant: fpl::instant_of(now),
    };
    let mut pages = BTreeMap::new();
    pages.insert("index.html".to_owned(), index_page(&site));
    pages.insert("chain.html".to_owned(), chain_page(&site));
    for entry in &site.entries {
        pages.insert(
            format!("todo/{}.html", entry.key.as_str()),
            todo_page(entry, &site),
        );
    }
    Ok(Files {
        pages,
        objects: object_files(&site.links),
        footer: site.footer(),
    })
}

/// What `suzatary web` prints when it is done.
#[must_use]
pub fn summary(
    outdir: &str,
    files: usize,
    objects: usize,
    entries: &[Entry],
    problems: &[String],
    now: Datetime,
) -> serde_json::Value {
    serde_json::json!({
        "out": outdir,
        "files": files,
        "objects": objects,
        "todos": entries.len(),
        "open": entries
            .iter()
            .filter(|entry| one_outcome(entry.outcome()).is_none())
            .count(),
        "verify": if problems.is_empty() {
            serde_json::json!("healthy")
        } else {
            serde_json::json!(problems)
        },
        "generated": generated(now),
    })
}
