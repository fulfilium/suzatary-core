//! One connection: a template applied to what a person chose.

use std::fmt;

use prodrome_data::literal::{ProdromeError, Value};
use serde::{Deserialize, Serialize};

use super::{AccountKind, Group, Params, Seen, Standing, Step, Table, Template, TemplateId};
use prodrome_data::literal::codec::{Constructor, Fields, Literal, record};
use suzatary_health::health::Refused;
use suzatary_health::health::SourceKey;
use suzatary_health::health::plugin::CredentialName;
use suzatary_mail::mail::account_email::AccountEmail;

const MODEL_KEY: &str = "model";

/// A template applied to parameters of its shape; [`Connection::new`] is its only constructor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Connection {
    template: TemplateId,
    params: Params,
}

/// A connection's identity: a mail account is one key by its address, whatever syncs it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub group: Group,
    /// An address, a label, a folder, the model or a channel.
    pub name: String,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.group.as_str(), self.name)
    }
}

impl Connection {
    pub fn new(template: TemplateId, params: Params) -> Result<Self, Refused> {
        let shape = template.template().shape();
        if params.shape() != shape {
            return Err(Refused::Params {
                template: template.as_str(),
                wanted: shape.wanted(),
            });
        }
        Ok(Self { template, params })
    }

    #[must_use]
    pub const fn of_channel(channel: super::Channel) -> Self {
        Self {
            template: channel.template(),
            params: Params::Nothing,
        }
    }

    /// A connection from the template's name and its parameters' texts.
    pub fn parse(template: &str, params: &[String]) -> Result<Self, Refused> {
        let id = TemplateId::parse(template)?;
        Self::new(
            id,
            Params::parse(id.template().shape(), id.as_str(), params)?,
        )
    }

    #[must_use]
    pub const fn id(&self) -> TemplateId {
        self.template
    }

    #[must_use]
    pub const fn template(&self) -> &'static Template {
        self.template.template()
    }

    #[must_use]
    pub const fn params(&self) -> &Params {
        &self.params
    }

    #[must_use]
    pub fn key(&self) -> Key {
        let template = self.template();
        let name = match (template.table, &self.params) {
            (Table::Always, _) => "data".to_owned(),
            (Table::Model, _) => MODEL_KEY.to_owned(),
            (Table::Channel(channel), _) => channel.as_str().to_owned(),
            (_, Params::Label(label)) => label.as_str().to_owned(),
            (_, Params::Address(email) | Params::Microsoft { email, .. }) => {
                email.as_str().to_owned()
            }
            (_, Params::Folder(folder)) => folder.to_string(),
            (_, Params::Nothing) => template.id.as_str().to_owned(),
        };
        Key {
            group: template.group,
            name,
        }
    }

    /// Whether writing it changes `held`, the connection under its key, instead of duplicating it:
    /// a Gmail account read through lieer's own sign-in moves to sign-in with Google, which reads
    /// the same mail and its calendars too.
    #[must_use]
    pub fn moves(&self, held: &Connection) -> bool {
        self.key() == held.key()
            && matches!(
                (held.template, self.template),
                (TemplateId::Gmail, TemplateId::Google)
            )
    }

    /// For a Gmail account read through lieer, the Google connection that [moves](Self::moves)
    /// it onto the box's own sign-in; none for any other.
    #[must_use]
    pub fn moved_to_google(&self) -> Option<Self> {
        let TemplateId::Gmail = self.template else {
            return None;
        };
        Self::new(TemplateId::Google, self.params.clone()).ok()
    }

    /// The sources its health is read from; none for what the box reads nothing from.
    #[must_use]
    pub fn sources(&self) -> Vec<SourceKey> {
        match (self.template().table, &self.params) {
            (Table::Account(kind), Params::Address(email) | Params::Microsoft { email, .. }) => {
                let email = AccountEmail::folded(email);
                let mut sources = vec![SourceKey::Mail(email.clone())];
                if kind == AccountKind::Google {
                    sources.push(SourceKey::GoogleCalendar(email));
                }
                sources
            }
            (Table::Link(fragment), Params::Label(label)) => vec![SourceKey::Link {
                plugin: fragment.plugin.clone(),
                credential: CredentialName::link(fragment.plugin, label),
            }],
            (Table::Folder, _) => vec![SourceKey::Drive],
            // `Connection::new` refuses the mismatched shapes; the box reads nothing from the rest.
            (
                Table::Link(_),
                Params::Nothing | Params::Address(_) | Params::Microsoft { .. } | Params::Folder(_),
            )
            | (Table::Account(_) | Table::Always | Table::Model | Table::Channel(_), _) => {
                Vec::new()
            }
        }
    }

    /// What the owner calls it at the start of a sentence (`your Work calendar`).
    #[must_use]
    pub fn called(&self) -> String {
        let template = self.template();
        match (template.table, &self.params) {
            (Table::Link(_), Params::Label(label)) if template.group == Group::Calendar => {
                format!("your {} calendar", label.shown())
            }
            (Table::Link(_), Params::Label(label)) => format!("the {} link", label.shown()),
            (_, Params::Address(email) | Params::Microsoft { email, .. }) => email.to_string(),
            (_, Params::Folder(folder)) => format!("the Drive folder {folder}"),
            (Table::Always, _) => "the data repository".to_owned(),
            (
                Table::Model
                | Table::Channel(_)
                | Table::Link(_)
                | Table::Account(_)
                | Table::Folder,
                _,
            ) => template.title.to_owned(),
        }
    }

    /// The place in the instance file it writes; no table holds a secret's value.
    #[must_use]
    pub fn writes(&self) -> String {
        match (self.template().table, &self.params) {
            (Table::Link(fragment), _) => {
                format!(
                    "one link for {}",
                    super::plugin_called(fragment.plugin.as_str())
                )
            }
            (Table::Account(_), _) => format!("a [[mail.accounts]] entry for {}", self.key().name),
            (Table::Folder, _) => format!("{} in [secretary] drive_allow", self.key().name),
            (Table::Channel(channel), _) => {
                format!("the {} channel in [notify]", channel.as_str())
            }
            (Table::Always, _) => "nothing: every deployment has one".to_owned(),
            (Table::Model, _) => "nothing: every deployment runs its model".to_owned(),
        }
    }

    #[must_use]
    pub fn steps(&self) -> Vec<Step> {
        let template = self.template();
        let plugin = match template.table {
            Table::Link(fragment) => Some(fragment.plugin),
            _ => None,
        };
        template
            .steps
            .iter()
            .filter_map(|shape| shape.fill(plugin, &self.params))
            .collect()
    }

    #[must_use]
    pub fn secrets(&self) -> Vec<String> {
        self.steps()
            .into_iter()
            .filter_map(|step| match step {
                Step::Secret { name, .. } => Some(name),
                _ => None,
            })
            .collect()
    }

    /// The secret-store names that go with it when it is removed, the Google grant included.
    #[must_use]
    pub fn kept(&self) -> Vec<String> {
        self.steps()
            .into_iter()
            .filter_map(|step| match step {
                Step::Secret { name, .. } => Some(name),
                Step::AuthCode { account } => Some(suzatary_google::google::grant_name(&account)),
                Step::DeviceCode { .. }
                | Step::SendingConsent { .. }
                | Step::GoogleClient
                | Step::SignIn
                | Step::Terminal { .. } => None,
            })
            .collect()
    }

    /// Its steps' standing, joined; the reader joins its check's results.
    #[must_use]
    pub fn standing(&self, seen: &Seen<'_>) -> Standing {
        self.steps()
            .iter()
            .enumerate()
            .map(|(at, step)| step.standing(seen).at_step(at))
            .fold(Standing::Kept, Standing::join)
    }

    #[must_use]
    pub fn unmet(&self, seen: &Seen<'_>) -> Vec<Step> {
        self.steps()
            .into_iter()
            .filter(|step| step.standing(seen) != Standing::Kept)
            .collect()
    }
}

pub const CONNECTION: Constructor = ("Connection", &["template", "params"]);

// By hand: its reads refuse as `Fields` does.
impl Literal for Connection {
    fn to_value(&self) -> Value {
        record(
            CONNECTION,
            vec![
                Value::str(self.template.as_str()),
                self.params.texts().to_value(),
            ],
        )
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::of(value, CONNECTION)?;
        Self::parse(
            &fields.get::<String>("template")?,
            &fields.get::<Vec<String>>("params")?,
        )
        .map_err(|refused| ProdromeError::invalid(refused.to_string()))
    }
}

#[derive(Serialize, Deserialize)]
struct Spelled {
    template: String,
    params: Vec<String>,
}

impl Serialize for Connection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Spelled {
            template: self.template.as_str().to_owned(),
            params: self.params.texts(),
        }
        .serialize(serializer)
    }
}

// Read through `parse`, so a stored act names only a connection the constructor admits.
impl<'de> Deserialize<'de> for Connection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spelled = Spelled::deserialize(deserializer)?;
        Self::parse(&spelled.template, &spelled.params).map_err(serde::de::Error::custom)
    }
}
