//! Door 1 of a public store: an event reaches a store whose charter says `public = true` only
//! from a writer that store takes, and only after the leak guard has read its `Reading`: every
//! string it holds and what each of its markup fields draws.
//! A `Change` rests on its todo's heads, so a stored object is an ancestor of every later write
//! to that todo, and a published history never takes a word back: the check runs before the
//! store holds the object, or not at all.

use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use prodrome_data::event::Actor;
use prodrome_data::event::Hash;
use prodrome_data::literal::Value;
use suzatary_base::capability::{RenderError, Typeset};
use suzatary_base::chains::ChainName;
use suzatary_base::chains::ChainPolicy;
use suzatary_base::reference::{Found, GenesisPrefix, GlobalTodo, Reference, mentioned, mentions};
use suzatary_held::leak::{Matcher, Opaque, Scope};
use suzatary_plan::wait::{Audience, Kept};
use suzatary_record::record::TodoEvent;
use suzatary_signatures::chain_set::NotAMember;
use suzatary_store::store::{Rule, Shut};

use crate::instance::Local;
use crate::stamp::{self, Package};
use suzatary_secretary::agent;

/// Door 1 of one public store opened from one place: whom it takes writes from, what it reads,
/// and the typesetter that draws a markup field as every renderer draws it.
pub struct Door {
    lists: Lists,
    writers: BTreeSet<Actor>,
    typeset: Arc<dyn Typeset + Send>,
}

/// Where a chain's store is opened: how the box tells apart two writers that author as `assistant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    /// A command at the box's terminal, where `assistant` is the main session.
    AtTerminal,
    /// The server and the secretary's commands, where `assistant` is the in-app secretary: its
    /// model runs with no tools (`suzatary_io_model::model`), so it writes through no terminal.
    ByTheBox,
}

/// What door 1 reads at each write: the box-local tier's two term lists, the deployment's own
/// words from its instance file, and gitleaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lists {
    terms: [PathBuf; 2],
    instance: PathBuf,
    scanner: Scanner,
}

/// The gitleaks door 1 runs, and its rules.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scanner {
    program: PathBuf,
    rules: PathBuf,
}

/// Where a text of an event's [`Reading`] stands, which a refusal names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Site {
    /// A string the event holds, at its field's path.
    Held(String),
    /// What a markup field draws: its HTML body or that body's text.
    Drawn(&'static str),
    /// Typst's message about a markup field that does not draw.
    Said(&'static str),
}

impl std::fmt::Display for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Site::Held(path) => write!(f, "field {path}"),
            Site::Drawn(field) => write!(f, "field {field}, as drawn,"),
            Site::Said(field) => write!(
                f,
                "field {field} does not draw, and typst's message about it"
            ),
        }
    }
}

/// What door 1 matches of one event: every string it holds, then, for each field the record holds
/// as markup ([`suzatary_record::record::Record::markup`]), what it draws in the closed world, or
/// typst's message where it does not draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reading {
    /// Each text, beside its site, in the order a refusal is chosen: held, drawn, said.
    pub(crate) texts: Vec<(Site, String)>,
    /// The first markup field that does not draw, and why: a public store refuses the event.
    pub(crate) undrawn: Option<(&'static str, String)>,
}

impl Reading {
    /// `event` read: its strings decoded, and each nonempty markup field drawn once by `typeset`,
    /// which the box runs as the child that caps a compile's CPU time and memory
    /// (`suzatary_io_typeset::typeset::Closed`).
    #[must_use]
    pub(crate) fn of(event: &TodoEvent, typeset: &dyn Typeset) -> Self {
        let mut texts: Vec<(Site, String)> = published(event)
            .into_iter()
            .map(|(path, text)| (Site::Held(path), text))
            .collect();
        let mut said = Vec::new();
        let mut undrawn = None;
        if let TodoEvent::Authored(record) = event {
            for (field, source) in record.payload.markup() {
                if source.as_str().is_empty() {
                    continue;
                }
                match drawing(typeset, source.as_str()) {
                    Ok(drawn) => {
                        texts.push((Site::Drawn(field), drawn.html));
                        texts.push((Site::Drawn(field), drawn.text));
                    }
                    Err(RenderError::Refused(message)) => {
                        said.push((Site::Said(field), message.clone()));
                        undrawn.get_or_insert((field, message));
                    }
                    Err(RenderError::Suspicious(reason)) => {
                        undrawn.get_or_insert((field, format!("its render carries {reason}")));
                    }
                }
            }
        }
        texts.extend(said);
        Reading { texts, undrawn }
    }
}

impl Door {
    /// The door of a public chain's store opened from `opened`: the roster's trusted actors, never
    /// the triage agent, and from the box never the secretary.
    #[must_use]
    pub fn of(
        lists: Lists,
        policy: &ChainPolicy,
        opened: Opened,
        typeset: Arc<dyn Typeset + Send>,
    ) -> Door {
        let trusted = policy
            .roster
            .iter()
            .filter(|actor| !policy.untrusted.actors().contains(*actor))
            .cloned()
            .collect();
        Door::named(lists, trusted, opened, typeset)
    }

    /// The door of a public store whose writers are `writers`, never the triage agent, and from
    /// the box never the secretary.
    #[must_use]
    pub fn named(
        lists: Lists,
        writers: BTreeSet<Actor>,
        opened: Opened,
        typeset: Arc<dyn Typeset + Send>,
    ) -> Door {
        let barred: &[suzatary_base::writer::Writer] = match opened {
            Opened::AtTerminal => &[agent::WRITES_AS],
            Opened::ByTheBox => &[agent::WRITES_AS, suzatary_secretary::secretary::WRITES_AS],
        };
        let writers = writers
            .into_iter()
            .filter(|actor| !barred.iter().any(|writer| writer.actor() == actor.as_str()))
            .collect();
        Door {
            lists,
            writers,
            typeset,
        }
    }

    /// Each event's verdict, written into the same place of `verdicts`, which holds one place for
    /// each event. The lists are read once and gitleaks runs once for the whole batch, and the
    /// verdicts are the ones each event gets alone (`door_1_judges_a_batch_as_each_event_alone`).
    pub fn judge(&self, events: &[&TodoEvent], verdicts: &mut [Result<(), Shut>]) {
        // The deployment's stores, read once for the batch and only if an event names a todo.
        let stores = std::cell::OnceCell::new();
        let mut open = Vec::new();
        for (i, (event, verdict)) in events.iter().zip(verdicts.iter_mut()).enumerate() {
            *verdict = self.writes(event, &stores);
            if verdict.is_ok() {
                open.push(i);
            }
        }
        if open.is_empty() {
            return;
        }
        let read = self.read(events, &open);
        // Read where any text, held or drawn, names a todo by genesis, so an event's verdict does
        // not depend on what else rides its batch. Stores that do not read begin nothing here.
        let names = read
            .iter()
            .flat_map(|reading| &reading.texts)
            .any(|(_, text)| {
                mentioned(text)
                    .iter()
                    .any(|(_, reference)| matches!(reference, Reference::Global(_)))
            });
        let begun: Vec<Hash> = match names.then(|| stores.get_or_init(|| self.lists.stores())) {
            Some(Ok(stores)) => stores
                .iter()
                .filter_map(|(_, home, _)| match home {
                    Home::Begun(home) => Some(home.clone()),
                    Home::NotBegun | Home::Unread(_) => None,
                })
                .collect(),
            Some(Err(_)) | None => Vec::new(),
        };
        match self.lists.judge(&read, &begun) {
            Ok(judged) => {
                for (i, verdict) in open.into_iter().zip(judged) {
                    verdicts[i] = verdict;
                }
            }
            Err(why) => {
                for i in open {
                    verdicts[i] = Err(unjudged(&why));
                }
            }
        }
    }

    /// The readings of `events` at `open`, in that order, each field's compile in a child of
    /// its own, at most one per core at a time.
    fn read(&self, events: &[&TodoEvent], open: &[usize]) -> Vec<Reading> {
        let width = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        let typeset = &*self.typeset;
        std::thread::scope(|scope| {
            let parts: Vec<_> = open
                .chunks(open.len().div_ceil(width))
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .map(|&i| Reading::of(events[i], typeset))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            parts
                .into_iter()
                .flat_map(|part| part.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
                .collect()
        })
    }

    /// The store's rule: [`Door::judge`] over a batch of one.
    pub(crate) fn rule(self) -> Rule<TodoEvent> {
        Box::new(move |event| {
            let mut verdict = [Ok(())];
            self.judge(&[event], &mut verdict);
            let [verdict] = verdict;
            verdict
        })
    }

    fn writes(&self, event: &TodoEvent, stores: &OnceCell<Stores>) -> Result<(), Shut> {
        if self.writers.contains(event.actor()) {
            return self.names_no_chain(event, stores);
        }
        let named: Vec<&str> = self.writers.iter().map(Actor::as_str).collect();
        Err(Shut::Refused(format!(
            "a public store takes writes from {named:?} alone, and not from {} here: the \
             secretary's and the triage agent's writes go to a private chain",
            event.actor().as_str()
        )))
    }
}

/// What a store of the deployment begins, as door 1 reads it.
enum Home {
    Begun(Hash),
    /// It holds no object yet, or it is a followed store no fetch has confirmed.
    NotBegun,
    Unread(String),
}

/// The deployment's stores as door 1 reads them, or why they cannot be read.
type Stores = Result<Vec<(ChainName, Home, Kept)>, String>;

impl Door {
    /// A public store names a todo by its store's genesis, never by a chain name of this
    /// deployment, which means nothing to its readers and is the deployment's own word, and never
    /// a task of a private chain, by either name.
    fn names_no_chain(&self, event: &TodoEvent, stores: &OnceCell<Stores>) -> Result<(), Shut> {
        let named: Vec<(String, Reference)> = published(event)
            .into_iter()
            .flat_map(|(path, text)| {
                mentions(&text)
                    .into_iter()
                    .map(move |mention| (path.clone(), mention))
            })
            .filter(|(_, mention)| {
                matches!(
                    mention,
                    Reference::Todo(_) | Reference::Global(_) | Reference::Batch { .. }
                )
            })
            .collect();
        if named.is_empty() {
            return Ok(());
        }
        let stores = stores
            .get_or_init(|| self.lists.stores())
            .as_ref()
            .map_err(|why| unjudged(why))?;
        // A chain of this deployment that does not read could be the one a name points into.
        if let Some((name, why)) = stores.iter().find_map(|(name, home, kept)| match home {
            Home::Unread(why) if kept.own() => Some((name, why)),
            Home::Begun(_) | Home::NotBegun | Home::Unread(_) => None,
        }) {
            return Err(unjudged(&format!(
                "the chain {name} does not read ({why}), so it cannot tell whether a todo this \
                 write names is one of it"
            )));
        }
        let private = |genesis: &GenesisPrefix| {
            let homes = stores
                .iter()
                .filter_map(|(_, home, kept)| match (home, kept) {
                    (Home::Begun(home), Kept::Private) => Some(home),
                    (Home::Begun(_) | Home::NotBegun | Home::Unread(_), _) => None,
                });
            !matches!(genesis.among(homes), Found::Unheld)
        };
        let rewrite = "a revision restates every field, so rewrite the field that holds it";
        for (path, mention) in named {
            let refused = match &mention {
                Reference::Todo(local) => {
                    let shared = stores.iter().find_map(|(name, home, kept)| match home {
                        Home::Begun(home)
                            if *name == local.chain && kept.audience() == Audience::Anyone =>
                        {
                            Some(home)
                        }
                        Home::Begun(_) | Home::NotBegun | Home::Unread(_) => None,
                    });
                    match shared.filter(|home| !private(&GenesisPrefix::whole(home))) {
                        Some(home) => format!(
                            "field {path} names {local} by this deployment's name for its store, \
                             which a public store's readers do not hold: write {} instead; \
                             {rewrite}",
                            GlobalTodo {
                                genesis: GenesisPrefix::whole(home),
                                id: local.id.clone(),
                            }
                        ),
                        None => format!(
                            "field {path} names {local}, a todo of a chain of this deployment, \
                             and a public store names no task of a private chain; {rewrite}"
                        ),
                    }
                }
                Reference::Global(global) if private(&global.genesis) => format!(
                    "field {path} names {global}, and its genesis begins a private chain of this \
                     deployment: a public store names no task of a private chain; {rewrite}"
                ),
                Reference::Batch { chain, .. } => format!(
                    "field {path} names a held batch of the chain {chain}, this deployment's \
                     name, and a public store publishes its words; {rewrite}"
                ),
                Reference::Global(_)
                | Reference::Mail { .. }
                | Reference::Thread { .. }
                | Reference::Briefing { .. }
                | Reference::Proposal(_)
                | Reference::Target(_) => continue,
            };
            return Err(Shut::Refused(refused));
        }
        Ok(())
    }
}

impl Scanner {
    /// The package's own; a cargo build's is `gitleaks` on `PATH` with the code root's rules.
    fn of(package: &Package, code: &Path) -> Self {
        match package {
            Package::Installed(out) => Scanner {
                program: out.join(stamp::GITLEAKS),
                rules: out.join(stamp::SECRET_RULES),
            },
            Package::Dev => Scanner {
                program: PathBuf::from("gitleaks"),
                rules: code.join("scripts").join("secret-rules.toml"),
            },
        }
    }

    /// For each event, its secret at the least line gitleaks reports (the least rule there),
    /// as a refusal naming its site. `Err` says why gitleaks did not run.
    fn secrets(&self, events: &[&Reading]) -> Result<Vec<Option<Shut>>, String> {
        let failed = |why: &dyn std::fmt::Display| format!("gitleaks did not run ({why})");
        let scratch = suzatary_os::scratch::Scratch::temporary("door").map_err(|e| failed(&e))?;
        let template = scratch.path().join("template");
        std::fs::write(
            &template,
            "{{ range . }}{{ .StartLine }}\t{{ .RuleID }}\n{{ end }}",
        )
        .map_err(|e| failed(&e))?;
        let mut text = String::new();
        let mut starts = Vec::new();
        for (event, reading) in events.iter().enumerate() {
            for (site, value) in &reading.texts {
                starts.push((text.matches('\n').count() + 1, event, site));
                text.push_str(value);
                text.push('\n');
            }
        }
        let mut scanner = Command::new(&self.program)
            .arg("stdin")
            .arg("--config")
            .arg(&self.rules)
            .args([
                "--no-banner",
                "--log-level=error",
                "--redact",
                "--max-decode-depth=3",
            ])
            .args([
                "--ignore-gitleaks-allow",
                "--exit-code=3",
                "--report-format=template",
            ])
            .arg("--report-path=-")
            .arg("--report-template")
            .arg(&template)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| failed(&e))?;
        scanner
            .stdin
            .take()
            .ok_or_else(|| failed(&"no stdin"))?
            .write_all(text.as_bytes())
            .map_err(|e| failed(&e))?;
        let ran = scanner.wait_with_output().map_err(|e| failed(&e))?;
        let mut found: Vec<Option<(usize, String, &Site)>> = vec![None; events.len()];
        match ran.status.code() {
            Some(0) => {}
            Some(3) => {
                let report = String::from_utf8_lossy(&ran.stdout);
                let findings: Vec<(usize, &str)> = report
                    .lines()
                    .filter_map(|line| line.split_once('\t'))
                    .filter_map(|(line, rule)| Some((line.parse::<usize>().ok()?, rule)))
                    .collect();
                if findings.is_empty() {
                    return Err(failed(&"its report names no line"));
                }
                for (line, rule) in findings {
                    let Some(&(_, event, site)) = starts
                        .partition_point(|(start, _, _)| *start <= line)
                        .checked_sub(1)
                        .and_then(|at| starts.get(at))
                    else {
                        continue;
                    };
                    let least = &mut found[event];
                    if least
                        .as_ref()
                        .is_none_or(|(held, kept, _)| (line, rule) < (*held, kept.as_str()))
                    {
                        *least = Some((line, rule.to_owned(), site));
                    }
                }
            }
            _ => return Err(failed(&ran.status)),
        }
        Ok(found
            .into_iter()
            .map(|found| {
                found.map(|(_, rule, site)| {
                    refused(site, format!("holds a secret (gitleaks rule {rule})"))
                })
            })
            .collect())
    }
}

impl Lists {
    pub(crate) fn of(local: &Local, code: &Path, instance: &Path) -> Self {
        Lists {
            terms: [local.personal_terms(), local.context_terms()],
            instance: instance.to_owned(),
            scanner: Scanner::of(Package::running(), code),
        }
    }

    /// Each event's verdict, refused with the site and the rule's kind, never the words; then,
    /// where nothing matched, refused for its first markup field that does not draw. `Err` says
    /// why the lists could not be read.
    fn judge(&self, events: &[Reading], begun: &[Hash]) -> Result<Vec<Result<(), Shut>>, String> {
        let words: Vec<String> = super::instance::words(&self.instance)
            .map_err(|e| format!("the deployment's own words cannot be read ({e})"))?
            .into_iter()
            .collect();
        let matcher = Matcher::new(&self.terms()?, &words, &[], Scope::Whole);
        let mut verdicts: Vec<Result<(), Shut>> = events
            .iter()
            .map(|reading| matched(&matcher, reading, begun))
            .collect();
        let scanned: Vec<usize> = (0..verdicts.len())
            .filter(|&i| verdicts[i].is_ok())
            .collect();
        let read: Vec<&Reading> = scanned.iter().map(|&i| &events[i]).collect();
        for (i, secret) in scanned.into_iter().zip(self.scanner.secrets(&read)?) {
            if let Some(shut) = secret {
                verdicts[i] = Err(shut);
            }
        }
        for (verdict, reading) in verdicts.iter_mut().zip(events) {
            if let (Ok(()), Some((field, why))) = (&verdict, &reading.undrawn) {
                *verdict = Err(Shut::Refused(format!(
                    "a public store holds no event whose field {field} does not draw in the \
                     closed world, which every renderer of the store compiles in: {why}"
                )));
            }
        }
        Ok(verdicts)
    }

    /// Every store the instance reads as it stands, with what it begins and whose it is.
    fn stores(&self) -> Stores {
        let instance = crate::instance::Instance::open(&self.instance)
            .map_err(|e| format!("the deployment's stores cannot be read ({e:#})"))?;
        let read = super::plan_read(&instance);
        let set = read.set(suzatary_base::capability::Clock::now(
            &suzatary_io_local::clock::SystemClock,
        ));
        Ok(super::todos::kept(&instance)
            .into_iter()
            .map(|(name, kept)| {
                let home = match set.held(&name) {
                    Ok(held) => match suzatary_signatures::chain_set::home(held.dag) {
                        Ok(Some(home)) => Home::Begun(home),
                        Ok(None) => Home::NotBegun,
                        Err(several) => Home::Unread(format!("it holds {several} geneses")),
                    },
                    Err(NotAMember::Absent(_)) => Home::NotBegun,
                    Err(why @ (NotAMember::Unread(..) | NotAMember::Several(..))) => {
                        Home::Unread(why.to_string())
                    }
                };
                (name, home, kept)
            })
            .collect())
    }

    /// Both lists' terms, read as `scripts/public-tree.sh` reads them.
    fn terms(&self) -> Result<Vec<String>, String> {
        let mut terms = Vec::new();
        for path in &self.terms {
            let text = std::fs::read_to_string(path).map_err(|e| {
                format!(
                    "the box-local tier's {} cannot be read ({})",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    e.kind()
                )
            })?;
            terms.extend(
                text.lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(str::to_owned),
            );
        }
        Ok(terms)
    }
}

/// What a writer's markup draws, in the two forms a renderer publishes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Drawing {
    /// Typst's HTML body, past the tripwire: what the app and `prodrome pages` serve, its
    /// attributes included.
    pub(crate) html: String,
    /// That body's text, inline elements joined: what `prodrome draw` sets as glyphs.
    pub(crate) text: String,
}

/// `source` compiled in the closed world, narrowed to its body and past the tripwire.
///
/// # Errors
/// Typst's refusal, read against `source`, or the tripwire's.
pub(crate) fn drawing(typeset: &dyn Typeset, source: &str) -> Result<Drawing, RenderError> {
    let document = typeset
        .closed(source)
        .map_err(|rejected| RenderError::Refused(rejected.said(source)))?;
    let html = suzatary_base::narrowing::narrowed(&document)?;
    let text = suzatary_io_mail::mailbox::text::html_to_text(&html);
    Ok(Drawing { html, text })
}

/// Every string `event` holds, beside its field's path: what door 1 reads of it before it draws
/// the markup fields (`Reading`).
#[must_use]
pub fn published(event: &TodoEvent) -> Vec<(String, String)> {
    let value = event.to_value();
    let mut fields = Vec::new();
    strings(String::new(), &value, &mut fields);
    fields
        .into_iter()
        .map(|(field, text)| (field, text.to_owned()))
        .collect()
}

/// The first text, in the reading's order, that names a term, a name or an address, or that is
/// opaque. The matcher reads each text [`unnamed`] by the geneses in `begun`.
fn matched(matcher: &Matcher, reading: &Reading, begun: &[Hash]) -> Result<(), Shut> {
    for (site, text) in &reading.texts {
        if let Some(layer) = matcher.hits(&unnamed(text, begun)).into_values().min() {
            return Err(refused(site, layer));
        }
        if let Some(opaque) = Opaque::of(text.as_bytes()) {
            return Err(refused(site, opaque));
        }
    }
    Ok(())
}

/// `text` with the genesis of each todo it names by genesis blanked, where that genesis begins a
/// store in `begun`: one genesis in forty begins with a run of decimal digits that reads as a
/// telephone number. A writer of this deployment cannot choose a held genesis's characters. A
/// followed store's publisher can, but writes nothing here and would have to know the number
/// already, and a follow is added only in the instance file. A genesis no store begins is a
/// writer's text, read as written, so `todo:+` and a number names nothing.
fn unnamed(text: &str, begun: &[Hash]) -> String {
    let mut blanked = text.to_owned();
    for name in mentioned(text)
        .into_iter()
        .filter_map(|(at, reference)| match reference {
            Reference::Global(global) if !matches!(global.genesis.among(begun), Found::Unheld) => {
                let from = at.start + "todo:+".len();
                Some(from..from + global.genesis.as_str().len())
            }
            Reference::Global(_)
            | Reference::Todo(_)
            | Reference::Mail { .. }
            | Reference::Thread { .. }
            | Reference::Briefing { .. }
            | Reference::Proposal(_)
            | Reference::Batch { .. }
            | Reference::Target(_) => None,
        })
    {
        blanked.replace_range(name.clone(), &" ".repeat(name.len()));
    }
    blanked
}

/// Every string `value` holds, decoded, beside its field's path: a printed object escapes a line
/// break, and a term just after `\n` is no whole word there.
fn strings<'a>(at: String, value: &'a Value, out: &mut Vec<(String, &'a str)>) {
    match value {
        Value::Str(text) => out.push((at, text)),
        Value::Tuple(items) => {
            for (i, item) in items.iter().enumerate() {
                strings(format!("{at}[{i}]"), item, out);
            }
        }
        Value::Call(call) => {
            for (name, field) in &call.fields {
                let path = if at.is_empty() {
                    name.clone()
                } else {
                    format!("{at}.{name}")
                };
                strings(path, field, out);
            }
        }
        Value::None
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::Datetime(_)
        | Value::Timedelta(_) => {}
    }
}

fn refused(site: &Site, kind: impl std::fmt::Display) -> Shut {
    Shut::Refused(format!("a public store holds no event whose {site} {kind}"))
}

fn unjudged(why: &str) -> Shut {
    Shut::Unjudged(format!(
        "door 1 reads every write to a public store, and {why}, so it takes none"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The sentence of the least layer a matcher with no lists finds in `text`.
    fn phoned(text: &str) -> Option<String> {
        let matcher = Matcher::new(&[], &[], &[], Scope::Whole);
        matcher
            .hits(text)
            .into_values()
            .min()
            .map(|layer| layer.to_string())
    }

    /// A genesis that begins with `digits` decimal digits, the rest of it lowercase hex.
    fn genesis() -> impl Strategy<Value = Hash> {
        (8usize..=64)
            .prop_flat_map(|digits| ("[0-9]{64}", "[0-9a-f]{64}", Just(digits)))
            .prop_map(|(decimal, hex, digits)| {
                Hash::new(format!("{}{}", &decimal[..digits], &hex[digits..])).expect("hex")
            })
    }

    /// The seed case that failed `a_public_store_refuses_a_chain_name_and_says_the_genesis_form`:
    /// a genesis that begins with nine decimal digits reads as a telephone number until the door
    /// knows it begins a store of the deployment.
    #[test]
    fn a_genesis_of_nine_digits_is_a_name() {
        let home = Hash::new(format!("123456789{}", "a".repeat(55))).expect("hex");
        let wait = format!("todo:+{}/zzfirst", home.as_str());
        assert_eq!(phoned(&wait).as_deref(), Some("names a telephone number"));
        assert_eq!(phoned(&unnamed(&wait, &[home])), None);
    }

    /// Only the held genesis is blanked: a number beside the name, a number written as its id
    /// and a writer's digits after a held prefix are read as written. A run of digits as the id,
    /// with no plus before it, is no telephone number, as it was before.
    #[test]
    fn what_stands_beside_a_held_genesis_is_read_as_written() {
        let home = Hash::new(format!("123456789{}", "a".repeat(55))).expect("hex");
        let held = format!("todo:+{}/", &home.as_str()[..12]);
        let number = concat!("+961 ", "3 123 456");
        for (text, said) in [
            (format!("Call {number} {held}x"), true),
            (format!("{held}x then {number}"), true),
            (format!("{held}call-{}", concat!("415-867", "-5309")), true),
            (format!("todo:+{}0/x", &home.as_str()[..9]), true),
            (format!("{held}{}", concat!("961312", "3456")), false),
        ] {
            assert_eq!(
                phoned(&unnamed(&text, std::slice::from_ref(&home))).as_deref(),
                said.then_some("names a telephone number"),
                "{text}"
            );
        }
    }

    proptest! {
        /// THE LAW OF A NAME BY GENESIS: a wait by a genesis a store of the deployment begins,
        /// whole or by any prefix of eight or more, is no telephone number however many decimal
        /// digits it begins with; by a genesis no store begins, the matcher reads the text as
        /// written, so the phone layer judges it as it always did.
        #[test]
        fn a_held_genesis_is_a_name_at_every_prefix(home in genesis(), other in genesis()) {
            prop_assume!(home.as_str()[..8] != other.as_str()[..8]);
            for len in 8..=64 {
                let prefix = GenesisPrefix::new(&home.as_str()[..len]).expect("a prefix");
                prop_assert_eq!(prefix.among([&home]), Found::One(&home));
                let wait = format!("todo:+{}/zzfirst", prefix.as_str());
                prop_assert_eq!(phoned(&unnamed(&wait, std::slice::from_ref(&home))), None);
                prop_assert_eq!(unnamed(&wait, std::slice::from_ref(&other)), wait);
            }
        }
    }
}
