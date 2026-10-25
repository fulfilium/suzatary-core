//! `AGENT.md`, the guide a coding agent follows to set up a deployment: its options are
//! `suzatary init --options` to the byte, and everything it tells the agent to run exists.
//!
//! - Every invocation of the app names a verb of the deployed argument tree at every level,
//!   with flags that verb takes and a `--help` that answers, or a verb the template's script
//!   answers itself: `secret` with a name `secretspec.toml` declares, `tool` with one of the
//!   tools the script names, `which` and `shell` with nothing.
//! - Every unit `systemctl` or `journalctl` is pointed at is one the NixOS module declares,
//!   every flag given to a script is one its text spells, and every repository path is here.

use super::repo;

use clap::Subcommand as _;
use suzatary_cli::commands::init::options;
use suzatary_cli::verb::Verb;

fn guide() -> String {
    std::fs::read_to_string(repo().join("AGENT.md")).expect("AGENT.md reads")
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repo().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The deployed binary's argument tree, as `main.rs` builds it.
fn app() -> clap::Command {
    Verb::augment_subcommands(
        clap::Command::new("suzatary").arg(
            clap::Arg::new("instance")
                .long("instance")
                .global(true)
                .num_args(1),
        ),
    )
}

/// What the template's `suzatary` script answers without the app, read from the script.
struct Script {
    /// Its own verbs, its `case` arms.
    verbs: Vec<String>,
    /// The tools its header says `tool` reaches, by command name, and the shell they run in.
    tools: Vec<String>,
    /// The names `secretspec.toml` declares, which `secret` sets.
    secrets: Vec<String>,
    /// The plugin credentials the guide's own `[[plugins]]` tables declare.
    plugins: Vec<String>,
}

impl Script {
    fn read(guide: &str) -> Self {
        let script = read("templates/user/suzatary");
        let verbs = script
            .lines()
            .filter_map(|line| {
                let arm = line.trim_start().split_once(')')?.0;
                (!arm.is_empty() && arm.bytes().all(|b| b.is_ascii_lowercase()))
                    .then(|| arm.to_owned())
            })
            .collect();
        let header: String = script
            .lines()
            .take_while(|line| line.starts_with('#'))
            .map(|line| line.trim_start_matches('#').trim())
            .collect::<Vec<_>>()
            .join(" ");
        let named = header
            .split_once("tools (")
            .and_then(|(_, rest)| rest.split_once(')'))
            .expect("the script's header names its tools")
            .0;
        let tools = named
            .split(',')
            .filter_map(|tool| tool.split_whitespace().last())
            .map(str::to_owned)
            .chain(["sh".to_owned()])
            .collect();
        let secrets = read("secretspec.toml")
            .lines()
            .filter_map(|line| {
                line.split_once(" = {")
                    .map(|(name, _)| name.trim().to_owned())
            })
            .collect();
        let plugins = guide
            .lines()
            .filter_map(|line| line.trim().strip_prefix("credentials = ["))
            .flat_map(|rest| rest.trim_end_matches(']').split(','))
            .map(|name| name.trim().trim_matches('"').to_owned())
            .filter(|name| name.starts_with("PLUGIN_"))
            .collect();
        Self {
            verbs,
            tools,
            secrets,
            plugins,
        }
    }

    /// Why these words are not what the script answers, or `None` when they are.
    fn refused(&self, words: &[String]) -> Option<String> {
        let (verb, rest) = words.split_first()?;
        match verb.as_str() {
            "secret" => {
                let name = rest.first()?;
                // A placeholder, or a credential a manifest in the guide declares, which the
                // file every verb writes for its instance declares beside `secretspec.toml`.
                let placeholder = name == "NAME" || name.contains('<');
                let declared = self.secrets.contains(name) || self.plugins.contains(name);
                (!placeholder && !declared)
                    .then(|| format!("neither secretspec.toml nor a manifest declares {name}"))
            }
            "tool" => {
                let tool = rest.first()?;
                (!self.tools.contains(tool))
                    .then(|| format!("`{tool}` is not among the script's tools {:?}", self.tools))
            }
            "which" | "shell" => (!rest.is_empty()).then(|| format!("`{verb}` takes nothing")),
            _ => None,
        }
    }
}

/// Every piece of command text: each inline code span, read across the line breaks of its
/// paragraph, and each line of a fenced block with its continuations joined and its prompt
/// dropped.
fn commands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fenced = false;
    let mut pending = String::new();
    let mut paragraph = String::new();
    let spans = |paragraph: &mut String, out: &mut Vec<String>| {
        out.extend(paragraph.split('`').skip(1).step_by(2).map(str::to_owned));
        paragraph.clear();
    };
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            spans(&mut paragraph, &mut out);
            fenced = !fenced;
            continue;
        }
        if fenced {
            let line = line.trim();
            let line = line.strip_prefix("$ ").unwrap_or(line);
            if let Some(head) = line.strip_suffix('\\') {
                pending.push_str(head);
            } else {
                pending.push_str(line);
                out.push(std::mem::take(&mut pending));
            }
        } else if line.trim().is_empty() {
            spans(&mut paragraph, &mut out);
        } else {
            paragraph.push(' ');
            paragraph.push_str(line.trim());
        }
    }
    spans(&mut paragraph, &mut out);
    out
}

/// Each simple command in `text`, as words.
fn simple(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.split(['&', '|', ';', ')', '"'])
        .map(|segment| segment.split_whitespace().collect())
}

/// Each invocation of the app in `text`: the words after `suzatary` or `./suzatary`.
fn invocations(text: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for words in simple(text) {
        for (at, word) in words.iter().enumerate() {
            let word = word.rsplit("$(").next().unwrap_or(word);
            if word == "suzatary" || word == "./suzatary" {
                out.push(words[at + 1..].iter().map(|w| (*w).to_owned()).collect());
            }
        }
    }
    out
}

/// Why this invocation names nothing the app answers, or `None` when it does.
fn refused(app: &clap::Command, words: &[String]) -> Option<String> {
    let mut command = app;
    let mut path = vec!["suzatary"];
    let mut words = words.iter().peekable();
    while let Some(sub) = words
        .peek()
        .and_then(|word| command.find_subcommand(word.as_str()))
    {
        command = sub;
        path.push(words.next().expect("peeked"));
    }
    // A command with verbs under it takes one of them, at every level.
    if command.has_subcommands()
        && let Some(word) = words.peek().filter(|word| !word.starts_with('-'))
    {
        return Some(format!("`{}` has no verb `{word}`", path.join(" ")));
    }
    for word in words {
        if let Some(flag) = word.strip_prefix("--") {
            let flag = flag.split('=').next().unwrap_or(flag);
            if !takes(command, flag) {
                return Some(format!("`{}` takes no --{flag}", path.join(" ")));
            }
        } else if let Some(short) = word.strip_prefix('-').and_then(|rest| {
            let mut chars = rest.chars();
            chars.next().filter(|_| chars.next().is_none())
        }) && !takes_short(command, short)
        {
            return Some(format!("`{}` takes no -{short}", path.join(" ")));
        }
    }
    let argv: Vec<&str> = path.iter().copied().chain(["--help"]).collect();
    let answered = app
        .clone()
        .try_get_matches_from(argv)
        .is_err_and(|error| error.kind() == clap::error::ErrorKind::DisplayHelp);
    (!answered).then(|| format!("`{} --help` does not answer", path.join(" ")))
}

/// Whether `command` takes `--flag`, its own or a global one.
fn takes(command: &clap::Command, flag: &str) -> bool {
    flag == "help"
        || flag == "instance"
        || command
            .get_arguments()
            .any(|arg| arg.get_long() == Some(flag))
}

/// Whether `command` takes `-short`, its own or help's.
fn takes_short(command: &clap::Command, short: char) -> bool {
    short == 'h'
        || command
            .get_arguments()
            .any(|arg| arg.get_short() == Some(short))
}

/// The user units, system services and timers the NixOS module declares, by unit name.
fn declared_units() -> Vec<String> {
    let module = read("nix/nixos-module.nix");
    let mut out = Vec::new();
    for (prefix, suffix) in [
        ("systemd.user.services.", "service"),
        ("systemd.user.timers.", "timer"),
        ("systemd.user.paths.", "path"),
        ("systemd.services.", "service"),
        ("systemd.timers.", "timer"),
    ] {
        for rest in module.split(prefix).skip(1) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            out.push(format!("{name}.{suffix}"));
        }
    }
    out
}

#[test]
fn the_guides_options_are_inits_own() {
    let text = guide();
    let begin = text
        .find(options::BEGIN)
        .expect("AGENT.md holds the options' first marker");
    let end = text[begin..]
        .find(options::END)
        .map(|at| begin + at + options::END.len() + 1)
        .expect("AGENT.md holds the options' last marker");
    assert_eq!(
        &text[begin..end],
        options::markdown(),
        "AGENT.md's options are not `suzatary init --options`; paste its output between the markers"
    );
}

/// What the guide says leaves the box is what the code sends: the section is
/// `suzatary doctor --outbound`'s output, byte for byte.
#[test]
fn the_guides_outbound_section_is_the_codes() {
    use suzatary_cli::commands::outbound;
    let text = guide();
    let begin = text
        .find(outbound::BEGIN)
        .expect("AGENT.md holds the outbound section's first marker");
    let end = text[begin..]
        .find(outbound::END)
        .map(|at| begin + at + outbound::END.len() + 1)
        .expect("AGENT.md holds the outbound section's last marker");
    assert_eq!(
        &text[begin..end],
        outbound::markdown(),
        "AGENT.md's outbound section is not `suzatary doctor --outbound`; paste its output \
         between the markers"
    );
}

#[test]
fn every_command_the_guide_names_answers() {
    let (app, script) = (app(), Script::read(&guide()));
    assert!(
        ["secret", "tool", "which", "shell"]
            .iter()
            .all(|verb| script.verbs.contains(&(*verb).to_owned())),
        "{:?}",
        script.verbs
    );
    let mut named = 0;
    let wrong: Vec<String> = commands(&guide())
        .iter()
        .flat_map(|text| invocations(text))
        .filter(|words| !words.is_empty())
        .inspect(|_| named += 1)
        .filter_map(|words| {
            let why = if script.verbs.contains(&words[0]) {
                script.refused(&words)
            } else {
                refused(&app, &words)
            };
            why.map(|why| format!("{why}: {words:?}"))
        })
        .collect();
    assert!(
        named > 20,
        "found only {named} commands; the scan is broken"
    );
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn every_unit_the_guide_names_is_the_modules() {
    let declared = declared_units();
    assert!(
        declared.contains(&"suzatary-web.service".to_owned()),
        "{declared:?}"
    );
    let mut named = 0;
    let wrong: Vec<String> = commands(&guide())
        .iter()
        .filter(|text| text.contains("systemctl") || text.contains("journalctl"))
        .flat_map(|text| {
            text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '.'))
                .filter(|word| word.starts_with("suzatary-") || word.starts_with("proton-bridge"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .inspect(|_| named += 1)
        .filter(|unit| {
            let unit = if unit.contains('.') {
                unit.clone()
            } else {
                format!("{unit}.service")
            };
            !declared.contains(&unit)
        })
        .collect();
    assert!(named > 5, "found only {named} units; the scan is broken");
    assert!(
        wrong.is_empty(),
        "units the module does not declare: {wrong:?}"
    );
}

#[test]
fn every_flag_the_guide_gives_a_script_is_its_own() {
    let mut named = 0;
    let wrong: Vec<String> = commands(&guide())
        .iter()
        .flat_map(|text| {
            simple(text)
                .map(|words| words.join(" "))
                .collect::<Vec<_>>()
        })
        .filter_map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let at = words.iter().position(|word| {
                word.starts_with("scripts/")
                    && std::path::Path::new(word)
                        .extension()
                        .is_some_and(|ext| ext == "sh")
            })?;
            let script = read(words[at]);
            named += 1;
            let unknown: Vec<&&str> = words[at + 1..]
                .iter()
                .filter(|word| word.starts_with("--") && !script.contains(**word))
                .collect();
            (!unknown.is_empty()).then(|| format!("{}: {unknown:?}", words[at]))
        })
        .collect();
    assert!(
        named > 2,
        "found only {named} script calls; the scan is broken"
    );
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn every_file_the_guide_points_to_is_here() {
    let commands = commands(&guide());
    let missing: Vec<&str> = commands
        .iter()
        .flat_map(|text| text.split_whitespace())
        .map(|word| word.trim_matches(|c: char| "\"'`(),.:".contains(c)))
        // The app's checkout, where the guide clones it.
        .map(|word| word.strip_prefix("~/src/suzatary/").unwrap_or(word))
        .filter(|word| {
            ["app/", "scripts/", "docs/", "nix/", "templates/"]
                .iter()
                .any(|root| word.starts_with(root))
                || ["CLAUDE.md", "CONTRIBUTING.md", "flake.nix"].contains(word)
        })
        .filter(|word| !repo().join(word).exists())
        .collect();
    assert!(
        missing.is_empty(),
        "AGENT.md points to files that are not here: {missing:?}"
    );
}
