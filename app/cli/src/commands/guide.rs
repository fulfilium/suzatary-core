//! The guide (`docs/guide/guide.typ`): one Typst text, compiled to HTML in the closed world and
//! to a PDF by typst (`nix build .#docs`), around a demo deployment that is `init`'s own answer
//! to the cast's flags.
//!
//! The text calls four names beyond typst's library, and each world defines them: `shot`, a
//! picture (a link to it in the closed world, which reads no file), and `demo-command`,
//! `demo-instance` and `demo-steps`, the demo's command, file and printed steps as raw blocks.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use suzatary_core::cast::CAST;
use suzatary_core::escape::escape_string;

use super::init;

/// The world a guide is compiled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum World {
    /// The app's own: typst's library without `html`, `plugin`, files or packages.
    Closed,
    /// typst with the design system and the pictures beside the text.
    Pdf,
}

/// What `suzatary-dev guide` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The guide's directory, holding `guide.typ`.
    #[arg(long)]
    from: PathBuf,
    /// Where `guide.html` and the PDF's source `guide.typ` are written.
    #[arg(long)]
    out: PathBuf,
}

/// The flags the cast's deployment was begun with: the fixture's owner and her one account.
const DEMO: [&str; 14] = [
    "--name",
    CAST.owner.name,
    "--city",
    CAST.city,
    "--zone",
    "Europe/London",
    "--box",
    "ada-box.tailnet.ts.net",
    "--login",
    CAST.owner.email,
    "--email",
    CAST.owner.email,
    "--mail-provider",
    "gmail",
];

/// How the guide's reader runs the app: the template's script, from their repository.
const SCRIPT: &str = "./suzatary";

/// The design system's working document, for the PDF; the closed world has no package.
const PDF: &str = r#"#import "@local/typst-design:0.1.0": working-document
#show: working-document
#let shot(file, caption) = figure(box(width: 100%, height: 9cm, image(file, fit: "contain")), caption: caption)
"#;

const CLOSED: &str = "#let shot(file, caption) = block(link(file, emph(caption)))\n";

/// The guide in `world`: its preamble, the demo, then the text.
///
/// # Errors
/// The demo's flags no longer make an onboarding.
pub fn source(world: World, guide: &str) -> anyhow::Result<String> {
    let onboarding = init::answered(DEMO)?;
    let command = format!("$ {SCRIPT} init {}", flags());
    let raw = |lang: &str, text: &str| {
        format!(
            "raw(block: true, lang: \"{lang}\", \"{}\")",
            escape_string(text)
        )
    };
    let steps: Vec<String> = onboarding
        .steps(&suzatary_core::connect::Reach::Script(SCRIPT.to_owned()))
        .iter()
        .map(ToString::to_string)
        .collect();
    Ok(format!(
        "{}#let demo-command = {}\n#let demo-instance = {}\n#let demo-steps = {}\n\n{guide}",
        match world {
            World::Closed => CLOSED,
            World::Pdf => PDF,
        },
        raw("console", &command),
        raw(
            "toml",
            &onboarding.printed(&suzatary_core::cast::keys::root())?
        ),
        raw(
            "console",
            &format!("next, at a terminal:\n{}", steps.join("\n"))
        ),
    ))
}

/// The demo's flags as a shell line, a value after its flag, quoted where it has a space.
fn flags() -> String {
    DEMO.iter()
        .map(|word| {
            if word.contains(' ') {
                format!("'{word}'")
            } else {
                (*word).to_owned()
            }
        })
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|pair| pair.join(" "))
        .collect::<Vec<_>>()
        .join(" \\\n    ")
}

/// The guide as HTML, compiled in the closed world and past the tripwire.
///
/// # Errors
/// The text does not compile there, or the tripwire refuses what it made.
pub fn html(guide: &str) -> anyhow::Result<String> {
    let html = suzatary_io::typeset::compile(&suzatary_core::template::Job::text(&source(
        World::Closed,
        guide,
    )?))
    .map_err(|rejected| anyhow::anyhow!("the guide does not compile: {rejected:?}"))?;
    if let Some(reason) = suzatary_core::tripwire::suspicious(&html) {
        anyhow::bail!("the tripwire refuses the guide: {reason}");
    }
    Ok(html)
}

/// `suzatary-dev guide`: write `guide.html` and the PDF's source beside each other.
///
/// # Errors
/// The text is unreadable, does not compile, or an output cannot be written.
pub fn run(args: &Args) -> anyhow::Result<()> {
    let text = read(&args.from.join("guide.typ"))?;
    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("creating {}", args.out.display()))?;
    write(&args.out.join("guide.html"), &html(&text)?)?;
    write(&args.out.join("guide.typ"), &source(World::Pdf, &text)?)
}

fn read(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}
