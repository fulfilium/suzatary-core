//! What the owner is told when the box asks for a secret: the template's words and, where one is
//! known, the page of the service that issues it. Data, keyed by the name, since every name a
//! deployment may set is derived from the template that declares it.

use crate::coding::RoutineId;

/// How to find one secret's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Help {
    /// One sentence: where the value is found.
    pub text: &'static str,
    /// The page that issues it; always https, and only ever this table's or the declared
    /// routine's own.
    pub link: Option<String>,
}

/// Where a secret's value is issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// No page is known.
    Unknown,
    /// One fixed page.
    At(&'static str),
    /// The page of the routine `[coding]` declares, which only a deployment knows.
    Routine,
}

/// A secret's words as data: what a template's step says of it, and where its value is issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Words {
    /// What the owner calls it; in a plugin's link, `{label}` stands for the link's own label.
    pub label: &'static str,
    /// One sentence: where the value is found.
    pub text: &'static str,
    /// Where it is issued.
    pub page: Page,
}

impl Help {
    /// The help for the secret named `name` on a deployment whose `[coding]` declares `routine`;
    /// `None` for a name no template here knows, and for one issued by a routine none declares.
    #[must_use]
    pub fn of(name: &str, routine: Option<&RoutineId>) -> Option<Self> {
        let Words { text, page, .. } = Words::of(name)?;
        let link = match page {
            Page::Unknown => None,
            Page::At(href) => Some(href.to_owned()),
            Page::Routine => Some(routine?.page()),
        };
        Some(Self { text, link })
    }

    /// What the owner calls the secret named `name`: the table's label, a plugin's link named by
    /// its own label, or for a name no template here knows, the plugin it belongs to, or that it
    /// is a key the deployment declares.
    #[must_use]
    pub fn label(name: &str) -> String {
        let credential = name
            .strip_prefix("PLUGIN_")
            .and_then(|rest| rest.split_once('_'));
        match (Words::of(name), credential) {
            (Some(words), Some((_, suffix))) => words.label.replace(
                "{label}",
                &crate::connect::Label::of_suffix(suffix)
                    .map_or_else(|_| suffix.to_owned(), |label| label.shown()),
            ),
            (Some(words), None) => words.label.to_owned(),
            (None, Some((plugin, _))) => {
                format!("a credential of the {} plugin", plugin.to_lowercase())
            }
            (None, None) => "a key this deployment declares".to_owned(),
        }
    }
}

impl Words {
    /// The words for the secret named `name`: its template's step's, or for a name no default
    /// template asks for, this table's.
    fn of(name: &str) -> Option<Self> {
        crate::connect::secret_words(name).or_else(|| {
            match name {
                crate::coding::ROUTINE_TOKEN => Some(Self {
                    label: "the coding routine's token",
                    text: "On the routine's page, generate its API trigger's token.",
                    page: Page::Routine,
                }),
                // Set at a terminal alone, as a mail transport's secret is; named here
                // so a page says what it is without its name.
                "PROTON_BRIDGE_PASSWORD" => Some(Self {
                    label: "the Proton Bridge password",
                    text: "In `protonmail-bridge --cli`, `info` prints it.",
                    page: Page::Unknown,
                }),
                crate::google::CLIENT_ID | crate::google::CLIENT_SECRET => Some(Self {
                    label: "the box's Google client",
                    text: "Make it once in the Google Cloud console, from the card that asks \
                               for it, and paste its downloaded file there.",
                    page: Page::At(crate::google::CONSOLE),
                }),
                grant if grant.starts_with("GOOGLE_GRANT_") => Some(Self {
                    label: "a Google account's sign-in with Google",
                    text: "Sign the account in with Google from its card; the box keeps \
                               what Google grants.",
                    page: Page::Unknown,
                }),
                crate::settings::DATA_PUSH_TOKEN => Some(Self {
                    label: "the data repository's push token",
                    text: "On GitHub, create a fine-grained token for the data repository \
                               alone, with Contents read and write.",
                    page: Page::At("https://github.com/settings/personal-access-tokens/new"),
                }),
                crate::repos::TOKEN => Some(Self {
                    label: "the repositories' token",
                    text: "On GitHub, create a fine-grained token for the repositories the \
                               secretary reaches alone, with Contents and Pull requests read \
                               and write.",
                    page: Page::At("https://github.com/settings/personal-access-tokens/new"),
                }),
                _ => None,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every link is https, so a card never draws another scheme as a button.
    #[test]
    fn every_link_is_https() {
        let names = ["NTFY_TOPIC", "FCM_SERVICE_ACCOUNT_B64"]
            .into_iter()
            .chain([
                crate::coding::ROUTINE_TOKEN,
                crate::settings::DATA_PUSH_TOKEN,
                crate::repos::TOKEN,
                "PLUGIN_ICS_WORK",
            ]);
        let routine = routine();
        for name in names {
            let help = Help::of(name, Some(&routine)).unwrap_or_else(|| panic!("{name} has help"));
            assert!(
                help.link.is_none_or(|link| link.starts_with("https://")),
                "{name}"
            );
        }
    }

    fn routine() -> RoutineId {
        "trig_01Example".parse().expect("a routine id")
    }
}
