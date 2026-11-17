//! Door 1 of a public store: an event reaches a store whose charter says `public = true` only
//! from a writer that store takes, and only after the leak guard has read its `Reading`: every
//! string it holds and what each of its markup fields draws.
//! A `Change` rests on its todo's heads, so a stored object is an ancestor of every later write
//! to that todo, and a published history never takes a word back: the check runs before the
//! store holds the object, or not at all.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use prodrome_core::event::Actor;
use prodrome_core::literal::Value;
use suzatary_base::capability::{RenderError, Typeset};
use suzatary_base::chains::ChainPolicy;
use suzatary_held::leak::{Matcher, Opaque, Scope};
use suzatary_record::record::TodoEvent;
use suzatary_store::store::{Rule, Shut};

use crate::instance::Local;
use crate::stamp::{self, Package};

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
    /// model runs with no tools (`suzatary_io::model`), so it writes through no terminal.
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
    /// (`suzatary_io::typeset::Closed`).
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
            Opened::AtTerminal => &[suzatary_core::agent::WRITES_AS],
            Opened::ByTheBox => &[
                suzatary_core::agent::WRITES_AS,
                suzatary_core::secretary::WRITES_AS,
            ],
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
        let mut open = Vec::new();
        for (i, (event, verdict)) in events.iter().zip(verdicts.iter_mut()).enumerate() {
            *verdict = self.writes(event);
            if verdict.is_ok() {
                open.push(i);
            }
        }
        if open.is_empty() {
            return;
        }
        let read = self.read(events, &open);
        match self.lists.judge(&read) {
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

    fn writes(&self, event: &TodoEvent) -> Result<(), Shut> {
        if self.writers.contains(event.actor()) {
            return Ok(());
        }
        let named: Vec<&str> = self.writers.iter().map(Actor::as_str).collect();
        Err(Shut::Refused(format!(
            "a public store takes writes from {named:?} alone, and not from {} here: the \
             secretary's and the triage agent's writes go to a private chain",
            event.actor().as_str()
        )))
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
        let scratch = suzatary_io::scratch::Scratch::temporary("door").map_err(|e| failed(&e))?;
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
    fn judge(&self, events: &[Reading]) -> Result<Vec<Result<(), Shut>>, String> {
        let words: Vec<String> = super::instance::words(&self.instance)
            .map_err(|e| format!("the deployment's own words cannot be read ({e})"))?
            .into_iter()
            .collect();
        let matcher = Matcher::new(&self.terms()?, &words, &[], Scope::Whole);
        let mut verdicts: Vec<Result<(), Shut>> = events
            .iter()
            .map(|reading| matched(&matcher, reading))
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
    let text = suzatary_io::mailbox::text::html_to_text(&html);
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
/// opaque.
fn matched(matcher: &Matcher, reading: &Reading) -> Result<(), Shut> {
    for (site, text) in &reading.texts {
        if let Some(layer) = matcher.hits(text).into_values().min() {
            return Err(refused(site, layer));
        }
        if let Some(opaque) = Opaque::of(text.as_bytes()) {
            return Err(refused(site, opaque));
        }
    }
    Ok(())
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
