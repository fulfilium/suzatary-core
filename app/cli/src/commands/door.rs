//! Door 1 of a public store: an event reaches a store whose charter says `public = true` only
//! from a writer that store takes, and only after the leak guard has read every string it holds.
//! A `Change` rests on its todo's heads, so a stored object is an ancestor of every later write
//! to that todo, and a published history never takes a word back: the check runs before the
//! store holds the object, or not at all.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use prodrome_core::event::Actor;
use prodrome_core::literal::Value;
use suzatary_base::chains::ChainPolicy;
use suzatary_held::leak::{Matcher, Opaque, Scope};
use suzatary_record::record::TodoEvent;
use suzatary_store::store::{Rule, Shut};

use crate::instance::Local;

/// Where a chain's store is opened: how the box tells apart two writers that author as `assistant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    /// A command at the box's terminal, where `assistant` is the main session.
    AtTerminal,
    /// The server and the secretary's commands, where `assistant` is the in-app secretary: its
    /// model runs with no tools (`suzatary_io::model`), so it writes through no terminal.
    ByTheBox,
}

/// What door 1 reads at each write: the box-local tier's two term lists and gitleaks's rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lists {
    terms: [PathBuf; 2],
    rules: PathBuf,
}

impl Lists {
    pub(crate) fn of(local: &Local, code: &Path) -> Self {
        Lists {
            terms: [local.personal_terms(), local.context_terms()],
            rules: code.join("scripts").join("secret-rules.toml"),
        }
    }

    /// Refused with the field and the rule's kind, never the words.
    fn judge(&self, event: &TodoEvent) -> Result<(), Shut> {
        let matcher = Matcher::new(&self.terms()?, &[], &[], Scope::Whole);
        let value = event.to_value();
        let mut fields = Vec::new();
        strings(String::new(), &value, &mut fields);
        for (field, text) in &fields {
            if let Some(layer) = matcher.hits(text).into_values().min() {
                return Err(refused(field, layer));
            }
            if let Some(opaque) = Opaque::of(text.as_bytes()) {
                return Err(refused(field, opaque));
            }
        }
        self.secrets(&fields)
    }

    /// Both lists' terms, read as `scripts/public-tree.sh` reads them.
    fn terms(&self) -> Result<Vec<String>, Shut> {
        let mut terms = Vec::new();
        for path in &self.terms {
            let text = std::fs::read_to_string(path).map_err(|e| {
                unjudged(&format!(
                    "the box-local tier's {} cannot be read ({})",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    e.kind()
                ))
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

    /// gitleaks over the fields, its first finding's line mapped back to its field.
    fn secrets(&self, fields: &[(String, &str)]) -> Result<(), Shut> {
        let failed =
            |why: &dyn std::fmt::Display| unjudged(&format!("gitleaks did not run ({why})"));
        let scratch = suzatary_io::scratch::Scratch::temporary("door").map_err(|e| failed(&e))?;
        let template = scratch.path().join("template");
        std::fs::write(
            &template,
            "{{ range . }}{{ .StartLine }}\t{{ .RuleID }}\n{{ end }}",
        )
        .map_err(|e| failed(&e))?;
        let mut text = String::new();
        let mut starts = Vec::new();
        for (field, value) in fields {
            starts.push((text.matches('\n').count() + 1, field));
            text.push_str(value);
            text.push('\n');
        }
        let mut scanner = Command::new("gitleaks")
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
        match ran.status.code() {
            Some(0) => Ok(()),
            Some(3) => {
                let report = String::from_utf8_lossy(&ran.stdout);
                let (line, rule) = report
                    .lines()
                    .find_map(|found| found.split_once('\t'))
                    .and_then(|(line, rule)| Some((line.parse::<usize>().ok()?, rule)))
                    .ok_or_else(|| failed(&"its report names no line"))?;
                let field = starts.iter().rev().find(|(start, _)| *start <= line);
                Err(refused(
                    field.map_or("", |(_, field)| field.as_str()),
                    format!("holds a secret (gitleaks rule {rule})"),
                ))
            }
            _ => Err(failed(&ran.status)),
        }
    }
}

/// The rule of a public chain's store opened from `opened`.
pub(crate) fn rule(lists: Lists, policy: &ChainPolicy, opened: Opened) -> Rule<TodoEvent> {
    let writers = writers(policy, opened);
    Box::new(move |event| {
        if !writers.contains(event.actor()) {
            let named: Vec<&str> = writers.iter().map(Actor::as_str).collect();
            return Err(Shut::Refused(format!(
                "a public store takes writes from {named:?} alone, and not from {} here: the \
                 secretary's and the triage agent's writes go to a private chain",
                event.actor().as_str()
            )));
        }
        lists.judge(event)
    })
}

/// The roster's trusted actors, never the triage agent, and from the box never the secretary.
fn writers(policy: &ChainPolicy, opened: Opened) -> BTreeSet<Actor> {
    let barred: &[suzatary_base::writer::Writer] = match opened {
        Opened::AtTerminal => &[suzatary_core::agent::WRITES_AS],
        Opened::ByTheBox => &[
            suzatary_core::agent::WRITES_AS,
            suzatary_core::secretary::WRITES_AS,
        ],
    };
    policy
        .roster
        .iter()
        .filter(|actor| !policy.untrusted.actors().contains(*actor))
        .filter(|actor| !barred.iter().any(|writer| writer.actor() == actor.as_str()))
        .cloned()
        .collect()
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

fn refused(field: &str, kind: impl std::fmt::Display) -> Shut {
    Shut::Refused(format!(
        "a public store holds no event whose field {field} {kind}"
    ))
}

fn unjudged(why: &str) -> Shut {
    Shut::Unjudged(format!(
        "door 1 reads every write to a public store, and {why}, so it takes none"
    ))
}
