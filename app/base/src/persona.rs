//! The person this deployment serves, `instance.toml`'s `[persona]`.
//! The manuals carry `{{persona.*}}` placeholders that [`Persona::render`] fills at read time.

/// What a manual or the weather line knows of the person: no path, no secret, no mail address.
/// The instance file that holds it is pushed, so private words live in [`Prose`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Persona {
    /// How the manuals address them: `{{persona.name}}`.
    pub name: String,
    /// The city `suzatary weather` forecasts.
    pub city: String,
    /// The person's zone by name (`America/New_York`): what a day is to them.
    pub zone: crate::zone::Zone,
    /// Read from the box-local `persona.toml`, never `[persona]`, so a push cannot carry it.
    #[serde(skip)]
    pub prose: Prose,
}

/// The person's own prose, box-local in `<data>/local`: carried by the vault, never by a push.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Prose {
    /// Topics worth surfacing in a briefing; `{{persona.news_filter}}` joins them into a sentence.
    pub news_filter: Vec<String>,
    /// This person's email voice, one rule per line; `{{persona.email_voice}}` renders bullets.
    pub email_voice: Vec<String>,
    /// The scale's anchors, each `"value: meaning"`; `{{persona.anchors_table}}` makes a table.
    pub anchors: Vec<String>,
}

impl Default for Persona {
    /// A placeholder name, so an unconfigured manual looks unconfigured.
    fn default() -> Self {
        Self {
            name: "(unconfigured persona)".to_owned(),
            city: String::new(),
            zone: crate::zone::Zone::UTC,
            prose: Prose::default(),
        }
    }
}

impl Persona {
    /// Fill in the placeholders; a template need not mention every one.
    #[must_use]
    pub fn render(&self, template: &str) -> String {
        template
            .replace("{{persona.name}}", &self.name)
            .replace(
                "{{persona.news_filter}}",
                &Self::english_list(&self.prose.news_filter),
            )
            .replace(
                "{{persona.email_voice}}",
                &Self::bullets(&self.prose.email_voice),
            )
            .replace("{{persona.anchors_table}}", &self.anchors_table())
    }

    /// `["a"]` → `"a"`; `["a", "b"]` → `"a and b"`; `["a", "b", "c"]` → `"a, b and c"`.
    fn english_list(items: &[String]) -> String {
        match items {
            [] => "nothing".to_owned(),
            [only] => only.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    }

    fn bullets(items: &[String]) -> String {
        items
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The anchors as a Markdown table; an entry with no `:` prints whole, so it stays visible.
    fn anchors_table(&self) -> String {
        use std::fmt::Write as _;
        let mut rows = String::from("| value | means |\n|---|---|");
        for line in &self.prose.anchors {
            let (value, meaning) = line.split_once(':').unwrap_or((line.as_str(), ""));
            let _ = write!(rows, "\n| {} | {} |", value.trim(), meaning.trim());
        }
        rows
    }
}
