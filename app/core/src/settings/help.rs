//! What the owner is told when the box asks for a secret, keyed by its name.

use crate::coding::RoutineId;

/// How to find one secret's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Help {
    /// Where the value is found.
    pub text: &'static str,
    /// The issuing page, always https.
    pub link: Option<String>,
}

/// Where a secret's value is issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// No page is known.
    Unknown,
    /// One fixed page.
    At(&'static str),
    /// The page of the routine `[coding]` declares.
    Routine,
}

/// A secret's words as data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Words {
    /// What the owner calls it; `{label}` is a plugin link's label.
    pub label: &'static str,
    /// Where the value is found.
    pub text: &'static str,
    /// Where it is issued.
    pub page: Page,
}

impl Help {
    /// The help, or `None` for an unknown name or undeclared routine.
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

    /// What the owner calls the secret `name`, never the name itself.
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
    /// The template's words for `name`, else this table's.
    fn of(name: &str) -> Option<Self> {
        crate::connect::secret_words(name).or_else(|| {
            match name {
                crate::coding::ROUTINE_TOKEN => Some(Self {
                    label: "the coding routine's token",
                    text: "On the routine's page, generate its API trigger's token.",
                    page: Page::Routine,
                }),
                // Set only at a terminal; listed so a page can label it.
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
