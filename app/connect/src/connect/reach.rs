//! How a person reaches the app and its tools at a terminal, which every terminal step is
//! spelled in.

/// The way to the app's binary, its tools and its secret store, from a terminal on the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// A user repository's script (`./suzatary`), which runs the app its lock names, a
    /// command among the app's tools, and `secretspec` against the file the app declares its
    /// names in.
    Script(String),
    /// The app's binary and its tools on `PATH`, and the file `suzatary secretspec` prints, which
    /// declares every name the instance may set as of the moment the step runs.
    Installed,
    /// The tools on `PATH` and `secretspec`'s own file: what a replica, which holds no
    /// deployment, spells.
    Plain,
}

/// A reach that knows no script and no deployment.
pub static PLAIN: Reach = Reach::Plain;

impl Reach {
    /// One of the app's verbs, with its arguments.
    #[must_use]
    pub fn app(&self, verb: &str) -> String {
        match self {
            Reach::Script(script) => format!("{script} {verb}"),
            Reach::Installed | Reach::Plain => format!("suzatary {verb}"),
        }
    }

    /// A command among the app's tools.
    #[must_use]
    pub fn tool(&self, command: &str) -> String {
        match self {
            Reach::Script(script) => format!("{script} tool {command}"),
            Reach::Installed | Reach::Plain => command.to_owned(),
        }
    }

    /// The secret `name`, set at `secretspec`'s prompt against the file that declares it when
    /// the step runs, so its value is never an argument and no path is spelled that a later
    /// connection could leave behind.
    #[must_use]
    pub fn secret(&self, name: &str) -> String {
        match self {
            Reach::Script(script) => format!("{script} secret {name}"),
            Reach::Installed => {
                format!("secretspec set {name} --file \"$(suzatary secretspec)\"")
            }
            Reach::Plain => format!("secretspec set {name}"),
        }
    }
}

/// A command a person runs at a terminal: its text a template's (`&'static str`) or a
/// connection's, filled (`String`), and spelled by a [`Reach`] only when it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run<T> {
    /// One of the app's tools, with its arguments.
    Tool(T),
    /// A script of the app's tools, run as one tool by `sh -c`.
    Script(T),
    /// The secret of this name, set at the secret store's prompt.
    Secret(T),
    /// A command of the person's own shell, run as written.
    Shell(T),
}

impl<T> Run<T> {
    /// The same command, its text mapped.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Run<U> {
        match self {
            Run::Tool(text) => Run::Tool(f(text)),
            Run::Script(text) => Run::Script(f(text)),
            Run::Secret(text) => Run::Secret(f(text)),
            Run::Shell(text) => Run::Shell(f(text)),
        }
    }

    /// Its text.
    pub const fn text(&self) -> &T {
        match self {
            Run::Tool(text) | Run::Script(text) | Run::Secret(text) | Run::Shell(text) => text,
        }
    }
}

impl<T: AsRef<str>> Run<T> {
    /// What the person types, as `reach` reaches the app.
    #[must_use]
    pub fn spelled(&self, reach: &Reach) -> String {
        match self {
            Run::Tool(command) => reach.tool(command.as_ref()),
            Run::Script(script) => reach.tool(&format!("sh -c {}", word(script.as_ref()))),
            Run::Secret(name) => reach.secret(name.as_ref()),
            Run::Shell(command) => command.as_ref().to_owned(),
        }
    }

    /// What a page says of it: a secret by its words and never its name, which only a terminal
    /// is told; any other command as `reach` spells it.
    #[must_use]
    pub fn shown(&self, reach: &Reach) -> String {
        match self {
            Run::Secret(name) => format!(
                "set {}, typed at the secret store's prompt",
                crate::settings::Help::label(name.as_ref())
            ),
            other => other.spelled(reach),
        }
    }
}

/// `text` as one POSIX shell word: as it is when every character is one no shell reads, else
/// in single quotes, each quote within closed, escaped and opened again.
#[must_use]
pub fn word(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@%+=:,./_-".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A word reads back, through `sh`, as the text it quotes, whatever that holds.
    #[test]
    fn a_word_is_its_text_to_a_shell() {
        for text in [
            "ada@example.test",
            "o'b@x.test",
            "a;rm -rf ~@x.test",
            "$(id)@x.test",
            "a b",
            "''",
            "",
        ] {
            let said = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("printf %s {}", word(text)))
                .output()
                .expect("sh runs");
            assert_eq!(String::from_utf8_lossy(&said.stdout), text, "{text:?}");
        }
        let script = Run::Script(format!("gmi init {}", word("o'b@x.test")));
        let spelled = script.spelled(&PLAIN);
        let said = std::process::Command::new("sh")
            .arg("-c")
            .arg(spelled.replacen("sh -c", "printf %s", 1))
            .output()
            .expect("sh runs");
        assert_eq!(
            String::from_utf8_lossy(&said.stdout),
            "gmi init 'o'\\''b@x.test'"
        );
    }
}
