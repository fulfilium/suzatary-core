//! `suzatary plugin sync`: run each plugin `[[plugins]]` declares and learn what it reports into
//! the cache tier; every Google account's calendars and the Drive's probe ride the same timer.
//!
//! Each item is admitted at the plugin's declared schema and learned only where its store does
//! not already read it as reported, so a timer may sync as often as it likes. The cursor is saved
//! after the learning, so a run cut short is run again. Each run prints one journal line and joins
//! what came of it, and of each link it fetched, into the sources' health, which Settings reads.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use clap::{Args as ClapArgs, Subcommand};
use prodrome_core::declared::Declared;
use prodrome_core::fpl::instant_of;
use suzatary_core::capability::Clock as _;
use suzatary_core::plugin::{Answer, Came, CredentialName, LastRun, Manifest, admit};
use suzatary_core::store::{Learnt, learn};
use suzatary_io::clock::SystemClock;
use suzatary_io::durable;
use suzatary_io::http::Ureq;
use suzatary_io::objects::Objects;
use suzatary_io::plugin_store::{Begun, PluginStore};
use suzatary_io::present::Present as _;
use suzatary_plugin::{Budget, Effects, Fetch, Host, Run, Source};

use crate::instance::{Instance, Plugins};

/// Where the app's reference plugins are, when no `--components` names a directory: the box's
/// timer roots `.#suzatary-plugins` and passes it here.
pub const COMPONENTS: &str = "SUZATARY_PLUGINS";

/// `suzatary plugin …`.
#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Sync each named plugin into the cache tier (every declared one, and every Google
    /// account's calendars and the Drive's probe, when none is named).
    Sync(SyncArgs),
}

/// `suzatary plugin sync`.
#[derive(Debug, ClapArgs)]
pub struct SyncArgs {
    /// The plugins to sync, by name; every plugin `[[plugins]]` declares when none is named.
    names: Vec<String>,
    /// The directory the plugins' components and schemas are read from: `$SUZATARY_PLUGINS`,
    /// else the data root.
    #[arg(long)]
    components: Option<PathBuf>,
}

/// Run it.
///
/// # Errors
/// An unreadable instance, a plugin not declared, or any plugin's run that did not end with
/// what it learned stored; every plugin named still runs.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    match &args.command {
        Command::Sync(args) => sync_all(path, args),
    }
}

fn sync_all(path: &Path, args: &SyncArgs) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let plugins = &instance.plugins;
    let chosen: Vec<&Manifest> = if args.names.is_empty() {
        plugins.manifests.iter().collect()
    } else {
        args.names
            .iter()
            .map(|name| {
                plugins.named(name).with_context(|| {
                    format!("no plugin named {name:?} in {}", instance.path.display())
                })
            })
            .collect::<anyhow::Result<_>>()?
    };
    let health = suzatary_io::source_health::beside(&instance.mail.heartbeat);
    let (google, calendars_stored) = if args.names.is_empty() {
        probe_drive(&instance, &health);
        super::google::sync_calendars(&instance, &health)
    } else {
        (Vec::new(), true)
    };
    for line in &google {
        println!("{line}");
    }
    let mut unstored: Vec<String> = Vec::new();
    if !calendars_stored {
        unstored.push("a Google account's calendars".to_owned());
    }
    if chosen.is_empty() && google.is_empty() {
        println!("plugin: none declared in {}", instance.path.display());
    }
    if !chosen.is_empty() {
        let components = components(&instance, args.components.clone());
        let host = Host::new(Budget::default()).context("starting the plugin host")?;
        let effects = effects(&instance);
        for manifest in chosen {
            let report = sync(&host, manifest, &components, plugins, &effects, &health);
            println!("{}", report.line);
            if report.learnt.is_none() {
                unstored.push(manifest.name().to_string());
            }
        }
    }
    if unstored.is_empty() {
        Ok(())
    } else {
        bail!(
            "plugin sync did not store {}: the lines above say why",
            unstored.join(", ")
        )
    }
}

/// List every Drive folder the secretary may read and join what came of it into `health`; a
/// register that cannot be written is a journal line, since the next run probes again.
fn probe_drive(instance: &Instance, health: &suzatary_io::source_health::Health) {
    use suzatary_core::capability::Drive as _;
    let drive = super::serve::interpreters::DriveAccess::of(instance);
    let at = instant_of(SystemClock.now());
    let probed = suzatary_core::drive::probe(drive.port(), &drive.allowed_folders(), at);
    if let Err(failed) = health.join(&probed) {
        println!("drive: its probe was not recorded: {failed}");
    }
}

/// Where the plugins are read from: `named`, else `$SUZATARY_PLUGINS`, else the data root.
#[must_use]
pub fn components(instance: &Instance, named: Option<PathBuf>) -> PathBuf {
    named
        .or_else(|| std::env::var_os(COMPONENTS).map(PathBuf::from))
        .unwrap_or_else(|| instance.data().to_path_buf())
}

/// What a plugin's sync reaches: the transport bounding every plugin (no redirect, nothing
/// resolving off the public internet), the instance's secret store, and the clock.
#[must_use]
pub fn effects(instance: &Instance) -> Effects {
    Effects {
        http: Arc::new(Ureq::public_only()),
        secrets: Arc::new(instance.secrets()),
    }
}

/// What one plugin's sync came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The journal's line.
    pub line: String,
    /// What the store learned, or `None` when the run's learning was not stored (a run that
    /// reports nothing stores nothing, and that is stored).
    pub learnt: Option<Learnt>,
}

/// Sync one plugin with `effects`: load its component from `components`, run it from its
/// cursor, admit each item into its declared schema, learn the admitted into its store, save the
/// cursor, and join what came of the run and of each link it fetched into `health`.
///
/// Never an error: whatever went wrong is the report's line and the plugin's health, and the
/// next run tries again.
#[must_use]
pub fn sync(
    host: &Host,
    manifest: &Manifest,
    components: &Path,
    plugins: &Plugins,
    effects: &Effects,
    health: &suzatary_io::source_health::Health,
) -> Report {
    let name = manifest.name();
    let at = SystemClock.now();
    let (report, fetches) = sync_once(host, manifest, components, plugins, effects);
    let came = match report.learnt {
        Some(_) => Came::Learnt,
        None => Came::Failed {
            why: report.line.clone(),
        },
    };
    let run = LastRun {
        at: instant_of(at),
        came,
    };
    if let Err(err) = health.join(&run.health(manifest, &fetches)) {
        eprintln!("plugin {name}: its health was not recorded: {err}");
    }
    report
}

/// One run of `manifest`'s plugin, and the fetches it made under a declared credential.
fn sync_once(
    host: &Host,
    manifest: &Manifest,
    components: &Path,
    plugins: &Plugins,
    effects: &Effects,
) -> (Report, Vec<(CredentialName, Answer)>) {
    let name = manifest.name();
    let dir = plugins.beside(name);
    let failed = |why: String| {
        let line = format!("plugin {name}: {why}");
        (Report { line, learnt: None }, Vec::new())
    };
    let source = match bundle(manifest, components).and_then(|(component, schema)| {
        host.load(manifest.clone(), &component, &schema)
            .map_err(|refused| format!("not loaded: {refused}"))
    }) {
        Ok(source) => source,
        Err(why) => return failed(why),
    };
    let (store, cursor) = match PluginStore::at(plugins.store(name)).begin(source.schema()) {
        // A store begun afresh holds nothing its cursor says was learned, so the run starts
        // from no cursor and reports everything in its window again.
        Ok(Begun::Afresh(store)) => (store, None),
        Ok(Begun::Kept(store)) => match std::fs::read(dir.join("cursor")).present() {
            Ok(cursor) => (store, cursor),
            Err(err) => return failed(format!("its cursor could not be read: {err}")),
        },
        Err(err) => return failed(format!("its store could not begin: {err}")),
    };
    let run = source.sync(effects, cursor.as_deref());
    for said in &run.said {
        eprintln!("plugin {name} said: {said}");
    }
    let fetches: Vec<(CredentialName, Answer)> = run.fetches.iter().filter_map(link).collect();
    let (learnt, what) = match learned(&source, &store, &dir, &run) {
        Ok((learnt, what)) => (Some(learnt), what),
        Err(what) => (None, what),
    };
    let links: Vec<String> = fetches
        .iter()
        .map(|(link, answer)| match answer {
            Answer::Status(status) => format!("{link} {status}"),
            Answer::Refused(why) => format!("{link} refused ({why})"),
        })
        .collect();
    let line = format!(
        "plugin {name}: {what}; links: {}",
        if links.is_empty() {
            "none fetched".to_owned()
        } else {
            links.join(", ")
        },
    );
    (Report { line, learnt }, fetches)
}

/// The plugin's component and its declared schema, each read once from under `components`; the
/// bytes returned are the bytes the host loads.
fn bundle(manifest: &Manifest, components: &Path) -> Result<(Vec<u8>, String), String> {
    let component = inside(components, manifest.component().as_str())?;
    let bytes = std::fs::read(&component)
        .map_err(|err| format!("its component {} is unreadable: {err}", component.display()))?;
    let schema = inside(components, manifest.schema().as_str())?;
    let text = std::fs::read_to_string(&schema)
        .map_err(|err| format!("its schema {} is unreadable: {err}", schema.display()))?;
    Ok((bytes, text))
}

/// `relative` under `root`, once every link in it is followed: a manifest's path is plain
/// segments, and this refuses one that a link inside the root turns into a path outside it.
fn inside(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let base = root
        .canonicalize()
        .map_err(|err| format!("its bundle's root {} is unreadable: {err}", root.display()))?;
    let path = base
        .join(relative)
        .canonicalize()
        .map_err(|err| format!("{relative} is unreadable under {}: {err}", root.display()))?;
    if path.starts_with(&base) {
        Ok(path)
    } else {
        Err(format!(
            "{relative} resolves outside {}, and a bundle is read only from under its root",
            root.display()
        ))
    }
}

/// Admit and learn the run's items, then save its cursor: what was learnt and the line's opening
/// clause, or, when nothing was stored, only the clause. A run with an item its schema refuses
/// learns nothing and keeps its old cursor, so the next run asks again.
fn learned(
    source: &Source,
    store: &Objects<Declared>,
    dir: &Path,
    run: &Run,
) -> Result<(Learnt, String), String> {
    let synced = run
        .result
        .as_ref()
        .map_err(|failed| format!("the run failed: {failed}"))?;
    let events = synced
        .items
        .iter()
        .map(|item| admit(source.schema(), source.manifest().name(), item))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|refused| format!("refused an item: {refused}"))?;
    let learnt = learn(store, events).map_err(|err| format!("its store refused: {err}"))?;
    let what = format!(
        "{} item{}, {} written, {} known",
        synced.items.len(),
        if synced.items.len() == 1 { "" } else { "s" },
        learnt.written,
        learnt.known,
    );
    match durable::replace(&dir.join("cursor"), &synced.cursor, durable::Mode::Readable) {
        Ok(()) => Ok((learnt, what)),
        // Learnt and not saved: the next run reports the same items again and learns nothing.
        Err(err) => Ok((learnt, format!("{what}, its cursor unsaved ({err})"))),
    }
}

/// A fetch under the declared credential it named; none for a fetch that named none.
fn link(fetch: &Fetch) -> Option<(CredentialName, Answer)> {
    let answer = match &fetch.outcome {
        Ok(status) => Answer::Status(*status),
        Err(refusal) => Answer::Refused(refusal.clone()),
    };
    Some((fetch.credential.clone()?, answer))
}
