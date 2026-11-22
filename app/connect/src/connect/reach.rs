//! How a person reaches the app and its tools at a terminal on the box.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// A user repository's script (`./suzatary`), which runs the app, its tools and `secretspec`.
    Script(String),
    /// The app and its tools on `PATH`, with the secretspec file `suzatary secretspec` prints.
    Installed,
    /// The tools on `PATH` and `secretspec`'s own file, for a replica that holds no deployment.
    Plain,
}

pub static PLAIN: Reach = Reach::Plain;

impl Reach {
    #[must_use]
    pub fn app(&self, verb: &str) -> String {
        match self {
            Reach::Script(script) => format!("{script} {verb}"),
            Reach::Installed | Reach::Plain => format!("suzatary {verb}"),
        }
    }

    #[must_use]
    pub fn tool(&self, command: &str) -> String {
        match self {
            Reach::Script(script) => format!("{script} tool {command}"),
            Reach::Installed | Reach::Plain => command.to_owned(),
        }
    }

    /// Sets the secret `name` at `secretspec`'s prompt, so its value is never an argument.
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

/// A command a person runs at a terminal, spelled by a [`Reach`] only when it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run<T> {
    Tool(T),
    /// A script of the app's tools, run as one tool by `sh -c`.
    Script(T),
    /// The name of a secret, set at the secret store's prompt.
    Secret(T),
    /// A command of the person's own shell, run as written.
    Shell(T),
}

impl<T> Run<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Run<U> {
        match self {
            Run::Tool(text) => Run::Tool(f(text)),
            Run::Script(text) => Run::Script(f(text)),
            Run::Secret(text) => Run::Secret(f(text)),
            Run::Shell(text) => Run::Shell(f(text)),
        }
    }

    pub const fn text(&self) -> &T {
        match self {
            Run::Tool(text) | Run::Script(text) | Run::Secret(text) | Run::Shell(text) => text,
        }
    }
}

impl<T: AsRef<str>> Run<T> {
    #[must_use]
    pub fn spelled(&self, reach: &Reach) -> String {
        match self {
            Run::Tool(command) => reach.tool(command.as_ref()),
            Run::Script(script) => reach.tool(&format!("sh -c {}", word(script.as_ref()))),
            Run::Secret(name) => reach.secret(name.as_ref()),
            Run::Shell(command) => command.as_ref().to_owned(),
        }
    }

    /// What a page shows: a secret by its label, never its name, which only a terminal is told.
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

/// `text` as one POSIX shell word: bare when no character is special, else single-quoted.
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

    /// A word reads back through `sh` as the text it quotes.
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
