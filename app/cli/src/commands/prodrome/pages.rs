//! `suzatary prodrome pages`: a store written as static pages for a host that serves files and
//! runs nothing: the open list, and each todo's page, priced by the core at one instant and drawn
//! by Typst. The pages are a function of the store's objects, this application and the instant.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use prodrome_core::event::TodoId;
use prodrome_core::literal::Datetime;
use suzatary_base::chains::ChainName;
use suzatary_io::typeset::Compiler;
use suzatary_signatures::chain_set::Read;
use suzatary_todo::pages::{Linked, Listed, TodoPage, TodosPage};
use suzatary_todo::rendering::{TODO, TODOS};
use suzatary_todo::shown::{Shown, open_rows, shown};
use suzatary_todo::template::Called;

use crate::instance::charter::Charter;

/// What `prodrome pages` takes.
#[derive(Debug, clap::Args)]
pub struct Paging {
    /// The store to draw.
    #[arg(long, value_name = "DIR")]
    store: PathBuf,
    /// The instant every price is taken at, an ISO date or instant (UTC).
    #[arg(long, value_name = "INSTANT", value_parser = crate::commands::at::instant)]
    at: Datetime,
    /// The directory to write the pages into, absent or empty.
    #[arg(long, value_name = "DIR")]
    into: PathBuf,
    /// The chain name a reference gives this store, `todo:<NAME>/<id>`.
    #[arg(long, value_name = "NAME", default_value = "roadmap", value_parser = |name: &str| ChainName::new(name))]
    named: ChainName,
}

/// The app's own stylesheets, light and dark by the reader's settings, beside the pages.
const STYLESHEETS: [(&str, &str); 2] = [
    (
        "tokens.css",
        include_str!("../../../../../web/static/tokens.css"),
    ),
    (
        "style.css",
        include_str!("../../../../../web/static/style.css"),
    ),
];

const LIST: &str = "index.html";

pub(super) fn run(paging: &Paging) -> anyhow::Result<()> {
    if Charter::read(&paging.store)? != Charter::Public {
        bail!(
            "{} is not chartered public, and the pages publish everything it holds",
            paging.store.display()
        );
    }
    empty(&paging.into)?;
    let read = crate::commands::store_at::by_path(&paging.store)?;
    let site = site(
        read,
        paging.at,
        &paging.named,
        &suzatary_io::typeset::Closed::this_binary(),
    )?;
    write(&paging.into, &site)?;
    println!("wrote {} files into {}", site.len(), paging.into.display());
    Ok(())
}

/// Every file of the site by its path: the list, a page for each todo with content, open or
/// closed, and the stylesheets.
fn site(
    (read, name): (Read, ChainName),
    at: Datetime,
    named: &ChainName,
    compiler: &dyn Compiler,
) -> anyhow::Result<BTreeMap<String, String>> {
    let set = read.set(at);
    let held = set.held(&name).context("reading the store")?;
    let entries = set.entries(&name).context("folding the store")?;
    let by_hash = held
        .dag
        .objects()
        .iter()
        .map(|(name, envelope)| (name.clone(), envelope))
        .collect();
    let mut todos: Vec<(&TodoId, Shown)> = Vec::new();
    for entry in &entries {
        if let Some(one) = shown(entry, &by_hash).map_err(|r| anyhow::anyhow!("{}", r.message()))? {
            todos.push((&entry.key, one));
        }
    }
    let links: Vec<Linked> = todos
        .iter()
        .map(|(id, _)| Linked {
            reference: format!("{named}/{}", id.as_str()),
            page: format!("{}.html", id.as_str()),
        })
        .collect();
    let at = in_words(at);
    let rows = open_rows(
        &entries,
        &suzatary_record::decoration::records_of(held.nodes),
    );
    let list = TodosPage {
        at: at.clone(),
        rows: rows
            .into_iter()
            .map(|row| Listed {
                page: todo_path(&row.id),
                row,
            })
            .collect(),
    };
    let mut files: BTreeMap<String, String> = STYLESHEETS
        .iter()
        .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
        .collect();
    files.insert(
        LIST.to_owned(),
        page(&Page::List, &drawn(compiler, &TODOS.call(&list)?)?),
    );
    for (id, shown) in todos {
        let todo = TodoPage {
            at: at.clone(),
            list: format!("../{LIST}"),
            links: links.clone(),
            shown,
        };
        let body = drawn(compiler, &TODO.call(&todo)?).with_context(|| format!("drawing {id}"))?;
        files.insert(todo_path(id.as_str()), page(&Page::Todo(id), &body));
    }
    Ok(files)
}

fn todo_path(id: &str) -> String {
    format!("todo/{id}.html")
}

fn drawn(compiler: &dyn Compiler, called: &Called) -> anyhow::Result<String> {
    Ok(suzatary_io::render::drawn(compiler, called)?)
}

/// Which page a shell holds.
enum Page<'a> {
    List,
    Todo(&'a TodoId),
}

/// The smallest document around typst's body. A todo's page holds typst's rendering of a writer's
/// markup, which the app's stylesheet styles under `.rich`. A title is a constant or a todo's id,
/// and an id is `[a-z0-9_-]+`, so nothing in one needs escaping.
fn page(page: &Page<'_>, body: &str) -> String {
    let (title, up, main) = match page {
        Page::List => ("Open todos", "", "<main>"),
        Page::Todo(id) => (id.as_str(), "../", "<main class=\"rich\">"),
    };
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"color-scheme\" content=\"light dark\"><title>{title}</title>\
         <link rel=\"stylesheet\" href=\"{up}tokens.css\"><link rel=\"stylesheet\" href=\"{up}style.css\">\
         </head><body>{main}{body}</main></body></html>\n"
    )
}

/// "2026-11-16 10:03 UTC".
fn in_words(at: Datetime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        at.year(),
        at.month(),
        at.day(),
        at.hour(),
        at.minute()
    )
}

/// `dir`, made where absent, and empty, so no page of an earlier build stays beside this one's.
fn empty(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    if std::fs::read_dir(dir)?.next().is_some() {
        bail!(
            "{} is not empty; the pages are written into an empty directory",
            dir.display()
        );
    }
    Ok(())
}

fn write(dir: &Path, files: &BTreeMap<String, String>) -> anyhow::Result<()> {
    for (path, text) in files {
        let file = dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
    }
    Ok(())
}
