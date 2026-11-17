//! `suzatary prodrome draw`: a public store's open list as one picture, `roadmap.svg` and
//! `roadmap-dark.svg`, and the `README.md` that shows it, written beside the store; and door 2's
//! question, what those pictures are when the store at a commit is drawn again.
//!
//! The picture is a function of the store's objects and of the renderer this binary pins: the
//! app's one Typst path (`suzatary_io::typeset::compile`, typst's HTML export of one
//! `html.frame`, which typst draws as SVG), the design system the package links, and typst's own
//! faces compiled into the binary. Nothing else reaches it: no clock (prices are read at the
//! instant of the store's latest event), no font of the machine, no path, no locale; every map is
//! ordered. Typst draws a glyph as an outline, so no guard can read the picture's words: door 2
//! admits a picture only when drawing the store again gives its exact bytes, and then its words
//! are the words of objects door 1 has read.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, bail};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use schemars::JsonSchema;
use serde::Serialize;
use sha2::Digest as _;
use suzatary_base::capability::Typeset;
use suzatary_base::chains::ChainName;
use suzatary_signatures::chain_set::Read;
use suzatary_store::store::Reads as _;
use suzatary_todo::template::{Kept, Library, Output, Template};

use crate::instance::charter::Charter;
use crate::stamp::Package;

/// The light picture, beside the store.
pub(crate) const LIGHT: &str = "roadmap.svg";
/// The dark picture, beside the store.
pub(crate) const DARK: &str = "roadmap-dark.svg";
/// The page that shows the pictures, beside the store.
pub(crate) const README: &str = "README.md";

/// The rows a picture draws; the rest are counted beneath them. The README is a preview of the
/// page it links to, and a picture of every row of a long list would outgrow what GitHub inlines.
const ROWS: usize = 30;

/// What `prodrome draw` takes.
#[derive(Debug, clap::Args)]
pub struct Drawing {
    /// The store, chartered public.
    #[arg(long, value_name = "DIR")]
    store: std::path::PathBuf,
    /// The page that hosts the list, which the README's picture links to.
    #[arg(
        long,
        value_name = "URL",
        required_unless_present = "admissions",
        conflicts_with = "admissions"
    )]
    link: Option<String>,
    /// Write nothing; print each picture as drawn now, `<sha256>  <path>` as `sha256sum` prints
    /// it, for door 2 (`scripts/public-scan.sh`). Prints nothing for a store not chartered public.
    #[arg(long)]
    admissions: bool,
}

/// What a run of `prodrome draw` does, parsed once from its flags.
enum Asked {
    /// Write the pictures and the README linking to this page.
    Write(Link),
    /// Print what door 2 admits.
    Admissions,
}

/// The address the README links to: `https://`, and nothing that would end an attribute.
struct Link(String);

impl Link {
    fn parse(text: &str) -> anyhow::Result<Link> {
        let ok = text.starts_with("https://")
            && text.len() > "https://".len()
            && !text
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || "\"'<>`\\".contains(c));
        if !ok {
            bail!("--link {text:?} is not an https address an attribute can hold");
        }
        Ok(Link(text.to_owned()))
    }
}

/// The light and dark pictures, linked to `link`: the README's one element.
fn shown(link: &Link) -> String {
    format!(
        "<a href=\"{}\"><picture><source media=\"(prefers-color-scheme: dark)\" \
         srcset=\"{DARK}\"><img alt=\"The open tasks, most urgent first\" src=\"{LIGHT}\">\
         </picture></a>\n",
        link.0
    )
}

/// The picture's value: its theme, and the list.
#[derive(Debug, Clone, Serialize, JsonSchema)]
struct Picture {
    theme: Theme,
    list: Listed,
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
enum Theme {
    Light,
    Dark,
}

/// The open list as a picture reads it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "state")]
enum Listed {
    /// The store holds no event: no todo, and no instant to price one at.
    Begun,
    /// Priced at the instant of the store's latest event, `YYYY-MM-DD HH:MM` in UTC.
    Priced {
        at: String,
        rows: Vec<Line>,
        /// The open todos past [`ROWS`].
        more: usize,
    },
}

/// One open todo.
#[derive(Debug, Clone, Serialize, JsonSchema)]
struct Line {
    /// Its body as one line of plain text ([`title`]).
    title: String,
    price: Price,
    /// Its record's candidates: more than one is a conflict, and the row shows the candidate
    /// of the least object name, as every one-record reading does, and says how many there are.
    records: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "kind")]
enum Price {
    /// A fulfillment in [0, 1]; a conflict prices as its most urgent world.
    Valued { value: f64 },
    /// `∅`: no price.
    Absent,
    /// Its price names a todo the store does not hold.
    Unlinked,
}

static PICTURE: Template<Picture> = Template::new(
    "/picture.typ",
    &[
        ("/picture.typ", include_str!("picture.typ")),
        ("/tokens.typ", include_str!("../../../../../tokens.typ")),
    ],
    Library::Open,
    Output::Document,
    Kept::Nowhere,
    &[],
);

/// The store's two pictures, as drawn now.
pub(crate) struct Drawn {
    light: String,
    dark: String,
}

impl Drawn {
    /// Each picture beside its file's name.
    fn files(&self) -> [(&'static str, &str); 2] {
        [(LIGHT, &self.light), (DARK, &self.dark)]
    }
}

/// `suzatary prodrome draw`.
pub(crate) fn run(args: &Drawing) -> anyhow::Result<()> {
    // clap takes exactly one of `--link` and `--admissions`.
    let asked = match &args.link {
        Some(link) => Asked::Write(Link::parse(link)?),
        None => Asked::Admissions,
    };
    let public = Charter::read(&args.store)? == Charter::Public;
    match (asked, public) {
        (Asked::Admissions, false) => Ok(()),
        (Asked::Write(_), false) => bail!(
            "{} is not chartered public, and draw draws a picture to publish",
            args.store.display()
        ),
        (Asked::Admissions, true) => {
            for (path, svg) in drawn(&args.store)?.files() {
                println!(
                    "{}  {path}",
                    hex::encode(sha2::Sha256::digest(svg.as_bytes()))
                );
            }
            Ok(())
        }
        (Asked::Write(link), true) => {
            let drawn = drawn(&args.store)?;
            let readme = shown(&link);
            for (path, text) in drawn.files().into_iter().chain([(README, readme.as_str())]) {
                let at = args.store.join(path);
                suzatary_io::durable::replace(
                    &at,
                    text.as_bytes(),
                    suzatary_io::durable::Mode::Readable,
                )
                .with_context(|| format!("writing {}", at.display()))?;
            }
            println!(
                "drew {} and {} and wrote {README} in {}",
                LIGHT,
                DARK,
                args.store.display()
            );
            Ok(())
        }
    }
}

/// The store at `store` drawn in both themes.
fn drawn(store: &Path) -> anyhow::Result<Drawn> {
    let list = listed(store, &suzatary_io::typeset::Closed::this_binary())?;
    let packages = packages(&Package::running().typst_packages()?)?;
    suzatary_io::typeset::bundled_fonts();
    let draw = |theme| -> anyhow::Result<String> {
        let called = PICTURE
            .call(&Picture {
                theme,
                list: list.clone(),
            })
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut job = called.job().clone();
        job.files.extend(packages.clone());
        let html = suzatary_io::typeset::compile(&job).map_err(|refused| {
            anyhow::anyhow!(
                "the picture does not draw: {}",
                refused.said(job.main_text())
            )
        })?;
        framed(&html)
    };
    Ok(Drawn {
        light: draw(Theme::Light)?,
        dark: draw(Theme::Dark)?,
    })
}

/// The open list of the store at `store`, priced at the instant of its latest event.
fn listed(store: &Path, typeset: &dyn Typeset) -> anyhow::Result<Listed> {
    let objects = crate::commands::chain_at(store.to_owned(), Untrusted::none())
        .objects()
        .context("reading the store")?;
    let Some(at) = objects
        .iter()
        .filter_map(|(_, envelope)| envelope.event().map(suzatary_record::record::TodoEvent::at))
        .max()
    else {
        return Ok(Listed::Begun);
    };
    let name = ChainName::new("store")?;
    let read = Read::of_objects([(name.clone(), Untrusted::none(), objects)]);
    let set = read.set(at);
    let nodes = set.held(&name).context("reading the store")?.nodes;
    let entries = set.entries(&name).context("folding the store")?;
    let records = suzatary_record::decoration::records_of(nodes);
    let open = suzatary_todo::shown::open(&entries, &records);
    let more = open.len().saturating_sub(ROWS);
    let rows = open
        .into_iter()
        .take(ROWS)
        .map(|(entry, record)| Line {
            title: title(record.payload.body.as_str(), typeset),
            price: match entry.value() {
                Ok(Some(value)) => Price::Valued { value },
                Ok(None) => Price::Absent,
                Err(_) => Price::Unlinked,
            },
            records: entry.content().len(),
        })
        .collect();
    Ok(Listed::Priced {
        at: minute(at),
        rows,
        more,
    })
}

/// A body as one line of text: the text of what the closed world makes of it, in the child that
/// caps its time and memory, or its source as it stands where the closed world refuses it. A
/// writer's markup is never evaluated in the picture's own compile.
fn title(body: &str, typeset: &dyn Typeset) -> String {
    let text = crate::commands::door::drawing(typeset, body)
        .map_or_else(|_| body.to_owned(), |drawn| drawn.text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn minute(at: Datetime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        at.year(),
        at.month(),
        at.day(),
        at.hour(),
        at.minute()
    )
}

/// The one frame of typst's HTML document, as an SVG file: the document's body must be exactly one
/// `<svg>`, and its root loses the inline `style` typst gives a frame placed in a page, so its
/// `width` and `height` attributes size it.
fn framed(html: &str) -> anyhow::Result<String> {
    let body = html
        .split_once("<body>")
        .and_then(|(_, rest)| rest.rsplit_once("</body>"))
        .map(|(body, _)| body)
        .filter(|body| body.starts_with("<svg ") && body.ends_with("</svg>"))
        .ok_or_else(|| anyhow::anyhow!("the picture's document is not one frame"))?;
    let (root, rest) = body
        .split_once('>')
        .ok_or_else(|| anyhow::anyhow!("the frame's root tag does not end"))?;
    let root = match root.split_once(" style=\"") {
        Some((before, after)) => {
            let (_, after) = after
                .split_once('"')
                .ok_or_else(|| anyhow::anyhow!("the frame's style does not end"))?;
            format!("{before}{after}")
        }
        None => root.to_owned(),
    };
    Ok(format!("{root}>{rest}\n"))
}

/// Every Typst package under the package path `root` (`<root>/<namespace>/<name>/<version>/`),
/// keyed as a compile finds a file (`@local/typst-design:0.1.0/lib.typ`): the manifest and every
/// `.typ` file its `exclude` does not name, as `scripts/typst-packages.mjs` bundles them for the
/// tab.
fn packages(root: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    #[derive(serde::Deserialize)]
    struct Manifest {
        package: Excluded,
    }
    #[derive(serde::Deserialize)]
    struct Excluded {
        #[serde(default)]
        exclude: Vec<String>,
    }
    let read = |path: &Path| {
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
    };
    let mut files = BTreeMap::new();
    for home in below(root, 3)? {
        let Some(spec) = home
            .strip_prefix(root)?
            .to_str()
            .and_then(|spec| spec.rsplit_once('/'))
            .map(|(name, version)| format!("@{name}:{version}"))
        else {
            bail!("{}: a package path that is not text", home.display());
        };
        let Manifest { package } = toml::from_str(&read(&home.join("typst.toml"))?)
            .with_context(|| format!("{}/typst.toml", home.display()))?;
        for path in walked(&home)? {
            let rel = path.strip_prefix(&home)?.to_string_lossy().into_owned();
            let excluded = package
                .exclude
                .iter()
                .any(|x| rel == *x || rel.starts_with(&format!("{x}/")));
            let typst = Path::new(&rel)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("typ"));
            if (rel == "typst.toml" || typst) && !excluded {
                files.insert(format!("{spec}/{rel}"), read(&path)?);
            }
        }
    }
    if files.is_empty() {
        bail!("no Typst package under {}", root.display());
    }
    Ok(files)
}

/// The directories `depth` levels below `dir`.
fn below(dir: &Path, depth: usize) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut found = vec![dir.to_owned()];
    for _ in 0..depth {
        let mut next = Vec::new();
        for dir in found {
            for entry in
                std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?
            {
                let path = entry?.path();
                if path.is_dir() {
                    next.push(path);
                }
            }
        }
        found = next;
    }
    Ok(found)
}

/// Every file below `dir`.
fn walked(dir: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(walked(&path)?);
        } else {
            files.push(path);
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use suzatary_io::typeset::InProcess;

    use super::title;

    /// A body the closed world takes is drawn as the text it makes; one it refuses (it reaches for
    /// `plugin`) as its source; either is one line.
    #[test]
    fn a_title_is_the_closed_worlds_text_or_the_source() {
        assert_eq!(title("Ship *it*\n now", &InProcess), "Ship it now");
        let plugin = "Load #plugin(bytes(()))";
        assert_eq!(title(plugin, &InProcess), plugin);
        let paged = title("Before #pagebreak() after", &InProcess);
        assert!(!paged.contains('\n'), "{paged}");
    }
}
