//! One thing a person does for a connection.

use super::{Checked, Params, Reach, Run, Seen, Standing};
use crate::mail::address::EmailAddress;
use crate::settings::{Help, Words};

/// Which secret a step asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretName {
    /// One name, whatever the parameters.
    Fixed(&'static str),
    /// The link its label names on the template's plugin: `PLUGIN_<PLUGIN>_<LABEL>`.
    Link,
}

/// A step as a template declares it: data, its blanks filled by a connection's parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepShape {
    /// A secret, with the template's words on what it is and where it is issued.
    Secret {
        /// Which.
        name: SecretName,
        /// Its words.
        help: Words,
    },
    /// The account's device-code sign-in, to read its mail.
    DeviceCode,
    /// The account's device-code consent to send mail, a grant apart from reading's.
    SendingConsent,
    /// The box's one Google Cloud client, made by the owner and pasted into its card.
    GoogleClient,
    /// The account's sign-in with Google: an authorization code the consent sends back to the
    /// box.
    AuthCode,
    /// The model's sign-in, which the deployment runs.
    SignIn,
    /// A command at a terminal; `{email}` and `{subdir}` stand for the account's.
    Terminal {
        /// What to run.
        run: Run<&'static str>,
        /// Why, in a few words.
        why: &'static str,
    },
}

/// One thing the person does for a connection, filled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Paste a secret into a card's write-only field: the value travels only in the tap.
    Secret {
        /// The name it is stored under.
        name: String,
        /// The template's words for it.
        help: Words,
    },
    /// Tap the account's sign-in, which the box runs as the device-code flow, IMAP only.
    DeviceCode {
        /// The account.
        account: EmailAddress,
    },
    /// Tap the account's consent to send, which the box runs as the device-code flow, SMTP
    /// only.
    SendingConsent {
        /// The account.
        account: EmailAddress,
    },
    /// Make the box's Google Cloud client and paste its downloaded file into the card's field.
    GoogleClient,
    /// Tap the account's sign-in with Google: the box opens Google's consent page, and Google
    /// sends the owner back to the box with a code it trades for the grant.
    AuthCode {
        /// The account.
        account: EmailAddress,
    },
    /// Tap the model's sign-in, which the deployment runs on the box.
    SignIn,
    /// Run a command at a terminal on the box, which only the person can do.
    Terminal {
        /// What to run.
        run: Run<String>,
        /// Why.
        why: &'static str,
    },
}

impl StepShape {
    /// This step for a connection of `params`, on `plugin` where the template writes a
    /// plugin's credential; `None` where its blanks name a parameter the shape lacks, which
    /// the catalog's law rules out for every template.
    #[must_use]
    pub fn fill(&self, plugin: Option<&str>, params: &Params) -> Option<Step> {
        let email = match params {
            Params::Address(email) | Params::Microsoft { email, .. } => Some(email),
            Params::Nothing | Params::Label(_) | Params::Folder(_) => None,
        };
        Some(match *self {
            StepShape::Secret {
                name: SecretName::Fixed(name),
                help,
            } => Step::Secret {
                name: name.to_owned(),
                help,
            },
            StepShape::Secret {
                name: SecretName::Link,
                help,
            } => match (plugin, params) {
                (Some(plugin), Params::Label(label)) => Step::Secret {
                    name: format!("PLUGIN_{}_{}", plugin.to_ascii_uppercase(), label.as_str()),
                    help,
                },
                _ => return None,
            },
            StepShape::DeviceCode => Step::DeviceCode {
                account: email?.clone(),
            },
            StepShape::SendingConsent => Step::SendingConsent {
                account: email?.clone(),
            },
            StepShape::GoogleClient => Step::GoogleClient,
            StepShape::AuthCode => Step::AuthCode {
                account: email?.clone(),
            },
            StepShape::SignIn => Step::SignIn,
            StepShape::Terminal { run, why } => {
                let text = run.text();
                let names = text.contains("{email}") || text.contains("{subdir}");
                // Each value is one shell word wherever it stands, so no address is read as
                // more of the command than itself.
                let run = match email {
                    Some(email) => run.map(|text| {
                        text.replace("{email}", &super::reach::word(email.as_str()))
                            .replace(
                                "{subdir}",
                                &super::reach::word(&crate::mail::account::slug_of(email.as_str())),
                            )
                    }),
                    None if names => return None,
                    None => run.map(str::to_owned),
                };
                Step::Terminal { run, why }
            }
        })
    }
}

impl Step {
    /// What the person does, as a card lists it, a terminal's command spelled as `reach` reaches
    /// the app; a secret by its words, never its name.
    #[must_use]
    pub fn sentence(&self, reach: &Reach) -> String {
        match self {
            Step::Secret { name, .. } => format!("set {} in its card's field", Help::label(name)),
            Step::DeviceCode { account } => format!("sign {account} in from its card"),
            Step::SendingConsent { account } => {
                format!("sign {account} in to send mail from its card")
            }
            Step::GoogleClient => {
                "make the box's Google Cloud client and paste its file into its card".to_owned()
            }
            Step::AuthCode { account } => format!("sign {account} in with Google from its card"),
            Step::SignIn => "sign the model in from its card".to_owned(),
            Step::Terminal { run, why } => {
                format!("at a terminal on the box: {} ({why})", run.shown(reach))
            }
        }
    }

    /// Where it stands as `seen` reads it: in place once taken, else what it asks.
    #[must_use]
    pub fn standing(&self, seen: &Seen<'_>) -> Standing {
        match (self, seen.checked) {
            (Step::Secret { name, .. }, _) => Standing::of_secret(name, (seen.secret)(name)),
            (Step::DeviceCode { account }, _) if (seen.signed_in)(account) => Standing::Kept,
            (Step::DeviceCode { .. }, _) => Standing::missing("its sign-in to read mail"),
            (Step::SendingConsent { account }, _) if (seen.may_send)(account) => Standing::Kept,
            (Step::SendingConsent { .. }, _) => Standing::missing("its sign-in to send mail"),
            (Step::GoogleClient, _) => [crate::google::CLIENT_ID, crate::google::CLIENT_SECRET]
                .into_iter()
                .map(|name| Standing::of_secret(name, (seen.secret)(name)))
                .fold(Standing::Kept, Standing::join),
            (Step::AuthCode { account }, _) if (seen.signed_in)(account) => Standing::Kept,
            (Step::AuthCode { .. }, _) => Standing::missing("its sign-in with Google"),
            (Step::SignIn | Step::Terminal { .. }, Checked::Once) => Standing::Kept,
            (Step::SignIn, Checked::Never) => Standing::missing("the model's sign-in"),
            (Step::Terminal { run, why }, Checked::Never) => Standing::ByHand {
                step: 0,
                command: run.shown(seen.reach),
                why: (*why).to_owned(),
            },
        }
    }
}
